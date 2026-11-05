//! The folders directly inside one, listed for a front end with no terminal
//! to pick one from: the panel's explorer, which fills the folder to link or
//! to scan under (#212).
//!
//! At a terminal the folder to link is typed, with `ls` and completion to
//! find it. A browser has neither, and cannot walk the disk itself, so the
//! panel asks for one folder at a time and draws what it is answered with.
//! What that holds is a rule, so it is core's, as a scan's is
//! ([`crate::scan`]):
//!
//! - **The folder, and its trail from the root down to it**, each folder on
//!   the way by its name and its path, so a front end puts no path together
//!   to go up.
//! - **Its subfolders, in the order of their names**, each with what it is
//!   ([`Is`]), whether the workspace links it and whether it is a link, and
//!   what the folder itself is, by the same words, so the one open can be
//!   chosen too.
//! - **Nothing a request may not name** ([`policy::path`]): a `.mindweaver`,
//!   and a link leading into one, are never listed, since nothing in one
//!   may be linked or scanned. A hidden folder is listed only when asked
//!   for, and counted either way.
//! - **No more than [`FOLDERS_CAP`] subfolders**, and whether it stopped
//!   there.
//! - **What it could not read or name, beside the rest** ([`BrowseFailure`]):
//!   one folder whose type will not read hides none of the others.
//!
//! Only read: a few names are looked at in each subfolder, as a scan looks
//! at them, no marker is opened, no ledger read and nothing made. One level
//! at a time, so a link is listed and opened like any folder, since a
//! repository is often reached through one, and cannot make a listing loop.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::code::{codes, Said};
use crate::paths;
use crate::policy;
use crate::workspace::{MindProject, WeaverWorkspace};

/// The most subfolders one listing holds: a thousand.
///
/// More than any folder somebody keeps repositories in holds, and few
/// enough that an answer stays quick to build and to draw: a folder of
/// thousands is one whose path is typed rather than picked from a list.
pub const FOLDERS_CAP: usize = 1000;

/// One folder and the folders directly inside it, as a front end may offer
/// them to be chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Folders {
    /// The folder listed, absolute, as it was asked for.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What it is itself, so it can be chosen as one of its subfolders can.
    pub is: Is,
    /// Whether the workspace links it.
    pub linked: bool,
    /// Each folder from the root down to it, it last: what a front end
    /// offers to go up by, each by the path it is asked for by.
    pub trail: Vec<Crumb>,
    /// Its subfolders, in the order of their names, hidden ones only when
    /// asked for, and no more than [`FOLDERS_CAP`].
    pub folders: Vec<Folder>,
    /// How many of its subfolders are hidden, their names starting with a
    /// `.`, listed or not: what asking for them would list, the bound
    /// aside.
    pub hidden: usize,
    /// Whether it holds more subfolders than [`FOLDERS_CAP`], which
    /// `folders` stops at.
    pub truncated: bool,
    /// What it could not read or name, each with its code
    /// ([`BrowseFailure`]): a subfolder whose type will not read, or whose
    /// path no request could send back, a hidden one only when hidden
    /// ones are asked for.
    pub failures: Vec<Said>,
    /// The home the front end was given, normalized, as a folder a request
    /// may name, for a way back to it: none when there is none, or when it
    /// is no such path ([`policy::path`]).
    pub home: Option<String>,
}

/// One folder on the way down to the one listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Crumb {
    /// Its name, or the root's path for the root, which has none.
    pub name: String,
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
}

/// One folder directly inside the one listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Folder {
    pub name: String,
    /// Where it is, absolute, as a request names it to list it in turn.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What it is.
    pub is: Is,
    /// Whether the workspace links it, by the path an entry leads to, as
    /// `mindweaver link` would tell.
    pub linked: bool,
    /// Whether it is a link to a folder rather than a folder: listed and
    /// opened as one all the same.
    pub symlink: bool,
}

/// What a folder is, the strongest of what it holds, as a front end says
/// it in words.
///
/// What a link and a scan look for: a mind project by any of its markers
/// ([`MindProject::is_marked`]), which is linked as it is; a weaver
/// workspace, which is loaded rather than linked; a git repository, which
/// `mind init` makes a project; and any other folder, which is still
/// chosen, for the command it goes to to say what it needs. A project at a
/// workspace's root is said to be a project, which is what it is linked as.
///
/// Serialized as [`slug`](Is::slug), by hand rather than derived from the
/// variant, which would tie the contract to a name chosen for Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Is {
    Project,
    Workspace,
    Repository,
    Folder,
}

impl Is {
    /// Every one, strongest first.
    pub const ALL: [Is; 4] = [Is::Project, Is::Workspace, Is::Repository, Is::Folder];

    /// How JSON says it: one lowercase word, which once printed is a
    /// contract.
    pub const fn slug(self) -> &'static str {
        match self {
            Is::Project => "project",
            Is::Workspace => "workspace",
            Is::Repository => "repository",
            Is::Folder => "folder",
        }
    }

    /// What `folder` is, looked at without opening anything: each marker
    /// is a name looked for, never a file read, so a marker that would not
    /// open still says a project is there, as a scan lists one.
    fn of(folder: &Path) -> Is {
        if MindProject::is_marked(folder) {
            Is::Project
        } else if WeaverWorkspace::is_marked(folder) {
            Is::Workspace
        } else if folder.join(".git").exists() {
            // `exists`, not `is_dir`: a worktree or a submodule has a `.git`
            // file, as a scan reads it.
            Is::Repository
        } else {
            Is::Folder
        }
    }
}

impl Serialize for Is {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

// Under a name of its own: `Is` alone says nothing of what it is about in
// a folder where every type of the panel's is a file.
#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// What a folder is, the strongest of what it holds: a mind project, a
    /// weaver workspace, a git repository, or any other folder.
    Is as "FolderIs", by slug { Project, Workspace, Repository, Folder }
}

/// Why a folder could not be listed at all.
#[derive(Debug, Error)]
pub enum BrowseError {
    #[error("{path}: no such folder")]
    Missing { path: PathBuf },
    #[error("{path} is not a folder")]
    NotAFolder { path: PathBuf },
    /// Anything else the system says, permission denied among it.
    #[error("{path}: cannot be listed: {source}")]
    Unlisted {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(BrowseError {
    Self::Missing { .. } => "browse_missing",
    Self::NotAFolder { .. } => "browse_not_a_folder",
    Self::Unlisted { .. } => "browse_unlisted",
});

impl BrowseError {
    /// What the system said when `path` was looked at or listed.
    fn of(path: &Path, source: io::Error) -> BrowseError {
        let path = path.to_path_buf();
        match source.kind() {
            io::ErrorKind::NotFound => BrowseError::Missing { path },
            io::ErrorKind::NotADirectory => BrowseError::NotAFolder { path },
            _ => BrowseError::Unlisted { path, source },
        }
    }
}

/// Something inside a folder listed that could not be read or named,
/// collected beside what was.
#[derive(Debug, Error)]
pub enum BrowseFailure {
    /// What it is could not be read, so whether it is a folder is unknown:
    /// a link that loops, or one into a folder nobody may look in.
    #[error("{path}: what it is cannot be read: {source}")]
    Unread {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A folder whose path is not UTF-8, which no request can send back to
    /// list it or to name it to a command, so it is not listed.
    #[error(
        "{path} has a name that is not UTF-8, which no request can send back, so it is not listed"
    )]
    NameUnfit { path: PathBuf },
}

codes!(BrowseFailure {
    Self::Unread { .. } => "browse_entry_unread",
    Self::NameUnfit { .. } => "browse_name_unfit",
});

impl BrowseFailure {
    /// The folder, or the entry, it is about.
    pub fn path(&self) -> &Path {
        match self {
            BrowseFailure::Unread { path, .. } | BrowseFailure::NameUnfit { path } => path,
        }
    }
}

/// One folder-like entry, before it is judged.
struct Entry {
    name: String,
    path: PathBuf,
    symlink: bool,
    hidden: bool,
}

impl Folders {
    /// The folders directly inside `folder`, an absolute path a request may
    /// name ([`policy::path`]), each judged against `workspace`, hidden
    /// ones listed only when `hidden` asks for them, with `home` named for
    /// a way back to it.
    ///
    /// Held to the path policy by whoever asks first, as a scan's roots
    /// are: here each subfolder is held to it again, and one it refuses is
    /// left out, neither listed nor counted.
    pub fn read(
        workspace: &WeaverWorkspace,
        folder: &Path,
        hidden: bool,
        home: Option<&Path>,
    ) -> Result<Folders, BrowseError> {
        let metadata = fs::metadata(folder).map_err(|source| BrowseError::of(folder, source))?;
        if !metadata.is_dir() {
            return Err(BrowseError::NotAFolder {
                path: folder.to_path_buf(),
            });
        }
        let mut failures: Vec<BrowseFailure> = Vec::new();
        let mut entries = entries(folder, hidden, &mut failures)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Held to the policy on the way to being listed, never as a filter
        // of the whole folder first: only what is listed, or counted as
        // hidden, is followed through its links.
        let named = |entry: &Entry| {
            entry
                .path
                .to_str()
                .is_some_and(|path| policy::path(path).is_ok())
        };
        let hidden_count = entries
            .iter()
            .filter(|entry| entry.hidden && named(entry))
            .count();
        let mut listed: Vec<Folder> = Vec::new();
        let mut truncated = false;
        for entry in entries.iter().filter(|entry| hidden || !entry.hidden) {
            if entry.path.to_str().is_none() {
                failures.push(BrowseFailure::NameUnfit {
                    path: entry.path.clone(),
                });
                continue;
            }
            if !named(entry) {
                continue;
            }
            if listed.len() == FOLDERS_CAP {
                truncated = true;
                break;
            }
            listed.push(Folder {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is: Is::of(&entry.path),
                linked: workspace.is_linked(&entry.path),
                symlink: entry.symlink,
            });
        }

        failures.sort_by(|a, b| a.path().cmp(b.path()));
        Ok(Folders {
            path: folder.to_path_buf(),
            is: Is::of(folder),
            linked: workspace.is_linked(folder),
            trail: trail(folder),
            folders: listed,
            hidden: hidden_count,
            truncated,
            failures: failures
                .iter()
                .map(|failure| Said::of(failure.code(), failure))
                .collect(),
            // Spelled as normalizing it spells it, as core takes the home
            // everywhere else, so a launch's `HOME=/home/you/` still gives a
            // way back; one relative or in a `.mindweaver` stays refused.
            home: home
                .map(paths::normalize)
                .as_deref()
                .and_then(Path::to_str)
                .filter(|home| policy::path(home).is_ok())
                .map(str::to_owned),
        })
    }
}

/// Every entry of `folder` that is a folder, or a link to one, with its
/// name as UTF-8 writes it; what will not read said in `failures`, of a
/// hidden entry only when `hidden` asks for hidden ones.
///
/// A name that is not UTF-8 is kept, as near as UTF-8 writes it, for the
/// listing to say it rather than list it, and only when it would be
/// listed: a hidden one not asked for is passed over as hidden ones are.
/// So is a hidden entry whose type will not read, told hidden by its name
/// before its type is read: a switch left off says nothing of what it
/// hides, whatever that is.
fn entries(
    folder: &Path,
    hidden: bool,
    failures: &mut Vec<BrowseFailure>,
) -> Result<Vec<Entry>, BrowseError> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).map_err(|source| BrowseError::of(folder, source))? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => {
                failures.push(BrowseFailure::Unread {
                    path: folder.to_path_buf(),
                    source,
                });
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name();
        let is_hidden = name.as_encoded_bytes().first() == Some(&b'.');
        let mut unread = |path: PathBuf, source: io::Error| {
            if hidden || !is_hidden {
                failures.push(BrowseFailure::Unread { path, source });
            }
        };
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(source) => {
                unread(path, source);
                continue;
            }
        };
        // The entry's own type, which does not follow a link; a link is
        // followed once, to tell whether it leads to a folder. One leading
        // nowhere is no folder, as a file is none: only one that cannot be
        // told is said.
        let symlink = kind.is_symlink();
        let is_folder = if symlink {
            match fs::metadata(&path) {
                Ok(metadata) => metadata.is_dir(),
                Err(source) if source.kind() == io::ErrorKind::NotFound => false,
                Err(source) => {
                    unread(path, source);
                    continue;
                }
            }
        } else {
            kind.is_dir()
        };
        if !is_folder {
            continue;
        }
        found.push(Entry {
            hidden: is_hidden,
            name: name.to_string_lossy().into_owned(),
            path,
            symlink,
        });
    }
    Ok(found)
}

/// Each folder from the root down to `folder`, it last.
fn trail(folder: &Path) -> Vec<Crumb> {
    let mut trail: Vec<Crumb> = folder
        .ancestors()
        .map(|at| Crumb {
            name: at
                .file_name()
                .unwrap_or(at.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path: at.to_path_buf(),
        })
        .collect();
    trail.reverse();
    trail
}

#[cfg(test)]
#[path = "browse_test.rs"]
mod tests;
