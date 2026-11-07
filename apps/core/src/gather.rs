//! Harvesting artifacts from somewhere else into a weaver workspace.
//!
//! Gathering fills the workspace's own shelf and stops there. Nothing is
//! written into a mind project: which of the gathered skills a project should
//! carry is a separate decision, made later and by somebody, and doing it here
//! would mean a `git clone` silently editing repositories.
//!
//! What lands where: a source's clone goes to
//! `.mindweaver/cache/<source>/`, and each artifact it yields is copied to
//! `.mindweaver/<kind folder>/<source>/<name>/`. Namespacing by source is what
//! lets two repositories both offer `commit-style`; [`crate::ledger`] is what
//! remembers which is which.

mod watch;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

use crate::adapter::gix::Gix;
use crate::artifact::{Artifact, ArtifactError, ValidationIssue};
use crate::code::codes;
use crate::copy::{self, Change};
use crate::hash;
use crate::kind::{Kind, Layout};
use crate::ledger::{self, Action, Ledger, LedgerError, Merged, Outcome, SourceKind, Upstream};
use crate::lock::LockError;
use crate::marketplace::{Dialect, Holding};
use crate::paths;
use crate::plugin::{self, Manifest};
use crate::port::{Cloned, GitError, Remote};
use crate::url::{carries_credentials, fetched, redacted};
use crate::value::{ArtifactName, Origin, Redacted, Revision, SourceName};
use crate::workspace::WeaverWorkspace;

pub use watch::{Progress, Watch};

/// The folder inside a source that is searched unless another is named:
/// none, so the whole repository is searched from its root, as far down as
/// [`SEARCH_DEPTH`].
///
/// The root rather than `skills`, which it was until #101: a repository
/// keeps its skills where its tools put them (`skills/<name>`,
/// `.claude/skills/<name>`, `plugins/<plugin>/skills/<name>`, a `SKILL.md`
/// of its own at its root), and only a search of all of it finds each
/// layout without being told. A source an older build gathered without
/// `--path` was recorded at `skills`, and is gathered again from there
/// (`Request::again`): its folder is part of what makes it the source it
/// is ([`ledger::Source::subdirectory`]).
pub const DEFAULT_SUBDIRECTORY: &str = "";

/// The names a licence file beside a skill's manifest goes by, compared
/// without case, in the order one is looked for when a folder holds more
/// than one: both spellings, bare, as Markdown or as text.
pub const LICENSE_FILES: [&str; 6] = [
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "LICENCE",
    "LICENCE.md",
    "LICENCE.txt",
];

/// The tag a file names its licence after, as SPDX spells it.
const SPDX_TAG: &str = "SPDX-License-Identifier:";

/// How much of a licence file is read for the line that names it: the head
/// of it, where both the tag and a title are, and never a file somebody
/// made large enough to be a reason not to finish.
const LICENSE_READ: u64 = 64 * 1024;

/// Where to gather from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A git repository, at a branch or tag, or at whatever its HEAD names.
    Git {
        url: String,
        reference: Option<String>,
    },
}

/// As an object holding its kind, as the ledger files it, and what that kind
/// is given: for a git repository its URL, only through [`redacted`], and
/// the branch or tag asked for.
///
/// By hand rather than derived: a derive would tag it with the variant's Rust
/// name, where the kind's slug is the word the ledger and a report already
/// hold. The URL is redacted here even though [`gather`] refuses one that
/// carries a credential, because a request is built before it is refused.
impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Source::Git { url, reference } => {
                let mut source = serializer.serialize_struct("Source", 3)?;
                source.serialize_field("kind", &self.kind())?;
                source.serialize_field("url", &redacted(url))?;
                source.serialize_field("reference", reference)?;
                source.end()
            }
        }
    }
}

impl Source {
    /// Which kind of source this is, as the ledger files it.
    pub fn kind(&self) -> SourceKind {
        match self {
            Source::Git { .. } => SourceKind::Git,
        }
    }

    /// The address, as the user typed it: credentials included, which is why
    /// [`gather`] refuses one that carries any before it is stored or shown.
    pub fn url(&self) -> &str {
        match self {
            Source::Git { url, .. } => url,
        }
    }

    /// The branch or tag asked for, if any.
    pub fn reference(&self) -> Option<&str> {
        match self {
            Source::Git { reference, .. } => reference.as_deref(),
        }
    }
}

/// One gather: what to take, from where, and out of which folder, and who is
/// following it.
#[derive(Debug, Clone)]
pub struct Request {
    pub source: Source,
    /// The folder inside the source to search, empty for the whole of it.
    pub subdirectory: String,
    /// What to look for there.
    pub kind: Kind,
    /// Told how the gather is going, and able to stop it. A request carries
    /// it, rather than [`gather`] taking it beside one, so a front end that
    /// builds requests in one place and runs them in another hands over both
    /// together.
    pub watch: Watch,
    /// What fetches the source: [`Gix`] unless another [`Remote`] is handed
    /// over ([`through`](Self::through)), as a test hands a fake.
    pub remote: Arc<dyn Remote>,
    /// What to call the source, as somebody typed it: `owner-repo` when
    /// none is given ([`SourceName::derived`]), and a source gathered
    /// before called this from now on when one is.
    pub name: Option<String>,
    /// The commit to take, as somebody typed it, rather than whatever the
    /// branch is at: held to [`pin`] before anything is fetched, and kept
    /// beside the branch ([`ledger::Source::pin`]).
    pub pin: Option<String>,
    /// How the folder searched is looked through: every common layout, as
    /// for a repository, unless it is a plugin a marketplace lists (#103).
    pub search: Search,
}

/// How a gather looks through the folder it searches for skills.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Search {
    /// Every common layout, as far down as [`SEARCH_DEPTH`]: a repository's
    /// skills, wherever its tools keep them, as `gather git` takes them.
    #[default]
    Layouts,
    /// A plugin a marketplace's catalog lists (#103), the folder searched
    /// being its root: the skills its entry names, each a folder from
    /// there, when it names them, and otherwise its `skills/` folder and
    /// the folders its manifest names ([`plugin::Manifest`], #104), each as
    /// far down as [`SEARCH_DEPTH`], or a `SKILL.md` at its root, which
    /// makes it one skill. What else of it a plugin holds is no skill.
    Plugin {
        /// The skills its entry names, as the catalog writes each.
        skills: Option<Vec<String>>,
        /// Its licence, as the catalog gives it: a skill's own for one
        /// whose manifest, licence file and plugin's manifest say none
        /// ([`license_of`]).
        license: Option<String>,
        /// The dialect of the catalog listing it, which says the order its
        /// manifest is looked for in ([`plugin::Format::read_for`]).
        dialect: Dialect,
    },
}

impl Search {
    /// The licence a skill found this way has when it says none itself.
    fn license(&self) -> Option<&str> {
        match self {
            Search::Layouts => None,
            Search::Plugin { license, .. } => license.as_deref(),
        }
    }

    /// The formats the folder searched is looked in for a manifest of, in
    /// order: for a repository, the Agent Plugins format's alone, which
    /// makes it a package whose skills are its `skills/<child>` (#104), a
    /// Codex or a Claude Code plugin's manifest changing nothing of how a
    /// repository is searched (#101); for a plugin, each, as
    /// [`plugin::Format::read_for`] orders them.
    fn formats(&self) -> Vec<plugin::Format> {
        match self {
            Search::Layouts => vec![plugin::Format::AgentPlugins],
            Search::Plugin { dialect, .. } => plugin::Format::read_for(*dialect).to_vec(),
        }
    }
}

impl Request {
    /// Gather skills from a git repository, searched from its root.
    pub fn git(url: impl Into<String>) -> Self {
        Self {
            source: Source::Git {
                url: url.into(),
                reference: None,
            },
            subdirectory: DEFAULT_SUBDIRECTORY.to_owned(),
            kind: Kind::Skill,
            watch: Watch::new(),
            remote: Arc::new(Gix),
            name: None,
            pin: None,
            search: Search::default(),
        }
    }

    /// Gather what `typed` addresses, as somebody types a source on a
    /// command line ([`crate::url::address`]): a URL, `owner/repo`,
    /// `owner/repo@ref`, or a GitHub or GitLab tree URL, with the branch or
    /// tag `reference` and the folder `subdirectory` given beside it, when
    /// they are.
    ///
    /// What the address names and an option names again has to be the
    /// same, or it is refused (`gather_ref_contradicted`,
    /// `gather_path_contradicted`): which of two branches or folders was
    /// meant is not something to guess. Refused here, before anything is
    /// registered or fetched, in words that quote neither the address nor
    /// the option with a credential in it ([`redacted`]); a credential the
    /// address carries is refused by the gather, as one in any URL is. So
    /// is a tree URL whose branch or folder names none once its escapes are
    /// read (`gather_tree_undecodable`, [`crate::url::Undecodable`]).
    pub fn addressed(
        typed: &str,
        reference: Option<String>,
        subdirectory: Option<String>,
    ) -> Result<Self, GatherError> {
        let said = |what: &str| redacted(what).into_owned();
        let address =
            crate::url::address(typed).map_err(|undecodable| GatherError::TreeUndecodable {
                address: said(typed),
                segment: said(&undecodable.segment),
            })?;
        let reference = agreed(address.reference, reference, |named, given| {
            GatherError::RefContradicted {
                address: said(typed),
                named: said(&named),
                given: said(&given),
            }
        })?;
        let subdirectory = agreed(address.path, subdirectory, |named, given| {
            GatherError::PathContradicted {
                address: said(typed),
                named: said(&named),
                given: said(&given),
            }
        })?;
        let request = Request::git(address.url).at(reference);
        Ok(match subdirectory {
            Some(folder) => request.from_subdirectory(folder),
            None => request,
        })
    }

    /// Gather `source` again as the ledger records it was gathered last:
    /// from its URL, at its branch or tag, out of its folder, for the kind
    /// of artifact it gathers, at the commit it is pinned to when it is
    /// one, and under the name it has, which a request naming none leaves
    /// as it is.
    ///
    /// At its pin, rather than at whatever the branch is at now: a pin is
    /// what the shelf is said to hold for good, moved only by a gather told
    /// another (`gather git --pin`), never by one told to bring the source
    /// up to date. From its URL as it was recorded, so a relative path is
    /// read from where the gather runs, as one typed is.
    ///
    /// A marketplace's is the request its catalog is fetched with, a git
    /// repository as any other (#103): what is gathered of it is its
    /// children's, each a source of its own.
    pub fn again(source: &ledger::Source) -> Self {
        match source.kind {
            SourceKind::Git | SourceKind::Marketplace => Request::git(source.url.as_str())
                .from_subdirectory(source.subdirectory.clone())
                .at(source.reference.clone())
                .pinned(source.pin.as_ref().map(|pin| pin.as_str().to_owned()))
                .gathering(source.gathers),
        }
    }

    /// Search `subdirectory` alone, rather than the whole repository.
    pub fn from_subdirectory(mut self, subdirectory: impl Into<String>) -> Self {
        self.subdirectory = subdirectory.into();
        self
    }

    /// Clone that branch or tag rather than the remote's HEAD.
    pub fn at(mut self, reference: Option<impl Into<String>>) -> Self {
        let reference = reference.map(Into::into);
        match &mut self.source {
            Source::Git { reference: at, .. } => *at = reference,
        }
        self
    }

    /// Call the source `name`, when one is given, rather than what its URL
    /// names it.
    pub fn named(mut self, name: Option<impl Into<String>>) -> Self {
        self.name = name.map(Into::into);
        self
    }

    /// Take the commit `pin` names, when one is given, rather than whatever
    /// the branch is at.
    pub fn pinned(mut self, pin: Option<impl Into<String>>) -> Self {
        self.pin = pin.map(Into::into);
        self
    }

    /// Take artifacts of `kind` from the folder searched, rather than
    /// skills.
    pub fn gathering(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    /// Look through the folder searched as `search` says, rather than for
    /// every common layout.
    pub fn searching(mut self, search: Search) -> Self {
        self.search = search;
        self
    }

    /// Report to `watch`, and stop when it is cancelled.
    pub fn watched_by(mut self, watch: Watch) -> Self {
        self.watch = watch;
        self
    }

    /// Fetch through `remote` rather than [`Gix`]: the port a gather's one
    /// step outside core goes through, everything after it the same.
    pub fn through(mut self, remote: Arc<dyn Remote>) -> Self {
        self.remote = remote;
        self
    }
}

/// What an address `named` and an option `given` beside it come to: one of
/// them, when only one says anything or both say the same, and what
/// `contradicted` makes of the two otherwise.
fn agreed(
    named: Option<String>,
    given: Option<String>,
    contradicted: impl FnOnce(String, String) -> GatherError,
) -> Result<Option<String>, GatherError> {
    match (named, given) {
        (Some(named), Some(given)) if named != given => Err(contradicted(named, given)),
        (named, given) => Ok(given.or(named)),
    }
}

/// The commit `typed` names, when a gather can be pinned to it: its full
/// id, 40 hexadecimal digits in either case, kept in lowercase as git
/// spells one, so two spellings of one commit are one pin.
///
/// The full id rather than any revision git reads: a pin is what the shelf
/// is said to hold for good, which an abbreviation another commit may come
/// to share, or a branch, which moves, cannot say. A branch or a tag is
/// what `--ref` takes.
pub fn pin(typed: &str) -> Option<Revision> {
    let digits = typed.len() == 40 && typed.bytes().all(|byte| byte.is_ascii_hexdigit());
    digits.then(|| Revision::from(typed.to_ascii_lowercase()))
}

/// One artifact that reached the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Harvested {
    /// The name it declares, which is what it will be asked for by: a
    /// rule's route, which is its name.
    pub name: ArtifactName,
    /// The folder it came from inside the source, which is the folder it
    /// kept: for a rule, its file, by its route from the source's folder on
    /// the shelf.
    pub directory: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Where it now sits in the workspace.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
}

/// What one gather did.
///
/// Split by what happened rather than counted once, because "3 skills"
/// hides the only question worth asking after a second gather: what
/// changed.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Serialize)]
pub struct Report {
    /// The revision that was gathered from, when there was one.
    pub revision: Option<Revision>,
    /// The folder under `.mindweaver/<kind>/` this source owns.
    pub directory: String,
    pub added: Vec<Harvested>,
    pub updated: Vec<Harvested>,
    pub unchanged: Vec<Harvested>,
    /// Artifacts in the source that could not be taken. One broken skill must
    /// not cost the forty next to it.
    ///
    /// Not serialized, nor `warnings`: a front end says both as its
    /// warnings, which in JSON are the envelope's, as a scan's failures are.
    #[serde(skip)]
    pub failures: Vec<Failure>,
    /// What is wrong with artifacts that were taken anyway.
    #[serde(skip)]
    pub warnings: Vec<Warning>,
    /// What the source is called, which `--from` takes for what it
    /// supplied: added after the rest (#98).
    pub name: SourceName,
    /// What the source supplied before and holds no more at the commit
    /// taken, now off the shelf, each where it was: added after the rest
    /// (#100), and on the shelf no longer, so not in [`total`](Self::total).
    pub removed: Vec<Harvested>,
    /// What the package in the Agent Plugins format a repository's gather
    /// searched holds beside its skills, which is not taken (#104), said as
    /// a marketplace says what a plugin it lists holds
    /// ([`crate::marketplace::Child`]). Empty for a plugin a catalog lists,
    /// which its catalog's gather says by the plugin's entry.
    ///
    /// Not serialized, nor `mcp_files`: a front end says both in a note.
    #[serde(skip)]
    pub holds: Vec<Holding>,
    /// Which of [`crate::marketplace::MCP_FILES`] that package holds, by
    /// name, so a person knows which file to read; never imported.
    #[serde(skip)]
    pub mcp_files: Vec<&'static str>,
    /// The kind of artifact it gathered, as its source gathers it
    /// ([`ledger::Source::gathers`]): added after the rest (#111).
    pub kind: Kind,
}

impl Report {
    /// Everything that is now on the shelf because of this gather.
    pub fn total(&self) -> usize {
        self.added.len() + self.updated.len() + self.unchanged.len()
    }

    /// Whether the source yielded nothing at all.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Gather what `request` asks for into `workspace`, recording it in `ledger`.
///
/// The ledger is passed in rather than opened here so a caller can hold one
/// open across several gathers, and so tests can use one that never touches a
/// disk.
pub fn gather(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    // Held for the whole gather, the clone included, until its line is in
    // the log: two gathers of one source at once would clone into one folder
    // and each replace the shelf copies the other was making. One that cannot
    // have it is not logged, because no gather was attempted.
    let _lock = workspace.lock()?;
    let outcome = harvest(workspace, ledger, request);
    // What was fetched, `owner/repo` read as the GitHub repository it names.
    let url = fetched(request.source.url());
    logged(ledger, &url, &outcome)?;
    outcome
}

/// Note in `ledger`'s log what gathering from `url` came to, `outcome`.
///
/// The log records what was attempted, not only what worked: a gather that
/// failed is the entry somebody opens the log to find. A log write that
/// fails on the error path is swallowed, because the error it would replace
/// is the more useful of the two.
pub(crate) fn logged(
    ledger: &Ledger,
    url: &str,
    outcome: &Result<Report, GatherError>,
) -> Result<(), GatherError> {
    match outcome {
        Ok(report) => {
            // What was removed only when something was, so the line every
            // gather before removals wrote reads as it did.
            let removed = match report.removed.len() {
                0 => String::new(),
                count => format!("{count} removed, "),
            };
            let detail = format!(
                "{} added, {} updated, {} unchanged, {removed}{} failed",
                report.added.len(),
                report.updated.len(),
                report.unchanged.len(),
                report.failures.len()
            );
            ledger.log(Action::Gather, Some(url), Outcome::Ok, Some(&detail))?;
        }
        // Not logged: a cancel leaves the ledger as it found it, and the
        // person who cancelled knows already.
        Err(GatherError::Cancelled) => {}
        Err(error) => {
            let _ = ledger.log(
                Action::Gather,
                Some(url),
                Outcome::Failed,
                Some(&error.to_string()),
            );
        }
    }
    Ok(())
}

/// What a gather of a request would take, as a dry run of it found it
/// ([`preview`]): nothing of it is on the shelf.
///
/// What it serializes to is a contract, as a report's is: each field by its
/// name.
#[derive(Debug, Serialize)]
pub struct Preview {
    /// The revision the source is at, which a gather would take.
    pub revision: Option<Revision>,
    /// Each skill a gather would take, in the order of where it is in the
    /// repository.
    pub found: Vec<Previewed>,
    /// What keeps the rest from being taken, as a gather would say it, but
    /// for each path in the clone, which is said by where it is in the
    /// repository: the clone the dry run read is taken away again. A copy
    /// that fails is said by where its skill is in the repository too,
    /// where a gather says the shelf's folder it was not put in, and its
    /// reason as the copy gives it.
    ///
    /// Not serialized, nor `warnings`, as a report's are not: a front end
    /// says both as its warnings.
    #[serde(skip)]
    pub failures: Vec<Failure>,
    /// What is wrong with what would be taken anyway, each by where its
    /// manifest, or a rule's file, is in the repository, as there is no
    /// copy on the shelf yet.
    #[serde(skip)]
    pub warnings: Vec<Warning>,
    /// The kind of artifact it would take: added after the rest (#111).
    pub kind: Kind,
}

/// One skill, or one rule, a gather would take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Previewed {
    /// The name it declares, which it would be asked for by.
    pub name: ArtifactName,
    /// The folder the shelf would file it under, or a rule's file, by its
    /// route.
    pub directory: String,
    /// Where it is in the repository, from its root, as the ledger would
    /// record it: `.` for a skill that is the whole of it, and a rule's
    /// file.
    pub upstream_path: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
}

/// What the folder a dry run clones into, and copies what it would take
/// into, is named by, before what makes it its own
/// ([`copy::unique_name`]): a `+`, which no folder a source is given holds,
/// so it is never one.
const DRY_RUN: &str = "+dry-run-";

/// The file in a dry run's folder its run holds a lock on for as long as it
/// lives ([`DryRun`]).
const DRY_RUN_LOCK: &str = ".lock";

/// A dry run's own folder in the cache ([`DRY_RUN`]), taken away when it is
/// dropped, whatever came of the run.
///
/// Held, for as long as it lives, by a lock on a file in it
/// ([`DRY_RUN_LOCK`]), which a run a Ctrl-C ends lets go of with its
/// process: the binaries install no signal handler (see the open
/// questions), so such a run takes nothing away, and the next dry run or
/// gather takes away each folder whose lock nobody holds
/// ([`sweep_dry_runs`]). A lock rather than the process id in the name, for
/// the reason a [`copy::Stage`] is held by one: whether a process is still
/// running is a question each platform answers differently, and a lock let
/// go of when its process dies answers it on all of them.
pub(crate) struct DryRun {
    pub(crate) folder: PathBuf,
    // Only ever dropped, before the folder is taken away.
    lock: Option<fs::File>,
}

impl DryRun {
    /// Take a folder of one's own in `cache`, making `cache` first, and
    /// taking away first what interrupted runs left there.
    pub(crate) fn open(cache: &Path) -> io::Result<DryRun> {
        fs::create_dir_all(cache)?;
        sweep_dry_runs(cache);
        let folder = cache.join(format!("{DRY_RUN}{}", copy::unique_name()));
        fs::create_dir(&folder)?;
        // Locked under another name, and given the one a sweep looks for
        // only once held, so no sweep finds this run's lock before it is
        // held and takes the folder from under it.
        let locking = folder.join(format!("{DRY_RUN_LOCK}.new"));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&locking)?;
        // None on a filesystem without locks, where no sweep can tell a run
        // going on from a leftover either, so none takes one away.
        let lock = file.try_lock().is_ok().then_some(file);
        fs::rename(&locking, folder.join(DRY_RUN_LOCK))?;
        Ok(DryRun { folder, lock })
    }
}

impl Drop for DryRun {
    fn drop(&mut self) {
        // Let go of first, as a file held open is not removed on every
        // platform: a sweep that takes the folder in between takes what
        // this would.
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.folder);
    }
}

/// Take away each dry run's folder in `cache` whose lock nobody holds,
/// which an interrupted run left ([`DryRun`]): what a dry run and a gather
/// do before they fetch.
///
/// Said nowhere, since nothing in it was anybody's but the run's, and one
/// that will not go is tried again by the next. A folder with no lock in
/// it yet is a run's that is being made, and is left.
pub(crate) fn sweep_dry_runs(cache: &Path) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let named = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(DRY_RUN));
        if !named || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let folder = entry.path();
        let Ok(lock) = fs::OpenOptions::new()
            .write(true)
            .open(folder.join(DRY_RUN_LOCK))
        else {
            continue;
        };
        if lock.try_lock().is_ok() {
            drop(lock);
            let _ = fs::remove_dir_all(&folder);
        }
    }
}

/// What a gather of `request` would take, a dry run of it: the source
/// fetched and searched as a gather fetches and searches it, refused for
/// what a gather refuses before it fetches, and nothing else done.
///
/// Nothing registered, nothing put on the shelf, nothing recorded, nothing
/// logged, and the workspace's lock not taken, since none of what writers
/// take turns at is written: `ledger` is only read, for a `--name` another
/// source has, so one read without being made
/// ([`WeaverWorkspace::ledger_for_reading`]) is the one to hand it. The
/// clone it needs, and the copies of what it would take ([`look`]), are
/// made in a folder of its own under the workspace's cache ([`DryRun`]) and
/// taken away once read, whatever came of reading them, with the cache and
/// the workspace's folder when it was the dry run that made them. So it
/// leaves no clone: not in the folder a source's clone is kept in, which
/// would then hold another commit than the one its shelf came from, nor in
/// a folder of its own, which nothing would take away. A Ctrl-C ends the
/// process without that, and the next dry run or gather takes the folder
/// away.
pub fn preview(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Preview, GatherError> {
    let (Checked { pinned, .. }, _) = checked(ledger, request)?;
    let url = fetched(request.source.url());
    let made: Vec<PathBuf> = [workspace.weaver_dir(), workspace.cache_dir()]
        .into_iter()
        .filter(|folder| !folder.exists())
        .collect();
    let cache = workspace.cache_dir();
    let outcome = DryRun::open(&cache)
        .map_err(|source| GitError::Cache {
            path: cache.clone(),
            source,
        })
        .map_err(GatherError::from)
        .and_then(|run| look(request, &url, pinned.as_ref(), &run.folder));
    // Only while each is empty, so nothing another command put there since
    // goes with it.
    for folder in made.iter().rev() {
        let _ = fs::remove_dir(folder);
    }
    outcome
}

/// Fetch `request`'s source into a folder in `into` and say what a gather
/// would take of it: the dry run, once it has a folder of its own.
///
/// Each skill it would take is copied as a gather copies it onto the
/// shelf, into another folder in `into`, beside the clone, so what a gather
/// would refuse to copy, a link leading out of the repository, to nothing
/// or back into its own folder, is refused here too, and said as the
/// gather says it (`gathered_copy_failed`). Holding each to the bounds the
/// copy holds it to without copying was the other way, and is rejected: a
/// second walk of a skill's tree beside the copy's, which the two could
/// come to disagree about.
fn look(
    request: &Request,
    url: &str,
    pin: Option<&Revision>,
    into: &Path,
) -> Result<Preview, GatherError> {
    let watch = &request.watch;
    let clone = fetch(request, url, pin, &into.join("clone"))?;
    let mut failures = Vec::new();
    let Offered { taken, .. } = offered_as_kept(&clone.root, request, url, &mut failures)?;
    if watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    let mut preview = Preview {
        revision: clone.revision,
        found: Vec::with_capacity(taken.len()),
        failures: Vec::new(),
        warnings: Vec::new(),
        kind: request.kind,
    };
    let shelf = into.join("shelf");
    for (directory, folder, artifact) in &taken {
        if let Err(source) = shelve(request, &clone.root, directory, &shelf, folder) {
            failures.push(Failure::Copy {
                path: directory.clone(),
                source,
            });
            continue;
        }
        let upstream_path = upstream(directory, &clone.root, folder);
        let route = match upstream_path.as_str() {
            "." => Path::new(""),
            route => Path::new(route),
        };
        let at = read_from(request.kind, route);
        preview.warnings.extend(
            filed_issues(artifact, folder)
                .into_iter()
                .map(|issue| Warning {
                    path: at.clone(),
                    issue,
                }),
        );
        preview.found.push(Previewed {
            name: artifact.name().to_owned(),
            directory: kept_as(request.kind, folder),
            upstream_path,
            summary: artifact.summary().map(str::to_owned),
        });
    }
    preview.failures = failures
        .into_iter()
        .map(|failure| failure.in_repository(&clone.root))
        .collect();
    preview.failures.sort_by(|a, b| a.path().cmp(b.path()));
    preview.warnings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(preview)
}

/// What the scrub [`WeaverWorkspace::ledger_to_write`] runs took out.
#[derive(Debug, Default)]
pub struct Scrub {
    /// The sources whose URL carried credentials, as they are stored now.
    /// Gathering one again needs the credential from somewhere else.
    pub sources: Vec<ledger::Source>,
    /// How many action log entries named a URL with credentials.
    pub actions: usize,
    /// Clones that still hold them, because they could not be rewritten.
    /// Their source keeps its URL until they can, so the next scrub tries
    /// again rather than forgetting the clone needs it.
    pub failures: Vec<ScrubFailure>,
    /// Folders of a source that became one with another, which nothing
    /// points into any more and which could not be removed.
    pub leftovers: Vec<Leftover>,
    /// The sources the step to schema 7 made one with another ([`Merged`]):
    /// said by the first command that writes, whichever command took the
    /// step, as what the scrub takes out is, and again by each after it
    /// while a folder of one will not go ([`Self::leftovers`]).
    pub merged: Vec<Merged>,
}

impl Scrub {
    /// Whether there was nothing to take out, which is every time but the
    /// first after an older build stored a credential.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.actions == 0
            && self.failures.is_empty()
            && self.leftovers.is_empty()
            && self.merged.is_empty()
    }

    /// Add what a later scrub took out, for a change that opened the ledger
    /// to write more than once ([`crate::lifecycle::rename`]).
    ///
    /// A clone that could not be rewritten keeps its source's URL, so every
    /// scrub tries it again and fails again: one thing to tell, as two
    /// branches of one URL are one credential.
    pub(crate) fn absorb(&mut self, later: Scrub) {
        for source in later.sources {
            if !self.sources.iter().any(|seen| seen.url == source.url) {
                self.sources.push(source);
            }
        }
        self.actions += later.actions;
        for failure in later.failures {
            if !self.failures.iter().any(|seen| seen.url == failure.url) {
                self.failures.push(failure);
            }
        }
        self.leftovers.extend(later.leftovers);
        self.merged.extend(later.merged);
    }
}

/// Take out the credentials a Mindweaver from before they were refused
/// stored: in the URLs of sources, in the action log, and in the clones it
/// made from those URLs.
///
/// Run by [`WeaverWorkspace::ledger_to_write`], which is how a command that
/// writes opens the ledger, so it comes before anything is written. Every
/// time rather than once, as a schema migration would, because an older build
/// can keep writing to the same file after this one has scrubbed it, and the
/// schema steps are spoken for by the changes that need a new version. With
/// nothing to do it still scans the sources and the action log, and the first
/// time it finds something is the only time it says so. A read is left alone:
/// what it shows is redacted on the way out anyway, and reading should not
/// write.
///
/// First it removes the clone and the shelf folders of each source the step
/// to schema 7 made one with another
/// ([`LedgerStore::merged`](crate::port::LedgerStore::merged)), and hands
/// back each merge to be said. Removed rather than left, because the shelf
/// is read from its folders ([`crate::Catalog::discover_gathered`]): one
/// left behind would be listed as skills no source owns, and its clone
/// keeps whatever credential it was made with. Here, rather than by
/// whichever command took the step, because that is a read as often as
/// not, which says nothing and removes nothing: the ledger keeps each merge
/// until a command that writes has removed its folders, so a folder that
/// will not go is tried again, and said again with its merge, each time.
///
/// The workspace is held ([`WeaverWorkspace::lock_waiting`], up to `wait`)
/// while clones are rewritten or removed, since a gather beside it would be
/// cloning into those same folders. Only then: with no clone to rewrite or
/// remove, which is every time but the first after an older build stored a
/// credential or an upgrade made two sources one, the scrub writes nothing
/// but rows, which SQLite keeps apart, and a change to an artifact opening
/// the ledger to drop a record does not wait behind a gather to do it. The
/// sources are looked for again once it is held, since a scrub kept waiting
/// may find that another has done its work.
///
/// Each clone is scrubbed through `remote`, the adapter that knows which of
/// its files name the URL it was made from.
pub(crate) fn scrub(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    wait: Duration,
    remote: &dyn Remote,
) -> Result<Scrub, LedgerError> {
    let mut scrub = Scrub::default();
    let _lock = if ledger.tainted_sources()?.is_empty() && ledger.merged()?.is_empty() {
        None
    } else {
        Some(workspace.lock_waiting(wait)?)
    };
    for merged in ledger.merged()? {
        let left = scrub.leftovers.len();
        remove_folders_of(workspace, &merged.directory, &mut scrub.leftovers);
        if scrub.leftovers.len() == left {
            ledger.cleared_merge(&merged.directory)?;
        }
        scrub.merged.push(merged);
    }
    for source in ledger.tainted_sources()? {
        // The clone first and the row after it: a row rewritten first would
        // no longer say that its clone still needs the same.
        if let Err(error) = remote.scrub(&workspace.cache_dir().join(&source.directory)) {
            scrub.failures.push(ScrubFailure {
                url: source.url.clone(),
                source: error,
            });
            continue;
        }
        ledger.scrub_source(source.id)?;
        // Two branches or folders of one URL are one credential to tell
        // somebody about.
        if !scrub.sources.iter().any(|seen| seen.url == source.url) {
            scrub.sources.push(source);
        }
    }
    scrub.actions = ledger.scrub_log()?;
    Ok(scrub)
}

/// Remove the clone and the shelf folders of a source called `directory`
/// that the ledger no longer has, noting each that will not go.
///
/// A directory that is not one folder name was not named by this ledger, and
/// is left alone rather than followed out of the workspace.
fn remove_folders_of(workspace: &WeaverWorkspace, directory: &str, leftovers: &mut Vec<Leftover>) {
    if !crate::install::is_folder_name(directory) {
        return;
    }
    let shelves = Kind::ALL
        .iter()
        .map(|kind| workspace.gathered_dir(*kind).join(directory));
    for path in std::iter::once(workspace.cache_dir().join(directory)).chain(shelves) {
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => leftovers.push(Leftover { path, source }),
        }
    }
}

/// A folder of a source that became one with another, which could not be
/// removed.
#[derive(Debug, Error)]
#[error(
    "{path}: left by a source that is now one with another at the same URL, and cannot be \
     removed: {source}"
)]
pub struct Leftover {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

codes!(Leftover => "source_leftover");

/// A clone made from a URL with credentials that could not be rewritten
/// without them.
#[derive(Debug, Error)]
#[error("{url}: its clone still holds the credentials it was gathered with: {source}")]
pub struct ScrubFailure {
    /// The source's URL, without them.
    pub url: Redacted,
    #[source]
    pub source: GitError,
}

codes!(ScrubFailure => "clone_unscrubbed");

/// What a request is refused for before anything is registered or
/// fetched, and what it is when it is not: the name it gives its source,
/// and the commit it is pinned to.
pub(crate) struct Checked {
    pub(crate) name: Option<SourceName>,
    pub(crate) pinned: Option<Revision>,
}

/// Hold `request` to what a gather refuses before it registers or fetches
/// anything, a dry run as much as a gather ([`preview`]), and look up the
/// source it gathers into, if a gather has registered it, which `--name`
/// must not take another's name from.
fn checked(
    ledger: &Ledger,
    request: &Request,
) -> Result<(Checked, Option<ledger::Source>), GatherError> {
    checked_as(ledger, request, request.source.kind())
}

/// [`checked`], for a source of the `kind` given rather than its request's
/// own: a marketplace's catalog is fetched with a git request, and is a
/// source of a kind of its own (#103).
pub(crate) fn checked_as(
    ledger: &Ledger,
    request: &Request,
    kind: SourceKind,
) -> Result<(Checked, Option<ledger::Source>), GatherError> {
    let checked = refused(request)?;

    // Found by the URL every spelling of it is ([`crate::url::canonical`]),
    // so a second spelling gathers into the source the first registered,
    // and registered as fetched.
    let url = fetched(request.source.url());
    let found = ledger.source(
        kind,
        &url,
        request.source.reference(),
        &request.subdirectory,
        request.kind,
    )?;
    // Refused before anything is registered: one name is one source, and
    // which of two a `--from` named is not something to guess.
    if let Some(name) = &checked.name {
        if let Some(holder) = ledger.source_named(name)? {
            if found.as_ref().is_none_or(|found| found.id != holder.id) {
                return Err(GatherError::NameTaken {
                    name: name.clone(),
                    origin: holder.origin(),
                });
            }
        }
    }
    Ok((checked, found))
}

/// What `request` is refused for before anything is registered or fetched
/// whichever source it gathers into, or what it is when it is not: what a
/// marketplace's plugin is held to, which is found by its entry rather than
/// its URL (#103).
pub(crate) fn refused(request: &Request) -> Result<Checked, GatherError> {
    // Refused before anything is registered or fetched, rather than used for
    // this one fetch and dropped from what is stored: a token on the command
    // line is already where `ps` and the shell history show it, and every
    // report names the URL. A git credential helper, or the front end once
    // #110 lands, keeps it off the command line altogether.
    //
    // So is a URL typed where the branch, the folder or the name goes,
    // which the ledger keeps as that branch or folder, word for word, and an
    // error repeats: it is somebody's credential all the same.
    let typed = [
        Some(request.source.url()),
        request.source.reference(),
        Some(request.subdirectory.as_str()),
        request.name.as_deref(),
        request.pin.as_deref(),
    ];
    if let Some(carrying) = typed
        .into_iter()
        .flatten()
        .find(|text| carries_credentials(text))
    {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(carrying).into_owned(),
        });
    }
    // Before anything is fetched, and so before a search that would start
    // outside the clone, in the workspace's cache and shelf or above them,
    // and take what it found there for the repository's.
    if outside(&request.subdirectory) {
        return Err(GatherError::PathOutside {
            subdirectory: request.subdirectory.clone(),
        });
    }
    let name = match &request.name {
        Some(typed) => Some(
            SourceName::parse(typed).ok_or_else(|| GatherError::NameInvalid {
                name: typed.clone(),
            })?,
        ),
        None => None,
    };
    let pinned = match &request.pin {
        Some(typed) => {
            Some(pin(typed).ok_or_else(|| GatherError::PinInvalid { pin: typed.clone() })?)
        }
        None => None,
    };

    // Skills and rules are gatherable (#111), an MCP server not: it is
    // imported from a registry (`gather mcp`), never harvested out of a
    // repository. Matching on the kind rather than on its layout keeps the
    // compiler asking: a fourth kind has to answer this question before it
    // can be added.
    match request.kind {
        Kind::Skill | Kind::Rule => {}
        Kind::Mcp => return Err(GatherError::NotGatherable { kind: request.kind }),
    }

    // Before the source is registered, so a gather cancelled before it began
    // has nothing to undo.
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    Ok(Checked { name, pinned })
}

/// What the repository cloned at `clone`, fetched from `url`, offers of
/// the kind `request` gathers, as that kind is kept: a folder holding a
/// manifest for a skill ([`offered`]), a file of its extension for a rule
/// ([`offered_files`]). Refused for a kind no gather takes ([`refused`]).
fn offered_as_kept(
    clone: &Path,
    request: &Request,
    url: &str,
    failures: &mut Vec<Failure>,
) -> Result<Offered, GatherError> {
    match request.kind.layout() {
        Layout::Directory { manifest } => offered(clone, request, url, manifest, failures),
        Layout::Files { extension } => offered_files(clone, request, url, extension, failures),
        Layout::Flat { .. } => Err(GatherError::NotGatherable { kind: request.kind }),
    }
}

/// Put the artifact of `request`'s kind found at `at` in the repository
/// cloned at `clone` on `shelf`, a source's folder there, under `filed`,
/// as an install files it ([`crate::install::copy_at`]): a skill's folder
/// replaced whole, leaving out what no copy holds
/// ([`left_off_the_shelf`]), its links followed only within the clone; a
/// rule's file replaced with what it holds, read through a link the search
/// held inside the clone already ([`files`]).
fn shelve(
    request: &Request,
    clone: &Path,
    at: &Path,
    shelf: &Path,
    filed: &str,
) -> io::Result<Change> {
    let destination = crate::install::copy_at(shelf, request.kind, filed);
    match request.kind.layout() {
        Layout::Directory { .. } => {
            let leave_out = left_off_the_shelf(request, clone, at);
            copy::replace_leaving(at, &destination, clone, &leave_out)
        }
        Layout::Files { .. } | Layout::Flat { .. } => copy::replace_file(at, &destination),
    }
}

/// The file an artifact of `kind` at `at` is read from: a skill's manifest
/// in its folder, and a rule's file itself, which a warning names.
fn read_from(kind: Kind, at: &Path) -> PathBuf {
    match kind.layout() {
        Layout::Directory { manifest } => at.join(manifest),
        Layout::Files { .. } | Layout::Flat { .. } => at.to_path_buf(),
    }
}

/// What the ledger records of the copy of an artifact of `kind` on the
/// shelf at `at`: [`hash::tree`] of a skill's folder, and [`hash::bytes`]
/// of a rule's file, the recipe an install of each keeps of a copy, so a
/// copy installed from it hashes alike.
fn hashed(kind: Kind, at: &Path) -> io::Result<crate::value::ContentHash> {
    match kind.layout() {
        Layout::Directory { .. } => hash::tree(at),
        Layout::Files { .. } | Layout::Flat { .. } => fs::read(at).map(|bytes| hash::bytes(&bytes)),
    }
}

/// Whether `subdirectory`, a folder of a repository as somebody typed it
/// (`--path`, or a tree URL's), names one outside it: absolute, or
/// climbing out of the repository's root, lexically, as every route is
/// read ([`paths::normalize`]), as an installed rule's route is refused
/// when it climbs ([`crate::install::is_installable_name`]).
pub(crate) fn outside(subdirectory: &str) -> bool {
    let path = Path::new(subdirectory);
    path.components()
        .any(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
        || paths::normalize(path).components().next() == Some(Component::ParentDir)
}

/// The work itself, with the logging left to [`gather`].
fn harvest(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    let (Checked { name, pinned }, found) = checked(ledger, request)?;
    let url = fetched(request.source.url());
    let new = found.is_none();
    let mut source = ledger.source_for(
        request.source.kind(),
        &url,
        request.source.reference(),
        &request.subdirectory,
        request.kind,
    )?;
    let was = source.name.clone();
    if let Some(name) = name.filter(|name| *name != source.name) {
        ledger.rename_source(source.id, &name)?;
        source.name = name;
    }

    let outcome = take(workspace, ledger, request, &url, &source, pinned.as_ref());
    if !new && source.name != was && matches!(outcome, Err(GatherError::Cancelled)) {
        // Called what it was again, as a cancel leaves the ledger.
        ledger.rename_source(source.id, &was)?;
    }
    if new && matches!(outcome, Err(GatherError::Cancelled)) {
        // Undone rather than never done: the clone needs the folder name that
        // registering gives it before anything is fetched. Holding the
        // registration in a transaction until the gather ends was the other
        // way, and is rejected: every other write to the ledger would wait
        // for as long as the network takes.
        ledger.forget_source(source.id)?;
        // Nothing points at its clone any more. One that will not go is left
        // for the next source given its folder name, whose clone replaces
        // whatever is there, as every clone does.
        let _ = fs::remove_dir_all(workspace.cache_dir().join(&source.directory));
    }
    outcome
}

/// Fetch `source` from `url`, as it was typed this time, read what it
/// offers, and put that on the shelf: the gather, once its source is
/// registered.
///
/// From the URL typed rather than the one the source was first registered
/// with, though both are one repository's: the second spelling is the one
/// somebody asked for now, an ssh one where https needs a credential the
/// helpers lack, or the reverse. At `pin` when there is one, which the
/// source keeps beside its branch from then on, as it keeps none once a
/// gather takes the branch as it is.
fn take(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    url: &str,
    source: &ledger::Source,
    pin: Option<&Revision>,
) -> Result<Report, GatherError> {
    sweep_dry_runs(&workspace.cache_dir());
    let cache = workspace.cache_dir().join(own_folder(source)?);
    let clone = fetch(request, url, pin, &cache)?;
    place(workspace, ledger, request, url, source, pin, &clone)
}

/// The folder name the ledger records for `source`, which its clone is a
/// folder of the cache by and its shelf a folder of the shelf's: refused
/// unless it is one folder name, since a fetch replaces whatever its clone's
/// folder holds, and `..` would be the workspace's own. No ledger names one
/// so (`LedgerStore::source_for`, `child_for`), and a row edited by hand is
/// refused here, before either is joined, as `gather remove` refuses it
/// ([`crate::sources`]).
///
/// Refused too when it is the folder the HTTP cache keeps
/// ([`HTTP_CACHE_DIR`](crate::workspace::HTTP_CACHE_DIR)), which no ledger
/// gives a source either, but one a build from before it gave a source whose
/// URL reads as `http`: a fetch there would replace the cache's answers with
/// a clone. `gather remove` takes such a source and its folder, the cache's
/// answers with it, which are only kept answers, and a gather of its URL then
/// gives it a folder of its own.
pub(crate) fn own_folder(source: &ledger::Source) -> Result<&str, GatherError> {
    if crate::install::is_folder_name(&source.directory)
        && source.directory != crate::workspace::HTTP_CACHE_DIR
    {
        Ok(&source.directory)
    } else {
        Err(GatherError::FolderNotItsOwn {
            name: source.name.clone(),
            directory: source.directory.clone(),
        })
    }
}

/// Fetch `request`'s source from `url` into `into`, at `pin` when there is
/// one, telling its watch: a cancel said as the gather's.
pub(crate) fn fetch(
    request: &Request,
    url: &str,
    pin: Option<&Revision>,
    into: &Path,
) -> Result<Cloned, GatherError> {
    request.watch.report(Progress::Connecting);
    match &request.source {
        Source::Git { reference, .. } => request
            .remote
            .fetch(url, reference.as_deref(), pin, into, &request.watch)
            .map_err(|error| match error {
                GitError::Cancelled => GatherError::Cancelled,
                error => error.into(),
            }),
    }
}

/// Read what the repository at `clone`, fetched from `url`, offers as
/// `request` searches it, and put that on the shelf of `source`, which is
/// registered: what a gather does once it has a clone, whichever fetched
/// it.
pub(crate) fn place(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    url: &str,
    source: &ledger::Source,
    pin: Option<&Revision>,
    clone: &Cloned,
) -> Result<Report, GatherError> {
    let watch = &request.watch;
    let shelf = workspace
        .gathered_dir(request.kind)
        .join(own_folder(source)?);
    let searched = clone.root.join(&request.subdirectory);
    let mut report = Report {
        revision: clone.revision.clone(),
        directory: source.directory.clone(),
        added: Vec::new(),
        updated: Vec::new(),
        unchanged: Vec::new(),
        failures: Vec::new(),
        warnings: Vec::new(),
        name: source.name.clone(),
        removed: Vec::new(),
        holds: Vec::new(),
        mcp_files: Vec::new(),
        kind: request.kind,
    };

    // Everything is read before anything is placed, rather than each
    // artifact read and placed in turn, so that the shelf only starts to
    // change once all of it is known: what the source supplied before
    // among it, to tell what its commit no longer holds.
    let supplied: Vec<Gathered> = ledger
        .gathered()?
        .into_iter()
        .filter(|gathered| gathered.source.id == source.id && gathered.kind == request.kind)
        .collect();
    let Offered {
        held,
        taken,
        licenses: plugins,
        holds,
        mcp_files,
    } = offered_as_kept(&clone.root, request, url, &mut report.failures)?;
    report.holds = holds;
    report.mcp_files = mcp_files;

    // The last moment a cancel is honoured, and the reason everything is read
    // first: past here the shelf and the ledger change, and a gather stopped
    // halfway through that would leave them neither as they were nor as the
    // source has them. `Progress::cancellable` says so of every step after.
    if watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    // Noted only now, and not by a gather that fails before it gets here,
    // since the revision a source names is the one its shelf holds.
    ledger.saw_source(source.id, clone.revision.as_deref(), pin)?;

    watch.report(Progress::Placing {
        done: 0,
        total: taken.len(),
    });
    for (done, (directory, folder, artifact)) in taken.iter().enumerate() {
        let destination = crate::install::copy_at(&shelf, request.kind, folder);
        let placed = shelve(request, &clone.root, directory, &shelf, folder);
        watch.report(Progress::Placing {
            done: done + 1,
            total: taken.len(),
        });
        let change = match placed {
            Ok(change) => change,
            Err(source) => {
                report.failures.push(Failure::Copy {
                    path: destination,
                    source,
                });
                continue;
            }
        };

        // Lenient, as the Agent Skills client guide advises for skills
        // written for other agents: what is wrong with one is said, and it is
        // taken anyway. Installing it is where it is held to the rules.
        report.warnings.extend(
            filed_issues(artifact, folder)
                .into_iter()
                .map(|issue| Warning {
                    path: read_from(request.kind, &destination),
                    issue,
                }),
        );

        // Of the copy on the shelf rather than the folder in the clone: it
        // is what an install copies from, and what a link in the clone led
        // to is in it, where the hash would refuse the link itself. One that
        // cannot be read back is recorded with no hash, as an install
        // records a copy it cannot read back, rather than not at all.
        let content_hash = match hashed(request.kind, &destination) {
            Ok(hash) => Some(hash),
            Err(source) => {
                report.failures.push(Failure::Unhashed {
                    path: destination.clone(),
                    source,
                });
                None
            }
        };
        let stated = artifact
            .manifest()
            .and_then(|manifest| manifest.license.as_deref());
        // A plugin's licence, as its own manifest gives it and then as its
        // catalog does, for a skill that says none of its own, after its
        // manifest and its licence file: what the skill says of itself is
        // nearer to it than what its plugin says, and what a plugin says of
        // itself nearer than what a catalog says of the plugin it lists.
        let license = match request.kind.layout() {
            Layout::Directory { .. } => license_of(stated, &destination)
                .or_else(|| plugins.of(directory))
                .or_else(|| request.search.license().map(str::to_owned)),
            Layout::Files { .. } | Layout::Flat { .. } => license_above(directory, &searched),
        };
        let recorded = relative(workspace, &destination);
        ledger.record(
            source,
            request.kind,
            artifact.name(),
            &recorded,
            artifact.summary(),
            &Upstream {
                revision: clone.revision.clone(),
                path: upstream(directory, &clone.root, folder),
                content_hash,
                license,
            },
        )?;

        let harvested = Harvested {
            name: artifact.name().to_owned(),
            directory: kept_as(request.kind, folder),
            summary: artifact.summary().map(str::to_owned),
            path: destination,
        };
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }

    take_off(
        workspace,
        ledger,
        source,
        &shelf,
        &held,
        supplied,
        &mut report,
    )?;

    report.failures.sort_by(|a, b| a.path().cmp(b.path()));
    // Stable, so one file's warnings stay in the order `validate` gives them.
    report.warnings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

/// What no copy on the shelf holds at the top of the skill at `directory`,
/// found as `request` searches the repository cloned at `clone`: what no
/// install copies, so the shelf holds what a project is given and a
/// plugin's hooks and servers are never offered, and git's own folder,
/// which only a skill that is the whole repository has at its top: its
/// clone's.
///
/// And for a skill that is the whole of the plugin a marketplace lists
/// ([`Search::Plugin`]), what that plugin holds beside its skills where it
/// keeps each ([`crate::marketplace::Holding`]): said of it and never taken,
/// as from a plugin whose skills are folders of it.
fn left_off_the_shelf(request: &Request, clone: &Path, directory: &Path) -> Vec<&'static str> {
    let mut leave_out = crate::install::left_out_names().to_vec();
    leave_out.push(".git");
    let plugin = matches!(request.search, Search::Plugin { .. });
    if plugin && paths::normalize(directory) == paths::normalize(&clone.join(&request.subdirectory))
    {
        leave_out.extend(crate::marketplace::Holding::kept_in());
    }
    leave_out
}

/// What the repository cloned at `clone`, fetched from `url`, offers of
/// the kind `request` gathers, in the folder it searches: every folder found
/// holding a `manifest`, by the folder it is filed under, whether or not it
/// could be taken (`held`), and each skill that was read and shares a name
/// with no other (`taken`), what keeps the rest from being taken among
/// `failures`. What a gather and a dry run of one ([`preview`]) both find,
/// by the one search.
struct Offered {
    held: Vec<String>,
    taken: Vec<(PathBuf, String, Artifact)>,
    /// What the plugins its skills are in say of their licence (#104).
    licenses: Licenses,
    /// What the package a repository's gather searched holds beside its
    /// skills, and in which of the MCP files ([`Report::holds`]).
    holds: Vec<Holding>,
    mcp_files: Vec<&'static str>,
}

/// The licences the manifests of the plugins a gather's skills are in give
/// (#104): a skill's own for one that says none.
struct Licenses {
    /// The plugin's or the package's searched, when its manifest gives one.
    searched: Option<String>,
    /// Each package in the Agent Plugins format the search came to below
    /// the folder searched, by its folder, with its licence as its
    /// `plugin.json` gives it.
    packages: Vec<(PathBuf, Option<String>)>,
}

impl Licenses {
    /// What the plugins in `searched`, in the repository whose clone
    /// resolves to `bound`, say of the licence of the skills `found`,
    /// `plugin` the manifest of `searched` itself.
    fn of_plugins(
        found: &[PathBuf],
        searched: &Path,
        bound: &Path,
        plugin: Option<Manifest>,
    ) -> Licenses {
        let mut packages: Vec<(PathBuf, Option<String>)> = Vec::new();
        for package in found.iter().filter_map(|directory| package_of(directory)) {
            if package == searched || packages.iter().any(|(seen, _)| seen == package) {
                continue;
            }
            if let Ok(Some(manifest)) =
                Manifest::read(package, bound, &[plugin::Format::AgentPlugins])
            {
                packages.push((package.to_path_buf(), manifest.license));
            }
        }
        Licenses {
            searched: plugin.and_then(|plugin| plugin.license),
            packages,
        }
    }

    /// The licence of the skill found at `directory`: its package's, when
    /// it is one of a package the search came to and that gives one, as
    /// the plugin nearest to it, and otherwise the one searched's.
    fn of(&self, directory: &Path) -> Option<String> {
        package_of(directory)
            .and_then(|package| self.packages.iter().find(|(at, _)| at == package))
            .and_then(|(_, license)| license.clone())
            .or_else(|| self.searched.clone())
    }
}

/// The folder whose `skills` folder `directory` is in: the package it is a
/// skill of, when that folder is a package in the Agent Plugins format,
/// whose skills are those alone ([`children`]).
fn package_of(directory: &Path) -> Option<&Path> {
    directory
        .parent()
        .filter(|skills| skills.file_name().is_some_and(|name| name == "skills"))
        .and_then(Path::parent)
}

/// Read what the clone at `clone` offers ([`Offered`]), telling `request`'s
/// watch each folder read.
fn offered(
    clone: &Path,
    request: &Request,
    url: &str,
    manifest: &str,
    failures: &mut Vec<Failure>,
) -> Result<Offered, GatherError> {
    let watch = &request.watch;
    let searched = clone.join(&request.subdirectory);
    if !searched.is_dir() {
        return Err(GatherError::NoSuchFolder {
            subdirectory: request.subdirectory.clone(),
            url: url.to_owned(),
        });
    }
    let bound = fs::canonicalize(clone).map_err(|source| GatherError::Read {
        path: clone.to_path_buf(),
        source,
    })?;
    let plugin = match Manifest::read(&searched, &bound, &request.search.formats()) {
        Ok(plugin) => plugin,
        Err(unreadable) => {
            failures.push(Failure::ManifestUnreadable {
                path: unreadable.path,
                reason: unreadable.reason,
            });
            None
        }
    };
    let found = candidates(
        &searched,
        clone,
        request,
        plugin.as_ref(),
        manifest,
        failures,
    )?;
    // What a skill at the repository's root is filed under.
    let repository = crate::url::repository(url);
    let mut taken = Vec::with_capacity(found.len());
    watch.report(Progress::Harvesting {
        done: 0,
        total: found.len(),
    });
    for (done, directory) in found.iter().enumerate() {
        if let Some(read) = read(directory, clone, repository.as_deref(), failures) {
            taken.push(read);
        }
        watch.report(Progress::Harvesting {
            done: done + 1,
            total: found.len(),
        });
    }
    let taken = unclashed(request.kind, taken, clone, failures);
    let held = found
        .iter()
        .filter_map(|directory| filed(directory, clone, repository.as_deref()))
        .collect();
    // For a repository's gather, whose search reads the Agent Plugins
    // format's manifest alone ([`Search::formats`]), `plugin` is there only
    // when the folder searched is a package; a catalog's gather says what a
    // plugin it lists holds by its entry.
    let (holds, mcp_files) = match (&request.search, &plugin) {
        (Search::Layouts, Some(package)) => {
            let mut holds = package.declares.clone();
            holds.extend(Holding::held_at(&searched));
            holds.sort();
            holds.dedup();
            (holds, Holding::mcp_files_at(&searched))
        }
        _ => (Vec::new(), Vec::new()),
    };
    Ok(Offered {
        held,
        taken,
        licenses: Licenses::of_plugins(&found, &searched, &bound, plugin),
        holds,
        mcp_files,
    })
}

/// What the folder `request` searches in the repository cloned at `clone`,
/// fetched from `url`, offers as rules ([`Offered`]): every file of
/// `extension` in it, as far down as [`SEARCH_DEPTH`] ([`files`]), each
/// named by its route from that folder without its extension, the name a
/// project's rules are given ([`crate::catalog::route`]), telling
/// `request`'s watch each file read.
///
/// One whose route is no rule's name ([`crate::install::is_installable_name`])
/// is said (`gathered_route_invalid`) and not taken, nor one whose name is
/// not UTF-8 (`gathered_name_not_utf8`), nor one that leads out of the
/// repository (`gathered_link_outside`); one that cannot be read is said
/// and not taken, and still held, so what an earlier gather took of it
/// stays on the shelf ([`take_off`]). The rest is taken, a rule whose front
/// matter cannot be read among it: that is a warning ([`filed_issues`]).
fn offered_files(
    clone: &Path,
    request: &Request,
    url: &str,
    extension: &str,
    failures: &mut Vec<Failure>,
) -> Result<Offered, GatherError> {
    let watch = &request.watch;
    let searched = clone.join(&request.subdirectory);
    if !searched.is_dir() {
        return Err(GatherError::NoSuchFolder {
            subdirectory: request.subdirectory.clone(),
            url: url.to_owned(),
        });
    }
    let bound = fs::canonicalize(clone).map_err(|source| GatherError::Read {
        path: clone.to_path_buf(),
        source,
    })?;
    let mut found = Vec::new();
    files(&searched, 1, extension, &mut found)?;
    found.sort();
    let mut held = Vec::new();
    let mut taken = Vec::with_capacity(found.len());
    watch.report(Progress::Harvesting {
        done: 0,
        total: found.len(),
    });
    for (done, file) in found.iter().enumerate() {
        if let Some(read) = read_rule(file, &searched, clone, &bound, &mut held, failures)? {
            taken.push(read);
        }
        watch.report(Progress::Harvesting {
            done: done + 1,
            total: found.len(),
        });
    }
    let taken = unclashed(request.kind, taken, clone, failures);
    Ok(Offered {
        held,
        taken,
        licenses: Licenses {
            searched: None,
            packages: Vec::new(),
        },
        holds: Vec::new(),
        mcp_files: Vec::new(),
    })
}

/// The rule at `file`, found in the folder `searched` of the repository
/// cloned at `clone`, whose links resolve within `bound`: the file, the
/// route it is filed under, which `held` is told of, and the rule, read.
/// `None`, with the reason among `failures`, when it is not taken.
///
/// A file that is a link is taken as what it leads to, inside the clone, as
/// a skill's copy follows one ([`copy::tree`]'s `within`), and refused
/// otherwise, before anything of it is read: what it leads to would be put
/// on the shelf and into every agent's instructions in a project.
fn read_rule(
    file: &Path,
    searched: &Path,
    clone: &Path,
    bound: &Path,
    held: &mut Vec<String>,
    failures: &mut Vec<Failure>,
) -> Result<Option<(PathBuf, String, Artifact)>, GatherError> {
    let (resolved, inside) =
        copy::followed(file, &[bound.to_path_buf()]).map_err(|source| GatherError::Read {
            path: file.to_path_buf(),
            source,
        })?;
    if !inside {
        failures.push(Failure::LinkOutside {
            path: file.to_path_buf(),
            to: resolved,
        });
        return Ok(None);
    }
    let Some(route) = crate::catalog::route(searched, file) else {
        failures.push(Failure::UnusableName {
            path: file.to_path_buf(),
        });
        return Ok(None);
    };
    if !crate::install::is_installable_name(Kind::Rule, &route) {
        failures.push(Failure::RouteInvalid {
            path: file.to_path_buf(),
            problem: crate::artifact::issue_list(&crate::artifact::name_issues(&route)),
            route,
        });
        return Ok(None);
    }
    held.push(route.clone());
    match Artifact::rule(file, route.clone(), clone) {
        Ok(artifact) => Ok(Some((file.to_path_buf(), route, artifact))),
        Err(error) => {
            failures.push(Failure::Artifact(error));
            Ok(None)
        }
    }
}

/// The files of `extension` in `folder`, `depth` folders below the one a
/// gather of rules searches, into `found`, and in each folder in it as far
/// down as [`SEARCH_DEPTH`]: a rule as deep as a skill's folder may be,
/// bounded for the reason a skill's search is.
///
/// What a project's rules are found by passes over (`catalog::is_hidden`)
/// is passed over, so the shelf holds nothing discovery would not read back
/// from it, and so is what [`PASSED_OVER`] names, wherever it is. A folder
/// that is a link is not entered, for the reason a skill's search enters
/// none; a file that is one is found, for [`read_rule`] to hold to the
/// clone. At the root of the folder searched, its README and its licence
/// ([`about_the_rules`]) are the collection's own, and not taken.
fn files(
    folder: &Path,
    depth: usize,
    extension: &str,
    found: &mut Vec<PathBuf>,
) -> Result<(), GatherError> {
    let unread = |source| GatherError::Read {
        path: folder.to_path_buf(),
        source,
    };
    for entry in fs::read_dir(folder).map_err(unread)? {
        let entry = entry.map_err(unread)?;
        let path = entry.path();
        if PASSED_OVER.iter().any(|name| entry.file_name() == *name)
            || crate::catalog::is_hidden(&path)
        {
            continue;
        }
        let linked = entry.file_type().map_err(unread)?.is_symlink();
        if path.is_dir() {
            if !linked && depth < SEARCH_DEPTH {
                files(&path, depth + 1, extension, found)?;
            }
        } else if path.is_file()
            && crate::catalog::has_extension(&path, extension)
            && !(depth == 1 && about_the_rules(&entry.file_name()))
        {
            found.push(path);
        }
    }
    Ok(())
}

/// Whether a file called `name`, at the root of the folder a gather of
/// rules searches, is the collection's own rather than one of its rules:
/// its README, in any case and with any extension, and its licence, by the
/// names [`license_of`] finds one by ([`LICENSE_FILES`]), in any case.
fn about_the_rules(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    stem.eq_ignore_ascii_case("README")
        || LICENSE_FILES
            .iter()
            .any(|licence| name.eq_ignore_ascii_case(licence))
}

/// What the rule at `file`, in the folder `searched`, says its licence is:
/// what a licence file in its own folder names, else one in each folder
/// above it as far as `searched`, the nearest first, the collection's at
/// its root among them ([`license_of`]); else nothing, which is unknown.
///
/// Read from the clone rather than the shelf, which keeps no file of a
/// rule's but the rule. [`license_of`] reads only a file that is one, never
/// one that is a link, so nothing a link in the clone leads to is read.
fn license_above(file: &Path, searched: &Path) -> Option<String> {
    file.ancestors()
        .skip(1)
        .take_while(|folder| folder.starts_with(searched))
        .find_map(|folder| license_of(None, folder))
}

/// Take off the shelf what `source` supplied before (`supplied`) and does
/// not hold at the commit this gather took, which found artifacts to file
/// under the names `held` there, a skill's folder or a rule's route, and put
/// what it could take of them on `shelf`: each said in `report` as removed.
///
/// By its folder, as the shelf files it, or a rule by its file, the same
/// way ([`filed_on_shelf`]): one the commit has neither the
/// folder nor the name of any more goes, its copy and its record, and one
/// whose folder another name now holds, a skill that renamed itself, loses
/// its record and keeps the folder, which holds that other name. One taken
/// again under its name from another folder, a skill whose folder was
/// renamed, keeps its record, which names the folder it was taken into
/// now, and is no removal; its old folder goes all the same, since no
/// record names it any more and `validate` would read it on the shelf for
/// good. One whose folder is still there but could not be taken this time,
/// as it no longer reads, is kept as it was, the failure said beside it:
/// one broken skill must not take the copy that worked off the shelf. A
/// record whose path is not a folder of this source's shelf, which no
/// gather writes, loses its record and nothing else, as no folder it names
/// is the gather's to remove.
///
/// The copy goes before its record, and one that will not go keeps its
/// record, said as a failure: a record dropped first would leave a folder
/// the shelf still holds and nothing offers, which the next gather would
/// not know to remove.
fn take_off(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    source: &ledger::Source,
    shelf: &Path,
    held: &[String],
    supplied: Vec<Gathered>,
    report: &mut Report,
) -> Result<(), GatherError> {
    let taken: Vec<(&str, String)> = [&report.added, &report.updated, &report.unchanged]
        .into_iter()
        .flatten()
        .map(|harvested| {
            (
                harvested.name.as_str(),
                relative(workspace, &harvested.path),
            )
        })
        .collect();
    for gathered in supplied {
        // Taken again under its name: `record` has moved its row to the
        // folder it was taken into, which `gathered.path`, read before
        // anything was placed, may not be.
        let again = taken
            .iter()
            .any(|(name, _)| *name == gathered.name.as_str());
        let at = workspace.root().join(&gathered.path);
        let folder = filed_on_shelf(gathered.kind, shelf, &at);
        let placed = taken.iter().any(|(_, path)| *path == gathered.path);
        if (again && placed) || (!placed && held.contains(&folder)) {
            continue;
        }
        let shelved = crate::install::is_installable_name(gathered.kind, &folder)
            && relative(
                workspace,
                &crate::install::copy_at(shelf, gathered.kind, &folder),
            ) == gathered.path;
        if shelved && !placed {
            match take_away(gathered.kind, shelf, &at) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    report.failures.push(Failure::Unremoved {
                        path: at,
                        source: error,
                    });
                    continue;
                }
            }
        }
        if again {
            continue;
        }
        ledger.ungathered(source.id, gathered.kind, &gathered.name)?;
        report.removed.push(Harvested {
            name: gathered.name,
            directory: kept_as(gathered.kind, &folder),
            summary: gathered.summary,
            path: at,
        });
    }
    Ok(())
}

/// What the shelf filed the artifact of `kind` at `at` under, in a source's
/// folder `shelf`: a skill's folder's name, and a rule's route from there
/// without its extension, as a rule's name is its route
/// ([`crate::catalog::route`]). Empty when it is no folder or file the
/// shelf names, which no gather writes.
fn filed_on_shelf(kind: Kind, shelf: &Path, at: &Path) -> String {
    let filed = match kind.layout() {
        Layout::Directory { .. } => at
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned),
        Layout::Files { .. } | Layout::Flat { .. } => crate::catalog::route(shelf, at),
    };
    filed.unwrap_or_default()
}

/// What a report says an artifact of `kind` filed under `filed` is kept as
/// in its source's folder on the shelf ([`Harvested::directory`]): a
/// skill's folder, and a rule's file, by its route.
fn kept_as(kind: Kind, filed: &str) -> String {
    match kind.layout() {
        Layout::Directory { .. } => filed.to_owned(),
        Layout::Files { extension } | Layout::Flat { extension } => {
            format!("{filed}.{extension}")
        }
    }
}

/// Take the artifact of `kind` at `at` off a source's folder `shelf`: a
/// skill's folder whole, and a rule's file, with each folder above it its
/// route made that is left empty, up to `shelf` and not including it.
///
/// Through `remove_dir_all` for a folder, which takes a link away rather
/// than follow it, and `remove_file` for a file, which does the same.
fn take_away(kind: Kind, shelf: &Path, at: &Path) -> io::Result<()> {
    match kind.layout() {
        Layout::Directory { .. } => fs::remove_dir_all(at),
        Layout::Files { .. } | Layout::Flat { .. } => {
            fs::remove_file(at)?;
            let mut folder = at.parent();
            while let Some(above) =
                folder.filter(|above| above.starts_with(shelf) && *above != shelf)
            {
                if fs::remove_dir(above).is_err() {
                    break;
                }
                folder = above.parent();
            }
            Ok(())
        }
    }
}

/// One candidate folder, loaded to learn its name and its summary: the
/// folder, the name it is filed under, and the artifact. `None`, with the
/// reason among `failures`, when it cannot be taken.
///
/// `project` is the clone it was found in, which is where it was found and
/// not a claim that a clone is a mind project, and `repository` the name a
/// skill at its root is filed under ([`filed`]).
fn read(
    directory: &Path,
    clone: &Path,
    repository: Option<&str>,
    failures: &mut Vec<Failure>,
) -> Option<(PathBuf, String, Artifact)> {
    let artifact = match Artifact::skill(directory, clone) {
        Ok(artifact) => artifact,
        Err(error) => {
            failures.push(Failure::Artifact(error));
            return None;
        }
    };

    // The folder keeps the name the source gave it rather than the one the
    // skill declares. A disagreement between the two is something `validate`
    // reports; silently renaming the folder would repair the symptom and hide
    // it.
    let Some(folder) = filed(directory, clone, repository) else {
        failures.push(if is_root(directory, clone) {
            Failure::Unnamed {
                path: directory.to_path_buf(),
            }
        } else {
            Failure::UnusableName {
                path: directory.to_path_buf(),
            }
        });
        return None;
    };
    Some((directory.to_path_buf(), folder, artifact))
}

/// What a [`Failure::Clash`] of two artifacts of `kind` leaves somebody to
/// do: gather the skill they want by its folder, with `--path`. Two rules
/// of one route are one name in one folder but for their extension's case,
/// `.md` and `.MD`, which no `--path` tells apart, so nothing is said of it.
fn picked_by(kind: Kind) -> &'static str {
    match kind.layout() {
        Layout::Directory { .. } => ": gather the one you want with --path naming its folder",
        Layout::Files { .. } | Layout::Flat { .. } => "",
    }
}

/// What of `taken`, artifacts of `kind` read out of the repository cloned
/// at `clone`, no other read beside it shares a name with: each one that
/// does is said among `failures`, by the name it shares and where the
/// others are, and not taken.
///
/// Both names, the folder a skill is filed under on the shelf, where two
/// would be one folder, and the name it declares, which the ledger keeps
/// one row of a source's for and an install asks for: either way taking
/// one would be a pick nobody made, by whichever the search met last, and
/// taking neither says so where somebody can choose, with `--path`. The
/// rest of the source is taken all the same, and each folder is still one
/// the source holds, so what an earlier gather took of either stays on the
/// shelf as it was ([`take_off`]).
fn unclashed(
    kind: Kind,
    taken: Vec<(PathBuf, String, Artifact)>,
    clone: &Path,
    failures: &mut Vec<Failure>,
) -> Vec<(PathBuf, String, Artifact)> {
    let sharing: Vec<Vec<usize>> = taken
        .iter()
        .enumerate()
        .map(|(at, (_, folder, artifact))| {
            taken
                .iter()
                .enumerate()
                .filter(|(other, (_, its_folder, its))| {
                    *other != at && (its_folder == folder || its.name() == artifact.name())
                })
                .map(|(other, _)| other)
                .collect()
        })
        .collect();
    let routes: Vec<String> = taken
        .iter()
        .map(|(directory, folder, _)| upstream(directory, clone, folder))
        .collect();
    let mut kept = Vec::with_capacity(taken.len());
    for (at, read) in taken.iter().enumerate() {
        if sharing[at].is_empty() {
            continue;
        }
        let (directory, folder, artifact) = read;
        let filed_alike = sharing[at].iter().any(|other| taken[*other].1 == *folder);
        failures.push(Failure::Clash {
            path: directory.clone(),
            kind,
            name: if filed_alike {
                folder.clone()
            } else {
                artifact.name().to_string()
            },
            with: sharing[at]
                .iter()
                .map(|other| routes[*other].clone())
                .collect(),
        });
    }
    for (at, read) in taken.into_iter().enumerate() {
        if sharing[at].is_empty() {
            kept.push(read);
        }
    }
    kept
}

/// The folder the skill at `directory`, in the repository cloned at
/// `clone`, is filed under on the shelf: its own folder's name, or, for a
/// `SKILL.md` at the repository's root, which makes the whole of it one
/// skill, the repository's name (`repository`, [`crate::url::repository`]),
/// since the clone's folder is named for the workspace's own reasons. `None`
/// when that is no name to file one under: a folder's not in UTF-8, or no
/// repository's name at all.
fn filed(directory: &Path, clone: &Path, repository: Option<&str>) -> Option<String> {
    if is_root(directory, clone) {
        return repository
            .filter(|name| crate::install::is_folder_name(name))
            .map(str::to_owned);
    }
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

/// Whether `directory` is the root of the repository cloned at `clone`,
/// lexically, as a folder typed `.` is: the route from one to the other is
/// `.` ([`paths::relative_to`]).
fn is_root(directory: &Path, clone: &Path) -> bool {
    paths::relative_to(directory, clone).is_some_and(|route| route == Path::new("."))
}

/// What is wrong with `artifact` filed in a folder called `folder`: judged
/// where it is going rather than where it was found, as
/// [`crate::artifact::strict_issues`] judges a skill about to be written,
/// so a skill at a repository's root is held to the folder the shelf files
/// it in, which `validate` reads it from, and not to the clone's.
fn filed_issues(artifact: &Artifact, folder: &str) -> Vec<ValidationIssue> {
    match artifact.manifest() {
        Some(manifest) => crate::artifact::skill_issues(manifest, Some(folder)),
        None => artifact.validate(),
    }
}

/// What the skill at `folder`, its copy on the shelf, says its licence is,
/// as written: `stated`, its manifest's `license`, else what a licence file
/// beside the manifest names ([`LICENSE_FILES`]), else nothing, which is
/// unknown.
///
/// Of a file, the identifier a line of it tags with
/// `SPDX-License-Identifier:` when one does, since that is the licence
/// named in the one spelling every tool reads, put there to be found; and
/// otherwise its first line that is not blank, which is how a licence's
/// text opens (`MIT License`, `Apache License`), its title as the file
/// writes it. Telling a licence from the whole of its text, as GitHub's
/// licensee does, would be a guess this records as a fact, and is left to
/// whoever reads the file. A first line names no version (`Apache License`
/// is followed by `Version 2.0` on the next), which an identifier does.
///
/// Read from the shelf rather than the clone, so a licence file that is a
/// link is read as the copy resolved it, within the clone, and never
/// followed anywhere else. A file that cannot be read names nothing, as one
/// that is not there: the hash, which reads every file of the copy, has
/// said already that one cannot be.
pub fn license_of(stated: Option<&str>, folder: &Path) -> Option<String> {
    if let Some(stated) = stated.map(str::trim).filter(|stated| !stated.is_empty()) {
        return Some(stated.to_owned());
    }
    let entries: Vec<(String, PathBuf)> = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
        .collect();
    let file = LICENSE_FILES.iter().find_map(|wanted| {
        entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, path)| path)
    })?;
    let mut head = Vec::new();
    let opened = fs::File::open(file).ok()?;
    io::Read::read_to_end(&mut io::Read::take(opened, LICENSE_READ), &mut head).ok()?;
    let read = String::from_utf8_lossy(&head);
    // A byte order mark, which an editor on Windows may write, is no part
    // of the first line, and `trim` keeps it: U+FEFF is not white space.
    let text = read.strip_prefix('\u{feff}').unwrap_or(&read);
    let tagged = text.lines().find_map(|line| {
        let (_, after) = line.split_once(SPDX_TAG)?;
        // What closes the comment the tag is in, in the files that put it in
        // one, is no part of the identifier.
        let after = after.trim();
        let after = after
            .strip_suffix("-->")
            .or_else(|| after.strip_suffix("*/"))
            .unwrap_or(after);
        Some(after.trim()).filter(|identifier| !identifier.is_empty())
    });
    tagged
        .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .map(str::to_owned)
}

/// The directories under `root`, in the repository cloned at `clone`, that
/// hold a `manifest`, found as `request` searches ([`Search`]), in order,
/// with what keeps one found from being taken among `failures`.
///
/// For every layout, each at most [`SEARCH_DEPTH`] folders down: `root`
/// alone when it holds one, as a repository with a `SKILL.md` at its root
/// is one skill. A folder holding one is not looked into, for the reason
/// discovery has: the directory belongs to the artifact, assets and all,
/// and a skill that ships another as an example ships it as a file of its
/// own. Nor is one named in [`PASSED_OVER`], nor a link: a folder found
/// through a link would be found again where it leads, and a link out of
/// the clone, or back up it, would lead the search where no repository's
/// own skills are.
///
/// For a plugin a marketplace lists, the skills its entry names, each a
/// folder from `root` held to the repository lexically, as `--path` is
/// (`gathered_listed_outside`), and holding a `manifest`
/// (`gathered_listed_missing`); with none named, `root` alone when it holds
/// one, and otherwise what its `skills/` folder holds, and each folder its
/// own manifest, `plugin`, names under `skills` (#104), held to the
/// repository the same way (`gathered_manifest_outside`), each searched as
/// a repository is.
///
/// A package in the Agent Plugins format, the folder searched or one the
/// search comes to ([`plugin::is_package`]), offers its `skills/<child>`
/// alone, never deeper, as that format says, a `SKILL.md` at its root
/// among what it does not offer: one the search of a repository would find
/// further down is no skill of it. What its entry names is a plugin's
/// skills all the same, as the catalog says them.
///
/// Either way, a link that is itself a skill's folder is taken when it
/// leads to a folder inside the clone, as a copy follows it
/// ([`copy::tree`]'s `within`), and refused otherwise; one leading to a
/// folder found on its own is that folder, found once, where it is rather
/// than where the link is, so a repository linking `.claude/skills/<name>`
/// to `skills/<name>` offers one skill and not two of one name.
fn candidates(
    root: &Path,
    clone: &Path,
    request: &Request,
    plugin: Option<&Manifest>,
    manifest: &str,
    failures: &mut Vec<Failure>,
) -> Result<Vec<PathBuf>, GatherError> {
    let bound = fs::canonicalize(clone).map_err(|source| GatherError::Read {
        path: clone.to_path_buf(),
        source,
    })?;
    // By its `plugin.json` alone, as a package the search comes to is, and
    // not by `plugin`: an agent's own manifest, read before it for a plugin
    // a catalog lists (`plugin::Format::read_for`), would otherwise have a
    // folder `gather git` takes as a package searched to `SEARCH_DEPTH`.
    let package = plugin::is_package(root, &bound);
    let mut found = Vec::new();
    match &request.search {
        Search::Plugin {
            skills: Some(listed),
            ..
        } => {
            for each in listed {
                let route = Path::new(&request.subdirectory).join(each);
                let path = root.join(each);
                if outside(&route.to_string_lossy()) {
                    failures.push(Failure::ListedOutside {
                        path,
                        listed: each.clone(),
                    });
                } else if path.join(manifest).is_file() {
                    found.push(paths::normalize(&path));
                } else {
                    failures.push(Failure::ListedMissing {
                        path: paths::normalize(&path),
                    });
                }
            }
        }
        _ if package => children(&root.join("skills"), manifest, &mut found)?,
        // The folder searched is one skill, the repository itself when it
        // is its root, and nothing in it is another's.
        _ if root.join(manifest).is_file() => found.push(root.to_path_buf()),
        Search::Plugin { skills: None, .. } => {
            for folder in plugin_folders(root, request, plugin, failures) {
                if folder.is_dir() {
                    search(&folder, 1, manifest, &bound, &mut found)?;
                }
            }
        }
        Search::Layouts => search(root, 1, manifest, &bound, &mut found)?,
    }
    found.sort();
    found.dedup();

    let mut kept: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(found.len());
    for path in found {
        let (resolved, inside) =
            copy::followed(&path, std::slice::from_ref(&bound)).map_err(|source| {
                GatherError::Read {
                    path: path.clone(),
                    source,
                }
            })?;
        if !inside {
            failures.push(Failure::LinkOutside { path, to: resolved });
            continue;
        }
        let linked = is_link(&path);
        match kept.iter().position(|(_, seen)| *seen == resolved) {
            // The folder itself rather than a link to it: where it is in
            // the repository is the `upstream_path` worth recording.
            Some(at) if is_link(&kept[at].0) && !linked => kept[at] = (path, resolved),
            Some(_) => {}
            None => kept.push((path, resolved)),
        }
    }
    let mut found: Vec<PathBuf> = kept.into_iter().map(|(path, _)| path).collect();
    found.sort();
    Ok(found)
}

/// How many folders below the one a gather searches a skill's folder may
/// be: four, the depth of the deepest layout the tools that install skills
/// from a repository share, `plugins/<plugin>/skills/<name>`, and no
/// deeper.
///
/// Bounded, rather than the whole tree, so that what a repository keeps
/// further down, a vendored copy of another, its tests' fixtures, an
/// example inside an example, is not taken for what it offers, and so that
/// a search of a large repository, or of one made to be large, stays as
/// short as the layouts need it to be. A package in the Agent Plugins
/// format is not searched to it: its skills are its `skills/<child>`
/// alone, as that format says (#104).
pub const SEARCH_DEPTH: usize = 4;

/// The folders a gather's search never enters, wherever they are: git's
/// own, which no checkout holds a skill in, and a package manager's, which
/// holds whatever the packages it installed hold.
pub const PASSED_OVER: [&str; 2] = [".git", "node_modules"];

/// The folders a plugin at `root` keeps its skills in, its entry naming
/// none: its `skills/`, and each folder its manifest, `plugin`, names under
/// `skills`, from its root, once each, one that climbs out of the
/// repository lexically said (`gathered_manifest_outside`) and passed over.
///
/// Beside `skills/` rather than instead of it, for both agents' manifests:
/// a folder named there is where its skills are as well, and a gather that
/// took the one and not the other on an agent's behalf would guess at what
/// that agent loads, where taking both leaves nothing a plugin offers off
/// the shelf.
fn plugin_folders(
    root: &Path,
    request: &Request,
    plugin: Option<&Manifest>,
    failures: &mut Vec<Failure>,
) -> Vec<PathBuf> {
    let mut folders = vec![root.join("skills")];
    let Some(plugin) = plugin else {
        return folders;
    };
    for named in &plugin.skills {
        let route = Path::new(&request.subdirectory).join(named);
        if outside(&route.to_string_lossy()) {
            failures.push(Failure::ManifestOutside {
                path: root.join(plugin.format.file()),
                named: named.clone(),
            });
            continue;
        }
        let folder = paths::normalize(&root.join(named));
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

/// What the `skills` folder of a package in the Agent Plugins format holds
/// as its skills, into `found`: each folder in it holding a `manifest`, and
/// nothing further down, as that format says. A package with no `skills`
/// folder offers nothing, which is no failure.
fn children(skills: &Path, manifest: &str, found: &mut Vec<PathBuf>) -> Result<(), GatherError> {
    if !skills.is_dir() {
        return Ok(());
    }
    let unread = |source| GatherError::Read {
        path: skills.to_path_buf(),
        source,
    };
    for entry in fs::read_dir(skills).map_err(unread)? {
        let entry = entry.map_err(unread)?;
        if PASSED_OVER.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() && path.join(manifest).is_file() {
            found.push(path);
        }
    }
    Ok(())
}

/// Look through `folder`, `depth` folders below the one searched, for the
/// folders holding a `manifest`, putting each in `found`: a folder that is
/// a package in the Agent Plugins format, in the repository whose clone
/// resolves to `bound`, as that format says ([`children`]), and any other
/// as far down as [`SEARCH_DEPTH`].
fn search(
    folder: &Path,
    depth: usize,
    manifest: &str,
    bound: &Path,
    found: &mut Vec<PathBuf>,
) -> Result<(), GatherError> {
    let unread = |source| GatherError::Read {
        path: folder.to_path_buf(),
        source,
    };
    for entry in fs::read_dir(folder).map_err(unread)? {
        let entry = entry.map_err(unread)?;
        if PASSED_OVER.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let path = entry.path();
        let linked = entry.file_type().map_err(unread)?.is_symlink();
        if !path.is_dir() {
            continue;
        }
        let deeper = !linked && depth < SEARCH_DEPTH;
        if deeper && plugin::is_package(&path, bound) {
            children(&path.join("skills"), manifest, found)?;
        } else if path.join(manifest).is_file() {
            found.push(path);
        } else if deeper {
            search(&path, depth + 1, manifest, bound, found)?;
        }
    }
    Ok(())
}

/// Whether `path` is a link itself, whatever it leads to.
fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Where `directory` is in the repository cloned at `clone`: its route from
/// the repository's root, `/` separated, `folder` alone when it has none.
///
/// Lexical, as every route is ([`paths::relative_to`]), so a folder typed
/// `./skills/` is `skills` as it is in the repository.
fn upstream(directory: &Path, clone: &Path, folder: &str) -> String {
    paths::relative_to(directory, clone)
        .map_or_else(|| folder.to_owned(), |route| paths::as_route(&route))
}

/// A path as the ledger stores it: relative to the workspace, `/` separated,
/// so a workspace gathered on one platform reads on the other.
fn relative(workspace: &WeaverWorkspace, path: &Path) -> String {
    paths::relative_to(path, workspace.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| path.display().to_string())
}

/// One artifact that could not be taken, out of a source that mostly could,
/// or that was taken and could not be read back.
#[derive(Debug, Error)]
pub enum Failure {
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("{path}: its name is not valid UTF-8, so it cannot be filed")]
    UnusableName { path: PathBuf },
    #[error("{path}: cannot be copied into the workspace: {source}")]
    Copy {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Put on the shelf, but not read back for its hash, so recorded with
    /// none.
    #[error("{path}: put on the shelf, but cannot be read back to be hashed, so it is recorded with no hash: {source}")]
    Unhashed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// No longer in its source at the commit gathered, but its copy on the
    /// shelf will not go, so it is kept, still recorded and offered, for
    /// the next gather to take off.
    #[error("{path}: no longer in its source, but cannot be taken off the shelf, so it is kept, and still offered: {source}")]
    Unremoved {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Another artifact of `kind` in the same source is filed under `name`
    /// on the shelf, or declares it, at each of `with`, where it is in the
    /// repository, so neither is taken: which one a source means is not a
    /// gather's to guess ([`unclashed`]).
    #[error("{path}: another {} in this source is called `{name}` too, at {}, so neither is gathered{}", .kind.noun(), .with.join(", "), picked_by(*.kind))]
    Clash {
        path: PathBuf,
        kind: Kind,
        name: String,
        with: Vec<String>,
    },
    /// A `SKILL.md` at the root of a repository whose URL names no
    /// repository to file the skill under ([`crate::url::repository`]):
    /// `.`, say, or a folder with no name of its own.
    #[error("{path}: a SKILL.md at the repository's root is filed under the repository's name, and its URL names none, so it is not gathered: gather it by a URL or a path that ends in a name")]
    Unnamed { path: PathBuf },
    /// A skill's folder that is a link leading out of the repository, which
    /// a gather follows nowhere ([`copy::tree`]'s `within`).
    #[error("{path}: links to {to}, outside the repository, so it is not gathered")]
    LinkOutside { path: PathBuf, to: PathBuf },
    /// A skill a plugin's entry names (`listed`, as its catalog writes it)
    /// whose folder is absolute or climbs out of the repository, lexically
    /// ([`outside`]): no folder of it is looked at.
    #[error("{path}: its plugin's catalog entry names `{listed}` as a skill, which is no folder of the repository, so it is not gathered")]
    ListedOutside { path: PathBuf, listed: String },
    /// A skill a plugin's entry names whose folder holds no manifest, or is
    /// not there.
    #[error("{path}: its plugin's catalog entry names it as a skill, and it holds no SKILL.md, so it is not gathered")]
    ListedMissing { path: PathBuf },
    /// A plugin's manifest (`path`) that is there and cannot be read
    /// (#104): the plugin is searched as if it had none.
    #[error("{path}: its plugin's manifest cannot be read: {reason}, so its plugin is searched as if it had none")]
    ManifestUnreadable { path: PathBuf, reason: String },
    /// A folder of skills a plugin's manifest (`path`) names (`named`, as
    /// it writes it) that is absolute or climbs out of the repository,
    /// lexically ([`outside`]): no folder of it is looked at.
    #[error("{path}: names `{named}` as a folder of its plugin's skills, which is no folder of the repository, so nothing there is gathered")]
    ManifestOutside { path: PathBuf, named: String },
    /// A rule's file whose route from the folder searched, its name, breaks
    /// the rules every name is held to (`problem`), so no install could file
    /// a copy under it ([`crate::install::is_installable_name`]).
    #[error("{path}: the rule `{route}` cannot be named by its route ({problem}), so it is not gathered")]
    RouteInvalid {
        path: PathBuf,
        route: String,
        problem: String,
    },
}

codes!(Failure {
    Self::UnusableName { .. } => "gathered_name_not_utf8",
    Self::Copy { .. } => "gathered_copy_failed",
    Self::Unhashed { .. } => "gathered_unhashed",
    Self::Unremoved { .. } => "gathered_unremoved",
    Self::LinkOutside { .. } => "gathered_link_outside",
    Self::Unnamed { .. } => "gathered_root_unnamed",
    Self::Clash { .. } => "gathered_name_clash",
    Self::ListedOutside { .. } => "gathered_listed_outside",
    Self::ListedMissing { .. } => "gathered_listed_missing",
    Self::ManifestUnreadable { .. } => "gathered_manifest_unreadable",
    Self::ManifestOutside { .. } => "gathered_manifest_outside",
    Self::RouteInvalid { .. } => "gathered_route_invalid",
} carried {
    Self::Artifact(error) => error,
});

impl Failure {
    /// The same failure, each path in the repository cloned at `clone` said
    /// by its route from the repository's root instead, `.` for the root: a
    /// dry run's, whose clone is taken away once it is read
    /// ([`preview`]), so a path into it would name nothing. A path that is
    /// not in the clone, the shelf's or where a link leads, is left as it
    /// is.
    fn in_repository(self, clone: &Path) -> Self {
        let route = |path: PathBuf| match path.strip_prefix(clone) {
            Ok(route) if route.as_os_str().is_empty() => PathBuf::from("."),
            Ok(route) => route.to_path_buf(),
            Err(_) => path,
        };
        match self {
            Failure::Artifact(error) => Failure::Artifact(error.moved(route)),
            Failure::UnusableName { path } => Failure::UnusableName { path: route(path) },
            Failure::Unnamed { path } => Failure::Unnamed { path: route(path) },
            Failure::LinkOutside { path, to } => Failure::LinkOutside {
                path: route(path),
                to,
            },
            Failure::Clash {
                path,
                kind,
                name,
                with,
            } => Failure::Clash {
                path: route(path),
                kind,
                name,
                with,
            },
            Failure::Copy { path, source } => Failure::Copy {
                path: route(path),
                source,
            },
            Failure::ListedOutside { path, listed } => Failure::ListedOutside {
                path: route(path),
                listed,
            },
            Failure::ListedMissing { path } => Failure::ListedMissing { path: route(path) },
            Failure::ManifestUnreadable { path, reason } => Failure::ManifestUnreadable {
                path: route(path),
                reason,
            },
            Failure::ManifestOutside { path, named } => Failure::ManifestOutside {
                path: route(path),
                named,
            },
            Failure::RouteInvalid {
                path,
                route: invalid,
                problem,
            } => Failure::RouteInvalid {
                path: route(path),
                route: invalid,
                problem,
            },
            Failure::Unhashed { .. } | Failure::Unremoved { .. } => self,
        }
    }

    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            Failure::Artifact(error) => error.path(),
            Failure::UnusableName { path }
            | Failure::Unnamed { path }
            | Failure::ListedMissing { path }
            | Failure::ListedOutside { path, .. }
            | Failure::ManifestUnreadable { path, .. }
            | Failure::ManifestOutside { path, .. }
            | Failure::RouteInvalid { path, .. }
            | Failure::Clash { path, .. }
            | Failure::Copy { path, .. }
            | Failure::Unhashed { path, .. }
            | Failure::Unremoved { path, .. }
            | Failure::LinkOutside { path, .. } => path,
        }
    }
}

/// Something wrong with an artifact that was gathered anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Its manifest, where it now sits on the shelf.
    pub path: PathBuf,
    pub issue: ValidationIssue,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.issue)
    }
}

/// Why a gather could not happen at all, as opposed to happening and finding
/// some artifacts it could not take: those are in the [`Report`].
#[derive(Debug, Error)]
pub enum GatherError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// Its [`Watch`] was cancelled before anything was put on the shelf,
    /// which is as it was, and so is the ledger.
    #[error("cancelled, with nothing put on the shelf")]
    Cancelled,
    /// The URL carries a password or token, or a user name where a token can
    /// stand in for one, which the message names as a user name: calling it a
    /// password would send somebody looking for one that is not there. `url` is shown without it, which is the URL to
    /// gather instead. A URL typed as the branch or the folder is refused
    /// the same way, and is the one `url` names. A provider the front end supplies (#110) will be a
    /// second way to give the credential.
    #[error(
        "{url}: a password, a token or, over http(s), a user name in the URL is refused, because \
         gathering stores and shows the URL: gather it without one, and configure a git \
         credential helper that has it"
    )]
    CredentialsInUrl { url: String },
    #[error("{url}: has no `{subdirectory}` folder to gather from")]
    NoSuchFolder { subdirectory: String, url: String },
    /// The folder to search is absolute, or climbs out of the repository
    /// ([`outside`]).
    #[error(
        "`{subdirectory}` is no folder of the repository: --path names one from its root, and \
         never climbs out of it"
    )]
    PathOutside { subdirectory: String },
    /// `--name` named what no source can be called ([`SourceName::parse`]).
    #[error(
        "`{name}` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    )]
    NameInvalid { name: String },
    /// Another source is called that already, at `origin`.
    #[error(
        "`{name}` is the name of the source at {origin} already: gather this one under another \
         `--name`"
    )]
    NameTaken { name: SourceName, origin: Origin },
    /// `--pin` named what is no commit's full id ([`pin`]).
    #[error(
        "`{pin}` cannot pin a gather: it has to be a commit's full id, 40 hexadecimal digits \
         (a branch or a tag goes after --ref)"
    )]
    PinInvalid { pin: String },
    /// A kind no gather takes out of a repository: an MCP server, which
    /// `gather mcp` imports from a registry instead.
    #[error(
        "{} are not gathered from a repository: `mindweaver gather mcp` imports one from the MCP Registry",
        .kind.plural()
    )]
    NotGatherable { kind: Kind },
    /// The address names one branch or tag (`owner/repo@ref`, a tree URL)
    /// and `--ref` another ([`Request::addressed`]).
    #[error("{address} names the branch or tag `{named}`, and --ref names `{given}`: give one")]
    RefContradicted {
        address: String,
        named: String,
        given: String,
    },
    /// The address names one folder (a tree URL) and `--path` another
    /// ([`Request::addressed`]).
    #[error("{address} names the folder `{named}`, and --path names `{given}`: give one")]
    PathContradicted {
        address: String,
        named: String,
        given: String,
    },
    /// A tree URL's branch or a folder in it names none once its escapes
    /// are read ([`crate::url::Undecodable`]): `segment`, as it was typed.
    #[error(
        "{address}: `{segment}` names no branch or folder once its escapes are read: give the \
         repository's URL, with the branch after --ref and the folder after --path"
    )]
    TreeUndecodable { address: String, segment: String },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The folder the ledger records for the source is not one folder name,
    /// or is the one the HTTP cache keeps ([`own_folder`]): the condition
    /// `gather remove` meets, said for a gather.
    #[error(
        "the source `{name}` is recorded with the folder `{directory}`, which is no folder of its \
         own on the shelf or in the cache, so nothing is gathered"
    )]
    FolderNotItsOwn { name: SourceName, directory: String },
}

codes!(GatherError {
    Self::Cancelled => "gather_cancelled",
    Self::CredentialsInUrl { .. } => "credentials_in_url",
    Self::NoSuchFolder { .. } => "gather_no_such_folder",
    Self::PathOutside { .. } => "gather_path_outside",
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
    Self::PinInvalid { .. } => "gather_pin_invalid",
    Self::NotGatherable { .. } => "kind_not_gatherable",
    Self::RefContradicted { .. } => "gather_ref_contradicted",
    Self::PathContradicted { .. } => "gather_path_contradicted",
    Self::TreeUndecodable { .. } => "gather_tree_undecodable",
    Self::Read { .. } => "gather_read_failed",
} carried {
    Self::Git(error) => error,
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
} echoed {
    Self::FolderNotItsOwn { .. } => "source_folder_not_its_own",
});

/// Everything the ledger knows about, for a front end to list.
///
/// Re-exported here because the shelf and its record are one feature: a caller
/// that gathers is the caller that lists.
pub use ledger::Gathered;

#[cfg(test)]
#[path = "gather_test.rs"]
mod tests;
