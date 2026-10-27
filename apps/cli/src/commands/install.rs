//! The commands that put one skill into one project and take it back out:
//! `install` and `uninstall`, at either level. The screen `flayer install`
//! opens on its own is [`crate::install`].

use std::path::Path;

use mindflayer_core::{AgentHomes, FlayerWorkspace, Kind, MindProject};

use crate::commands::gather::scrubbed;
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};

/// Put one skill into one project, from wherever it is offered.
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
/// So is each folder the project's settings name for skills that no install
/// uses ([`MindProject::misplaced_targets`]): no copy went there, and an
/// install refused for want of any other is refused for that.
///
/// `homes` are the agents' own folders the command line decided, which core
/// refuses to install into.
pub(crate) fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
    from: Option<&str>,
    directory: &Path,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};
    use mindflayer_core::Excluded;

    let mut warnings = load_warnings(workspace);
    warnings.extend(misplaced(project));
    let (ledger, scrub) = workspace.ledger_to_write().map_err(|error| Failure {
        error: Box::new(error.into()),
        warnings: warnings.clone(),
    })?;
    let said = scrubbed(&scrub);
    // What the scrub said goes with an error too: it happened all the same.
    let mut told = said.clone().lines();
    told.extend(warnings.iter().cloned());
    let with_warnings = |error: CliError| Failure {
        error: Box::new(error),
        warnings: told.clone(),
    };
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
    let done =
        installing::install(workspace, &ledger, project, &candidate, homes).map_err(|error| {
            // What it cleared before it failed comes first, being what it did,
            // after only what the scrub before it did: on stderr with the error,
            // the only place a failure prints.
            let mut told = said.clone().lines();
            told.extend(cleared(project, error.swept()));
            told.extend(warnings.iter().cloned());
            Failure {
                error: Box::new(error.into()),
                warnings: told,
            }
        })?;
    let unhashed = done.installed.unhashed().map(str::to_owned);
    let mut outcome = match done.installed {
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
            Installed::Foreign { name, path } => Outcome {
                stdout: String::new(),
                stderr: vec![format!(
                    "warning: {into} already has a {name} at {} that Mindflayer did not put there, so it was left alone",
                    shown(project, &path)
                )],
                ok: false,
            },
    };
    outcome.stdout.push_str(&swept(project, &done.swept));
    // Installed all the same, but not all of what was offered, which is
    // worth a look before an agent runs it.
    for left in &done.left_out {
        outcome
            .stderr
            .push(format!("warning: {name} in {into} {}", left_out(left)));
        outcome.ok = false;
    }
    // A note rather than a warning: a project need not be in git, and
    // nothing went wrong, but what local mode keeps out of git is then kept
    // out of nothing.
    if let Some(note) = done.excluded.as_ref().and_then(Excluded::warning) {
        outcome.stderr.push(format!("note: {note}"));
    }
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
    Ok(said.before(outcome))
}

/// A warning for each folder `project`'s settings name for skills that no
/// install uses, the one kind installed for now.
pub(crate) fn misplaced(project: &MindProject) -> Vec<String> {
    project
        .misplaced_targets()
        .iter()
        .filter(|misplaced| misplaced.kind == Kind::Skill)
        .map(|misplaced| format!("warning: {misplaced}"))
        .collect()
}

/// A line for each thing interrupted installs had left in `project` that
/// installing cleared: on stdout, under what the install did, because it is
/// part of what the install did rather than something wrong with it.
pub(crate) fn swept(project: &MindProject, swept: &[std::path::PathBuf]) -> String {
    cleared(project, swept)
        .iter()
        .map(|line| format!("  {line}\n"))
        .collect()
}

/// What [`swept`] says of each, unindented, for where there is no line of
/// what the install did to put it under. Shown from the project, where
/// somebody would go and look.
pub(crate) fn cleared(project: &MindProject, swept: &[std::path::PathBuf]) -> Vec<String> {
    swept
        .iter()
        .map(|path| {
            format!(
                "cleared {}, left by an install that did not finish",
                shown(project, path)
            )
        })
        .collect()
}

/// What to say after a skill's name of what installing it left out.
pub(crate) fn left_out(left: &mindflayer_core::install::LeftOut) -> String {
    let name = left.path.file_name().map_or_else(
        || left.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!("was installed without its {name}, since {}", left.reason)
}

/// What to say of a skill taken out of a project whose exclude block could not
/// be edited to stop naming it.
pub(crate) fn still(name: &str, project: &str, reason: &str) -> String {
    format!(
        "warning: {name} is gone from {project}, but the exclude file still keeps its place out \
         of git: {reason}"
    )
}

/// A path in `project` as somebody standing in it would spell it, or as it
/// is when it is not in there.
pub(crate) fn shown(project: &MindProject, path: &Path) -> String {
    use mindflayer_core::paths;

    paths::relative_to(path, project.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| path.display().to_string())
}

/// Take one skill Mindflayer installed back out of one project.
pub(crate) fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Removed};

    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    let from = project.name();
    let failed = |error: CliError, said: super::gather::Scrubbed| Failure {
        error: Box::new(error),
        warnings: said.lines(),
    };
    let taken = match installing::uninstall(workspace, &ledger, project, Kind::Skill, name) {
        Ok(taken) => taken,
        Err(error) => return Err(failed(error.into(), said)),
    };
    match taken {
        Removed::Removed {
            name,
            still_excluded,
            ..
        } => {
            let mut outcome = Outcome::plain(format!("removed {name} from {from}\n"));
            if let Some(reason) = still_excluded {
                outcome.stderr.push(still(&name, from, &reason));
                outcome.ok = false;
            }
            Ok(said.before(outcome))
        }
        Removed::Foreign { name, path } => Ok(said.before(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {name} at {} in {from} was not installed by Mindflayer, so it was left alone",
                shown(project, &path)
            )],
            ok: false,
        })),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(failed(
            CliError::NotInstalled {
                name,
                project: from.to_owned(),
            },
            said,
        )),
    }
}
