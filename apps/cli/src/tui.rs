//! The TUI: every project a workspace manages, and a key for each command.
//!
//! `flayer` on its own opens it. What it changes, it changes by running a
//! command line — `flayer link beta`, `mind -C beta add skill deploy '…'` —
//! parsed by the parsers the binaries use and run through the same functions,
//! and those lines are printed when it closes. There is nothing the screen can
//! do that a command cannot, and nothing a command says that the screen says
//! differently.
//!
//! Split the way the install screen it embeds is: [`state`] is what a key
//! does, [`ui`] is what the screen looks like, and this file is the loop and
//! the work — the only parts that touch a terminal or a disk.

pub mod minds;
pub mod state;
pub mod ui;

use std::io::{self, IsTerminal as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use std::error::Error as _;

use clap::Parser;
use mindflayer_core::workspace::Member as Registered;
use mindflayer_core::{
    lifecycle, paths, scan, template, Catalog, FlayerWorkspace, Kind, MindProject, WorkspaceError,
};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{self, EnterAlternateScreen};
use ratatui::DefaultTerminal;

use crate::install::state::Screen as InstallScreen;
use crate::{
    as_stored, install, run as run_mind, run_flayer_cli, Cli, CliError, Command, Failure,
    FlayerCli, FlayerCommand, Outcome,
};
use state::{
    App, Choice, Done, Holdings, Item, LoadedSource, Member, Mode, Performed, Scope, Step, Task,
    Unlinked, View, Workspace,
};

/// Open the TUI on the workspace at or above `directory`, and run it until it
/// is closed.
///
/// What comes back is the transcript: every command that changed something,
/// so the session can be read, and repeated, once the screen is gone.
pub fn run(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    // Checked before anything is drawn: a screen written into a pipe is noise
    // in whatever reads it.
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    let app = App::new(directory.to_path_buf(), load(directory, home)?).at_home(home);
    open(app, "flayer")
}

/// Open the TUI straight onto the minds screen: what `flayer minds` does.
pub fn run_minds(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    let mut app = App::new(directory.to_path_buf(), load(directory, home)?).at_home(home);
    if let Some(workspace) = app.workspace() {
        app.mode = Mode::Minds(minds::Minds::of(workspace));
    }
    open(app, "flayer minds")
}

/// Open the TUI on the project at or above `directory`: what `mind` does on
/// its own.
///
/// A project a workspace manages opens in that workspace, on the project —
/// the same screen `flayer` opens, since that is where the project's
/// neighbours and its installs are. One no workspace manages opens on its own.
pub fn run_project(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    let mut app = App::opened_on(
        Scope::Project,
        directory.to_path_buf(),
        load_project(directory, home)?,
    )
    .at_home(home);
    app.focus_on(directory);
    open(app, "mind")
}

/// Run a TUI until it is closed, and hand back its transcript.
fn open(mut app: App, command: &'static str) -> Result<Outcome, Failure> {
    let mut terminal =
        ratatui::try_init().map_err(|source| CliError::NoTerminal { command, source })?;
    let outcome = drive(&mut terminal, &mut app);
    ratatui::restore();
    outcome.map_err(|source| CliError::Screen { source })?;

    Ok(Outcome {
        stdout: app.transcript(),
        stderr: Vec::new(),
        ok: true,
    })
}

/// Draw, wait for a key, do what it asks, repeat.
///
/// The only part a test cannot drive, which is why it holds nothing but the
/// loop: what a key means is [`App::press`]'s answer, what the work does is
/// [`step`]'s, and what the screen looks like is [`ui::draw`]'s.
fn drive(terminal: &mut DefaultTerminal, app: &mut App) -> Result<(), io::Error> {
    loop {
        terminal.draw(|frame| ui::draw(app, frame))?;

        // A poll rather than a blocking read, so a resize repaints instead of
        // waiting for somebody to press something first.
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Windows reports both press and release; acting on each would do
        // everything twice.
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.press(key) {
            Step::Stay => {}
            Step::Quit => return Ok(()),
            // An editor draws a screen of its own, so this one steps aside
            // for it — raw mode off, back to the normal screen — and is drawn
            // again from scratch once the editor is closed.
            Step::Run(task) if task.takes_terminal() => {
                ratatui::restore();
                step(app, task);
                terminal::enable_raw_mode()?;
                ratatui::crossterm::execute!(io::stdout(), EnterAlternateScreen)?;
                terminal.clear()?;
                while event::poll(Duration::ZERO)? {
                    let _ = event::read()?;
                }
            }
            Step::Run(task) => {
                // Said before it starts, because a gather is the network and
                // a frozen screen reads as a hung one.
                app.busy = Some(task.describe());
                terminal.draw(|frame| ui::draw(app, frame))?;
                step(app, task);
                app.busy = None;
                // Keys pressed while it ran were aimed at a screen that has
                // since changed, so they are dropped rather than replayed on
                // the new one.
                while event::poll(Duration::ZERO)? {
                    let _ = event::read()?;
                }
            }
        }
    }
}

/// Carry out a task and show what came of it: everything the loop does
/// between the key that asked and the next frame.
///
/// Public so a test can do exactly what the loop does, without a terminal.
pub fn step(app: &mut App, task: Task) {
    let performed = perform(&task, app);
    let view = task.reloads().then(|| {
        let home = app.home.as_deref();
        match app.opened {
            Scope::Workspace => load(&app.directory, home),
            Scope::Project => load_project(&app.directory, home),
        }
        .map_err(|error| error.to_string())
    });
    app.finish(&task, performed, view);
}

/// Do the work a task names.
pub fn perform(task: &Task, app: &App) -> Performed {
    let base = app.base().to_path_buf();
    match task {
        Task::OpenInstall => open_install(&base),
        Task::ApplyInstall => match &app.mode {
            Mode::Installing(screen) => Performed::Ran(vec![apply_install(&base, screen)]),
            _ => Performed::Ran(Vec::new()),
        },
        // Asked about rather than failed: `flayer link` would refuse it, and
        // `mind init` is what would make it work.
        Task::Link { path } if needs_init(&base.join(path)) => {
            Performed::NeedsInit { path: path.clone() }
        }
        _ => Performed::Ran(run_all(&task.commands(), &base, app.home.as_deref())),
    }
}

/// A directory that exists and is not a mind project.
fn needs_init(directory: &Path) -> bool {
    directory.is_dir()
        && matches!(
            MindProject::open(directory),
            Err(WorkspaceError::NotAProject { .. })
        )
}

/// Run command lines in order, stopping at the first that fails: each leans
/// on the one before, and `flayer link` after a failed `mind init` would only
/// fail again, less clearly.
fn run_all(lines: &[Vec<String>], base: &Path, home: Option<&Path>) -> Vec<Done> {
    let mut dones = Vec::new();
    for words in lines {
        let done = execute(words, base, home);
        let failed = done.error.is_some();
        dones.push(done);
        if failed {
            break;
        }
    }
    dones
}

/// Run one command line through the real parsers, from `base`.
///
/// The words are what a person would type from the workspace root, so a `-C`
/// among them is resolved against it rather than against wherever this
/// process happened to be started. The default workspace is the TUI's own,
/// passed along rather than looked up again, so no command it runs can reach
/// a different one.
pub fn execute(words: &[String], base: &Path, home: Option<&Path>) -> Done {
    let command = state::shell(words);
    let from = |directory: Option<PathBuf>| {
        Some(directory.map_or_else(|| base.to_path_buf(), |directory| base.join(directory)))
    };
    let home = Some(home.map_or_else(String::new, |home| home.to_string_lossy().into_owned()));

    let result = match words.first().map(String::as_str) {
        Some("mind") => match Cli::try_parse_from(words) {
            Err(error) => return Done::failed(command, error.to_string()),
            Ok(cli) if draws_mind(cli.command.as_ref()) => {
                return Done::failed(command, SCREEN_IN_A_SCREEN);
            }
            Ok(mut cli) => {
                cli.directory = from(cli.directory.take());
                cli.home = home;
                run_mind(&cli)
            }
        },
        Some("flayer") => match FlayerCli::try_parse_from(words) {
            Err(error) => return Done::failed(command, error.to_string()),
            Ok(cli) if draws(cli.command.as_ref()) => {
                return Done::failed(command, SCREEN_IN_A_SCREEN);
            }
            Ok(mut cli) => {
                cli.directory = from(cli.directory.take());
                cli.home = home;
                run_flayer_cli(&cli)
            }
        },
        _ => return Done::failed(command, "not a `mind` or `flayer` command"),
    };

    match result {
        Ok(outcome) => Done {
            command,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: None,
            ok: outcome.ok,
        },
        Err(failure) => Done {
            command,
            stdout: String::new(),
            stderr: failure.warnings,
            error: Some(failure.error.to_string()),
            ok: false,
        },
    }
}

/// Why a command that draws is not run from here.
const SCREEN_IN_A_SCREEN: &str = "opens a screen of its own, so it is not run from inside this one";

/// Whether a workspace command opens a screen of its own: the TUI, or the
/// install screen — `flayer install` with nothing named to install.
fn draws(command: Option<&FlayerCommand>) -> bool {
    matches!(
        command,
        None | Some(
            FlayerCommand::Tui | FlayerCommand::Minds | FlayerCommand::Install { name: None, .. }
        )
    )
}

/// The same for `mind`, whose own TUI is what it opens on its own.
fn draws_mind(command: Option<&Command>) -> bool {
    match command {
        None | Some(Command::Tui) => true,
        Some(Command::Flayer { command }) => draws(command.as_ref()),
        Some(_) => false,
    }
}

/// Build the install screen for the workspace at `base`.
fn open_install(base: &Path) -> Performed {
    let screen = FlayerWorkspace::open(base)
        .map_err(CliError::from)
        .and_then(|workspace| {
            let ledger = workspace.ledger().map_err(CliError::from)?;
            // What would not open is already on screen, beside what did.
            let (projects, _) = install::registered(&workspace);
            install::build(&workspace, &ledger, projects).map_err(CliError::from)
        });
    match screen {
        Ok(screen) => Performed::Install(screen),
        Err(error) => Performed::Ran(vec![Done::failed(
            String::from("flayer install"),
            error.to_string(),
        )]),
    }
}

/// Carry out what the install screen marked, the way `flayer install` does.
fn apply_install(base: &Path, screen: &InstallScreen) -> Done {
    let command = String::from("flayer install");
    let applied = FlayerWorkspace::open(base)
        .map_err(CliError::from)
        .and_then(|workspace| {
            let ledger = workspace.ledger().map_err(CliError::from)?;
            let mut outcome =
                install::apply(&workspace, &ledger, screen).map_err(CliError::from)?;
            let warnings = crate::load_warnings(&workspace);
            outcome.ok = outcome.ok && warnings.is_empty();
            outcome.stderr.extend(warnings);
            Ok(outcome)
        });
    match applied {
        Ok(outcome) => Done {
            command,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: None,
            ok: outcome.ok,
        },
        Err(error) => Done::failed(command, error.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Reading what the screen shows
// ---------------------------------------------------------------------------

/// Read everything the screen shows from the disk: the workspace at or above
/// `directory`, or else the default one in `home`, made there the first time.
pub fn load(directory: &Path, home: Option<&Path>) -> Result<View, CliError> {
    if let Some(workspace) = FlayerWorkspace::locate(directory)? {
        return Ok(View::Workspace(workspace_view(&workspace, false)));
    }
    Ok(match FlayerWorkspace::locate_or_default(directory, home)? {
        Some(workspace) => View::Workspace(workspace_view(&workspace, true)),
        None => View::Nowhere,
    })
}

/// What `mind` shows: the workspace managing the project at or above
/// `directory`, or that project on its own.
///
/// Opening a project's screen makes nothing: the default workspace in `home`
/// counts when it already manages the project, and is otherwise only where
/// `mind link` would put it.
pub fn load_project(directory: &Path, home: Option<&Path>) -> Result<View, CliError> {
    let Some(project) = MindProject::locate(directory)? else {
        return Ok(View::Nowhere);
    };
    let (above, default) = match FlayerWorkspace::locate(project.root())? {
        Some(workspace) => (Some(workspace), false),
        None => (FlayerWorkspace::default_at(home)?, true),
    };
    if let Some(workspace) = &above {
        if workspace.is_linked(project.root()) {
            return Ok(View::Workspace(workspace_view(workspace, default)));
        }
    }
    // `.`, because the commands run from the project itself.
    let entry = String::from(".");
    let root = project.root().to_path_buf();
    Ok(View::Workspace(Workspace {
        name: project.name().to_owned(),
        members: vec![held(entry, root.clone(), false, &project, &Kind::ALL)],
        root,
        unlinked: Vec::new(),
        shelf: 0,
        // What `mind load` has put into the workspace above, so the load form
        // can show it and unload it from here too.
        loaded: above.as_ref().map(loaded).unwrap_or_default(),
        scope: Scope::Project,
        above: above
            .map(|workspace| workspace.root().to_path_buf())
            .or_else(|| home.map(Path::to_path_buf)),
        default: false,
    }))
}

/// A workspace as the screen shows it: its own artifacts first, then every
/// project it manages.
fn workspace_view(workspace: &FlayerWorkspace, default: bool) -> Workspace {
    let own = workspace.own();
    let mut members = vec![held(
        String::new(),
        workspace.root().to_path_buf(),
        true,
        &own,
        &workspace.own_kinds(),
    )];
    members.extend(workspace.members().into_iter().map(member));
    let unlinked = scan(workspace)
        .found
        .into_iter()
        .filter(|found| !found.linked)
        .map(|found| Unlinked {
            route: found.route,
            mind: found.mind,
        })
        .collect();
    Workspace {
        name: workspace.name().to_owned(),
        root: workspace.root().to_path_buf(),
        members,
        unlinked,
        shelf: shelf(workspace),
        loaded: loaded(workspace),
        scope: Scope::Workspace,
        above: None,
        default,
    }
}

/// Every workspace this one loads, with how many of its own each offers —
/// counted from the install machinery's own offers, so the number is what
/// the install screen shows.
fn loaded(workspace: &FlayerWorkspace) -> Vec<LoadedSource> {
    use mindflayer_core::install::{offered_by_loads, Offer};
    let offers: Vec<Offer> = offered_by_loads(workspace).offers;
    let loads = workspace.loads();
    let mut shown: Vec<LoadedSource> = Vec::new();
    for (index, loaded) in loads.iter().enumerate() {
        let entry = as_stored(&loaded.entry);
        // A second spelling of one workspace offers what the first does.
        let first = loads[..index]
            .iter()
            .find(|earlier| crate::same_directory(&earlier.root, &loaded.root))
            .map_or_else(|| entry.clone(), |earlier| as_stored(&earlier.entry));
        shown.push(match &loaded.workspace {
            Ok(source) => LoadedSource {
                root: loaded.root.clone(),
                offers: Ok(offers
                    .iter()
                    .filter(|offer| {
                        matches!(offer, Offer::Loaded { entry: from, .. } if *from == first)
                    })
                    .count()),
                name: source.name().to_owned(),
                entry,
            },
            Err(error) => LoadedSource {
                root: loaded.root.clone(),
                name: entry.clone(),
                offers: Err(error.to_string()),
                entry,
            },
        });
    }
    shown
}

/// One registry entry, opened and read.
fn member(registered: Registered) -> Member {
    let entry = as_stored(&registered.entry);
    match registered.project {
        Ok(project) => held(entry, registered.root, false, &project, &Kind::ALL),
        Err(error) => Member {
            name: entry.clone(),
            entry,
            root: registered.root,
            own: false,
            holdings: Err(error.to_string()),
        },
    }
}

/// What one holder holds, read from the disk: a project, or the workspace's
/// own for the kinds it keeps apart.
fn held(entry: String, root: PathBuf, own: bool, project: &MindProject, kinds: &[Kind]) -> Member {
    let catalog = Catalog::discover_kinds(std::slice::from_ref(project), kinds);
    let mut items: Vec<Item> = catalog
        .artifacts()
        .iter()
        .map(|artifact| Item {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            summary: artifact.summary().unwrap_or("").to_owned(),
            issues: artifact
                .validate()
                .iter()
                .map(ToString::to_string)
                .collect(),
            broken: None,
        })
        .collect();

    // A file that does not read is listed where it would have been, by the
    // name its place gives it, so it can be opened and fixed from here. Only
    // what cannot be pinned to an artifact — a folder that cannot be listed —
    // is left as a warning on its own.
    let mut warnings = Vec::new();
    for failure in catalog.failures() {
        match lifecycle::identify(project, failure.path()) {
            Some((kind, name)) => items.push(Item {
                kind,
                name,
                summary: String::new(),
                issues: Vec::new(),
                // The reason without the path, which the row already says.
                broken: Some(
                    failure
                        .source()
                        .map_or_else(|| failure.to_string(), ToString::to_string),
                ),
            }),
            None => warnings.push(failure.to_string()),
        }
    }
    items.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    let directories = Kind::ALL
        .into_iter()
        .map(|kind| (kind, within(project, kind)))
        .collect();
    let templates = match template::templates(project) {
        Ok(templates) => templates
            .into_iter()
            .map(|template| Choice {
                name: template.name,
                kind: template.kind,
            })
            .collect(),
        Err(error) => {
            warnings.push(error.to_string());
            Vec::new()
        }
    };

    Member {
        entry,
        root,
        name: project.name().to_owned(),
        own,
        holdings: Ok(Holdings {
            items,
            warnings,
            directories,
            templates,
        }),
    }
}

/// Where a project keeps a kind, relative to the project.
fn within(project: &MindProject, kind: Kind) -> String {
    let directory = project.directory_for(kind);
    paths::relative_to(&directory, project.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| directory.display().to_string())
}

/// How much is on the shelf, without creating a ledger just to look.
fn shelf(workspace: &FlayerWorkspace) -> usize {
    if !workspace.ledger_path().is_file() {
        return 0;
    }
    workspace
        .ledger()
        .and_then(|ledger| ledger.gathered())
        .map_or(0, |gathered| gathered.len())
}
