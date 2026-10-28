//! `flayer install`: the screen for putting skills and rules into projects.
//!
//! The screen only marks boxes. Everything marked is carried out in one batch
//! when you say so, and what that batch did is printed afterwards as an
//! ordinary [`Outcome`], so the report a user reads, and a test asserts on,
//! is the same shape every other command produces.

pub mod state;
pub mod ui;

use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::time::Duration;

use mindflayer_core::install::{self, Installed, Removed};
use mindflayer_core::ledger::Ledger;
use mindflayer_core::{AgentHomes, Excluded, FlayerWorkspace, Kind, MindProject};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::commands::gather::scrubbed;
use crate::commands::install::{named, render_said};
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};
use state::{Pending, Screen, Step, Target};

/// Build the screen for a workspace, run it, and carry out what it asked for.
///
/// Built from the ledger as it is ([`open`]), without making one or scrubbing
/// it: opening the screen and leaving it changes nothing, so it is a read until
/// something is applied. Only applying opens the ledger to write to
/// ([`commit`]), which makes it and scrubs it, and installs nothing into the
/// agents' own folders, `homes`.
pub fn run(workspace: &FlayerWorkspace, homes: &AgentHomes) -> Result<Outcome, Failure> {
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
        Step::Apply => commit(workspace, &screen, homes)?,
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
/// read leaves the ledger as it finds it.
pub fn commit(
    workspace: &FlayerWorkspace,
    screen: &Screen,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    let applied = if scrub.sources.is_empty() {
        apply(workspace, &ledger, screen, homes)
    } else {
        rebuilt(workspace, &ledger, screen).and_then(|(screen, lost)| {
            let mut outcome = apply(workspace, &ledger, &screen, homes)?;
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
/// offering the same kind and name from the same place, and a warning for
/// each mark left with no such row.
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
            let (kind, name, origin) = (
                row.candidate.kind(),
                row.candidate.name(),
                row.candidate.origin(),
            );
            let carried = now
                .rows
                .iter()
                .position(|row| {
                    row.candidate.kind() == kind
                        && row.candidate.name() == name
                        && row.candidate.origin() == origin
                })
                .is_some_and(|index| now.mark(index, pending == Pending::Install));
            if !carried {
                lost.push(format!(
                    "warning: {}: {} from {origin} is no longer on the shelf once the ledger \
                     was scrubbed, so it was left as it was",
                    was.name(),
                    named(kind, name)
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

/// Carry out everything the screen marked, and say what happened.
///
/// With a ledger opened to write: [`commit`] opens it, and scrubs it first.
/// Nothing goes into the agents' own folders, `homes`.
pub fn apply(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    screen: &Screen,
    homes: &AgentHomes,
) -> Result<Outcome, install::InstallError> {
    let mut stdout = String::new();
    let mut stderr = Vec::new();
    // Said once a project, after the warnings, and no reason to fail.
    let mut notes: Vec<String> = Vec::new();
    // The projects and kinds an install was asked of, each told once of the
    // folders its settings name for the kind that no install uses.
    let mut installing: Vec<(&Path, Kind)> = Vec::new();
    let (mut installed, mut removed) = (0usize, 0usize);

    for (target, row, what) in screen.pending() {
        let target = &screen.targets[target];
        let entry = &target.rows[row];
        let project = &target.project;
        let kind = entry.candidate.kind();

        match what {
            Pending::Install => {
                if !installing.contains(&(project.root(), kind)) {
                    installing.push((project.root(), kind));
                    stderr.extend(crate::commands::install::misplaced(project, &[kind]));
                }
                // One that cannot be installed is said and passed over: the
                // rest were marked too, and what was already done is reported.
                // Only the ledger failing stops everything.
                let result =
                    match install::install(workspace, ledger, project, &entry.candidate, homes) {
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
                let name = named(kind, entry.candidate.name());
                for left in &done.left_out {
                    stderr.push(format!(
                        "warning: {}: {name} {}",
                        project.name(),
                        crate::commands::install::left_out(left)
                    ));
                }
                let mut noted: Vec<String> = Vec::new();
                noted.extend(
                    done.excluded
                        .as_ref()
                        .and_then(Excluded::warning)
                        .map(|note| format!("note: {note}")),
                );
                // What the project's rules came to, rendered with the one
                // installed: said of the project, as each line here is.
                if let Some(rendered) = &done.rendered {
                    let (warned, said) =
                        render_said(rendered, entry.candidate.name(), Some(project.name()));
                    once(&mut stderr, warned);
                    if let Some(reason) = &rendered.still_excluded {
                        stderr.push(format!(
                            "warning: {}: the exclude file still keeps out of git a file that \
                             is no longer Mindflayer's: {reason}",
                            project.name()
                        ));
                    }
                    noted.extend(said);
                }
                for note in noted {
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
                if let Some(reason) = done.installed.unhashed() {
                    stderr.push(format!(
                        "warning: {}: {name} was recorded without a hash of what it holds: {reason}",
                        project.name(),
                    ));
                }
                match done.installed {
                    Installed::Added { .. } | Installed::Updated { .. } => {
                        installed += 1;
                        let _ = writeln!(stdout, "{}: installed {name}", project.name());
                    }
                    Installed::Unchanged { .. } => {
                        let _ = writeln!(stdout, "{}: {name} was already current", project.name());
                    }
                    // Refused rather than obeyed: somebody wrote that file.
                    Installed::Foreign { path, .. } => stderr.push(format!(
                        "warning: {}: {name} is already at {} and Mindflayer did not put it there, so it was left alone",
                        project.name(),
                        crate::commands::install::shown(project, &path)
                    )),
                    // Or changed what Mindflayer put there, which the screen
                    // never forces.
                    Installed::Edited { paths, .. } => stderr.push(format!(
                        "warning: {}: {name} was changed since it was installed, at {}, so it was left alone",
                        project.name(),
                        crate::commands::install::listed(project, &paths)
                    )),
                }
                stdout.push_str(&crate::commands::install::swept(project, &done.swept));
            }
            Pending::Remove => {
                let taken = match install::uninstall(
                    workspace,
                    ledger,
                    project,
                    kind,
                    entry.candidate.name(),
                    homes,
                ) {
                    Err(install::InstallError::Ledger(error)) => return Err(error.into()),
                    Err(error) => {
                        stderr.push(format!("warning: {}: {error}", project.name()));
                        continue;
                    }
                    Ok(taken) => taken,
                };
                let name = named(kind, entry.candidate.name());
                match taken {
                    Removed::Removed {
                        still_excluded,
                        rendered,
                        ..
                    } => {
                        removed += 1;
                        let _ = writeln!(stdout, "{}: removed {name}", project.name());
                        // What the rules left came to without it.
                        if let Some(rendered) = rendered {
                            let (warned, said) = render_said(
                                &rendered,
                                entry.candidate.name(),
                                Some(project.name()),
                            );
                            once(&mut stderr, warned);
                            for note in said {
                                if !notes.contains(&note) {
                                    notes.push(note);
                                }
                            }
                        }
                        if let Some(reason) = still_excluded {
                            stderr.push(crate::commands::install::still(
                                &name,
                                project.name(),
                                &reason,
                            ));
                        }
                    }
                    Removed::Foreign { path, .. } => stderr.push(format!(
                        "warning: {}: {name} at {} was not installed by Mindflayer, so it was left alone",
                        project.name(),
                        crate::commands::install::shown(project, &path)
                    )),
                    Removed::Edited { paths, .. } => stderr.push(format!(
                        "warning: {}: {name} was changed since it was installed, at {}, so it was left alone",
                        project.name(),
                        crate::commands::install::listed(project, &paths)
                    )),
                    Removed::Absent { .. } => {
                        let _ = writeln!(stdout, "{}: {name} was not there", project.name());
                    }
                }
            }
        }
    }

    let _ = writeln!(stdout, "\n{} installed, {} removed", installed, removed);
    let ok = stderr.is_empty();
    stderr.extend(notes);
    Ok(Outcome { stdout, stderr, ok })
}

/// Add each of `warned`, what a render of a project's rules said, to
/// `stderr` unless it is there already. Every rule ticked in one project
/// renders all of its rules, so what is true of them as a whole, an
/// `AGENTS.md` git tracks say, would otherwise be said once a rule. What is
/// said of one rule names it, and so is said of each.
fn once(stderr: &mut Vec<String>, warned: Vec<String>) {
    for warning in warned {
        if !stderr.contains(&warning) {
            stderr.push(warning);
        }
    }
}
