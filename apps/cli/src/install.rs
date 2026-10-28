//! `flayer install`: the screen for putting skills and rules into projects.
//!
//! The screen only marks boxes. Everything marked is carried out when you
//! say so, as the `flayer install` and `flayer uninstall` lines it amounts
//! to ([`Screen::task`]), each parsed and run as typed, the way the TUI runs
//! every line; and what they said is printed afterwards as the TUI's
//! transcript prints it, each line and then what it said. So the report
//! shows exactly what ran, and can be run again without the screen.

pub mod state;
pub mod ui;

use std::io;
use std::time::Duration;

use mindflayer_core::gather::Watch;
use mindflayer_core::install;
use mindflayer_core::ledger::Ledger;
use mindflayer_core::{FlayerWorkspace, Kind, MindProject};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::commands::workspace::load_warnings;
use crate::tui::state::{transcript, Done};
use crate::{tui, CliError, Failure, Lookup, Outcome};
use state::{Screen, Step, Target};

/// Build the screen for a workspace, run it, and carry out what it asked for.
///
/// Built from the ledger as it is ([`open`]), without making one or scrubbing
/// it: opening the screen and leaving it changes nothing, so it is a read until
/// something is applied. Applying runs lines ([`apply`]), the first of which
/// opens the ledger to write, which makes it and scrubs it, as that command
/// does from the shell; each with `lookup`, so they install nothing into the
/// agents' own folders the command line was given, and use the workspace it
/// chose.
pub fn run(workspace: &FlayerWorkspace, lookup: &Lookup) -> Result<Outcome, Failure> {
    let (projects, mut warnings) = registered(workspace);
    warnings.extend(load_warnings(workspace));
    let screen = open(workspace, projects)?;

    if screen.targets.is_empty() {
        return Ok(Outcome {
            stdout: format!(
                "{} manages no projects yet\n  link one with `flayer link <path>`\n",
                workspace.name()
            ),
            stderr: warnings,
            ok: true,
        });
    }

    // `try_init` rather than `init`, which panics: this command is the one
    // thing here that needs a terminal, and being run without one (in CI,
    // with the output redirected) deserves a sentence rather than a panic.
    let mut terminal = ratatui::try_init().map_err(|source| CliError::NoTerminal {
        command: "flayer install",
        source,
    })?;
    let outcome = drive(&mut terminal, screen);
    ratatui::restore();

    let (screen, step) = outcome.map_err(|source| CliError::Screen { source })?;
    let mut outcome = match step {
        Step::Apply => apply(workspace, &screen, lookup),
        _ => Outcome {
            stdout: String::from("nothing applied\n"),
            stderr: Vec::new(),
            ok: true,
        },
    };
    outcome.ok = outcome.ok && warnings.is_empty();
    outcome.stderr.splice(0..0, warnings);
    Ok(outcome)
}

/// Build the screen from the ledger as it is, as a read: without making the
/// ledger, or scrubbing it of what an older build stored with a credential.
///
/// The screen `flayer install` opens and the one inside the TUI both open here
/// and apply as the lines [`Screen::task`] gives, so they cannot differ about
/// either.
pub fn open(workspace: &FlayerWorkspace, projects: Vec<MindProject>) -> Result<Screen, CliError> {
    let read = workspace.ledger_for_reading()?;
    Ok(build(workspace, &read, projects)?)
}

/// Carry out what `screen` marked: each line it amounts to ([`Screen::task`])
/// parsed and run from the workspace's root with `lookup`, as the TUI runs
/// one ([`tui::execute`]), and what each said, as the TUI's transcript says
/// it; failing when any line did not go through.
///
/// Every line is run, whatever came of the ones before, as the TUI runs the
/// same lines ([`crate::tui::state::Task::chained`]): each is boxes marked
/// one by one. Its own ledger is opened to write by each line, scrubbed
/// first, so a source a scrub makes one with another is offered from where
/// it was by the time the line asks for it, by the origin the screen showed,
/// which a scrub leaves as it was; and what only the source the scrub took
/// out had gathered is refused as not offered, said where its line is.
pub fn apply(workspace: &FlayerWorkspace, screen: &Screen, lookup: &Lookup) -> Outcome {
    let dones: Vec<Done> = screen
        .task()
        .commands()
        .iter()
        .map(|words| tui::execute(words, workspace.root(), lookup, &Watch::new()))
        .collect();
    Outcome {
        stdout: transcript(&dones),
        stderr: Vec::new(),
        ok: dones.iter().all(|done| done.ok),
    }
}

/// The projects a workspace manages, and what could not be opened.
pub fn registered(workspace: &FlayerWorkspace) -> (Vec<MindProject>, Vec<String>) {
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    (projects, warnings)
}

/// Ask core how everything of every kind that can be installed stands with
/// each project.
///
/// Public because it and [`apply`] are the two ends of this feature that do
/// not need a terminal: a test builds a screen, presses the keys a person
/// would, and applies the result.
pub fn build(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    projects: Vec<MindProject>,
) -> Result<Screen, install::InstallError> {
    let mut targets = Vec::new();
    for project in projects {
        let mut candidates = Vec::new();
        for kind in Kind::ALL {
            candidates.extend(install::survey(workspace, ledger, &project, kind)?);
        }
        let picked_by = workspace.naming(&project);
        let mut target = Target::new(project, picked_by, candidates);
        // Which of several offers of one name is the copy the project holds.
        let project = target.project.clone();
        target.settle(|candidate| install::is_current(workspace, ledger, &project, candidate));
        targets.push(target);
    }
    Ok(Screen::new(targets))
}

/// Draw, wait for a key, repeat.
///
/// The only part of this feature a test cannot drive, which is why it holds
/// nothing but the loop: what a key means is [`Screen::press`]'s answer and
/// what the screen looks like is [`ui::draw`]'s.
fn drive(terminal: &mut DefaultTerminal, mut screen: Screen) -> Result<(Screen, Step), io::Error> {
    loop {
        terminal.draw(|frame| ui::draw(&screen, frame))?;

        // A poll rather than a blocking read, so a resize repaints instead of
        // waiting for somebody to press something first.
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            // Windows reports both press and release; acting on each would
            // toggle every box twice.
            Event::Key(key) if key.kind == KeyEventKind::Press => match screen.press(key.code) {
                Step::Stay => {}
                step => return Ok((screen, step)),
            },
            _ => {}
        }
    }
}
