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

use mindflayer_core::{scan, Catalog, FlayerWorkspace, MindProject, Reference};

use crate::{
    holder, managed, managing, pick, project_here, working_directory, workspace_above,
    workspace_here,
};
use crate::{tui, Cli, CliError, Command, Failure, FlayerCli, FlayerCommand, Outcome};
use artifact::{add_to, edit_in, remove_from, rename_in, templates_of};
use gather::run_gather;
use install::{install_named, uninstall_named};
use project::{
    asked_directories, initialized, link_projects, linked, scanned, unlink_projects, unlinked,
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
            let directories = asked_directories(skills.as_deref(), rules.as_deref());
            let (project, outcome) = MindProject::init_with(&directory, &directories)?;
            Ok(Outcome::plain(initialized(
                "mind project",
                project.name(),
                &project.mind_dir(),
                outcome,
            )))
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
            let directories = asked_directories(skills.as_deref(), rules.as_deref());
            let (workspace, outcome) = FlayerWorkspace::init_with(directory, &directories)?;
            Ok(Outcome::plain(initialized(
                "flayer workspace",
                workspace.name(),
                &workspace.flayer_dir(),
                outcome,
            )))
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
