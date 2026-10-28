//! `status`: what each project holds, where each thing in it came from,
//! which agents read it, and whether it is all there.
//!
//! Core builds and orders the report ([`Status`]); this only lays it out, as
//! a table naming no more than its rows need to be told apart.

use mindflayer_core::{
    Agent, FlayerWorkspace, Kind, MarkerFile, MindProject, Missed, Status, StatusFailure, Unreached,
};

use super::consulted;
use super::read::looked_in;
use super::settings::spoken;
use crate::render::{json, printable, table};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// What the state column says of an entry that would not open.
const STALE: &str = "stale";

/// What a report has to say beside it, without the `warning: ` a table
/// puts before it, and the code of what it says when that has one: what
/// could not be read does, and what a project's agents do not see does not.
type Said = (Option<&'static str>, String);

/// `flayer status`: every project the workspace links, against its ledger,
/// which core reads without making it; as JSON when `as_json`.
pub(crate) fn status_of_workspace(workspace: &FlayerWorkspace, as_json: bool) -> Outcome {
    // In JSON, a workspace with nothing linked is one with no projects in
    // its data, which a script reads like any other.
    if workspace.config().projects.is_empty() && !as_json {
        // As `flayer list` says it, and for the same reason: a workspace with
        // nothing linked has nothing to report but how to link something.
        return Outcome::plain(format!(
            "{} manages no projects yet\n  link one with `flayer link <path>`\n",
            workspace.name()
        ));
    }
    let (status, failures) = Status::of_workspace(workspace);
    report(
        &status,
        said(&status, &failures),
        Layout::across(as_json),
        || workspace.projects().0,
    )
}

/// `flayer status -p`: one project the workspace links.
pub(crate) fn status_of_project(
    workspace: &FlayerWorkspace,
    project: MindProject,
    as_json: bool,
) -> Outcome {
    let (status, failures) = Status::of_project(&project, Some(workspace));
    report(
        &status,
        said(&status, &failures),
        Layout::one(as_json),
        || vec![project],
    )
}

/// `mind status`: the project at or above here, against the workspace `mind`
/// consults about it, when that one links it.
///
/// A project nothing links is still reported, as `mind list` reads one: what
/// is in it is its own or somebody's all the same, and the hint says why
/// nothing in it reads as installed.
///
/// The hint goes to stderr in JSON too: it is advice for whoever reads the
/// terminal, not something the data leaves out.
///
/// A workspace found that cannot be read is said as a warning beside the
/// report, in its envelope under `--json`, as [`consulting`](super::consulting)
/// says one beside a change, rather than refusing it: a read has nothing to
/// protect by refusing, and `mind list` reports the same project. Not one
/// `-W` or the activation names that is no workspace, which refuses this as
/// it refuses every other command: whoever chose it meant that one.
pub(crate) fn status_here(
    project: MindProject,
    lookup: &Lookup,
    as_json: bool,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(&project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (None, Some((Some(error.code()), error.to_string()))),
    };
    if let Some(workspace) = workspace
        .as_ref()
        .filter(|workspace| workspace.is_linked(project.root()))
    {
        return Ok(status_of_project(workspace, project, as_json));
    }
    let (status, failures) = Status::of_project(&project, None);
    let mut warnings: Vec<Said> = unread.iter().cloned().collect();
    warnings.extend(said(&status, &failures));
    let mut outcome = report(&status, warnings, Layout::one(as_json), || vec![project]);
    // Not when the workspace could not be read: the warning says why
    // nothing reads as installed, and that none links the project is not
    // known.
    let hint = match (workspace, unread) {
        (Some(workspace), _) => Some(format!(
            "hint: the workspace at {} does not link this project, so nothing in it reads as installed; \
             `mind link` links it",
            workspace.root().display()
        )),
        (None, None) => Some(String::from(
            "hint: no workspace links this project, so nothing in it reads as installed",
        )),
        (None, Some(_)) => None,
    };
    outcome.stderr.extend(hint.map(Warning::from));
    Ok(outcome)
}

/// How a report is laid out.
#[derive(Clone, Copy)]
enum Layout {
    /// As a table: `across` when it is about every project a workspace
    /// links, where a project column can tell rows apart.
    Table { across: bool },
    /// As JSON, in the envelope.
    Json,
}

impl Layout {
    fn across(as_json: bool) -> Self {
        if as_json {
            Layout::Json
        } else {
            Layout::Table { across: true }
        }
    }

    fn one(as_json: bool) -> Self {
        if as_json {
            Layout::Json
        } else {
            Layout::Table { across: false }
        }
    }
}

/// What a report could not read, each as a warning says it, without the
/// prefix, then each kind a project installs only where some of its agents
/// do not read, and each rule it received that some of them read nowhere.
///
/// A warning, and a non-zero exit, though the project is read whole: what
/// is installed there reaches those agents no more than if it had gone,
/// which is what a report is asked to say. Pointing them at the folder, or
/// `targets reset`, ends the first; the second is what a rule about some
/// files is, or a local project's tracked `AGENTS.md` keeps from Codex and
/// OpenCode, which sharing the project or `edit_tracked` ends.
fn said(status: &Status, failures: &[StatusFailure]) -> Vec<Said> {
    let mut said: Vec<Said> = failures
        .iter()
        .map(|failure| (Some(failure.code()), failure.to_string()))
        .collect();
    for project in &status.projects {
        let name = project
            .name
            .as_deref()
            .or(project.entry.as_deref())
            .unwrap_or("?");
        said.extend(
            project
                .unreached
                .iter()
                .map(|unreached| (None, unread(&printable(name), unreached, false))),
        );
        said.extend(
            project
                .missed
                .iter()
                .map(|missed| (None, unseen(&printable(name), missed))),
        );
    }
    said
}

/// What is said of `missed`, something `project` received that some of its
/// agents read nowhere: by `status`, and by the TUI's pane of the project.
pub(crate) fn unseen(project: &str, missed: &Missed) -> String {
    let does = if missed.agents.len() == 1 {
        "does"
    } else {
        "do"
    };
    format!(
        "{} {does} not read {} `{}` in {project}",
        spoken(&missed.agents),
        missed.kind,
        printable(&missed.name)
    )
}

/// What is said of `unreached`, a kind `project` installs only in folders
/// some of its agents do not read: by `status`, of the project as it is, and
/// by a change of its settings, of the project as the change would leave it
/// (`would`) or has left it.
pub(crate) fn unread(project: &str, unreached: &Unreached, would: bool) -> String {
    let kind = unreached.kind;
    let targets: Vec<String> = unreached
        .targets
        .iter()
        .map(|target| printable(target))
        .collect();
    let targets = targets.join(", ");
    let readers = spoken(&unreached.agents);
    let one = unreached.agents.len() == 1;
    if would {
        let tail = if one {
            "it would not see"
        } else {
            "none of them would see"
        };
        format!(
            "{readers} would read none of the folders {project} would install {kind}s in \
             ({targets}), so {tail} what is installed there until pointed at one"
        )
    } else {
        let (reads, tail) = if one {
            ("reads", "it does not see")
        } else {
            ("read", "none of them sees")
        };
        format!(
            "{readers} {reads} none of the folders {project} installs {kind}s in ({targets}), \
             so {tail} what is installed there until pointed at one"
        )
    }
}

/// The report laid out, with what could not be read (`warnings`) said: on
/// stderr beside a table, inside the envelope beside JSON. Then a hint for
/// each project an older marker marks ([`older`]), on stderr in JSON too, as
/// [`status_here`]'s hint is: advice for whoever reads the terminal, which
/// the data says as each project's `marker`.
///
/// `looked` gives the projects it read, asked only for a table that found
/// nothing, to say where it looked.
fn report(
    status: &Status,
    warnings: Vec<Said>,
    layout: Layout,
    looked: impl FnOnce() -> Vec<MindProject>,
) -> Outcome {
    let mut outcome = match layout {
        Layout::Json => json(status, warnings.into_iter().map(|(_, said)| said).collect()),
        Layout::Table { across } => {
            let stderr: Vec<Warning> = warnings
                .into_iter()
                .map(|(code, said)| Warning {
                    text: format!("warning: {said}"),
                    code,
                })
                .collect();
            let ok = stderr.is_empty();
            let stdout = rows(status, across).unwrap_or_else(|| {
                let mut text = String::from("nothing found\n");
                looked_in(&mut text, &looked());
                text
            });
            Outcome { stdout, stderr, ok }
        }
    };
    outcome.stderr.extend(older(status));
    outcome
}

/// What is said of each project an older Mindflayer's marker marks, a
/// `mind.toml` or a `settings.json`, as the TUI's header says it: nothing in
/// the table tells a `settings.json` from a `settings.toml`, which name the
/// same agents, though its settings can be changed only once `mind migrate`
/// has moved it. A hint rather than a warning, so the exit status is the
/// report's: the project is read whole, and what the report says of it is
/// true.
fn older(status: &Status) -> Vec<Warning> {
    status
        .projects
        .iter()
        .filter_map(|project| {
            let marker = project
                .marker
                .filter(|marker| *marker != MarkerFile::SettingsToml)?;
            let name = project
                .name
                .as_deref()
                .or(project.entry.as_deref())
                .unwrap_or("?");
            Some(
                format!(
                    "hint: {} is marked by a {marker}, which `mind migrate` moves to a {}",
                    printable(name),
                    MarkerFile::SettingsToml
                )
                .into(),
            )
        })
        .collect()
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
