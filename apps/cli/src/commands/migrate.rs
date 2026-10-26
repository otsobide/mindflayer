//! `flayer migrate`: bringing a workspace an older Mindflayer made into the
//! layout a new one has. `--library` is the one migration so far, and core's
//! [`migrate`] decides everything about it; here it is said.

use std::fmt::Write as _;

use mindflayer_core::migrate::{self, MigrateError, Plan};
use mindflayer_core::FlayerWorkspace;

use super::all_or_nothing;
use crate::{CliError, Failure, Outcome};

/// `flayer migrate --library`: what moving the workspace's own into its
/// library would do, or, with `yes`, doing it.
///
/// The dry run is a report, and exits as one, where `remove`'s is a
/// refusal: what it gives is a line per artifact to read before saying yes,
/// not one deletion that was asked for and did not happen. Refused, both say
/// every reason at once, as a command given several things does, since each
/// has to be put right before it can go ahead.
pub(crate) fn migrate_library(workspace: &FlayerWorkspace, yes: bool) -> Result<Outcome, Failure> {
    if !yes {
        let mut plan = migrate::library_plan(workspace);
        refused(std::mem::take(&mut plan.refusals))?;
        if plan.is_empty() {
            return Ok(Outcome::plain(String::from(NOTHING)));
        }
        let mut stdout = report(workspace, &plan, false);
        stdout.push_str("nothing moved: add --yes to go ahead\n");
        return Ok(Outcome::plain(stdout));
    }

    let mut migrating = workspace.clone();
    match migrate::library(&mut migrating) {
        Ok(migrated) if migrated.plan.is_empty() => Ok(Outcome::plain(String::from(NOTHING))),
        Ok(migrated) => Ok(Outcome::plain(report(&migrating, &migrated.plan, true))),
        // Never without a reason, so what follows the refusal is not reached.
        Err(MigrateError::Refused(refusals)) => {
            refused(refusals)?;
            Ok(Outcome::plain(String::from(NOTHING)))
        }
        Err(error) => {
            // Said with the error, being what the migration left where the
            // workspace no longer looks: it could not be put back.
            let warnings = error
                .stranded()
                .iter()
                .map(|one| {
                    format!(
                        "warning: {} was moved and could not be put back; move it to {} yourself",
                        one.to.display(),
                        one.from.display()
                    )
                })
                .collect();
            Err(Failure {
                error: Box::new(CliError::Migrate(error)),
                warnings,
            })
        }
    }
}

/// What a migration that has nothing to do says, with or without `--yes`.
const NOTHING: &str = "nothing to move: the workspace keeps its own in its library already\n";

/// Every refusal, the first as the error and the rest before it.
fn refused(refusals: Vec<migrate::Refusal>) -> Result<(), Failure> {
    match all_or_nothing(refusals.into_iter().map(CliError::Refused).collect()) {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// A line for each artifact moved, each one a project keeps, and the
/// marker's rewrite: as what will happen in a dry run, and as what did.
fn report(workspace: &FlayerWorkspace, plan: &Plan, done: bool) -> String {
    let (moving, leaving, uncounted) = if done {
        ("moved", "left", "no longer counts")
    } else {
        ("would move", "would leave", "would no longer count")
    };
    let mut text = String::new();
    for one in &plan.moves {
        let _ = writeln!(
            text,
            "{moving} {} to {}",
            one.from.display(),
            one.to.display()
        );
    }
    for left in &plan.left {
        // Said on its line, since it is what changes for whoever installed
        // it: the workspace listed and offered it, and will not.
        let _ = write!(
            text,
            "{leaving} {} where it is: the project at {} holds it",
            left.path.display(),
            left.project.display()
        );
        if left.counted {
            let _ = write!(text, ", and the workspace {uncounted} it as its own");
        }
        text.push('\n');
    }
    let kinds: Vec<&str> = plan
        .kinds
        .iter()
        .map(|repoint| repoint.kind.folder())
        .collect();
    let marker = workspace.config_path();
    let _ = writeln!(
        text,
        "{} {} in {} at the library, {} in {}",
        if done { "pointed" } else { "would point" },
        listed(&kinds),
        marker.display(),
        if done {
            "kept as it was"
        } else {
            "keeping it as it is"
        },
        plan.backup.display(),
    );
    text
}

/// `skills`, `skills and rules`, `a, b and c`.
fn listed(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
