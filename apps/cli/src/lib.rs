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

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindflayer_core::gather::{self, Report, Request};
use mindflayer_core::paths;
use mindflayer_core::scan::MAX_LISTED;
use mindflayer_core::{
    scan, Catalog, Directories, FlayerWorkspace, Initialization, Kind, MindProject, Reference,
    Registration, Scan, WorkspaceError,
};

pub(crate) mod commands;
pub(crate) mod error;
pub mod install;
pub(crate) mod parse;
pub(crate) mod render;
pub mod tui;

use commands::artifact::{add_to, edit_in, remove_from, rename_in, templates_of};
pub use commands::read::WORKSPACE;
use commands::read::{
    everywhere, list, list_workspace, one_project, show, validate, wanted, Level, Selector,
};
pub use error::CliError;
pub use parse::{Cli, Command, FlayerCli, FlayerCommand, GatherCommand};
use render::{as_stored, indent, plural, printable, table};
pub use render::{Failure, Outcome};

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
                    install::run(&workspace, &ledger)
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

/// What `init` was asked to put where, at either level.
///
/// Only what was asked for is set. A kind nobody mentioned is left to the
/// default rather than pinned to it here, so the default lives in one place.
fn asked_directories(skills: Option<&Path>, rules: Option<&Path>) -> Directories {
    let mut directories = Directories::default();
    if let Some(skills) = skills {
        directories = directories.with(Kind::Skill, skills);
    }
    if let Some(rules) = rules {
        directories = directories.with(Kind::Rule, rules);
    }
    directories
}

// ---------------------------------------------------------------------------
// Registering, and installing into, one project
// ---------------------------------------------------------------------------

/// `flayer link`: every path given, registered with the workspace here.
fn link_projects(
    directory: &Path,
    projects: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, home)?;
    // All or nothing: every path is opened before any is registered,
    // so a typo in the third does not leave the first two linked by a
    // command that reports failure. Opening is the check — a
    // directory with no `.mind` is not a project, and registering one
    // would only fail later, further from the command that caused it.
    let mut opened: Vec<MindProject> = Vec::new();
    let mut refused = Vec::new();
    for target in targets(directory, projects)? {
        match MindProject::open(&target) {
            Ok(project) => opened.push(project),
            Err(error) => refused.push(CliError::from(error)),
        }
    }
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut text = String::new();
    for project in &opened {
        let (entry, registration) = workspace.link(project)?;
        text.push_str(&linked(project.name(), &entry, registration));
    }
    Ok(Outcome::plain(text))
}

/// `flayer unlink`: every path given, dropped from the workspace here.
fn unlink_projects(
    directory: &Path,
    projects: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, home)?;
    let targets = targets(directory, projects)?;
    // All or nothing, for the reason `link` is: nothing is dropped
    // unless everything named is registered.
    let refused = targets
        .iter()
        .filter(|target| !workspace.is_linked(target))
        .map(|target| {
            CliError::Workspace(WorkspaceError::NotRegistered {
                path: target.clone(),
                workspace: workspace.root().to_path_buf(),
            })
        })
        .collect();
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    // One rewrite for all of them: two spellings of one project are
    // one project, not a second one to fail on halfway.
    Ok(Outcome::plain(unlinked(&workspace.unlink_all(&targets)?)))
}

/// What `link` says it did, at either level.
fn linked(name: &str, entry: &Path, registration: Registration) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!("linked {name} as {entry}\n"),
        Registration::AlreadyRegistered => format!("{name} is already linked as {entry}\n"),
    }
}

/// What `unlink` says it did: every spelling that pointed at the project, not
/// just the first. Reporting "unlinked" while one entry still registers it is
/// the failure this reports its way out of.
fn unlinked(removed: &[PathBuf]) -> String {
    removed.iter().fold(String::new(), |mut text, entry| {
        let _ = writeln!(text, "unlinked {}", as_stored(entry));
        text
    })
}

/// `load`, at either level: the paths are typed from `directory`, and the
/// workspace they go into is found from `start` — the directory itself for
/// `flayer`, the project for `mind`. With no paths it lists what that
/// workspace loads, without making the default one to say it loads nothing.
fn load_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    if workspaces.is_empty() {
        let workspace = FlayerWorkspace::locate_except(start, &[], home)?.ok_or_else(|| {
            CliError::NowhereToLoad {
                from: start.to_path_buf(),
                verb: "list the loads of",
                named: false,
            }
        })?;
        return Ok(loaded_listing(&workspace, home));
    }
    let sources = targets(directory, workspaces)?;
    // Every source opened before anything is found or made: a typo must not
    // leave a default workspace behind in your home.
    let mut opened = Vec::new();
    let mut refused = Vec::new();
    for source in &sources {
        match FlayerWorkspace::open(source) {
            Ok(source) => opened.push(source),
            Err(error) => refused.push(CliError::from(error)),
        }
    }
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut workspace = FlayerWorkspace::locate_or_default_except(start, &sources, home)?
        .ok_or_else(|| CliError::NowhereToLoad {
            from: start.to_path_buf(),
            verb: "load into",
            named: true,
        })?;
    let mut text = String::new();
    for source in &opened {
        let (entry, registration) = workspace.load(source)?;
        text.push_str(&loaded(&workspace, source.name(), &entry, registration));
    }
    text.push_str(&acted_on("into", &workspace, home));
    let stderr = load_warnings(&workspace);
    Ok(Outcome {
        stdout: text,
        ok: stderr.is_empty(),
        stderr,
    })
}

/// `unload`, at either level: all or nothing, in one rewrite of the marker,
/// and never making the default workspace only to find it loads nothing.
fn unload_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let sources = targets(directory, workspaces)?;
    let mut workspace =
        FlayerWorkspace::locate_except(start, &sources, home)?.ok_or_else(|| {
            CliError::NowhereToLoad {
                from: start.to_path_buf(),
                verb: "unload from",
                named: true,
            }
        })?;
    let refused = sources
        .iter()
        .filter(|source| !workspace.is_loaded(source))
        .map(|source| {
            CliError::Workspace(WorkspaceError::NotLoaded {
                path: source.clone(),
                workspace: workspace.root().to_path_buf(),
            })
        })
        .collect();
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut text = String::new();
    for entry in workspace.unload_all(&sources)? {
        let _ = writeln!(text, "unloaded {}", as_stored(&entry));
    }
    text.push_str(&acted_on("from", &workspace, home));
    Ok(Outcome::plain(text))
}

/// The line naming the workspace a load or an unload changed, because that is
/// not always the one you are standing in — never when you are standing in
/// the one being loaded.
fn acted_on(preposition: &str, workspace: &FlayerWorkspace, home: Option<&Path>) -> String {
    let default = if workspace.is_default(home) {
        ", the default one"
    } else {
        ""
    };
    format!(
        "  {preposition} the workspace at {}{default}\n",
        workspace.root().display()
    )
}

/// `flayer load` on its own: each loaded workspace, what it offers or why it
/// cannot be read, and which workspace loads them.
fn loaded_listing(workspace: &FlayerWorkspace, home: Option<&Path>) -> Outcome {
    let loads = workspace.loads();
    if loads.is_empty() {
        return Outcome::plain(format!(
            "nothing loaded\n  offer another workspace's own with `flayer load <path>`\n{}",
            acted_on("in", workspace, home)
        ));
    }
    let rows: Vec<Vec<String>> = loads
        .iter()
        .enumerate()
        .map(|(index, load)| {
            let entry = as_stored(&load.entry);
            // Two spellings of one workspace are one source, offered once,
            // under the first.
            let earlier = loads[..index]
                .iter()
                .find(|earlier| same_directory(&earlier.root, &load.root));
            match (&load.workspace, earlier) {
                (Ok(source), Some(earlier)) => vec![
                    printable(&entry),
                    printable(source.name()),
                    format!("the same as {}", printable(&as_stored(&earlier.entry))),
                ],
                (Ok(source), None) => vec![
                    printable(&entry),
                    printable(source.name()),
                    offering(workspace, &entry),
                ],
                (Err(_), _) => vec![
                    printable(&entry),
                    String::from("-"),
                    String::from("cannot be opened"),
                ],
            }
        })
        .collect();
    let mut stdout = table(&rows);
    stdout.push_str(&acted_on("in", workspace, home));
    let stderr = load_warnings(workspace);
    Outcome {
        ok: stderr.is_empty(),
        stdout,
        stderr,
    }
}

/// Whether two paths name one directory: as written, or on disk.
pub(crate) fn same_directory(a: &Path, b: &Path) -> bool {
    paths::normalize(a) == paths::normalize(b)
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// What `load` says it did, with what the loaded workspace has to install.
fn loaded(
    workspace: &FlayerWorkspace,
    name: &str,
    entry: &Path,
    registration: Registration,
) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!(
            "loaded {name} as {entry}: {}\n",
            offering(workspace, &entry)
        ),
        Registration::AlreadyRegistered => format!("{name} is already loaded as {entry}\n"),
    }
}

/// "2 skills to install", or that there is nothing yet — counted from what
/// the install machinery itself offers through that entry, so the number is
/// what `flayer install` will show.
fn offering(workspace: &FlayerWorkspace, entry: &str) -> String {
    use mindflayer_core::install::{offered_by_loads, Offer};
    let offers = offered_by_loads(workspace).offers;
    let counts: Vec<String> = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let count = offers
                .iter()
                .filter(|offer| {
                    offer.kind() == kind
                        && matches!(offer, Offer::Loaded { entry: from, .. } if from == entry)
                })
                .count();
            (count > 0).then(|| plural(count, kind.slug()))
        })
        .collect();
    if counts.is_empty() {
        String::from("nothing to install yet")
    } else {
        format!("{} to install", counts.join(" and "))
    }
}

/// Put one skill into one project, from wherever it is offered.
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
    from: Option<&str>,
    directory: &Path,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};

    let warnings = load_warnings(workspace);
    let with_warnings = |error: CliError| Failure {
        error: Box::new(error),
        warnings: warnings.clone(),
    };
    let ledger = workspace
        .ledger()
        .map_err(|error| with_warnings(error.into()))?;
    // As typed first, so a shelf source's address or `workspace` means what
    // it says; only when that names nothing, as a path to a loaded workspace.
    let offered = |from: Option<&str>| {
        installing::offered(workspace, &ledger, project, Kind::Skill, name, from)
    };
    let candidate = match (offered(from), from) {
        (Err(installing::InstallError::NotOffered { .. }), Some(typed)) => {
            offered(Some(&installing::resolve_from(workspace, directory, typed)))
        }
        (result, _) => result,
    }
    .map_err(|error| with_warnings(error.into()))?;
    let origin = candidate.origin();
    let into = project.name();
    let mut outcome = match installing::install(workspace, &ledger, project, &candidate)
        .map_err(|error| with_warnings(error.into()))?
    {
            Installed::Added { name, .. } => {
                Outcome::plain(format!("installed {name} into {into}, from {origin}\n"))
            }
            Installed::Updated { name, .. } => {
                Outcome::plain(format!("updated {name} in {into}, from {origin}\n"))
            }
            Installed::Unchanged { name, .. } => {
                Outcome::plain(format!("{name} in {into} was already current\n"))
            }
            // Refused rather than obeyed: somebody wrote that one.
            Installed::Foreign { name } => Outcome {
                stdout: String::new(),
                stderr: vec![format!(
                    "warning: {into} already has a {name} that Mindflayer did not put there, so it was left alone"
                )],
                ok: false,
            },
    };
    if !warnings.is_empty() {
        outcome.stderr.extend(warnings);
        outcome.ok = false;
    }
    Ok(outcome)
}

/// What the workspaces `workspace` loads could not offer, as `warning:` lines.
pub(crate) fn load_warnings(workspace: &FlayerWorkspace) -> Vec<String> {
    mindflayer_core::install::offered_by_loads(workspace)
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect()
}

/// Take one skill Mindflayer installed back out of one project.
fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Removed};

    let ledger = workspace.ledger().map_err(CliError::from)?;
    let from = project.name();
    match installing::uninstall(workspace, &ledger, project, Kind::Skill, name)
        .map_err(CliError::from)?
    {
        Removed::Removed { name, .. } => {
            Ok(Outcome::plain(format!("removed {name} from {from}\n")))
        }
        Removed::Foreign { name } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {name} in {from} was not installed by Mindflayer, so it was left alone"
            )],
            ok: false,
        }),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(CliError::NotInstalled {
            name,
            project: from.to_owned(),
        }
        .into()),
    }
}

// ---------------------------------------------------------------------------
// gather
// ---------------------------------------------------------------------------

/// Collect artifacts onto the workspace shelf, or say what is already on it.
fn run_gather(
    command: &GatherCommand,
    directory: &Path,
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, home)?;
    let ledger = workspace.ledger().map_err(CliError::from)?;

    match command {
        GatherCommand::Git {
            url,
            path,
            reference,
        } => {
            let request = Request::git(url.clone())
                .from_subdirectory(path.clone())
                .at(reference.clone());
            let report = gather::gather(&workspace, &ledger, &request).map_err(CliError::from)?;
            Ok(gathered(url, &report))
        }
        GatherCommand::List => {
            use mindflayer_core::install::{offered_by_loads, Offer};
            // What the workspaces it loads offer first, as the install screen
            // orders them; what they cannot offer is a warning beside the
            // rest, not a reason to show nothing.
            let loads = offered_by_loads(&workspace);
            let stderr: Vec<String> = loads
                .failures
                .iter()
                .map(|failure| format!("warning: {failure}"))
                .collect();
            let mut offers: Vec<Offer> = loads.offers;
            let mut stderr = stderr;
            for gathered in ledger.gathered().map_err(CliError::from)? {
                // Listed only if it could be installed: a name that is not one
                // folder name never can be, wherever it came from.
                if mindflayer_core::install::is_folder_name(&gathered.name) {
                    offers.push(Offer::Shelf(gathered));
                } else {
                    stderr.push(format!(
                        "warning: {}: declares the name `{}`, which is not one folder name, so it is not offered",
                        gathered.path, gathered.name
                    ));
                }
            }
            let ok = stderr.is_empty();
            Ok(Outcome {
                stdout: shelf_listing(&offers, workspace.loads().is_empty()),
                stderr,
                ok,
            })
        }
    }
}

/// What one gather did, in the order it is worth reading: the source, then
/// every artifact and what happened to it, then the count.
fn gathered(url: &str, report: &Report) -> Outcome {
    let mut stdout = String::new();
    let at = report
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let _ = writeln!(stdout, "{url}{at}");

    // Added first, then updated, then unchanged: a second gather is run to
    // find out what moved, and what moved should not be at the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
        ("unchanged", &report.unchanged),
    ] {
        for artifact in harvested {
            rows.push(vec![
                label.to_owned(),
                printable(&artifact.name),
                printable(artifact.summary.as_deref().unwrap_or("")),
            ]);
        }
    }
    if rows.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        let _ = writeln!(
            stdout,
            "\n{}: {} added, {} updated, {} unchanged",
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest.
    let stderr: Vec<String> = report
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}

/// Everything that can be installed from elsewhere, with where each thing
/// comes from — spelled as `--from` takes it.
///
/// Empty, it says how to fill it — and suggests loading only when nothing is
/// loaded yet; `flayer load` on its own lists what is.
fn shelf_listing(shelf: &[mindflayer_core::install::Offer], nothing_loaded: bool) -> String {
    if shelf.is_empty() {
        return if nothing_loaded {
            String::from(
                "nothing gathered yet\n  take some with `flayer gather git <url>`, \
                 or offer a workspace's own with `flayer load <path>`\n",
            )
        } else {
            String::from(
                "nothing gathered yet, and the loaded workspaces offer nothing\n  \
                 take some with `flayer gather git <url>`; `flayer load` lists what is loaded\n",
            )
        };
    }

    // The same rule a catalog listing follows: a kind column earns its place
    // only once more than one kind is on the shelf.
    let mut kinds: Vec<Kind> = shelf.iter().map(|entry| entry.kind()).collect();
    kinds.sort();
    kinds.dedup();
    let name_kind = kinds.len() > 1;

    let rows: Vec<Vec<String>> = shelf
        .iter()
        .map(|entry| {
            let mut row = Vec::new();
            if name_kind {
                row.push(entry.kind().slug().to_owned());
            }
            row.push(printable(entry.name()));
            row.push(printable(&entry.origin()));
            row.push(printable(entry.summary().unwrap_or("")));
            row
        })
        .collect();
    table(&rows)
}

/// A revision as a person refers to it.
fn short(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
}

// ---------------------------------------------------------------------------
// scan
// ---------------------------------------------------------------------------

/// Every repository under a workspace, and whether it is linked yet.
fn scanned(workspace: &FlayerWorkspace, scan: &Scan) -> Outcome {
    let mut stdout = String::new();
    if scan.found.is_empty() {
        let _ = writeln!(
            stdout,
            "no mind projects or git repositories under {}",
            workspace.root().display()
        );
    } else {
        let rows: Vec<Vec<String>> = scan
            .found
            .iter()
            .map(|found| {
                let what = match (found.linked, found.mind) {
                    (true, _) => "",
                    (false, true) => "mind project",
                    (false, false) => "git repository, not a mind project yet",
                };
                vec![
                    printable(&found.route),
                    String::from(if found.linked { "linked" } else { "not linked" }),
                    what.to_owned(),
                ]
            })
            .collect();
        stdout.push_str(&table(&rows));

        // Only the advice that applies: a hint about a case the listing does
        // not contain is one more line to read past.
        let mut hints = Vec::new();
        if scan.found.iter().any(|found| !found.linked && found.mind) {
            hints.push("link one with `flayer link <path>`");
        }
        if scan.found.iter().any(|found| !found.linked && !found.mind) {
            hints.push("a repository needs `mind -C <path> init` before it can be linked");
        }
        if !hints.is_empty() {
            stdout.push('\n');
            for hint in hints {
                let _ = writeln!(stdout, "  {hint}");
            }
        }
    }

    let mut stderr: Vec<String> = scan
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    if scan.truncated {
        stderr.push(format!(
            "warning: stopped after listing {MAX_LISTED} folders; anything further down can still be linked by path"
        ));
    }
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
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

/// What `init` says it did, at either level.
fn initialized(kind: &str, name: &str, marker: &Path, outcome: Initialization) -> String {
    let marker = marker.display();
    match outcome {
        Initialization::Created => format!("initialized {kind} `{name}` in {marker}\n"),
        Initialization::AlreadyInitialized => {
            format!("{kind} `{name}` already initialized in {marker}\n")
        }
    }
}
