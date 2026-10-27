//! The commands that change a project's settings and carry what is
//! installed into it along: `agents` and `share`, at either level.
//!
//! Core's [`reapply`] decides everything, what moves and what is left; here
//! it is said, as what would happen without `--yes` and as what did with it.

use std::fmt::Write as _;

use mindflayer_core::reapply::{self, Change, Ignore, Plan, Retarget, Why};
use mindflayer_core::{Agent, AgentHomes, Excluded, FlayerWorkspace, MindProject, Share};

use super::consulted;
use super::gather::scrubbed;
use super::install::{cleared, shown};
use crate::{AgentNames, AgentsChange, CliError, Failure, Lookup, Outcome};

/// The change an `agents` line asks for, and whether it said `--yes`,
/// before or after `add` or `remove`.
pub(crate) fn agents_change(change: Option<&AgentsChange>, named: &AgentNames) -> (Change, bool) {
    match change {
        None => (Change::Agents(named.agents.clone()), named.yes),
        Some(AgentsChange::Add(added)) => (
            Change::AddAgents(added.agents.clone()),
            named.yes || added.yes,
        ),
        Some(AgentsChange::Remove(removed)) => (
            Change::RemoveAgents(removed.agents.clone()),
            named.yes || removed.yes,
        ),
    }
}

/// `mind agents` and `mind share`: the change to the project at or above
/// here, carried through the workspace `mind` consults about it when that
/// one links it.
///
/// A project nothing links has its settings changed all the same, since they
/// are its own (D-09), with a hint saying what was not carried along, as
/// `mind status` says why nothing reads as installed. A workspace found that
/// cannot be read is a warning beside the change, as for any other change to
/// a project; one `-W` or the activation names that is no workspace refuses
/// it, as it refuses every command.
pub(crate) fn change_here(
    project: &MindProject,
    change: &Change,
    yes: bool,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (None, Some(format!("warning: {error}"))),
    };
    let mut result = change_project(workspace.as_ref(), project, change, yes, &lookup.agents);
    // Not when nothing changes, since then nothing was left to carry; nor
    // when the workspace could not be read, whose warning says why.
    let changed = !matches!(&result, Ok(outcome) if outcome.stdout.starts_with(NOTHING));
    let hint = match (&workspace, &unread) {
        _ if !changed => None,
        (Some(workspace), _) if !workspace.is_linked(project.root()) => Some(format!(
            "hint: the workspace at {} does not link this project, so nothing installed in it \
             was carried along; `mind link` links it",
            workspace.root().display()
        )),
        (None, None) => Some(String::from(
            "hint: no workspace links this project, so nothing installed in it was carried along",
        )),
        _ => None,
    };
    let unreadable = unread.is_some();
    let said: Vec<String> = unread.into_iter().chain(hint).collect();
    match &mut result {
        Ok(outcome) => {
            outcome.ok &= !unreadable;
            outcome.stderr.extend(said);
        }
        Err(failure) => failure.warnings.extend(said),
    }
    result
}

/// `flayer agents -p` and `flayer share -p`: the change to one project the
/// workspace manages, carrying what its ledger records along.
pub(crate) fn change_of(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    change: &Change,
    yes: bool,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    change_project(Some(workspace), project, change, yes, homes)
}

/// What starts the line a change that changes nothing prints.
const NOTHING: &str = "nothing to change";

/// The change, planned without `yes` and made with it, laid out.
///
/// The dry run is a report, and exits as one, as `flayer migrate`'s does:
/// what it gives is a line for each thing that would change, to read before
/// saying yes. Made, a copy left out of a folder an agent the project now
/// names reads, over somebody's skill or for want of one to copy, is a
/// warning, as an install refused over somebody's is.
fn change_project(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    change: &Change,
    yes: bool,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    if !yes {
        let plan = reapply::plan(workspace, project, change, homes).map_err(CliError::from)?;
        if plan.is_empty() {
            return Ok(Outcome::plain(nothing(project, change, &plan)));
        }
        let mut outcome = Outcome::plain(report(project, &plan, false));
        outcome
            .stdout
            .push_str("nothing changed: add --yes to go ahead\n");
        outcome.stderr.extend(outside(project, &plan));
        return Ok(outcome);
    }

    let applied = reapply::apply(workspace, project, change, homes).map_err(CliError::from)?;
    let said = scrubbed(&applied.scrub);
    let plan = &applied.plan;
    if plan.is_empty() {
        return Ok(said.before(Outcome::plain(nothing(project, change, plan))));
    }
    let mut outcome = Outcome::plain(report(project, plan, true));
    for line in cleared(project, &applied.swept) {
        let _ = writeln!(outcome.stdout, "  {line}");
    }
    let into = project.name();
    for retarget in &plan.installations {
        for left in &retarget.left {
            let name = &retarget.name;
            let at = shown(project, &left.path);
            let warning = match &left.why {
                Why::Foreign => format!(
                    "warning: {into} already has a {name} at {at} that Mindflayer did not put \
                     there, so it was left alone, and {name} was not copied there"
                ),
                Why::Unsourced => format!(
                    "warning: {name} was not copied to {at}: no copy of it in {into} still holds \
                     what was installed"
                ),
                Why::Edited | Why::Uncompared(_) => continue,
            };
            outcome.stderr.push(warning);
            outcome.ok = false;
        }
    }
    for failure in &applied.failures {
        outcome.stderr.push(format!("warning: {failure}"));
        outcome.ok = false;
    }
    outcome.stderr.extend(outside(project, plan));
    Ok(said.before(outcome))
}

/// What a change that changes nothing says: what the project is already.
fn nothing(project: &MindProject, change: &Change, plan: &Plan) -> String {
    let name = project.name();
    match change {
        Change::Share { .. } => format!(
            "{NOTHING}: {name} is {} already{}\n",
            plan.after.share,
            if plan.after.edit_tracked {
                ", with edit_tracked"
            } else {
                ""
            }
        ),
        _ => format!(
            "{NOTHING}: {name} names {} already\n",
            agents(&plan.after.agents)
        ),
    }
}

/// A line for each thing the change does, as what would happen in a dry
/// run, and as what did once it is made: the settings first, then what
/// keeps `.mind` out of git, each installation's copies, what is left, and
/// the exclude file.
fn report(project: &MindProject, plan: &Plan, done: bool) -> String {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    let name = project.name();
    let (before, after) = (&plan.before, &plan.after);
    let mut text = String::new();
    if before.agents != after.agents {
        let _ = writeln!(
            text,
            "{} agents to {} in {name}, where they were {}",
            would("set", "would set"),
            agents(&after.agents),
            agents(&before.agents)
        );
    }
    if before.share != after.share {
        let _ = writeln!(
            text,
            "{} share to {} in {name}, where it was {}",
            would("set", "would set"),
            after.share,
            before.share
        );
    }
    if before.edit_tracked != after.edit_tracked {
        let _ = writeln!(
            text,
            "{} edit_tracked to {} in {name}, where it was {}",
            would("set", "would set"),
            after.edit_tracked,
            before.edit_tracked
        );
    }
    match &plan.ignore {
        Some(Ignore::Write(path)) => {
            let _ = writeln!(
                text,
                "{} {}, keeping .mind out of git",
                would("wrote", "would write"),
                shown(project, path)
            );
        }
        Some(Ignore::Remove(path)) => {
            let _ = writeln!(
                text,
                "{} {}, so .mind can be committed",
                would("removed", "would remove"),
                shown(project, path)
            );
        }
        Some(Ignore::Kept(path)) => {
            let _ = writeln!(
                text,
                "{} {} as it is: it is not the one Mindflayer writes, so it may still keep .mind \
                 out of git",
                would("left", "would leave"),
                shown(project, path)
            );
        }
        None => {}
    }
    let local = after.share == Share::Local;
    for retarget in &plan.installations {
        moved(&mut text, project, retarget, done, local);
    }
    for path in &plan.foreign {
        let _ = writeln!(
            text,
            "{} {} where it is: Mindflayer did not install it",
            would("left", "would leave"),
            shown(project, path)
        );
    }
    for path in &plan.excluded {
        let _ = writeln!(
            text,
            "{} {} out of git",
            would("kept", "would keep"),
            shown(project, path)
        );
    }
    for line in &plan.unexcluded {
        let _ = writeln!(
            text,
            "{} {line} out of git",
            would("stopped keeping", "would stop keeping")
        );
    }
    text
}

/// The lines for one installation's copies. In a dry run a copy left out
/// of a new folder is one of them; made, it is a warning instead.
fn moved(text: &mut String, project: &MindProject, retarget: &Retarget, done: bool, local: bool) {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    let name = &retarget.name;
    for added in &retarget.added {
        let _ = writeln!(
            text,
            "{} {name} to {}, for {}{}",
            would("copied", "would copy"),
            shown(project, &added.path),
            spoken(&added.agents),
            if local { ", kept out of git" } else { "" }
        );
    }
    for path in &retarget.removed {
        let _ = writeln!(
            text,
            "{} {name} from {}",
            would("removed", "would remove"),
            shown(project, path)
        );
    }
    for path in &retarget.forgotten {
        let _ = writeln!(
            text,
            "{} {name}'s copy at {}, which is gone",
            would("forgot", "would forget"),
            shown(project, path)
        );
    }
    for left in &retarget.left {
        let at = shown(project, &left.path);
        match &left.why {
            Why::Edited => {
                let _ = writeln!(
                    text,
                    "{} {at} where it is, no longer Mindflayer's: it was changed since it was \
                     installed",
                    would("left", "would leave")
                );
            }
            Why::Uncompared(reason) => {
                let _ = writeln!(
                    text,
                    "{} {at} where it is, no longer Mindflayer's: whether it was changed since it \
                     was installed cannot be told: {reason}",
                    would("left", "would leave")
                );
            }
            Why::Foreign if !done => {
                let _ = writeln!(
                    text,
                    "would leave {at} as it is: Mindflayer did not put it there, so {name} would \
                     not be copied there"
                );
            }
            Why::Unsourced if !done => {
                let _ = writeln!(
                    text,
                    "would not copy {name} to {at}: no copy of it still holds what was installed"
                );
            }
            Why::Foreign | Why::Unsourced => {}
        }
    }
}

/// What a local project outside every git work tree is told, as an install
/// there tells it: a note, since nothing went wrong.
fn outside(project: &MindProject, plan: &Plan) -> Option<String> {
    plan.outside
        .then(|| {
            Excluded::Outside {
                project: project.root().to_path_buf(),
            }
            .warning()
        })
        .flatten()
        .map(|note| format!("note: {note}"))
}

/// Agents as `--agents` takes them, comma-separated, so the line holds no
/// space for a script splitting on spaces to cut.
fn agents(agents: &[Agent]) -> String {
    agents
        .iter()
        .map(|agent| agent.name())
        .collect::<Vec<_>>()
        .join(",")
}

/// Agents as a sentence says them: `codex`, `codex and opencode`.
fn spoken(agents: &[Agent]) -> String {
    match agents {
        [] => String::new(),
        [one] => one.name().to_owned(),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(|agent| agent.name()).collect();
            format!("{} and {}", rest.join(", "), last.name())
        }
    }
}
