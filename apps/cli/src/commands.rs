//! What each command does: `run` and `run_flayer_cli` dispatch a parsed
//! command line, and each family of commands has a module of its own.
//!
//! Split by family so that a change to one command touches one file, and two
//! pieces of work on different commands do not collide in the same one.

pub(crate) mod activate;
pub(crate) mod artifact;
pub(crate) mod gather;
pub(crate) mod install;
pub(crate) mod migrate;
pub(crate) mod project;
pub(crate) mod read;
pub(crate) mod status;
pub(crate) mod workspace;

use std::path::{Path, PathBuf};

use mindflayer_core::gather::Watch;
use mindflayer_core::resolve::{self, resolve, Intent};
use mindflayer_core::{paths, Catalog, FlayerWorkspace, MindProject, Reference, Share};

use crate::homes::Lookup;
use crate::{tui, Cli, CliError, Command, Failure, FlayerCli, FlayerCommand, Outcome};
use activate::{activate_command, which_command};
use artifact::{add_to, edit_in, remove_from, rename_in, templates_of};
use gather::run_gather;
use install::{install_named, uninstall_named};
use migrate::migrate_library;
use project::{
    init_project, init_workspace, link_here, link_projects, scan_workspace, unlink_here,
    unlink_projects,
};
use read::{
    everywhere, everywhere_and_the_shelf, list, list_workspace, one_project, show, validate,
    wanted, Level, Selector,
};
use status::{status_here, status_of_project, status_of_workspace};
use workspace::{load_command, unload_command};

/// Run `mind`.
pub fn run(cli: &Cli) -> Result<Outcome, Failure> {
    run_watched(cli, &Watch::new())
}

/// Run `mind`, telling `watch` how a gather goes and stopping one when it is
/// cancelled: how a front end that shows progress, or can stop what it
/// started, runs the same line `run` does.
///
/// A parameter beside the parsed line rather than a field of it, because it
/// is not something a person types: the line stays exactly what was typed.
pub fn run_watched(cli: &Cli, watch: &Watch) -> Result<Outcome, Failure> {
    run_with(
        cli,
        &Lookup::given(
            cli.home.as_deref(),
            cli.workspace.as_deref(),
            &cli.agent_homes,
        ),
        watch,
    )
}

/// Run `mind` with where to look for a workspace already decided: what
/// [`run_watched`] does once it has read that from the command line and the
/// environment, and what the TUI does with its own, so the lines it runs look
/// where its screen did. What `cli` says about it is not read again.
pub(crate) fn run_with(cli: &Cli, lookup: &Lookup, watch: &Watch) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    match cli.command.as_ref().unwrap_or(&Command::Tui) {
        Command::Flayer { command } => run_flayer(
            command.as_ref().unwrap_or(&FlayerCommand::Tui),
            &directory,
            lookup,
            watch,
        ),
        Command::Init {
            agents,
            shared,
            skills,
            rules,
        } => init_project(
            &directory,
            agents,
            if *shared { Share::Shared } else { Share::Local },
            skills.as_deref(),
            rules.as_deref(),
        ),
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
                    let workspace = consulted(&project, lookup)?;
                    add_to(
                        workspace.as_ref(),
                        &project,
                        *kind,
                        name,
                        description,
                        Some(template),
                    )
                }
                None => consulting(&project, lookup, |workspace| {
                    add_to(workspace, &project, *kind, name, description, None)
                }),
            }
        }
        Command::Edit { reference, editor } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                edit_in(workspace, &project, reference, editor.as_deref())
            })
        }
        Command::Rename { reference, to } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                rename_in(workspace, &project, reference, to)
            })
        }
        Command::Remove { reference, yes } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                remove_from(workspace, &project, reference, *yes)
            })
        }
        Command::Templates => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
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
        Command::Status { json } => status_here(project_here(&directory)?, lookup, *json),
        Command::Link => link_here(&directory, lookup),
        Command::Unlink => unlink_here(&directory, lookup),
        Command::Load { workspaces } => {
            let project = project_here(&directory)?;
            load_command(&directory, project.root(), workspaces, lookup)
        }
        Command::Unload { workspaces } => {
            let project = project_here(&directory)?;
            unload_command(&directory, project.root(), workspaces, lookup)
        }
        Command::Install { name, from } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, lookup)?;
            install_named(
                &workspace,
                &project,
                name,
                from.as_deref(),
                &directory,
                &lookup.agents,
            )
        }
        Command::Uninstall { name } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, lookup)?;
            uninstall_named(&workspace, &project, name)
        }
        Command::Tui => tui::run_project(&directory, lookup),
    }
}

/// Run `flayer`, which is the same as running `mind flayer`.
pub fn run_flayer_cli(cli: &FlayerCli) -> Result<Outcome, Failure> {
    run_flayer_cli_watched(cli, &Watch::new())
}

/// Run `flayer` with a gather's progress and cancel going through `watch`,
/// as [`run_watched`] does for `mind`.
pub fn run_flayer_cli_watched(cli: &FlayerCli, watch: &Watch) -> Result<Outcome, Failure> {
    run_flayer_with(
        cli,
        &Lookup::given(
            cli.home.as_deref(),
            cli.workspace.as_deref(),
            &cli.agent_homes,
        ),
        watch,
    )
}

/// [`run_with`], for `flayer`.
pub(crate) fn run_flayer_with(
    cli: &FlayerCli,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    run_flayer(
        cli.command.as_ref().unwrap_or(&FlayerCommand::Tui),
        &directory,
        lookup,
        watch,
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
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    match command {
        // Typed from the directory the command works in, as `link`'s paths
        // are, rather than from where it was run.
        FlayerCommand::Init {
            path,
            skills,
            rules,
        } => {
            let root = match path {
                Some(path) => typed(directory, path)?,
                None => directory.to_path_buf(),
            };
            init_workspace(&root, skills.as_deref(), rules.as_deref())
        }
        FlayerCommand::Activate { path, shell } => {
            activate_command(directory, path.as_deref(), *shell, lookup)
        }
        FlayerCommand::Which => which_command(directory, lookup),

        FlayerCommand::Add {
            project,
            kind,
            name,
            description,
            template,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            add_to(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
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
            let workspace = workspace_here(directory, lookup)?;
            edit_in(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                reference,
                editor.as_deref(),
            )
        }
        FlayerCommand::Rename {
            reference,
            to,
            project,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            rename_in(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                reference,
                to,
            )
        }
        FlayerCommand::Remove {
            reference,
            project,
            yes,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            remove_from(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                reference,
                *yes,
            )
        }
        FlayerCommand::Templates { project } => reading_workspace(directory, lookup, |workspace| {
            templates_of(Some(workspace), &workspace.holder(project.as_deref())?)
        }),

        FlayerCommand::Link { projects } => link_projects(directory, projects, lookup),
        FlayerCommand::Unlink { projects } => unlink_projects(directory, projects, lookup),

        FlayerCommand::Load { workspaces } => {
            load_command(directory, directory, workspaces, lookup)
        }
        FlayerCommand::Unload { workspaces } => {
            unload_command(directory, directory, workspaces, lookup)
        }

        // A screen, so it opens as one: on the default workspace before it
        // is made too, which its first change makes.
        FlayerCommand::Minds => tui::run_minds(directory, lookup),

        FlayerCommand::Scan => {
            reading_workspace(directory, lookup, |workspace| Ok(scan_workspace(workspace)))
        }

        FlayerCommand::Gather(command) => run_gather(command, directory, lookup, watch),

        FlayerCommand::Install {
            project,
            name,
            from,
        } => match (name, project) {
            // The screen reads until something on it is applied, so it is
            // opened the way a read is: on the default workspace before it
            // is made too, which manages nothing to install into.
            (None, None) => reading_workspace(directory, lookup, |workspace| {
                crate::install::run(workspace, &lookup.agents)
            }),
            (Some(name), Some(project)) => {
                let workspace = workspace_here(directory, lookup)?;
                install_named(
                    &workspace,
                    &workspace.pick(project)?,
                    name,
                    from.as_deref(),
                    directory,
                    &lookup.agents,
                )
            }
            // Half of one: a skill with no project to put it in, or a
            // project with nothing named to put there.
            (Some(_), None) => Err(CliError::InstallWhere.into()),
            (None, Some(_)) => Err(CliError::InstallWhat.into()),
        },
        FlayerCommand::Uninstall { project, name } => {
            let workspace = workspace_here(directory, lookup)?;
            uninstall_named(&workspace, &workspace.pick(project)?, name)
        }

        // Resolved as a read, on the default workspace before it is made
        // too: a workspace made for the occasion keeps its own in its
        // library from the start, and has nothing to move.
        FlayerCommand::Migrate { library: _, yes } => {
            reading_workspace(directory, lookup, |workspace| {
                migrate_library(workspace, *yes)
            })
        }

        FlayerCommand::Tui => tui::run(directory, lookup),

        FlayerCommand::List { kind, project } => {
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => one_project(
                    workspace.pick(project)?,
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
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => one_project(workspace.pick(project)?, kinds, read),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere(workspace, warnings, &projects, kinds, read)
                }
            })
        }
        // Read as `list` reads, on the default workspace before it is made
        // too, and from a ledger read without being made: a workspace that
        // has installed nothing has none, and a report is no reason to
        // create one.
        FlayerCommand::Status { project, json } => {
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => Ok(status_of_project(
                    workspace,
                    workspace.pick(project)?,
                    *json,
                )),
                None => Ok(status_of_workspace(workspace, *json)),
            })
        }
        FlayerCommand::Validate { target, project } => {
            let selector = Selector::parse(target.as_deref());
            let kinds = selector.kinds();
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                validate(catalog, &selector, level)
            };
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => one_project(workspace.pick(project)?, kinds, read),
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

/// A path a user typed, made absolute against the directory the command
/// works in.
fn typed(directory: &Path, path: &Path) -> Result<PathBuf, CliError> {
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

/// The flayer workspace a command that writes acts on: the one `lookup`
/// resolves from `directory`, which is the default one in home, made there
/// the first time a command needs it, when nothing chooses another. A
/// command that only reads asks [`workspace_to_read`] instead.
fn workspace_here(directory: &Path, lookup: &Lookup) -> Result<FlayerWorkspace, CliError> {
    resolve(&lookup.inputs(directory, Intent::Write))?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// The flayer workspace a command that only reads looks at: the one `lookup`
/// resolves from `directory` (the default one in home made, or as it will
/// be once something makes it).
///
/// Never made for the occasion. A read is no reason to write a marker into
/// somebody's home, and the workspace that would be made there answers every
/// read as the made one would a moment later.
fn workspace_to_read(directory: &Path, lookup: &Lookup) -> Result<FlayerWorkspace, CliError> {
    resolve(&lookup.inputs(directory, Intent::Read))?
        .map(|resolved| resolved.workspace)
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
    lookup: &Lookup,
    read: impl FnOnce(&FlayerWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    reading(&workspace_to_read(directory, lookup)?, read)
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
        let target = typed(directory, path)?;
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
/// linked; otherwise `flayer list` would answer a different question
/// depending on which directory it was run from.
fn managed(workspace: &FlayerWorkspace) -> (Vec<MindProject>, Vec<String>) {
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    (projects, warnings)
}

/// The workspace a project's own commands register it with: the one
/// `lookup` resolves from the project, whether or not it manages it yet;
/// it is where `mind link` adds it, and the default one in home when nothing
/// chooses another and there is none up the tree.
fn workspace_for(project: &MindProject, lookup: &Lookup) -> Result<FlayerWorkspace, CliError> {
    resolve(&lookup.inputs(project.root(), Intent::Write))?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// The workspace that manages a project, as core's
/// [`managing`](resolve::managing) resolves it: the one whose ledger records
/// what is installed there.
fn managing(project: &MindProject, lookup: &Lookup) -> Result<FlayerWorkspace, CliError> {
    resolve::managing(&lookup.inputs(project.root(), Intent::Write), project)?
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// The workspace a `mind` command consults about the project it changes, as
/// core's [`consulted`](resolve::consulted) resolves it: the one `mind link`
/// and `mind install` find, if it has been made.
///
/// The TUI asks it too, for the templates its add form offers a project, so
/// they are what the `mind` lines it runs will find.
pub(crate) fn consulted(
    project: &MindProject,
    lookup: &Lookup,
) -> Result<Option<FlayerWorkspace>, CliError> {
    Ok(
        resolve::consulted(&lookup.inputs(project.root(), Intent::Read), project)?
            .map(|resolved| resolved.workspace),
    )
}

/// Make a change to `project` with the workspace [`consulted`] about it.
///
/// One found that cannot be read is a warning after the change rather than a
/// refusal of it: the change is the project's, and the workspace was only to
/// be told. Not so for `mind add --template`, where the workspace lends what
/// the change is made from, which is why that asks [`consulted`] itself. Nor
/// for one `-W` or the activation names that is no workspace, which refuses
/// the change as it refuses every other command: whoever chose it meant that
/// one to be told, and a change made without it would leave its record
/// behind once the name is put right.
fn consulting(
    project: &MindProject,
    lookup: &Lookup,
    change: impl FnOnce(Option<&FlayerWorkspace>) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
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
