//! Finding the repositories under a workspace that it could manage.
//!
//! A workspace manages what it was told to, never what merely sits inside it,
//! so nothing here links anything. It answers the question that comes just
//! before a link: which directories under the workspace are mind projects, or
//! git repositories that could become one, and which are linked already.

use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::paths;
use crate::workspace::{FlayerWorkspace, MIND_CONFIG, MIND_DIR};

/// How far below the workspace root a scan looks.
///
/// Three levels reaches `repo`, `org/repo` and `github.com/org/repo` from a
/// workspace at the top, which is how people lay repositories out. Deeper than
/// that is somebody's source tree rather than a place repositories are kept.
pub const MAX_DEPTH: usize = 3;

/// How many directories a scan lists before it stops.
///
/// A workspace at the root of a home directory must not turn into a crawl of
/// the disk. A scan that stops early says so rather than passing for complete.
pub const MAX_LISTED: usize = 4096;

/// Folders that are never a place repositories are kept, and are often huge.
const SKIPPED: [&str; 2] = ["node_modules", "target"];

/// A directory a workspace could manage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Where it is.
    pub root: PathBuf,
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
#[derive(Debug, Default)]
pub struct Scan {
    /// Ordered by route.
    pub found: Vec<Found>,
    /// Folders that could not be listed. One unreadable folder must not hide
    /// the repositories beside it.
    pub failures: Vec<ScanFailure>,
    /// It stopped at [`MAX_LISTED`] before looking everywhere.
    pub truncated: bool,
}

/// Look under a workspace for mind projects and git repositories.
///
/// A repository is a leaf: the scan does not look inside one, because its
/// folders are its own and a repository nested in another is its submodule,
/// managed with it. Hidden folders and symlinked ones are not entered either —
/// the first are tooling, and the second can loop.
pub fn scan(workspace: &FlayerWorkspace) -> Scan {
    let root = workspace.root();
    let mut scan = Scan::default();

    // The workspace root is a candidate too: a workspace can manage the
    // project it sits in. Unlike every other repository it is still looked
    // inside, because it is also where the others are kept.
    if let Some(found) = judge(workspace, root) {
        scan.found.push(found);
    }

    // Breadth first, so that when the budget runs out it is the deepest
    // folders that went unseen rather than a whole neighbouring tree.
    let mut pending = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut listed = 0usize;
    while let Some((directory, depth)) = pending.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }
        if listed == MAX_LISTED {
            scan.truncated = true;
            break;
        }
        listed += 1;

        let children = match subdirectories(&directory) {
            Ok(children) => children,
            Err(source) => {
                scan.failures.push(ScanFailure {
                    path: directory,
                    source,
                });
                continue;
            }
        };
        for child in children {
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
    let mind = directory.join(MIND_DIR).join(MIND_CONFIG).is_file();
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
        root: directory.to_path_buf(),
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
