//! What each command does: `run` and `run_flayer_cli` dispatch a parsed
//! command line, and each family of commands has a module of its own.
//!
//! Split by family so that a change to one command touches one file, and two
//! pieces of work on different commands do not collide in the same one.

pub(crate) mod artifact;
pub(crate) mod gather;
pub(crate) mod install;
pub(crate) mod project;
pub(crate) mod read;
pub(crate) mod workspace;

use std::path::{Path, PathBuf};

use mindflayer_core::{paths, scan, Catalog, FlayerWorkspace, MindProject, Reference};

use crate::render::as_stored;
use crate::{tui, Cli, CliError, Command, Failure, FlayerCli, FlayerCommand, Outcome};
use artifact::{add_to, edit_in, remove_from, rename_in, templates_of};
use gather::run_gather;
use install::{install_named, uninstall_named};
use project::{
    init_project, init_workspace, link_projects, linked, scanned, unlink_projects, unlinked,
};
use read::{
    everywhere, list, list_workspace, one_project, show, validate, wanted, Level, Selector,
};
use workspace::{load_command, unload_command};

/// Run `mind`.
pub fn run(cli: &Cli) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    let home = default_home(cli.home.as_deref());
    let home = home.as_deref();
    match cli.command.as_ref().unwrap_or(&Command::Tui) {
        Command::Flayer { command } => run_flayer(
            command.as_ref().unwrap_or(&FlayerCommand::Tui),
            &directory,
            home,
        ),
        Command::Init { skills, rules } => {
            init_project(&directory, skills.as_deref(), rules.as_deref())
        }
        Command::Add {
            kind,
            name,
            description,
            template,
        } => add_to(
            &project_here(&directory)?,
            *kind,
            name,
            description,
            template.as_deref(),
        ),
        Command::Edit { reference, editor } => {
            edit_in(&project_here(&directory)?, reference, editor.as_deref())
        }
        Command::Rename { reference, to } => rename_in(&project_here(&directory)?, reference, to),
        Command::Remove { reference, yes } => {
            remove_from(&project_here(&directory)?, reference, *yes)
        }
        Command::Templates => templates_of(&project_here(&directory)?),
        Command::List { kind } => {
            let project = project_here(&directory)?;
            one_project(project, wanted(*kind), |catalog, projects, level| {
                Ok(list(catalog, projects, level))
            })
        }
        Command::Show { reference } => {
            let project = project_here(&directory)?;
            let reference = Reference::parse(reference);
            one_project(project, wanted(reference.kind()), |catalog, _, level| {
                show(catalog, &reference, level)
            })
        }
        Command::Validate { target } => {
            let project = project_here(&directory)?;
            let selector = Selector::parse(target.as_deref());
            one_project(project, selector.kinds(), |catalog, _, level| {
                validate(catalog, &selector, level)
            })
        }
        Command::Link => {
            let project = project_here(&directory)?;
            let mut workspace = workspace_above(&project, home)?;
            let (entry, registration) = workspace.link(&project)?;
            Ok(Outcome::plain(linked(project.name(), &entry, registration)))
        }
        Command::Unlink => {
            let project = project_here(&directory)?;
            let mut workspace = workspace_above(&project, home)?;
            let removed = workspace.unlink(project.root())?;
            Ok(Outcome::plain(unlinked(&removed)))
        }
        Command::Load { workspaces } => {
            let project = project_here(&directory)?;
            load_command(&directory, project.root(), workspaces, home)
        }
        Command::Unload { workspaces } => {
            let project = project_here(&directory)?;
            unload_command(&directory, project.root(), workspaces, home)
        }
        Command::Install { name, from } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, home)?;
            install_named(&workspace, &project, name, from.as_deref(), &directory)
        }
        Command::Uninstall { name } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, home)?;
            uninstall_named(&workspace, &project, name)
        }
        Command::Tui => tui::run_project(&directory, home),
    }
}

/// Run `flayer`, which is the same as running `mind flayer`.
pub fn run_flayer_cli(cli: &FlayerCli) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    let home = default_home(cli.home.as_deref());
    run_flayer(
        cli.command.as_ref().unwrap_or(&FlayerCommand::Tui),
        &directory,
        home.as_deref(),
    )
}

/// Where the default workspace lives: `--home`, then `$MINDFLAYER_HOME`, then
/// the user's home directory. Empty, from either, means there is none.
///
/// A string rather than a path because empty is a meaningful answer here,
/// and one a path cannot spell.
pub fn default_home(given: Option<&str>) -> Option<PathBuf> {
    let chosen = match given {
        Some(given) => given.to_owned(),
        None => match std::env::var("MINDFLAYER_HOME") {
            Ok(from_environment) => from_environment,
            Err(_) => return std::env::home_dir(),
        },
    };
    (!chosen.is_empty()).then(|| PathBuf::from(chosen))
}

/// The workspace half, shared by `mind flayer <cmd>` and `flayer <cmd>`.
///
/// Every command that changes an artifact acts on the workspace's own unless
/// `-p` names one of its projects, and is the same function `mind` runs for
/// that project: the two levels differ in what they hold, never in the rules.
fn run_flayer(
    command: &FlayerCommand,
    directory: &Path,
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    match command {
        FlayerCommand::Init { skills, rules } => {
            init_workspace(directory, skills.as_deref(), rules.as_deref())
        }

        FlayerCommand::Add {
            project,
            kind,
            name,
            description,
            template,
        } => {
            let workspace = workspace_here(directory, home)?;
            add_to(
                &holder(&workspace, project.as_deref())?,
                *kind,
                name,
                description,
                template.as_deref(),
            )
        }
        FlayerCommand::Edit {
            reference,
            project,
            editor,
        } => {
            let workspace = workspace_here(directory, home)?;
            edit_in(
                &holder(&workspace, project.as_deref())?,
                reference,
                editor.as_deref(),
            )
        }
        FlayerCommand::Rename {
            reference,
            to,
            project,
        } => {
            let workspace = workspace_here(directory, home)?;
            rename_in(&holder(&workspace, project.as_deref())?, reference, to)
        }
        FlayerCommand::Remove {
            reference,
            project,
            yes,
        } => {
            let workspace = workspace_here(directory, home)?;
            remove_from(&holder(&workspace, project.as_deref())?, reference, *yes)
        }
        FlayerCommand::Templates { project } => {
            let workspace = workspace_here(directory, home)?;
            templates_of(&holder(&workspace, project.as_deref())?)
        }

        FlayerCommand::Link { projects } => link_projects(directory, projects, home),
        FlayerCommand::Unlink { projects } => unlink_projects(directory, projects, home),

        FlayerCommand::Load { workspaces } => load_command(directory, directory, workspaces, home),
        FlayerCommand::Unload { workspaces } => {
            unload_command(directory, directory, workspaces, home)
        }

        FlayerCommand::Minds => {
            let workspace = workspace_here(directory, home)?;
            tui::run_minds(workspace.root(), home)
        }

        FlayerCommand::Scan => {
            let workspace = workspace_here(directory, home)?;
            Ok(scanned(&workspace, &scan(&workspace)))
        }

        FlayerCommand::Gather(command) => run_gather(command, directory, home),

        FlayerCommand::Install {
            project,
            name,
            from,
        } => {
            let workspace = workspace_here(directory, home)?;
            match (name, project) {
                (None, None) => {
                    let ledger = workspace.ledger().map_err(CliError::from)?;
                    crate::install::run(&workspace, &ledger)
                }
                (Some(name), Some(project)) => install_named(
                    &workspace,
                    &pick(&workspace, project)?,
                    name,
                    from.as_deref(),
                    directory,
                ),
                // Half of one: a skill with no project to put it in, or a
                // project with nothing named to put there.
                (Some(_), None) => Err(CliError::InstallWhere.into()),
                (None, Some(_)) => Err(CliError::InstallWhat.into()),
            }
        }
        FlayerCommand::Uninstall { project, name } => {
            let workspace = workspace_here(directory, home)?;
            uninstall_named(&workspace, &pick(&workspace, project)?, name)
        }

        FlayerCommand::Tui => tui::run(directory, home),

        FlayerCommand::List { kind, project } => match project {
            Some(project) => {
                let workspace = workspace_here(directory, home)?;
                one_project(
                    pick(&workspace, project)?,
                    wanted(*kind),
                    |catalog, projects, level| Ok(list(catalog, projects, level)),
                )
            }
            None => {
                let (workspace, projects, warnings) = managed(directory, home)?;
                everywhere(
                    &workspace,
                    warnings,
                    &projects,
                    wanted(*kind),
                    |catalog, projects, level| {
                        Ok(list_workspace(catalog, projects, &workspace, level))
                    },
                )
            }
        },
        FlayerCommand::Show { reference, project } => {
            let reference = Reference::parse(reference);
            let kinds = wanted(reference.kind());
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                show(catalog, &reference, level)
            };
            match project {
                Some(project) => {
                    let workspace = workspace_here(directory, home)?;
                    one_project(pick(&workspace, project)?, kinds, read)
                }
                None => {
                    let (workspace, projects, warnings) = managed(directory, home)?;
                    everywhere(&workspace, warnings, &projects, kinds, read)
                }
            }
        }
        FlayerCommand::Validate { target, project } => {
            let selector = Selector::parse(target.as_deref());
            let kinds = selector.kinds();
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                validate(catalog, &selector, level)
            };
            match project {
                Some(project) => {
                    let workspace = workspace_here(directory, home)?;
                    one_project(pick(&workspace, project)?, kinds, read)
                }
                None => {
                    let (workspace, projects, warnings) = managed(directory, home)?;
                    everywhere(&workspace, warnings, &projects, kinds, read)
                }
            }
        }
    }
}

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
