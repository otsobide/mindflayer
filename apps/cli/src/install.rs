//! `flayer install`: the screen for putting shelf skills into projects.
//!
//! The screen only marks boxes. Everything marked is carried out in one batch
//! when you say so, and what that batch did is printed afterwards as an
//! ordinary [`Outcome`] — so the report a user reads, and a test asserts on,
//! is the same shape every other command produces.

pub mod state;
pub mod ui;

use std::fmt::Write as _;
use std::io;
use std::time::Duration;

use mindflayer_core::install::{self, Installed, Removed};
use mindflayer_core::ledger::Ledger;
use mindflayer_core::{FlayerWorkspace, Kind, MindProject};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::{CliError, Failure, Outcome};
use state::{Pending, Screen, Step, Target};

/// Build the screen for a workspace, run it, and carry out what it asked for.
pub fn run(workspace: &FlayerWorkspace, ledger: &Ledger) -> Result<Outcome, Failure> {
    let (projects, warnings) = registered(workspace);
    let screen = build(workspace, ledger, projects).map_err(CliError::from)?;

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
    // thing here that needs a terminal, and being run without one — in CI,
    // with the output redirected — deserves a sentence rather than a panic.
    let mut terminal = ratatui::try_init().map_err(|source| CliError::NoTerminal { source })?;
    let outcome = drive(&mut terminal, screen);
    ratatui::restore();

    let (screen, step) = outcome.map_err(|source| CliError::Screen { source })?;
    let mut outcome = match step {
        Step::Apply => apply(workspace, ledger, &screen).map_err(CliError::from)?,
        _ => Outcome {
            stdout: String::from("nothing applied\n"),
            stderr: Vec::new(),
            ok: true,
        },
    };
    outcome.stderr.splice(0..0, warnings);
    outcome.ok = outcome.ok && outcome.stderr.is_empty();
    Ok(outcome)
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

/// Ask core how each shelf entry stands with each project.
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
        let candidates = install::survey(workspace, ledger, &project, Kind::Skill)?;
        targets.push(Target::new(project, candidates));
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

/// Carry out everything the screen marked, and say what happened.
pub fn apply(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    screen: &Screen,
) -> Result<Outcome, install::InstallError> {
    let mut stdout = String::new();
    let mut stderr = Vec::new();
    let (mut installed, mut removed) = (0usize, 0usize);

    for (target, row, what) in screen.pending() {
        let target = &screen.targets[target];
        let entry = &target.rows[row];
        let project = &target.project;

        match what {
            Pending::Install => {
                match install::install(workspace, ledger, project, &entry.candidate)? {
                    Installed::Added { name, .. } | Installed::Updated { name, .. } => {
                        installed += 1;
                        let _ = writeln!(stdout, "{}: installed {name}", project.name());
                    }
                    Installed::Unchanged { name, .. } => {
                        let _ = writeln!(stdout, "{}: {name} was already current", project.name());
                    }
                    // Refused rather than obeyed: somebody wrote that file.
                    Installed::Foreign { name } => stderr.push(format!(
                        "warning: {}: {name} is already there and Mindflayer did not put it there, so it was left alone",
                        project.name()
                    )),
                }
            }
            Pending::Remove => {
                let name = entry.candidate.name();
                match install::uninstall(workspace, ledger, project, Kind::Skill, name)? {
                    Removed::Removed { name, .. } => {
                        removed += 1;
                        let _ = writeln!(stdout, "{}: removed {name}", project.name());
                    }
                    Removed::Foreign { name } => stderr.push(format!(
                        "warning: {}: {name} was not installed by Mindflayer, so it was left alone",
                        project.name()
                    )),
                    Removed::Absent { name } => {
                        let _ = writeln!(stdout, "{}: {name} was not there", project.name());
                    }
                }
            }
        }
    }

    let _ = writeln!(stdout, "\n{} installed, {} removed", installed, removed);
    let ok = stderr.is_empty();
    Ok(Outcome { stdout, stderr, ok })
}
