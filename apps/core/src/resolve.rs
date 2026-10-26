//! Which workspace a command uses: one function every front end feeds, so
//! no two of them can come to different answers (D-08).
//!
//! In order: the workspace the command line names (`-W`), then the one
//! activated for the shell (`$MINDFLAYER_WORKSPACE`), then the first one
//! found walking up from where the command starts, then the default one in
//! home. The first two were chosen on purpose, so they win over where
//! somebody happens to be standing, as an activated Python virtualenv does.
//! Walking up first would have an activated workspace silently lose to any
//! workspace above the current directory, which is what activating one is
//! meant to get away from.
//!
//! A choice that does not open as a workspace is an error, and never a
//! reason to carry on down the list: carrying on would act on a workspace
//! nobody chose, and say nothing about it. Nor is a chosen workspace ever
//! made for the occasion; that is `flayer init`'s to do.
//!
//! The front end reads the environment and hands over what it says, as it
//! hands over home: core never reads it, which is what lets a test pin every
//! input to nothing.

use std::fmt;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::workspace::{
    absolute, same_directory, FlayerWorkspace, WorkspaceError, FLAYER_CONFIG, FLAYER_DIR,
};

/// What a command means to do with the workspace it is given, which decides
/// whether the default one may be made for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Only look. The default workspace is the one made in home or, until
    /// something makes it, the one that would be
    /// ([`FlayerWorkspace::unmade`]): reading is no reason to write a marker
    /// into somebody's home.
    Read,
    /// Change it. The default workspace is made in home the first time one
    /// is needed, so there is always somewhere to link a project into.
    Write,
}

/// Which rule chose the workspace.
///
/// Returned beside it rather than worked out again afterwards: whether a
/// workspace is the default one, say, is not something its path alone
/// answers once `-W` can name the directory home is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Named on the command line.
    Explicit,
    /// Activated for the shell.
    Activated,
    /// The first one found walking up from where the command started.
    Above,
    /// The default one, in home.
    Home,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Reason::Explicit => "named",
            Reason::Activated => "activated",
            Reason::Above => "found above",
            Reason::Home => "default",
        })
    }
}

/// Everything that decides which workspace a command uses.
///
/// Paths rather than strings, with an empty path for an empty value: the
/// front end has already decided which of its options and variables said
/// what, and the only question left about a value is whether it was given.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The workspace the command line names: its root, or its `.mindflayer`.
    ///
    /// Given empty, it names none, and switches the activated one off too.
    /// That is what lets a test pin it, the way `--home=` pins home: a test
    /// that left it out would pick up whatever the shell running it had
    /// activated.
    pub flag: Option<&'a Path>,
    /// The workspace activated for the shell, as the front end read it.
    /// Empty means none, as if it were not set.
    pub activated: Option<&'a Path>,
    /// Where the walk up starts.
    pub start: &'a Path,
    /// Where the default workspace lives; `None` when there is no default.
    pub home: Option<&'a Path>,
    pub intent: Intent,
}

/// A workspace, and the rule that chose it.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub workspace: FlayerWorkspace,
    pub reason: Reason,
}

/// Why no workspace could be decided on.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// What the command line or the activation names does not open as a
    /// workspace: an error rather than a fallback, for the reason the module
    /// gives.
    #[error("the {reason} workspace cannot be used: {source}")]
    Chosen {
        reason: Reason,
        source: WorkspaceError,
    },
    /// One found by walking up, or the default, could not be read or made.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

/// The workspace a command uses, and why that one; `None` only when nothing
/// names one, none is above `start`, and there is no default.
pub fn resolve(inputs: &Inputs<'_>) -> Result<Option<Resolved>, ResolveError> {
    resolve_except(inputs, &[])
}

/// [`resolve`], passing over the workspaces rooted at any of `except` on the
/// way up and at home.
///
/// For `load`: run from inside the workspace being loaded, the first one up
/// the tree is that workspace, and a workspace cannot load itself, so the
/// walk carries on above it. At home it stops there instead. A command that
/// writes is refused, since loading the default workspace into itself is
/// not something to answer with another workspace; one that reads finds
/// nothing. A workspace named or activated is never passed over: it was
/// chosen, and loading it into itself is refused where loads are.
pub fn resolve_except(
    inputs: &Inputs<'_>,
    except: &[PathBuf],
) -> Result<Option<Resolved>, ResolveError> {
    if let Some((reason, path)) = chosen(inputs) {
        return open_chosen(path)
            .map(|workspace| Some(Resolved { workspace, reason }))
            .map_err(|source| ResolveError::Chosen { reason, source });
    }

    let start = absolute(inputs.start)?;
    let skipped = |root: &Path| except.iter().any(|path| same_directory(root, path));
    let above = start.ancestors().find(|candidate| {
        candidate.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file() && !skipped(candidate)
    });
    if let Some(root) = above {
        return Ok(Some(Resolved {
            workspace: FlayerWorkspace::open(root)?,
            reason: Reason::Above,
        }));
    }

    let Some(home) = inputs.home else {
        return Ok(None);
    };
    if skipped(home) {
        return match inputs.intent {
            Intent::Write => Err(WorkspaceError::LoadsItself {
                path: absolute(home)?,
            }
            .into()),
            Intent::Read => Ok(None),
        };
    }
    let workspace = match inputs.intent {
        Intent::Write => FlayerWorkspace::init(home)?.0,
        Intent::Read if home.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file() => {
            FlayerWorkspace::open(home)?
        }
        Intent::Read => FlayerWorkspace::unmade(home)?,
    };
    Ok(Some(Resolved {
        workspace,
        reason: Reason::Home,
    }))
}

/// The workspace somebody chose on purpose, and how; `None` when nobody did.
///
/// `-W` given at all, even empty, is the whole answer: given empty, it
/// chooses none, and the activated one is not asked either.
fn chosen<'a>(inputs: &Inputs<'a>) -> Option<(Reason, &'a Path)> {
    match (inputs.flag, inputs.activated) {
        (Some(flag), _) if flag.as_os_str().is_empty() => None,
        (Some(flag), _) => Some((Reason::Explicit, flag)),
        (None, Some(activated)) if !activated.as_os_str().is_empty() => {
            Some((Reason::Activated, activated))
        }
        (None, _) => None,
    }
}

/// Open a chosen workspace, named by its root or by its `.mindflayer`.
///
/// The folder is taken as well as the root because it is what a person
/// pointing at a workspace is often looking at, and it is the folder that
/// makes the directory a workspace. A root that holds a marker of its own is
/// a root, even when it happens to be called `.mindflayer` itself.
fn open_chosen(path: &Path) -> Result<FlayerWorkspace, WorkspaceError> {
    let path = absolute(path)?;
    let holds_marker = path.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file();
    let root = match path.parent() {
        Some(parent) if !holds_marker && path.file_name() == Some(FLAYER_DIR.as_ref()) => parent,
        _ => &path,
    };
    FlayerWorkspace::open(root)
}

#[cfg(test)]
#[path = "resolve_test.rs"]
mod tests;
