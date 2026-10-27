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
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::paths;
use crate::workspace::{FlayerWorkspace, MindProject};

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
}

/// The workspace root, to [`DEFAULT_DEPTH`].
impl Default for Request {
    fn default() -> Self {
        Request {
            roots: Vec::new(),
            depth: DEFAULT_DEPTH,
        }
    }
}

/// A directory a workspace could manage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Where it is.
    pub path: PathBuf,
    /// The route to it from the workspace root, `/` separated: what
    /// `flayer link` would be given.
    pub route: String,
    /// A mind project already, rather than a git repository that is not one
    /// yet and would need `mind init` before it can be linked.
    pub mind: bool,
    /// Whether the workspace links it already.
    pub linked: bool,
}

/// What a scan found.
#[derive(Debug)]
pub struct Scan {
    /// Where it looked, each once, in the order it was given them: the
    /// workspace root when it was given none.
    pub roots: Vec<PathBuf>,
    /// How many levels below each root it looked.
    pub depth: usize,
    /// Ordered by route.
    pub found: Vec<Found>,
    /// Folders that could not be listed. One unreadable folder must not hide
    /// the repositories beside it.
    pub failures: Vec<ScanFailure>,
    /// It stopped at [`MAX_LISTED`] before looking everywhere.
    pub truncated: bool,
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
    let mut scan = Scan {
        roots: roots.clone(),
        depth: request.depth,
        found: Vec::new(),
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
                scan.failures.push(ScanFailure { path: root, source });
                continue;
            }
        }
        if let Some(found) = judge(workspace, &root) {
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
                scan.failures.push(ScanFailure {
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
            if let Some(found) = judge(workspace, &child) {
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

    scan.found.sort_by(|a, b| a.route.cmp(&b.route));
    scan.failures.sort_by(|a, b| a.path.cmp(&b.path));
    scan
}

/// What a directory is to the workspace, if it is anything.
fn judge(workspace: &FlayerWorkspace, directory: &Path) -> Option<Found> {
    let mind = MindProject::is_marked(directory);
    // `exists`, not `is_dir`: a worktree or a submodule has a `.git` file.
    let git = directory.join(".git").exists();
    if !mind && !git {
        return None;
    }
    let route = paths::relative_to(directory, workspace.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| directory.display().to_string());
    Some(Found {
        path: directory.to_path_buf(),
        route,
        mind,
        linked: workspace.is_linked(directory),
    })
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

/// A folder a scan could not list.
#[derive(Debug, Error)]
#[error("{path}: cannot be listed: {source}")]
pub struct ScanFailure {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}
