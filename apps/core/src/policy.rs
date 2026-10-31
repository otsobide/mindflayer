//! What a request from outside may name: the path policy every task the
//! panel's browser sends passes before a line of it runs (#67, D-18).
//!
//! The panel turns a task into command lines and runs them as a person at a
//! terminal would, which is the point: nothing it does is something a
//! command could not. But a person typing a path knows what they meant, and
//! a browser is a stranger whose request reached the server with the token,
//! from a page that may not be the panel's. So a task is held to less than
//! a command line is:
//!
//! - **A project or a loaded workspace is named by its entry**, spelled as
//!   the workspace's marker spells it, which is how a snapshot lists it
//!   ([`linked`], [`loaded`]). Never by a name, nor by a path the front end
//!   joins itself: the front end turns the entry into the path the registry
//!   resolves, and names the project to the command by that.
//! - **A folder that is not linked yet is named by a path that reads the same
//!   from anywhere** ([`path`]): absolute, spelled as normalizing it would
//!   spell it, so that what the preview shows is the folder the command
//!   acts on, and outside every `.mindflayer`, where a workspace keeps its
//!   marker, its ledger, its library and the clones it gathered, none of
//!   which is a project to make or link. A path that leads into one through
//!   a link is refused as one inside it.
//! - **A workspace to switch to is named by its path as a list of core's
//!   spells it** ([`switched`]): one of the workspaces this machine knows,
//!   or the one the front end was launched on ([`crate::known::switchable`]),
//!   matched character for character against that list and never opened as
//!   given, so a front end that stays on one workspace goes to no other.
//!
//! Each refusal has a code, as every error core gives does
//! ([`crate::code`]), and the front end runs nothing of a task one of its
//! values is refused for. Here rather than in the panel's backend, because
//! a rule about which paths may be named is one a second front end would
//! otherwise write again, and could write differently.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::install;
use crate::known::Switchable;
use crate::paths;
use crate::resolve::Reason;
use crate::workspace::{FlayerWorkspace, Loaded, Member, WorkspaceError, FLAYER_DIR};

/// Why a value a request holds is not one it may name.
#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("`{given}` is a URL, not a folder: name a folder by its absolute path")]
    Url { given: String },
    #[error("`{given}` is not an absolute path: name a folder by its absolute path, which reads the same from anywhere")]
    Relative { given: String },
    #[error("`{given}` is not spelled as the folder it names, `{normal}`: no `.` or `..` part, and no doubled or trailing separator")]
    Unnormalized { given: String, normal: String },
    #[error("`{given}` is inside a `.mindflayer`, which holds a workspace's own files: name a folder outside it")]
    InMindflayer { given: String },
    /// A workspace to switch to that no list of what may be switched to
    /// holds, by the path it spells: a typo, a list read before it changed,
    /// or a path somebody put together, none of which is gone to.
    #[error("`{given}` is none of the workspaces this machine knows, nor the one the panel was launched on: only one of those is switched to")]
    Unlisted { given: String },
    /// An entry no registry holds, or a path no entry points at: the
    /// workspace's own error for an unlink or an unload of one, since it is
    /// the condition they meet too.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

codes!(PolicyError {
    Self::Url { .. } => "path_url",
    Self::Relative { .. } => "path_relative",
    Self::Unnormalized { .. } => "path_unnormalized",
    Self::InMindflayer { .. } => "path_in_mindflayer",
    Self::Unlisted { .. } => "switch_unlisted",
} carried {
    Self::Workspace(error) => error,
});

/// `given`, as a path a request may name: absolute, spelled exactly as
/// [`paths::normalize`] would spell it, and outside every `.mindflayer`,
/// both as given and once its links are followed.
///
/// Spelled as given is spelled normalized, rather than normalized for it, so
/// the path a preview shows is the one the command is given, character for
/// character. A path that does not exist yet is followed as far as it does
/// exist, the rest appended ([`install::resolved`]): `mind init` makes a
/// project in a folder that may be new, and one new under a link into a
/// `.mindflayer` would be made inside it.
pub fn path(given: &str) -> Result<PathBuf, PolicyError> {
    let path = Path::new(given);
    if !path.is_absolute() {
        // A URL is not absolute either, but says so in its own words: a
        // `file://` one names a folder, and is still not how one is named.
        return Err(if given.contains("://") {
            PolicyError::Url {
                given: given.to_owned(),
            }
        } else {
            PolicyError::Relative {
                given: given.to_owned(),
            }
        });
    }
    let normal = paths::normalize(path);
    if normal.as_os_str() != path.as_os_str() {
        return Err(PolicyError::Unnormalized {
            given: given.to_owned(),
            normal: normal.display().to_string(),
        });
    }
    let followed = install::resolved(path);
    if [Some(path), followed.as_deref()]
        .into_iter()
        .flatten()
        .any(in_mindflayer)
    {
        return Err(PolicyError::InMindflayer {
            given: given.to_owned(),
        });
    }
    Ok(normal)
}

/// The entry of `workspace`'s registry spelled `entry`, as a snapshot lists
/// it, with where it points and the project there, or why there is none.
///
/// By its spelling and nothing else: a project's name, which two projects
/// can declare, or another path to the same folder, would each be a second
/// way to name one, which a request has no need of. An entry whose project
/// has gone is still an entry, and the command it reaches says why it
/// cannot act on it.
pub fn linked(workspace: &FlayerWorkspace, entry: &str) -> Result<Member, PolicyError> {
    workspace
        .members()
        .into_iter()
        .find(|member| paths::as_route(&member.entry) == entry)
        .ok_or_else(|| {
            WorkspaceError::NotRegistered {
                path: PathBuf::from(entry),
                workspace: workspace.root().to_path_buf(),
            }
            .into()
        })
}

/// The entry of `workspace`'s `loaded` list spelled `entry`, as a snapshot
/// lists it, matched as [`linked`] matches a project's.
pub fn loaded(workspace: &FlayerWorkspace, entry: &str) -> Result<Loaded, PolicyError> {
    workspace
        .loads()
        .into_iter()
        .find(|loaded| paths::as_route(&loaded.entry) == entry)
        .ok_or_else(|| {
            WorkspaceError::NotLoaded {
                path: PathBuf::from(entry),
                workspace: workspace.root().to_path_buf(),
            }
            .into()
        })
}

/// `given`, a [`path`] some entry of `workspace`'s registry points at: how a
/// command that takes linked projects by their paths, `flayer unlink` from
/// the minds screen's marks, is given one.
pub fn linked_path(workspace: &FlayerWorkspace, given: &str) -> Result<PathBuf, PolicyError> {
    let path = path(given)?;
    if !workspace.is_linked(&path) {
        return Err(WorkspaceError::NotRegistered {
            path,
            workspace: workspace.root().to_path_buf(),
        }
        .into());
    }
    Ok(path)
}

/// `given`, a [`path`] some entry of `workspace`'s `loaded` list points at:
/// how `mind unload` is given the workspace it takes out.
pub fn loaded_path(workspace: &FlayerWorkspace, given: &str) -> Result<PathBuf, PolicyError> {
    let path = path(given)?;
    if !workspace.is_loaded(&path) {
        return Err(WorkspaceError::NotLoaded {
            path,
            workspace: workspace.root().to_path_buf(),
        }
        .into());
    }
    Ok(path)
}

/// The workspace `given` names among `listed`, what [`crate::known::switchable`]
/// lists, opened, with the rule it is chosen by once switched to: what a
/// front end that stays on one workspace goes to when asked to go to
/// another.
///
/// By the path the list spells it by, character for character, rather than
/// by where `given` leads: a path a request sent is never opened, nor
/// followed through a link, so nothing but a listed workspace is reached,
/// and `given` names nothing else for being another spelling of it. One
/// listed that does not open is refused with why it does not, never made:
/// a workspace is made by `flayer init`, and a list read a moment ago may
/// name one that has gone since. The one in use is taken, a switch to it
/// changing nothing, as a load of one loaded already changes nothing.
pub fn switched(
    listed: &[Switchable],
    given: &str,
) -> Result<(FlayerWorkspace, Reason), PolicyError> {
    let row = listed
        .iter()
        .find(|row| row.workspace.path.to_str() == Some(given))
        .ok_or_else(|| PolicyError::Unlisted {
            given: given.to_owned(),
        })?;
    Ok((FlayerWorkspace::open(&row.workspace.path)?, row.reason))
}

/// `path` as a command line's words hold it: a path the registry resolved,
/// which a front end hands a command in place of what it was asked.
///
/// Refused when it is not UTF-8, which no word of a command line can be,
/// rather than spelled lossily, which would name another folder.
pub fn spelled(path: &Path) -> Result<String, PolicyError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        WorkspaceError::NonUtf8Path {
            path: path.to_path_buf(),
        }
        .into()
    })
}

/// Whether some part of `path` is a `.mindflayer`.
fn in_mindflayer(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::Normal(FLAYER_DIR.as_ref()))
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
