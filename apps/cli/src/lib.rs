//! Command line interface for Mindflayer, split the way the model is.
//!
//! `mind <cmd>` acts on the mind project you are standing in. `mind flayer
//! <cmd>` acts on the flayer workspace above it, and the `flayer` binary is
//! the same tree reached directly, so `flayer link x` and `mind flayer link x`
//! are one command with two spellings.
//!
//! The parser, the work and the rendering all live here rather than in the
//! binaries so the tests drive the real command surface instead of shelling
//! out, and so both binaries cannot drift apart.

use std::path::{Path, PathBuf};

use mindflayer_core::paths;
use mindflayer_core::{FlayerWorkspace, MindProject};

pub(crate) mod commands;
pub(crate) mod error;
pub mod install;
pub(crate) mod parse;
pub(crate) mod render;
pub mod tui;

pub use commands::read::WORKSPACE;
pub use commands::{default_home, run, run_flayer_cli};
pub use error::CliError;
pub use parse::{Cli, Command, FlayerCli, FlayerCommand, GatherCommand};
use render::as_stored;
pub use render::{Failure, Outcome};

// ---------------------------------------------------------------------------
// Locating what a command acts on
// ---------------------------------------------------------------------------

/// The directory this invocation works in.
fn working_directory(path: Option<&Path>) -> Result<PathBuf, CliError> {
    match path {
        Some(dir) => Ok(dir.to_path_buf()),
        None => std::env::current_dir().map_err(CliError::CurrentDirectory),
    }
}

/// Resolve a path a user typed, against the directory the command works in.
fn resolve(directory: &Path, path: &Path) -> Result<PathBuf, CliError> {
    let joined = directory.join(path);
    let absolute = std::path::absolute(&joined).map_err(|source| CliError::Resolve {
        path: joined.clone(),
        source,
    })?;
    Ok(paths::normalize(&absolute))
}

/// The mind project the caller is standing in.
fn project_here(directory: &Path) -> Result<MindProject, CliError> {
    MindProject::locate(directory)?.ok_or_else(|| CliError::NotInProject(directory.to_path_buf()))
}

/// The flayer workspace the caller is standing in: the one at or above
/// `directory`, or else the default one in `home`.
fn workspace_here(directory: &Path, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_default(directory, home)?
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// Every path a command was given, resolved and each named once: the same
/// project typed twice is one project.
fn targets(directory: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut resolved: Vec<PathBuf> = Vec::new();
    for path in paths {
        let target = resolve(directory, path)?;
        if !resolved.contains(&target) {
            resolved.push(target);
        }
    }
    Ok(resolved)
}

/// A command given several things refuses them all when any is wrong, and
/// says why about each one rather than only the first.
fn all_or_nothing(mut refused: Vec<CliError>) -> Option<Failure> {
    if refused.is_empty() {
        return None;
    }
    let first = refused.remove(0);
    Some(Failure {
        error: Box::new(first),
        warnings: refused
            .iter()
            .map(|error| format!("error: {error}"))
            .collect(),
    })
}

/// The workspace here and the projects it manages.
///
/// Only the registered ones. A workspace manages what it was told to manage,
/// so a project that happens to sit inside it is not in scope until it is
/// linked — otherwise `flayer list` would answer a different question
/// depending on which directory it was run from.
fn managed(
    directory: &Path,
    home: Option<&Path>,
) -> Result<(FlayerWorkspace, Vec<MindProject>, Vec<String>), CliError> {
    let workspace = workspace_here(directory, home)?;
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    Ok((workspace, projects, warnings))
}

/// The workspace above a project, whether or not it manages it yet: where
/// `mind link` registers it — the default one in `home` when there is none
/// up the tree.
fn workspace_above(
    project: &MindProject,
    home: Option<&Path>,
) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_default(project.root(), home)?
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// The workspace above a project that manages it: the one whose ledger
/// records what is installed there.
fn managing(project: &MindProject, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    let workspace = workspace_above(project, home)?;
    if !workspace.is_linked(project.root()) {
        return Err(CliError::NotManaged {
            project: project.root().to_path_buf(),
            workspace: workspace.root().to_path_buf(),
        });
    }
    Ok(workspace)
}

/// What a workspace command changes: the workspace's own artifacts, or the
/// one project `-p` names.
fn holder(workspace: &FlayerWorkspace, project: Option<&str>) -> Result<MindProject, CliError> {
    match project {
        None => Ok(workspace.own()),
        Some(wanted) => pick(workspace, wanted),
    }
}

/// The one managed project `wanted` names: by its name, by its entry as the
/// marker spells it, or by its path from the workspace.
fn pick(workspace: &FlayerWorkspace, wanted: &str) -> Result<MindProject, CliError> {
    let target = paths::normalize(&workspace.root().join(wanted));
    let mut found: Vec<MindProject> = Vec::new();
    let mut broken = None;
    for member in workspace.members() {
        let spelled = as_stored(&member.entry) == wanted || member.root == target;
        match member.project {
            // Two spellings of one directory are one project, not a choice.
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
        // Named exactly, but it does not open: why is more use than "no
        // such project".
        0 => Err(match broken {
            Some(error) => CliError::Workspace(error),
            None => CliError::UnknownProject {
                wanted: wanted.to_owned(),
                known: workspace
                    .members()
                    .into_iter()
                    .filter_map(|member| member.project.ok())
                    .map(|project| project.name().to_owned())
                    .collect(),
            },
        }),
        _ => Err(CliError::AmbiguousProject {
            wanted: wanted.to_owned(),
            matches: found
                .iter()
                .map(|project| as_stored(&workspace.entry_for(project.root())))
                .collect(),
        }),
    }
}
