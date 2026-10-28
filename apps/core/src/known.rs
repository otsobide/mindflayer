//! The flayer workspaces this machine knows: a `known` array in the default
//! workspace's marker, the one in home, so switching between several flayers
//! has somewhere to read them from.
//!
//! In the default workspace rather than a file of its own beside it: home is
//! the one place every flayer on a machine can find, its marker is already
//! edited a key at a time (`edit_array`), under its lock, with every comment
//! and every other key kept, and a list a person may read and prune by hand
//! belongs beside the `projects` and `loaded` it reads like. A file of its
//! own would be one more format to version and lock. The ledger would hide
//! it from that person, and would have to be made to hold it.
//!
//! Never made for the occasion. A default workspace nothing has made yet
//! remembers nothing, and `flayer init` making one to remember another in
//! would write a marker into somebody's home for a list nobody has asked to
//! read: the rule every read follows ([`crate::resolve::Intent::Read`]).
//! The default workspace is never in its own list either. It is listed for
//! being the default, whatever `known` says.
//!
//! What remembers is `flayer init`, of each workspace it makes and of one
//! already there, which it otherwise leaves as it was: so a workspace made
//! before the default one, or by a build from before this list, is one
//! idempotent command from being known. Activating one remembers nothing,
//! since activating writes nothing (`docs/architecture.md`, "The workspaces
//! this machine knows").

use std::path::{Path, PathBuf};

use crate::workspace::{
    absolute, same_directory, FlayerWorkspace, Registration, WorkspaceError, FLAYER_CONFIG,
    FLAYER_DIR,
};

/// The default workspace at `home`, when there is one and it is made.
///
/// Opened only when its marker is there: a read of an unmade one would
/// answer from the template, which knows no workspace, and a write must not
/// make it.
fn made_default(home: Option<&Path>) -> Result<Option<FlayerWorkspace>, WorkspaceError> {
    let Some(home) = home else {
        return Ok(None);
    };
    let home = absolute(home)?;
    if !home.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file() {
        return Ok(None);
    }
    FlayerWorkspace::open(&home).map(Some)
}

/// Remember `workspace` in the default workspace at `home`: what `flayer
/// init` does with each workspace it makes, or finds already made.
///
/// `None` when nothing is written: there is no default (`home` is `None`),
/// it is not made yet, or `workspace` is the default one itself. Otherwise
/// the entry it is known by, and whether this added it or it was there
/// already, by this spelling or another.
pub fn remember(
    home: Option<&Path>,
    workspace: &FlayerWorkspace,
) -> Result<Option<(PathBuf, Registration)>, WorkspaceError> {
    let Some(mut default) = made_default(home)? else {
        return Ok(None);
    };
    if same_directory(default.root(), workspace.root()) {
        return Ok(None);
    }
    default.remember(workspace).map(Some)
}

/// Forget the workspace at `root` in the default workspace at `home`,
/// returning every entry that pointed at it, each spelling of it included.
///
/// A path rather than a workspace, since the entry most worth forgetting is
/// one whose workspace has gone. Refused, as `unlink` is, when nothing
/// remembers it: a typo far more often than a no-op. With no default made,
/// nothing remembers anything, which is the same refusal.
pub fn forget(home: &Path, root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
    match made_default(Some(home))? {
        Some(mut default) => default.forget(root),
        None => Err(WorkspaceError::NotKnown {
            path: root.to_path_buf(),
            workspace: absolute(home)?,
        }),
    }
}
