//! Harvesting artifacts from somewhere else into a flayer workspace.
//!
//! Gathering fills the workspace's own shelf and stops there. Nothing is
//! written into a mind project: which of the gathered skills a project should
//! carry is a separate decision, made later and by somebody, and doing it here
//! would mean a `git clone` silently editing repositories.
//!
//! What lands where: a source's clone goes to
//! `.mindflayer/cache/<source>/`, and each artifact it yields is copied to
//! `.mindflayer/<kind folder>/<source>/<name>/`. Namespacing by source is what
//! lets two repositories both offer `commit-style`; [`crate::ledger`] is what
//! remembers which is which.

mod credentials;
pub mod git;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{Artifact, ArtifactError, ValidationIssue};
use crate::copy::{self, Change};
use crate::kind::{Kind, Layout};
use crate::ledger::{self, Action, Ledger, LedgerError, Outcome, SourceKind};
use crate::paths;
use crate::url::{carries_credentials, redacted};
use crate::workspace::FlayerWorkspace;

/// The folder inside a source that is harvested unless another is named.
pub const DEFAULT_SUBDIRECTORY: &str = "skills";

/// Where to gather from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A git repository, at a branch or tag, or at whatever its HEAD names.
    Git {
        url: String,
        reference: Option<String>,
    },
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

/// One gather: what to take, from where, and out of which folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub source: Source,
    /// The folder inside the source to harvest.
    pub subdirectory: String,
    /// What to look for there.
    pub kind: Kind,
}

impl Request {
    /// Gather skills from a git repository's `skills` folder.
    pub fn git(url: impl Into<String>) -> Self {
        Self {
            source: Source::Git {
                url: url.into(),
                reference: None,
            },
            subdirectory: DEFAULT_SUBDIRECTORY.to_owned(),
            kind: Kind::Skill,
        }
    }

    /// Take from `subdirectory` instead of `skills`.
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
}

/// One artifact that reached the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harvested {
    /// The name it declares, which is what it will be asked for by.
    pub name: String,
    /// The folder it came from inside the source, which is the folder it kept.
    pub directory: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Where it now sits in the workspace.
    pub path: PathBuf,
}

/// What one gather did.
///
/// Split three ways rather than counted once, because "3 skills" hides the
/// only question worth asking after a second gather: what changed.
#[derive(Debug, Default)]
pub struct Report {
    /// The revision that was gathered from, when there was one.
    pub revision: Option<String>,
    /// The folder under `.mindflayer/<kind>/` this source owns.
    pub directory: String,
    pub added: Vec<Harvested>,
    pub updated: Vec<Harvested>,
    pub unchanged: Vec<Harvested>,
    /// Artifacts in the source that could not be taken. One broken skill must
    /// not cost the forty next to it.
    pub failures: Vec<Failure>,
    /// What is wrong with artifacts that were taken anyway.
    pub warnings: Vec<Warning>,
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
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    let outcome = harvest(workspace, ledger, request);

    // The log records what was attempted, not only what worked: a gather that
    // failed is the entry somebody opens the log to find. A log write that
    // fails on the error path is swallowed, because the error it would replace
    // is the more useful of the two.
    match &outcome {
        Ok(report) => {
            let detail = format!(
                "{} added, {} updated, {} unchanged, {} failed",
                report.added.len(),
                report.updated.len(),
                report.unchanged.len(),
                report.failures.len()
            );
            ledger.log(
                Action::Gather,
                Some(request.source.url()),
                Outcome::Ok,
                Some(&detail),
            )?;
        }
        Err(error) => {
            let _ = ledger.log(
                Action::Gather,
                Some(request.source.url()),
                Outcome::Failed,
                Some(&error.to_string()),
            );
        }
    }
    outcome
}

/// What [`scrub`] took out.
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
}

impl Scrub {
    /// Whether there was nothing to take out, which is every time but the
    /// first after an older build stored a credential.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.actions == 0 && self.failures.is_empty()
    }
}

/// Take out the credentials a Mindflayer from before they were refused
/// stored: in the URLs of sources, in the action log, and in the clones it
/// made from those URLs.
///
/// For a front end to run whenever a command that writes opens the ledger,
/// before it writes. Every time rather than once, as a schema migration
/// would, because an older build can keep writing to the same file after
/// this one has scrubbed it, and the schema steps are spoken for by the
/// changes that need a new version. Doing nothing costs one query, and the
/// first time it finds something is the only time it says so. A read is left
/// alone: what it shows is redacted on the way out anyway, and reading
/// should not write.
pub fn scrub(workspace: &FlayerWorkspace, ledger: &Ledger) -> Result<Scrub, LedgerError> {
    let mut scrub = Scrub::default();
    for source in ledger.tainted_sources()? {
        // The clone first and the row after it: a row rewritten first would
        // no longer say that its clone still needs the same.
        if let Err(error) = git::scrub(&workspace.cache_dir().join(&source.directory)) {
            scrub.failures.push(ScrubFailure {
                url: source.url.clone(),
                source: error,
            });
            continue;
        }
        ledger.scrub_source(source.id)?;
        // Two folders of one place are one thing to tell somebody about.
        if !scrub
            .sources
            .iter()
            .any(|seen| seen.url == source.url && seen.reference == source.reference)
        {
            scrub.sources.push(source);
        }
    }
    scrub.actions = ledger.scrub_log()?;
    Ok(scrub)
}

/// A clone made from a URL with credentials that could not be rewritten
/// without them.
#[derive(Debug, Error)]
#[error("{url}: its clone still holds the credentials it was gathered with: {source}")]
pub struct ScrubFailure {
    /// The source's URL, without them.
    pub url: String,
    #[source]
    pub source: git::GitError,
}

/// The work itself, with the logging left to [`gather`].
fn harvest(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    // Refused before anything is registered or fetched, rather than used for
    // this one fetch and dropped from what is stored: a token on the command
    // line is already where `ps` and the shell history show it, and every
    // report names the URL. A git credential helper, or the front end once
    // #110 lands, keeps it off the command line altogether.
    if carries_credentials(request.source.url()) {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(request.source.url()).into_owned(),
        });
    }

    // Only directory-shaped kinds are gatherable today. Matching on the kind
    // rather than on its layout keeps the compiler asking: a third kind has to
    // answer this question before it can be added.
    match request.kind {
        Kind::Skill => {}
        Kind::Rule => return Err(GatherError::NotGatherable { kind: request.kind }),
    }
    let Layout::Directory { manifest } = request.kind.layout() else {
        return Err(GatherError::NotGatherable { kind: request.kind });
    };

    let source = ledger.source_for(
        request.source.kind(),
        request.source.url(),
        request.source.reference(),
        &request.subdirectory,
    )?;

    let cache = workspace.cache_dir().join(&source.directory);
    let clone = match &request.source {
        Source::Git { url, reference } => git::clone(url, reference.as_deref(), &cache)?,
    };
    ledger.saw_source(source.id, clone.revision.as_deref())?;

    let harvested_from = clone.root.join(&request.subdirectory);
    if !harvested_from.is_dir() {
        return Err(GatherError::NoSuchFolder {
            subdirectory: request.subdirectory.clone(),
            url: request.source.url().to_owned(),
        });
    }

    let shelf = workspace.gathered_dir(request.kind).join(&source.directory);
    let mut report = Report {
        revision: clone.revision.clone(),
        directory: source.directory.clone(),
        ..Report::default()
    };

    for directory in candidates(&harvested_from, manifest)? {
        // The artifact is loaded to learn its name and its summary, then
        // dropped: `project` is the clone it was found in, which is where it
        // was found and not a claim that a clone is a mind project.
        let artifact = match Artifact::skill(&directory, &clone.root) {
            Ok(artifact) => artifact,
            Err(error) => {
                report.failures.push(Failure::Artifact(error));
                continue;
            }
        };

        // The folder keeps the name the source gave it rather than the one the
        // skill declares. A disagreement between the two is something
        // `validate` reports; silently renaming the folder would repair the
        // symptom and hide it.
        let Some(folder) = directory.file_name().and_then(|name| name.to_str()) else {
            report
                .failures
                .push(Failure::UnusableName { path: directory });
            continue;
        };
        let destination = shelf.join(folder);

        let change = match copy::replace(&directory, &destination, &clone.root) {
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
        report
            .warnings
            .extend(artifact.validate().into_iter().map(|issue| Warning {
                path: destination.join(manifest),
                issue,
            }));

        let recorded = relative(workspace, &destination);
        ledger.record(
            &source,
            request.kind,
            artifact.name(),
            &recorded,
            artifact.summary(),
            clone.revision.as_deref(),
        )?;

        let harvested = Harvested {
            name: artifact.name().to_owned(),
            directory: folder.to_owned(),
            summary: artifact.summary().map(str::to_owned),
            path: destination,
        };
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }

    report.failures.sort_by(|a, b| a.path().cmp(b.path()));
    // Stable, so one file's warnings stay in the order `validate` gives them.
    report.warnings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

/// The directories under `root` that hold a `manifest`.
///
/// Only the immediate ones, for the reason discovery has: the directory
/// belongs to the artifact, assets and all.
fn candidates(root: &Path, manifest: &str) -> Result<Vec<PathBuf>, GatherError> {
    let entries = fs::read_dir(root).map_err(|source| GatherError::Read {
        path: root.to_path_buf(),
        source,
    })?;

    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| GatherError::Read {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.join(manifest).is_file() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// A path as the ledger stores it: relative to the workspace, `/` separated,
/// so a workspace gathered on one platform reads on the other.
fn relative(workspace: &FlayerWorkspace, path: &Path) -> String {
    paths::relative_to(path, workspace.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| path.display().to_string())
}

/// One artifact that could not be taken, out of a source that mostly could.
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
}

impl Failure {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            Failure::Artifact(error) => error.path(),
            Failure::UnusableName { path } | Failure::Copy { path, .. } => path,
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
    Git(#[from] git::GitError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    /// The URL carries a password or token, or a user name where a token can
    /// stand in for one. `url` is shown without it, which is the URL to
    /// gather instead. A provider the front end supplies (#110) will be a
    /// second way to give the credential.
    #[error(
        "{url}: a password or token in the URL is refused, because gathering stores and shows \
         the URL: gather it without one, and configure a git credential helper that has it"
    )]
    CredentialsInUrl { url: String },
    #[error("{url}: has no `{subdirectory}` folder to gather from")]
    NoSuchFolder { subdirectory: String, url: String },
    #[error("{kind}s cannot be gathered yet")]
    NotGatherable { kind: Kind },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Everything the ledger knows about, for a front end to list.
///
/// Re-exported here because the shelf and its record are one feature: a caller
/// that gathers is the caller that lists.
pub use ledger::Gathered;
