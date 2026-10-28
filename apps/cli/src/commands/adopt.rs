//! `flayer adopt`: a skill somebody keeps in a project's agents' folders
//! brought into the workspace's library, and the project's copies recorded
//! as installed from there, as core plans and does it
//! ([`mindflayer_core::adopt`]). Here it is only said.
//!
//! No `mind` form: what it writes is the workspace's library, which `mind`
//! changes nowhere, as `flayer add` without `-p` has none.

use std::fmt::Write as _;

use mindflayer_core::adopt::{self, Git, Plan};
use mindflayer_core::{AgentHomes, Excluded, FlayerWorkspace, MindProject, Reference};

use crate::commands::gather::scrubbed;
use crate::commands::install::shown;
use crate::render::printable;
use crate::{CliError, Failure, Outcome};

/// What a dry run ends with.
const UNADOPTED: &str = "nothing adopted: add --yes to go ahead\n";

/// `flayer adopt -p <project> <name>`: what adopting the skill `typed`
/// names from `project` would do, without `yes`, nothing written; with it,
/// done, and said the same way in the past tense.
///
/// A line for the library copy, one for the copies recorded, one for each
/// target that has no copy, which reads as missing until an update puts one
/// there, and in a local project one for each copy kept out of git; a note
/// for one git tracks, which stays in git. What `gh skill` wrote of where it
/// came from is shown under the first line, and nothing more is made of it.
///
/// `homes` are the agents' own folders the command line decided, which core
/// refuses to adopt from.
pub(crate) fn adopt_from(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    typed: &str,
    yes: bool,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    let reference = Reference::parse(typed);
    if !yes {
        let plan = adopt::plan(workspace, project, &reference, homes).map_err(CliError::from)?;
        let mut outcome = said(workspace, &plan, false);
        outcome.stdout.push_str(UNADOPTED);
        return Ok(outcome);
    }
    let adopted = adopt::apply(workspace, project, &reference, homes).map_err(CliError::from)?;
    let mut outcome = said(workspace, &adopted.plan, true);
    // A note rather than a warning, as for an install: a project need not
    // be in git, and nothing went wrong.
    if let Some(note) = adopted.excluded.as_ref().and_then(Excluded::warning) {
        outcome.stderr.push(format!("note: {note}"));
    }
    Ok(scrubbed(&adopted.scrub).before(outcome))
}

/// What `plan` does, or did once `done`.
fn said(workspace: &FlayerWorkspace, plan: &Plan, done: bool) -> Outcome {
    let project = &plan.project;
    let into = project.name();
    let name = &plan.name;
    let (copy, record, keep) = if done {
        ("copied", "recorded", "kept")
    } else {
        ("would copy", "would record", "would keep")
    };
    let mut outcome = Outcome::plain(String::new());
    let _ = writeln!(
        outcome.stdout,
        "{copy} {name} from {into} into the library, at {}",
        plan.library.display()
    );
    if !plan.provenance.is_empty() {
        // Printable, as every row from a file somebody wrote is: this one is
        // often from somebody else's repository, and it is what is read
        // before saying yes, so a carriage return must not rewrite the line.
        let entries: Vec<String> = plan
            .provenance
            .iter()
            .map(|(key, value)| format!("{} {}", printable(key), printable(value)))
            .collect();
        let _ = writeln!(
            outcome.stdout,
            "  from gh skill, as its SKILL.md says: {}",
            entries.join(", ")
        );
    }
    let there: Vec<String> = plan
        .copies
        .iter()
        .filter(|copy| copy.there)
        .map(|copy| shown(project, &copy.path))
        .collect();
    let _ = writeln!(
        outcome.stdout,
        "{record} {name} in {into} as installed from workspace, at {}",
        there.join(", ")
    );
    let picked = workspace.naming(project);
    for missing in plan.copies.iter().filter(|copy| !copy.there) {
        let at = shown(project, &missing.path);
        let _ = if done {
            writeln!(
                outcome.stdout,
                "{record} {at} missing: `flayer update -p {picked}` puts it there"
            )
        } else {
            writeln!(
                outcome.stdout,
                "{record} {at} missing, for `flayer update -p {picked}` to put there"
            )
        };
    }
    for copy in &plan.copies {
        let at = shown(project, &copy.path);
        match copy.git {
            Some(Git::KeptOut) => {
                let _ = writeln!(outcome.stdout, "{keep} {at} out of git");
            }
            Some(Git::Tracked) => outcome.stderr.push(format!(
                "note: git tracks {at}, so it stays in git though {into} is local: what an update \
                 writes there shows in `git status`"
            )),
            None => {}
        }
    }
    outcome
}
