//! The commands that put one skill into one project and take it back out:
//! `install` and `uninstall`, at either level. The screen `flayer install`
//! opens on its own is [`crate::install`].

use std::path::Path;

use mindflayer_core::{FlayerWorkspace, Kind, MindProject};

use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};

/// Put one skill into one project, from wherever it is offered.
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
pub(crate) fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
    from: Option<&str>,
    directory: &Path,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};

    let warnings = load_warnings(workspace);
    let with_warnings = |error: CliError| Failure {
        error: Box::new(error),
        warnings: warnings.clone(),
    };
    let ledger = workspace
        .ledger()
        .map_err(|error| with_warnings(error.into()))?;
    // As typed first, so a shelf source's address or `workspace` means what
    // it says; only when that names nothing, as a path to a loaded workspace.
    let offered = |from: Option<&str>| {
        installing::offered(workspace, &ledger, project, Kind::Skill, name, from)
    };
    let candidate = match (offered(from), from) {
        (Err(installing::InstallError::NotOffered { .. }), Some(typed)) => {
            offered(Some(&installing::resolve_from(workspace, directory, typed)))
        }
        (result, _) => result,
    }
    .map_err(|error| with_warnings(error.into()))?;
    let origin = candidate.origin();
    let into = project.name();
    let done = installing::install(workspace, &ledger, project, &candidate)
        .map_err(|error| with_warnings(error.into()))?;
    let unhashed = done.unhashed().map(str::to_owned);
    let mut outcome = match done {
            Installed::Added { name, .. } => {
                Outcome::plain(format!("installed {name} into {into}, from {origin}\n"))
            }
            Installed::Updated { name, .. } => {
                Outcome::plain(format!("updated {name} in {into}, from {origin}\n"))
            }
            Installed::Unchanged { name, .. } => {
                Outcome::plain(format!("{name} in {into} was already current\n"))
            }
            // Refused rather than obeyed: somebody wrote that one.
            Installed::Foreign { name } => Outcome {
                stdout: String::new(),
                stderr: vec![format!(
                    "warning: {into} already has a {name} that Mindflayer did not put there, so it was left alone"
                )],
                ok: false,
            },
    };
    // Installed and recorded all the same, but without what tells a later
    // edit apart, which is worth a look at what is in the copy.
    if let Some(reason) = unhashed {
        outcome.stderr.push(format!(
            "warning: {name} in {into} was recorded without a hash of what it holds: {reason}"
        ));
        outcome.ok = false;
    }
    if !warnings.is_empty() {
        outcome.stderr.extend(warnings);
        outcome.ok = false;
    }
    Ok(outcome)
}

/// Take one skill Mindflayer installed back out of one project.
pub(crate) fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Removed};

    let ledger = workspace.ledger().map_err(CliError::from)?;
    let from = project.name();
    match installing::uninstall(workspace, &ledger, project, Kind::Skill, name)
        .map_err(CliError::from)?
    {
        Removed::Removed { name, .. } => {
            Ok(Outcome::plain(format!("removed {name} from {from}\n")))
        }
        Removed::Foreign { name } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {name} in {from} was not installed by Mindflayer, so it was left alone"
            )],
            ok: false,
        }),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(CliError::NotInstalled {
            name,
            project: from.to_owned(),
        }
        .into()),
    }
}
