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

use mindflayer_core::{paths, Catalog, FlayerWorkspace, MindProject, Reference};

use crate::homes::default_home;
use crate::render::as_stored;
use crate::{tui, Cli, CliError, Command, Failure, FlayerCli, FlayerCommand, Outcome};
use artifact::{add_to, edit_in, remove_from, rename_in, templates_of};
use gather::run_gather;
use install::{install_named, uninstall_named};
use project::{
    init_project, init_workspace, link_here, link_projects, scan_workspace, unlink_here,
    unlink_projects,
};
use read::{
    everywhere, everywhere_and_the_shelf, list, list_workspace, one_project, show, validate,
    wanted, Level, Selector,
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
        } => {
            let project = project_here(&directory)?;
            match template {
                // The workspace lends the template, so one that cannot be
                // read refuses the add: passed over, it would let a farther
                // template of the same name, a built-in one say, stand in
                // for its own and write the artifact from that.
                Some(template) => {
                    let workspace = consulted(&project, home)?;
                    add_to(
                        workspace.as_ref(),
                        &project,
                        *kind,
                        name,
                        description,
                        Some(template),
                    )
                }
                None => consulting(&project, home, |workspace| {
                    add_to(workspace, &project, *kind, name, description, None)
                }),
            }
        }
        Command::Edit { reference, editor } => {
            let project = project_here(&directory)?;
            consulting(&project, home, |workspace| {
                edit_in(workspace, &project, reference, editor.as_deref())
            })
        }
        Command::Rename { reference, to } => {
            let project = project_here(&directory)?;
            consulting(&project, home, |workspace| {
                rename_in(workspace, &project, reference, to)
            })
        }
        Command::Remove { reference, yes } => {
            let project = project_here(&directory)?;
            consulting(&project, home, |workspace| {
                remove_from(workspace, &project, reference, *yes)
            })
        }
        Command::Templates => {
            let project = project_here(&directory)?;
            consulting(&project, home, |workspace| {
                templates_of(workspace, &project)
            })
        }
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
        Command::Link => link_here(&directory, home),
        Command::Unlink => unlink_here(&directory, home),
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
                Some(&workspace),
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
                Some(&workspace),
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
            rename_in(
                Some(&workspace),
                &holder(&workspace, project.as_deref())?,
                reference,
                to,
            )
        }
        FlayerCommand::Remove {
            reference,
            project,
            yes,
        } => {
            let workspace = workspace_here(directory, home)?;
            remove_from(
                Some(&workspace),
                &holder(&workspace, project.as_deref())?,
                reference,
                *yes,
            )
        }
        FlayerCommand::Templates { project } => reading_workspace(directory, home, |workspace| {
            templates_of(Some(workspace), &holder(workspace, project.as_deref())?)
        }),

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
            reading_workspace(directory, home, |workspace| Ok(scan_workspace(workspace)))
        }

        FlayerCommand::Gather(command) => run_gather(command, directory, home),

        FlayerCommand::Install {
            project,
            name,
            from,
        } => match (name, project) {
            // The screen reads until something on it is applied, so it is
            // opened the way a read is: on the default workspace before it
            // is made too, which manages nothing to install into.
            (None, None) => reading_workspace(directory, home, crate::install::run),
            (Some(name), Some(project)) => {
                let workspace = workspace_here(directory, home)?;
                install_named(
                    &workspace,
                    &pick(&workspace, project)?,
                    name,
                    from.as_deref(),
                    directory,
                )
            }
            // Half of one: a skill with no project to put it in, or a
            // project with nothing named to put there.
            (Some(_), None) => Err(CliError::InstallWhere.into()),
            (None, Some(_)) => Err(CliError::InstallWhat.into()),
        },
        FlayerCommand::Uninstall { project, name } => {
            let workspace = workspace_here(directory, home)?;
            uninstall_named(&workspace, &pick(&workspace, project)?, name)
        }

        FlayerCommand::Tui => tui::run(directory, home),

        FlayerCommand::List { kind, project } => {
            reading_workspace(directory, home, |workspace| match project {
                Some(project) => one_project(
                    pick(workspace, project)?,
                    wanted(*kind),
                    |catalog, projects, level| Ok(list(catalog, projects, level)),
                ),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere(
                        workspace,
                        warnings,
                        &projects,
                        wanted(*kind),
                        |catalog, projects, level| {
                            Ok(list_workspace(catalog, projects, workspace, level))
                        },
                    )
                }
            })
        }
        FlayerCommand::Show { reference, project } => {
            let reference = Reference::parse(reference);
            let kinds = wanted(reference.kind());
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                show(catalog, &reference, level)
            };
            reading_workspace(directory, home, |workspace| match project {
                Some(project) => one_project(pick(workspace, project)?, kinds, read),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere(workspace, warnings, &projects, kinds, read)
                }
            })
        }
        FlayerCommand::Validate { target, project } => {
            let selector = Selector::parse(target.as_deref());
            let kinds = selector.kinds();
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                validate(catalog, &selector, level)
            };
            reading_workspace(directory, home, |workspace| match project {
                Some(project) => one_project(pick(workspace, project)?, kinds, read),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere_and_the_shelf(workspace, warnings, &projects, kinds, read)
                }
            })
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
/// `directory`, or else the default one in `home`, made there the first time
/// a command that writes needs it. A command that only reads asks
/// [`workspace_to_read`] instead.
fn workspace_here(directory: &Path, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_default(directory, home)?
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// The flayer workspace a command that only reads looks at: the one at or
/// above `directory`, or else the default one in `home` — made, or as it
/// will be once something makes it.
///
/// Never made for the occasion. A read is no reason to write a marker into
/// somebody's home, and the workspace that would be made there answers every
/// read as the made one would a moment later.
fn workspace_to_read(directory: &Path, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_unmade(directory, home)?
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// Run a read on the workspace [`workspace_to_read`] finds, with one line
/// more when that has not been made: where it will be.
///
/// A hint rather than a warning, and so no reason to fail: nothing is wrong
/// with a workspace nobody has needed yet, and the read says what it would
/// say of an empty one.
fn reading_workspace(
    directory: &Path,
    home: Option<&Path>,
    read: impl FnOnce(&FlayerWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    reading(&workspace_to_read(directory, home)?, read)
}

/// Run a read on a workspace found some other way, with the same one line
/// more when it has not been made.
fn reading(
    workspace: &FlayerWorkspace,
    read: impl FnOnce(&FlayerWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    let mut result = read(workspace);
    if !workspace.is_made() {
        let hint = unmade_hint(workspace);
        match &mut result {
            Ok(outcome) => outcome.stderr.push(hint),
            Err(failure) => failure.warnings.push(hint),
        }
    }
    result
}

/// What a read says about a default workspace nothing has made yet: that
/// it is not there, and where the first command that writes will put it.
fn unmade_hint(workspace: &FlayerWorkspace) -> String {
    format!(
        "hint: no workspace here or above, and the default one is not made yet; \
         the first command that writes to it, such as `flayer link <path>`, makes it in {}",
        workspace.flayer_dir().display()
    )
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

/// The projects a workspace manages, and a warning for each entry that
/// would not open.
///
/// Only the registered ones. A workspace manages what it was told to manage,
/// so a project that happens to sit inside it is not in scope until it is
/// linked — otherwise `flayer list` would answer a different question
/// depending on which directory it was run from.
fn managed(workspace: &FlayerWorkspace) -> (Vec<MindProject>, Vec<String>) {
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    (projects, warnings)
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

/// The workspace a `mind` command consults about the project it changes:
/// the one above it, or else the default one in `home` — the order `mind
/// link` and `mind install` look in.
///
/// The default only if it has been made: a change to a project is no reason
/// to write a marker into somebody's home, and a workspace made just now
/// holds no record to drop and no template to offer.
///
/// The TUI asks it too, for the workspace `mind` opens its screen on and for
/// the templates its add form offers a project, so both are what the `mind`
/// lines it runs will find.
pub(crate) fn consulted(
    project: &MindProject,
    home: Option<&Path>,
) -> Result<Option<FlayerWorkspace>, CliError> {
    match FlayerWorkspace::locate(project.root())? {
        Some(workspace) => Ok(Some(workspace)),
        None => Ok(FlayerWorkspace::default_at(home)?),
    }
}

/// Make a change to `project` with the workspace [`consulted`] about it.
///
/// One that cannot be read is a warning after the change rather than a
/// refusal of it: the change is the project's, and the workspace was only to
/// be told. Not so for `mind add --template`, where the workspace lends what
/// the change is made from, which is why that asks [`consulted`] itself.
fn consulting(
    project: &MindProject,
    home: Option<&Path>,
    change: impl FnOnce(Option<&FlayerWorkspace>) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(project, home) {
        Ok(workspace) => (workspace, None),
        Err(error) => (None, Some(format!("warning: {error}"))),
    };
    let mut result = change(workspace.as_ref());
    if let Some(warning) = unread {
        match &mut result {
            Ok(outcome) => {
                outcome.stderr.push(warning);
                outcome.ok = false;
            }
            Err(failure) => failure.warnings.push(warning),
        }
    }
    result
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
