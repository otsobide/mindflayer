//! `status`: what each project holds, where each thing in it came from,
//! which agents read it, and whether it is all there.
//!
//! Core builds and orders the report ([`Status`]); this only lays it out, as
//! a table naming no more than its rows need to be told apart.

use mindflayer_core::{Agent, FlayerWorkspace, Kind, MindProject, Status, StatusFailure};

use super::consulted;
use super::read::looked_in;
use crate::render::{printable, table};
use crate::{CliError, Failure, Lookup, Outcome};

/// What the state column says of an entry that would not open.
const STALE: &str = "stale";

/// `flayer status`: every project the workspace links, against its ledger,
/// read without being made.
pub(crate) fn status_of_workspace(workspace: &FlayerWorkspace) -> Result<Outcome, Failure> {
    if workspace.config().projects.is_empty() {
        // As `flayer list` says it, and for the same reason: a workspace with
        // nothing linked has nothing to report but how to link something.
        return Ok(Outcome::plain(format!(
            "{} manages no projects yet\n  link one with `flayer link <path>`\n",
            workspace.name()
        )));
    }
    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
    let (status, failures) = Status::of_workspace(workspace, &ledger);
    Ok(report(&status, failures, true, || workspace.projects().0))
}

/// `flayer status -p`: one project the workspace links.
pub(crate) fn status_of_project(
    workspace: &FlayerWorkspace,
    project: MindProject,
) -> Result<Outcome, Failure> {
    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
    let (status, failures) = Status::of_project(&project, Some((workspace, &ledger)));
    Ok(report(&status, failures, false, || vec![project]))
}

/// `mind status`: the project at or above here, against the workspace `mind`
/// consults about it, when that one links it.
///
/// A project nothing links is still reported, as `mind list` reads one: what
/// is in it is its own or somebody's all the same, and the hint says why
/// nothing in it reads as installed.
pub(crate) fn status_here(project: MindProject, lookup: &Lookup) -> Result<Outcome, Failure> {
    let workspace = consulted(&project, lookup)?;
    if let Some(workspace) = workspace
        .as_ref()
        .filter(|workspace| workspace.is_linked(project.root()))
    {
        return status_of_project(workspace, project);
    }
    let (status, failures) = Status::of_project(&project, None);
    let mut outcome = report(&status, failures, false, || vec![project]);
    outcome.stderr.push(match workspace {
        Some(workspace) => format!(
            "hint: the workspace at {} does not link this project, so nothing in it reads as installed; \
             `mind link` links it",
            workspace.root().display()
        ),
        None => String::from(
            "hint: no workspace links this project, so nothing in it reads as installed",
        ),
    });
    Ok(outcome)
}

/// The report as a table, with what could not be read said on stderr.
///
/// `across` is whether it is about every project a workspace links, where a
/// project column can tell rows apart; `looked` gives the projects it read,
/// asked only to say where it looked when it found nothing.
fn report(
    status: &Status,
    failures: Vec<StatusFailure>,
    across: bool,
    looked: impl FnOnce() -> Vec<MindProject>,
) -> Outcome {
    let stderr: Vec<String> = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    let ok = stderr.is_empty();
    let stdout = rows(status, across).unwrap_or_else(|| {
        let mut text = String::from("nothing found\n");
        looked_in(&mut text, &looked());
        text
    });
    Outcome { stdout, stderr, ok }
}

/// One line per artifact, and one per entry that would not open; none at
/// all when there is neither.
///
/// The project column only when the rows come from more than one project,
/// or one of them is stale, whose row is nothing but its name; the kind
/// column only when more than one kind is in play. The same rule `list`
/// follows, so a report never spends a column on something its reader could
/// not have been confused about.
fn rows(status: &Status, across: bool) -> Option<String> {
    let listed = status
        .projects
        .iter()
        .filter(|project| project.stale.is_some() || !project.artifacts.is_empty())
        .count();
    if listed == 0 {
        return None;
    }
    let stale = status
        .projects
        .iter()
        .any(|project| project.stale.is_some());
    let named = across && (listed > 1 || stale);
    let kinds: Vec<Kind> = status
        .projects
        .iter()
        .flat_map(|project| project.artifacts.iter().map(|artifact| artifact.kind))
        .collect();
    let kinded = kinds.iter().any(|kind| Some(kind) != kinds.first());

    let mut rows: Vec<Vec<String>> = Vec::new();
    for project in &status.projects {
        // Made printable as every cell is: a name, an entry and an origin are
        // all from files somebody wrote.
        let label = printable(
            project
                .name
                .as_deref()
                .or(project.entry.as_deref())
                .unwrap_or("?"),
        );
        if project.stale.is_some() {
            let mut row = vec![label.clone()];
            if kinded {
                row.push(String::new());
            }
            row.extend([
                String::new(),
                STALE.to_owned(),
                String::new(),
                String::new(),
            ]);
            rows.push(row);
            continue;
        }
        for artifact in &project.artifacts {
            let mut row = Vec::new();
            if named {
                row.push(label.clone());
            }
            if kinded {
                row.push(artifact.kind.slug().to_owned());
            }
            row.push(printable(&artifact.name));
            row.push(artifact.state.slug().to_owned());
            row.push(agents(&artifact.agents));
            row.push(printable(artifact.origin.as_deref().unwrap_or("")));
            rows.push(row);
        }
    }
    // A column nothing in it fills is left out: the names and the agents of
    // a report whose every row is a stale entry, which would otherwise pad
    // a gap between its name and the word.
    let filled: Vec<bool> = (0..rows.first().map_or(0, Vec::len))
        .map(|column| rows.iter().any(|row| !row[column].is_empty()))
        .collect();
    let rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .zip(&filled)
                .filter_map(|(cell, filled)| filled.then_some(cell))
                .collect()
        })
        .collect();
    Some(table(&rows))
}

/// The agents an artifact reaches as `--agents` takes them, comma-separated,
/// so the column holds no space; `-` for none, which is something to say: a
/// project's own skill where no agent reads reaches none.
fn agents(agents: &[Agent]) -> String {
    if agents.is_empty() {
        return String::from("-");
    }
    agents
        .iter()
        .map(|agent| agent.name())
        .collect::<Vec<_>>()
        .join(",")
}
