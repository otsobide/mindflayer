//! Finding the repositories a workspace could manage: under the workspace, or
//! under any folder somebody points it at.
//!
//! A workspace manages what it was told to, never what merely sits inside it,
//! so nothing here links anything. It answers the question that comes just
//! before a link: which directories are mind projects, or git repositories
//! that could become one, and which are linked already.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::agent::Agent;
use crate::code::codes;
use crate::install;
use crate::ledger::LedgerError;
use crate::paths;
use crate::relink::{self, Duplicate};
use crate::settings::ProjectId;
use crate::workspace::{same_directory, FlayerWorkspace, MindProject, WorkspaceError};

/// How far below each root a scan looks unless it is told otherwise.
///
/// Three levels reaches `repo`, `org/repo` and `github.com/org/repo` from a
/// folder at the top, which is how people lay repositories out. Deeper than
/// that is somebody's source tree rather than a place repositories are kept;
/// a scan told to go further is still held to [`MAX_LISTED`].
pub const DEFAULT_DEPTH: usize = 3;

/// How many directories a scan lists before it stops, whatever its depth and
/// however many roots it has.
///
/// A folder at the root of a home directory must not turn into a crawl of the
/// disk. A scan that stops early says so rather than passing for complete.
pub const MAX_LISTED: usize = 4096;

/// Folders that are never a place repositories are kept, and are often huge.
const SKIPPED: [&str; 2] = ["node_modules", "target"];

/// What a scan is asked to look through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The folders to look under, absolute: a front end makes what it was
    /// typed so, from the directory the command works in, as it does the
    /// paths `link` is given. None means the workspace root.
    pub roots: Vec<PathBuf>,
    /// How many levels below each root to look; none judges the roots alone.
    pub depth: usize,
    /// Where each route is spelled from: the directory `flayer link` would be
    /// run in with it. None means the workspace root, where the TUI runs its
    /// lines.
    pub from: Option<PathBuf>,
    /// How what it finds is ordered.
    pub order: Order,
}

/// The workspace root, to [`DEFAULT_DEPTH`], spelled from there, by route.
impl Default for Request {
    fn default() -> Self {
        Request {
            roots: Vec::new(),
            depth: DEFAULT_DEPTH,
            from: None,
            order: Order::Route,
        }
    }
}

/// How a scan orders what it found.
///
/// Here rather than in each front end, so `flayer scan --suggest` and the
/// minds screen list one order: a screen that sorted its own rows could
/// rank them differently from the command it says it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// By route, linked or not: what is there.
    #[default]
    Route,
    /// What is not linked first, by [`Reason`], best first, and by route
    /// within one; then what is, by route: what is worth linking, before
    /// what needs nothing.
    Suggested,
}

/// A directory a workspace could manage.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name. The panel reads it in what a scan it asks for is answered
/// with, so it says its TypeScript too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Found {
    /// Where it is.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What `flayer link` would be given for it, run where the request
    /// spells routes from: the route from there, `/` separated, when it is
    /// inside, and its path otherwise.
    pub route: String,
    /// The id its settings name it by (D-09), whatever it is called and
    /// wherever it is: none for a git repository that is not a project yet,
    /// for a project a `mind.toml` still marks, which has none, and for one
    /// whose marker would not open, which is a failure beside it.
    pub id: Option<ProjectId>,
    /// A mind project already, rather than a git repository that is not one
    /// yet and would need `mind init` before it can be linked.
    pub mind: bool,
    /// Whether the workspace links it already.
    pub linked: bool,
    /// The entry it moved from, when it is not linked and holds the id the
    /// workspace's ledger last saw at an entry where no project is any more
    /// ([`MindProject::is_vacant`]): spelled as `route` is, so `flayer
    /// relink <moved> <route>` run there follows it. None otherwise, and for
    /// an id another directory holds too, which is a copy rather than a
    /// move.
    pub moved: Option<String>,
    /// Why it is worth linking: the strongest of what it holds.
    pub reason: Reason,
    /// Each name at its root an agent reads something from
    /// ([`Agent::reads_at_root`], of every agent), in name order: what makes
    /// a repository's reason [`Reason::Agents`], and listed for a mind
    /// project too, which can hold them as well. Looked at, never followed
    /// or read: a link there counts as what it is called.
    pub agent_files: Vec<String>,
}

/// Why a directory found is worth linking, the strongest of what it holds,
/// best first.
///
/// What already makes it a project ranks above what only shows somebody
/// works in it: a mind project is linked as it is, where a repository needs
/// `mind init` first. Among projects, one its settings name by an id is
/// followed when it moves (D-09); among repositories, one an agent reads
/// something in is where skills and rules would be read.
///
/// Serialized as [`slug`](Reason::slug), a word a script compares, by hand
/// rather than derived from the variant, which would tie the contract to a
/// name chosen for Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// A mind project its settings name by an id.
    Id,
    /// A mind project with no id: a `mind.toml` marks it, or a marker that
    /// would not open, which is a failure beside it.
    Mind,
    /// A git repository holding something an agent reads at its root.
    Agents,
    /// A git repository and nothing more.
    Git,
}

impl Reason {
    /// How a report names it: one lowercase word, which once printed is a
    /// contract.
    pub const fn slug(self) -> &'static str {
        match self {
            Reason::Id => "id",
            Reason::Mind => "mind",
            Reason::Agents => "agents",
            Reason::Git => "git",
        }
    }
}

impl Serialize for Reason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

// Under a name of its own: `Reason` in TypeScript is already the rule that
// chose a workspace (`resolve::Reason`), and the two are written into one
// folder, a file to a name.
#[cfg(feature = "ts")]
crate::typescript::words! {
    /// Why a directory a scan found is worth linking, the strongest of what
    /// it holds, best first: a mind project its settings name by an id, one
    /// with none, a git repository an agent reads something in, and any
    /// other git repository.
    Reason as "ScanReason", by slug { Id, Mind, Agents, Git }
}

/// What a scan found.
#[derive(Debug, Serialize)]
pub struct Scan {
    /// Where it looked, each once, in the order it was given them: the
    /// workspace root when it was given none.
    #[serde(serialize_with = "paths::lossy_all")]
    pub roots: Vec<PathBuf>,
    /// How many levels below each root it looked.
    pub depth: usize,
    /// In the [`Order`] the request asked for.
    pub found: Vec<Found>,
    /// Each entry where no project is any more whose id the ledger never
    /// learned, as it learns them only when it is written, so nothing found
    /// can be told to be its project, moved: spelled as `route` is, for the
    /// `flayer relink <untraced> <route>` that follows it, which only
    /// somebody who knows where it went can type. In the order the marker
    /// lists them, and only when something found and not linked holds an id
    /// that no entry was matched to, which it could be; empty otherwise.
    pub untraced: Vec<String>,
    /// Folders that could not be listed, and markers that would not open.
    /// One unreadable folder must not hide the repositories beside it.
    ///
    /// Not serialized: a front end says them as its warnings, which in JSON
    /// are the envelope's, beside the data rather than in it.
    #[serde(skip)]
    pub failures: Vec<ScanFailure>,
    /// It stopped at [`MAX_LISTED`] before looking everywhere.
    pub truncated: bool,
}

#[cfg(test)]
thread_local! {
    /// How many scans this thread has run: what a test holds a read that
    /// must never walk the disk to, the snapshot's ([`crate::view`]). Per
    /// thread, since the tests beside it run on threads of their own and
    /// scan as they please.
    static SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many scans this thread has run ([`SCANS`]).
#[cfg(test)]
pub(crate) fn scans() -> usize {
    SCANS.with(std::cell::Cell::get)
}

/// Look for mind projects and git repositories under the roots `request`
/// names, or under the workspace root when it names none.
///
/// Every root is a candidate itself, and is looked inside even when it is a
/// repository, since it is where somebody said the others are kept. Below a
/// root a repository is a leaf: the scan does not look inside one, because
/// its folders are its own and a repository nested in another is its
/// submodule, managed with it. Hidden folders and symlinked ones are not
/// entered either: the first are tooling, and the second can loop. A root is
/// entered whatever it is called and however it is reached, because somebody
/// named it.
pub fn scan(workspace: &FlayerWorkspace, request: &Request) -> Scan {
    #[cfg(test)]
    SCANS.with(|scans| scans.set(scans.get() + 1));
    // A root somebody named that is not there is a typo worth a warning; the
    // workspace root not being there is an absence, since the default
    // workspace before it is made can be read in a home nobody has created
    // yet.
    let named = !request.roots.is_empty();
    let mut roots: Vec<PathBuf> = Vec::new();
    if named {
        for root in &request.roots {
            let root = paths::normalize(root);
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    } else {
        roots.push(workspace.root().to_path_buf());
    }
    let from = request.from.as_deref().unwrap_or(workspace.root());
    let names = agent_files();
    let judged = |directory: &Path, failures: &mut Vec<ScanFailure>| {
        judge(workspace, directory, from, &names, failures)
    };
    let mut scan = Scan {
        roots: roots.clone(),
        depth: request.depth,
        found: Vec::new(),
        untraced: Vec::new(),
        failures: Vec::new(),
        truncated: false,
    };

    // Every root starts the walk at once rather than one after another, so
    // the budget is spent breadth first across all of them: when it runs out
    // it is the deepest folders that went unseen, never a whole second root
    // behind a first that filled it. A folder two roots reach, one inside
    // the other, is looked at once.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut pending: VecDeque<(PathBuf, usize)> = VecDeque::new();
    for root in roots {
        // Said even when the scan lists nothing, at depth 0, so a typo is
        // never an empty answer.
        if named {
            if let Err(source) = fs::metadata(&root) {
                scan.failures
                    .push(ScanFailure::Unlisted { path: root, source });
                continue;
            }
        }
        if let Some(found) = judged(&root, &mut scan.failures) {
            scan.found.push(found);
        }
        seen.insert(root.clone());
        pending.push_back((root, 0));
    }

    let mut listed = 0usize;
    while let Some((directory, depth)) = pending.pop_front() {
        if depth >= request.depth {
            continue;
        }
        if listed == MAX_LISTED {
            scan.truncated = true;
            break;
        }
        listed += 1;

        let children = match subdirectories(&directory) {
            Ok(children) => children,
            Err(source) if !named && depth == 0 && source.kind() == io::ErrorKind::NotFound => {
                continue;
            }
            Err(source) => {
                scan.failures.push(ScanFailure::Unlisted {
                    path: directory,
                    source,
                });
                continue;
            }
        };
        for child in children {
            if !seen.insert(child.clone()) {
                continue;
            }
            if let Some(found) = judged(&child, &mut scan.failures) {
                scan.found.push(found);
                continue;
            }
            let skipped = child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SKIPPED.contains(&name));
            if !skipped {
                pending.push_back((child, depth + 1));
            }
        }
    }

    follow(workspace, from, &mut scan);
    match request.order {
        Order::Route => scan.found.sort_by(|a, b| a.route.cmp(&b.route)),
        // A linked one by route alone: its reason is why it was worth
        // linking, which says nothing once it is.
        Order::Suggested => scan.found.sort_by(|a, b| {
            let rank = |found: &Found| (found.linked, (!found.linked).then_some(found.reason));
            rank(a).cmp(&rank(b)).then_with(|| a.route.cmp(&b.route))
        }),
    }
    scan.failures.sort_by(|a, b| a.path().cmp(b.path()));
    scan
}

/// Say which of what `scan` found moved from an entry the workspace links,
/// and which ids more than one directory holds, among those found and those
/// linked: a failure naming each, and never a move ([`Duplicate`]).
///
/// A move is told by the ledger, which keeps the id it last saw at each
/// route ([`crate::ledger::Ledger::known_projects`]): a project found and not
/// linked, holding an id last seen at an entry where no project is any more,
/// is that entry's project. Only an entry nothing marks
/// ([`MindProject::is_vacant`]), not every one `flayer status` calls stale:
/// one whose marker is there and would not open still holds its project,
/// whose id nothing read, so a copy of it would pass for it moved; and one
/// that opens holding another id holds another project, which the ledger
/// follows the next time it is written
/// ([`FlayerWorkspace::ledger_to_write`]). The ledger is read only when
/// there is something to match, and one that cannot be read is a failure
/// beside what was found, which is listed with no move.
///
/// A stale entry whose id the ledger never learned is said too (`untraced`)
/// while something found is left unmatched: a project linked and moved
/// before anything wrote the ledger is found by nobody, and the entry is
/// what `flayer relink` would be given for it, where `flayer link` would
/// leave two entries for one project.
fn follow(workspace: &FlayerWorkspace, from: &Path, scan: &mut Scan) {
    let members = workspace.members();
    let mut held: Vec<(ProjectId, PathBuf)> = Vec::new();
    let found = scan
        .found
        .iter()
        .filter_map(|found| Some((found.id.clone()?, found.path.clone())));
    let linked = members.iter().filter_map(|member| {
        let settings = member.project.as_ref().ok()?.settings()?;
        Some((settings.id.clone(), member.root.clone()))
    });
    for (id, path) in found.chain(linked) {
        if !held
            .iter()
            .any(|(seen, at)| *seen == id && same_directory(at, &path))
        {
            held.push((id, path));
        }
    }
    let duplicates = relink::duplicates(&held);
    let copied = |id: &ProjectId| duplicates.iter().any(|duplicate| duplicate.id == *id);

    let stale: Vec<(String, &Path)> = members
        .iter()
        .filter(|member| member.project.is_err() && MindProject::is_vacant(&member.root))
        .map(|member| {
            (
                install::entry_for_root(workspace, &member.root),
                member.root.as_path(),
            )
        })
        .collect();
    let wanted = scan
        .found
        .iter()
        .any(|found| !found.linked && found.id.as_ref().is_some_and(|id| !copied(id)));
    if wanted && !stale.is_empty() {
        match workspace
            .ledger_for_reading()
            .and_then(|ledger| ledger.known_projects())
        {
            Ok(known) => {
                for found in scan.found.iter_mut().filter(|found| !found.linked) {
                    let Some(id) = found.id.as_ref().filter(|id| !copied(id)) else {
                        continue;
                    };
                    found.moved = known
                        .iter()
                        .find(|(seen, _)| seen == id)
                        .and_then(|(_, at)| stale.iter().find(|(route, _)| route == at))
                        .map(|(_, root)| route(root, from));
                }
                let unmatched = scan.found.iter().any(|found| {
                    !found.linked
                        && found.moved.is_none()
                        && found.id.as_ref().is_some_and(|id| !copied(id))
                });
                if unmatched {
                    for (entry, root) in &stale {
                        let spelled = route(root, from);
                        if !known.iter().any(|(_, at)| at == entry)
                            && !scan.untraced.contains(&spelled)
                        {
                            scan.untraced.push(spelled);
                        }
                    }
                }
            }
            Err(error) => scan.failures.push(ScanFailure::Ledger(error)),
        }
    }
    scan.failures
        .extend(duplicates.into_iter().map(ScanFailure::Duplicate));
}

/// Every name at a project's root any agent reads something from, each
/// once, in name order.
fn agent_files() -> Vec<PathBuf> {
    let mut names: Vec<PathBuf> = Agent::ALL
        .into_iter()
        .flat_map(Agent::reads_at_root)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// What a directory is to the workspace, if it is anything, with its route
/// spelled from `from`, which of `names` it holds, and a failure in
/// `failures` when its marker would not open.
///
/// A project whose marker would not open is listed all the same: its marker
/// still says it is one, and `flayer link`, refusing it, says why again.
fn judge(
    workspace: &FlayerWorkspace,
    directory: &Path,
    from: &Path,
    names: &[PathBuf],
    failures: &mut Vec<ScanFailure>,
) -> Option<Found> {
    let mind = MindProject::is_marked(directory);
    // `exists`, not `is_dir`: a worktree or a submodule has a `.git` file.
    let git = directory.join(".git").exists();
    if !mind && !git {
        return None;
    }
    let id = if mind {
        match MindProject::open(directory) {
            Ok(project) => project.settings().map(|settings| settings.id.clone()),
            Err(error) => {
                failures.push(ScanFailure::Unread(error));
                None
            }
        }
    } else {
        None
    };
    // Looked at without following, a few names in a directory already
    // found, so the walk stays as bounded as it was: nothing below the
    // directory is listed, and a link counts as there whatever it points
    // at, which a scan never follows.
    let agent_files: Vec<String> = names
        .iter()
        .filter(|name| fs::symlink_metadata(directory.join(name)).is_ok())
        .filter_map(|name| name.to_str().map(str::to_owned))
        .collect();
    let reason = match (mind, &id) {
        (true, Some(_)) => Reason::Id,
        (true, None) => Reason::Mind,
        (false, _) if !agent_files.is_empty() => Reason::Agents,
        (false, _) => Reason::Git,
    };
    Some(Found {
        path: directory.to_path_buf(),
        route: route(directory, from),
        id,
        mind,
        linked: workspace.is_linked(directory),
        moved: None,
        reason,
        agent_files,
    })
}

/// What `flayer link` run in `from` would be given for `directory`: the
/// route from there when it is inside, and its path otherwise.
///
/// Not a route that climbs out with `..`, which reads right from one
/// directory only and grows a `..` for every level between the two: the
/// path reads the same from anywhere, and is what somebody who pointed the
/// scan at a folder outside the workspace typed.
fn route(directory: &Path, from: &Path) -> String {
    paths::relative_to(directory, from)
        .filter(|route| route.components().next() != Some(Component::ParentDir))
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| directory.display().to_string())
}

/// Whether `moved`, the entry a scan spelling routes from `from` says a
/// directory moved from ([`Found::moved`]), is the one whose project was at
/// `root`, where the registry resolves an entry ([`crate::Member::root`]).
///
/// Matched by the directory each names rather than by spelling, since
/// `moved` is spelled as [`route`] spells it, by its path for a folder
/// outside `from`, and an entry as the marker has it, by a route climbing
/// out of the workspace for the same folder. One answer for the minds
/// screen, which offers the relink on an entry's row, and the panel's
/// backend, which names the entry a relink follows: the same match written
/// in each would let one offer a link where the other offers a relink.
pub fn moved_from(moved: &str, from: &Path, root: &Path) -> bool {
    paths::normalize(&from.join(moved)) == root
}

/// The directories directly inside `directory`, in name order, leaving out
/// hidden ones and symlinks.
fn subdirectories(directory: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut found = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        // The entry's own type, which does not follow a symlink: a link to a
        // directory is not entered.
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let hidden = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'));
        if !hidden {
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

/// What a scan could not read on its way, collected rather than raised.
#[derive(Debug, Error)]
pub enum ScanFailure {
    /// A folder it could not list, so nothing below it was seen: a root
    /// named that is not there among them.
    #[error("{path}: cannot be listed: {source}")]
    Unlisted {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A project whose marker would not open, listed without an id.
    #[error(transparent)]
    Unread(WorkspaceError),
    /// One id held by more than one directory found or linked: a copy,
    /// which is never taken for a move.
    #[error(transparent)]
    Duplicate(Duplicate),
    /// The ledger, read to tell which project moved, would not open: what
    /// was found is listed with no move.
    #[error(transparent)]
    Ledger(LedgerError),
}

codes!(ScanFailure {
    Self::Unlisted { .. } => "scan_folder_unlisted",
} carried {
    Self::Unread(error) => error,
    Self::Duplicate(duplicate) => duplicate,
    Self::Ledger(error) => error,
});

impl ScanFailure {
    /// The folder, or the marker, it is about: the first directory holding
    /// an id more than one holds.
    pub fn path(&self) -> &Path {
        match self {
            ScanFailure::Unlisted { path, .. } => path,
            ScanFailure::Unread(error) => error.path(),
            ScanFailure::Duplicate(duplicate) => duplicate
                .paths
                .first()
                .map_or(Path::new(""), PathBuf::as_path),
            ScanFailure::Ledger(error) => error.path(),
        }
    }
}

#[cfg(test)]
#[path = "scan_test.rs"]
mod tests;
