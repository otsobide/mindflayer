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

use crate::commands::gather::scrubbed;
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};
use state::{Pending, Screen, Step, Target};

/// Build the screen for a workspace, run it, and carry out what it asked for.
///
/// Built from the ledger as it is ([`open`]), without making one or scrubbing
/// it: opening the screen and leaving it changes nothing, so it is a read until
/// something is applied. Only applying opens the ledger to write to
/// ([`commit`]), which makes it and scrubs it.
pub fn run(workspace: &FlayerWorkspace) -> Result<Outcome, Failure> {
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
    // thing here that needs a terminal, and being run without one — in CI,
    // with the output redirected — deserves a sentence rather than a panic.
    let mut terminal = ratatui::try_init().map_err(|source| CliError::NoTerminal {
        command: "flayer install",
        source,
    })?;
    let outcome = drive(&mut terminal, screen);
    ratatui::restore();

    let (screen, step) = outcome.map_err(|source| CliError::Screen { source })?;
    let mut outcome = match step {
        Step::Apply => commit(workspace, &screen)?,
        _ => Outcome {
            stdout: String::from("nothing applied\n"),
            stderr: Vec::new(),
            ok: true,
        },
    };
    // Judged by the warnings rather than by stderr, which may also hold the
    // notes a scrub leaves, and those are not a failure.
    outcome.ok = outcome.ok && warnings.is_empty();
    outcome.stderr.splice(0..0, warnings);
    Ok(outcome)
}

/// Build the screen from the ledger as it is, as a read: without making the
/// ledger, or scrubbing it of what an older build stored with a credential.
///
/// The screen `flayer install` opens and the one inside the TUI both open here
/// and apply through [`commit`], so they cannot differ about either.
pub fn open(workspace: &FlayerWorkspace, projects: Vec<MindProject>) -> Result<Screen, CliError> {
    let read = workspace.ledger_for_reading()?;
    Ok(build(workspace, &read, projects)?)
}

/// Open the ledger to write, which makes it and scrubs it first, and carry out
/// what the screen marked; what the scrub took out is said before what was
/// done, and with an error as warnings, since it happened all the same.
///
/// The screen was built from a read, so this scrub is the first to see the
/// ledger. A scrub can make two sources one row, and a mark on the row that
/// went would install from folders the scrub removed, or copy the files and
/// then fail to record them against a source that is no longer there,
/// leaving a copy Mindflayer no longer counts as its own. So once a scrub has
/// taken out a source, what was marked is found again on the shelf it left
/// ([`rebuilt`]) before anything is carried out. Scrubbing when the screen
/// opened was the other way, and is rejected: opening it is a read, and a
/// read writes nothing.
pub fn commit(workspace: &FlayerWorkspace, screen: &Screen) -> Result<Outcome, Failure> {
    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    let applied = if scrub.sources.is_empty() {
        apply(workspace, &ledger, screen)
    } else {
        rebuilt(workspace, &ledger, screen).and_then(|(screen, lost)| {
            let mut outcome = apply(workspace, &ledger, &screen)?;
            outcome.ok = outcome.ok && lost.is_empty();
            outcome.stderr.splice(0..0, lost);
            Ok(outcome)
        })
    };
    match applied {
        Ok(outcome) => Ok(said.before(outcome)),
        Err(error) => Err(Failure {
            error: Box::new(error.into()),
            warnings: said.lines(),
        }),
    }
}

/// `screen` built again from `ledger`, with each mark made again on the row
/// offering the same name from the same place, and a warning for each mark
/// left with no such row.
///
/// Carried by name and origin rather than by the source row, which a scrub
/// can make one with another: the origin is shown without credentials before
/// a scrub as after it, so the row that went and the row it became one with
/// offer from the same place. What only the row that went had gathered went
/// with it, and a mark on that is not carried out.
fn rebuilt(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    screen: &Screen,
) -> Result<(Screen, Vec<String>), install::InstallError> {
    let projects = screen
        .targets
        .iter()
        .map(|target| target.project.clone())
        .collect();
    let mut rebuilt = build(workspace, ledger, projects)?;
    let mut lost = Vec::new();
    for (was, now) in screen.targets.iter().zip(&mut rebuilt.targets) {
        for row in &was.rows {
            let Some(pending) = row.pending() else {
                continue;
            };
            let (name, origin) = (row.candidate.name(), row.candidate.origin());
            let carried = now
                .rows
                .iter()
                .position(|row| row.candidate.name() == name && row.candidate.origin() == origin)
                .is_some_and(|index| now.mark(index, pending == Pending::Install));
            if !carried {
                lost.push(format!(
                    "warning: {}: {name} from {origin} is no longer on the shelf once the ledger \
                     was scrubbed, so it was left as it was",
                    was.name()
                ));
            }
        }
    }
    Ok((rebuilt, lost))
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
        let mut target = Target::new(project, candidates);
        // Which of several offers of one name is the copy the project holds.
        let project = target.project.clone();
        target.settle(|candidate| install::is_current(workspace, &project, candidate));
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

/// Carry out everything the screen marked, and say what happened.
///
/// With a ledger opened to write: [`commit`] opens it, and scrubs it first.
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
                // One that cannot be installed is said and passed over: the
                // rest were marked too, and what was already done is reported.
                // Only the ledger failing stops everything.
                let result = match install::install(workspace, ledger, project, &entry.candidate) {
                    Err(install::InstallError::Ledger(error)) => return Err(error.into()),
                    result => result,
                };
                let done = match result {
                    Ok(done) => done,
                    Err(error) => {
                        // What it cleared first is still something it did.
                        for line in crate::commands::install::cleared(project, error.swept()) {
                            let _ = writeln!(stdout, "{}: {line}", project.name());
                        }
                        stderr.push(format!("warning: {}: {error}", project.name()));
                        continue;
                    }
                };
                match done.installed {
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
                stdout.push_str(&crate::commands::install::swept(project, &done.swept));
            }
            Pending::Remove => {
                let name = entry.candidate.name();
                let taken = match install::uninstall(workspace, ledger, project, Kind::Skill, name)
                {
                    Err(install::InstallError::Ledger(error)) => return Err(error.into()),
                    Err(error) => {
                        stderr.push(format!("warning: {}: {error}", project.name()));
                        continue;
                    }
                    Ok(taken) => taken,
                };
                match taken {
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
