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

use crate::paths;
use crate::workspace::{
    absolute, same_directory, FlayerWorkspace, MindProject, WorkspaceError, FLAYER_CONFIG,
    FLAYER_DIR,
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
    /// is needed, so there is always somewhere to link a project into, the
    /// way there is always a global git config to write to.
    Write,
}

/// Which rule chose the workspace.
///
/// Returned beside it rather than worked out again afterwards: why a
/// workspace is the one in use is not something its path answers. The
/// directory home is can be named with `-W`, activated or found walking up
/// from below it as well as fallen back to, and only the last is what the
/// TUI means by "no workspace here or above". Whether a workspace *is* the
/// one in home is a question about the workspace rather than the rule, which
/// [`FlayerWorkspace::is_default`] answers from paths: `load` and `unload`
/// ask that, so they call it the default one however it was reached.
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

/// Why no workspace, or no project of one, could be decided on.
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
    /// The workspace resolved for a project does not link it, so its ledger
    /// has no say over what is installed there.
    #[error("{project}: not managed by the workspace at {workspace}")]
    NotManaged {
        project: PathBuf,
        workspace: PathBuf,
    },
    /// No project the workspace manages goes by that name, entry or path.
    /// `known` is what does, by name.
    #[error("no project called `{wanted}` in the workspace")]
    UnknownProject { wanted: String, known: Vec<String> },
    /// More than one does, each by its entry as the marker spells it.
    #[error("`{wanted}` names more than one project: {}", .matches.join(", "))]
    AmbiguousProject {
        wanted: String,
        matches: Vec<String>,
    },
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

/// The workspace that manages `project`: the one whose ledger records what
/// is installed there. Refused when the workspace resolved does not link the
/// project; `None` only when there is no workspace at all.
///
/// Resolved from the project's root, whatever `inputs` start from: a
/// project's workspace is the one above the project, not the one above
/// whichever of its folders a command was run in.
pub fn managing(
    inputs: &Inputs<'_>,
    project: &MindProject,
) -> Result<Option<FlayerWorkspace>, ResolveError> {
    let inputs = Inputs {
        start: project.root(),
        ..*inputs
    };
    let Some(Resolved { workspace, .. }) = resolve(&inputs)? else {
        return Ok(None);
    };
    if !workspace.is_linked(project.root()) {
        return Err(ResolveError::NotManaged {
            project: project.root().to_path_buf(),
            workspace: workspace.root().to_path_buf(),
        });
    }
    Ok(Some(workspace))
}

/// The workspace a change to `project` consults, to drop the record of
/// what it changes and to lend it templates: the one resolved from the
/// project's root, as [`managing`] resolves it, if it has been made.
///
/// Never made for the occasion, whatever `inputs` intend. A change to a
/// project is no reason to write a marker into somebody's home, and a
/// workspace made just now would hold no record to drop and no template to
/// offer.
pub fn consulted(
    inputs: &Inputs<'_>,
    project: &MindProject,
) -> Result<Option<Resolved>, ResolveError> {
    let inputs = Inputs {
        start: project.root(),
        intent: Intent::Read,
        ..*inputs
    };
    Ok(resolve(&inputs)?.filter(|resolved| resolved.workspace.is_made()))
}

impl FlayerWorkspace {
    /// What a command at the workspace's level changes: its own artifacts,
    /// or the one project it names ([`pick`](Self::pick)).
    pub fn holder(&self, project: Option<&str>) -> Result<MindProject, ResolveError> {
        match project {
            None => Ok(self.own()),
            Some(wanted) => self.pick(wanted),
        }
    }

    /// The one project this workspace manages that `wanted` names: by its
    /// name, by its entry as the marker spells it, or by its path from the
    /// workspace's root.
    ///
    /// Two spellings of one directory are one project rather than a choice
    /// between two. A project named exactly that will not open is refused
    /// with why, which is more use than being told there is no such project.
    pub fn pick(&self, wanted: &str) -> Result<MindProject, ResolveError> {
        let target = paths::normalize(&self.root().join(wanted));
        let mut found: Vec<MindProject> = Vec::new();
        let mut broken = None;
        for member in self.members() {
            let spelled = stored(&member.entry) == wanted || member.root == target;
            match member.project {
                Ok(project) if spelled || project.name() == wanted => {
                    if !found.iter().any(|other| other.root() == project.root()) {
                        found.push(project);
                    }
                }
                Err(error) if spelled => broken = Some(error),
                _ => {}
            }
        }
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(match broken {
                Some(error) => ResolveError::Workspace(error),
                None => ResolveError::UnknownProject {
                    wanted: wanted.to_owned(),
                    known: self
                        .members()
                        .into_iter()
                        .filter_map(|member| member.project.ok())
                        .map(|project| project.name().to_owned())
                        .collect(),
                },
            }),
            _ => Err(ResolveError::AmbiguousProject {
                wanted: wanted.to_owned(),
                matches: found
                    .iter()
                    .map(|project| stored(&self.entry_for(project.root())))
                    .collect(),
            }),
        }
    }
}

/// An entry as the marker spells it, `/`-separated on every platform.
fn stored(entry: &Path) -> String {
    paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
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
