//! The commands that put one skill or rule into one project and take it
//! back out: `install` and `uninstall`, at either level. The screen `flayer
//! install` opens on its own is [`crate::install`].

use std::path::Path;

use mindflayer_core::install::rules::Rendered;
use mindflayer_core::{AgentHomes, FlayerWorkspace, Kind, MindProject, Reference};

use crate::commands::gather::scrubbed;
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};

/// Put what `typed` names into one project, from wherever it is offered: a
/// skill or a rule, by its name alone or qualified (`rule:git/commits`), as
/// core finds it ([`offered_typed`](mindflayer_core::install::offered_typed)).
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
/// So is each folder the project's settings name for the kind that no install
/// uses ([`MindProject::misplaced_targets`]): no copy went there, and an
/// install refused for want of any other is refused for that. Every kind the
/// name could be, as core says ([`kinds_named`]), when it is refused before
/// it is known which.
///
/// [`kinds_named`]: mindflayer_core::install::kinds_named
///
/// `homes` are the agents' own folders the command line decided, which core
/// refuses to install into.
pub(crate) fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    typed: &str,
    from: Option<&str>,
    directory: &Path,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};
    use mindflayer_core::Excluded;

    let reference = Reference::parse(typed);
    let loads = load_warnings(workspace);
    let (ledger, scrub) = workspace.ledger_to_write().map_err(|error| {
        let mut warnings = loads.clone();
        warnings.extend(misplaced(project, &installing::kinds_named(&reference)));
        Failure {
            error: Box::new(error.into()),
            warnings,
        }
    })?;
    let said = scrubbed(&scrub);
    // As typed first, so a shelf source's address or `workspace` means what
    // it says; only when that names nothing, as a path to a loaded workspace.
    let offered = |from: Option<&str>| {
        installing::offered_typed(workspace, &ledger, project, &reference, from)
    };
    let candidate = match (offered(from), from) {
        (Err(installing::InstallError::NotOffered { .. }), Some(typed)) => {
            offered(Some(&installing::resolve_from(workspace, directory, typed)))
        }
        (result, _) => result,
    }
    .map_err(|error| {
        // What the scrub said goes with an error too: it happened all the
        // same.
        let mut told = said.clone().lines();
        told.extend(loads.iter().cloned());
        told.extend(misplaced(project, &installing::kinds_named(&reference)));
        Failure {
            error: Box::new(error.into()),
            warnings: told,
        }
    })?;
    let kind = candidate.kind();
    let mut warnings = loads;
    warnings.extend(misplaced(project, &[kind]));
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
            Installed::Added { name, .. } => Outcome::plain(format!(
                "installed {} into {into}, from {origin}\n",
                named(kind, &name)
            )),
            Installed::Updated { name, .. } => Outcome::plain(format!(
                "updated {} in {into}, from {origin}\n",
                named(kind, &name)
            )),
            Installed::Unchanged { name, .. } => Outcome::plain(format!(
                "{} in {into} was already current\n",
                named(kind, &name)
            )),
            // Refused rather than obeyed: somebody wrote that one.
            Installed::Foreign { name, path } => Outcome {
                stdout: String::new(),
                stderr: vec![format!(
                    "warning: {into} already has a {} at {} that Mindflayer did not put there, so it was left alone",
                    named(kind, &name),
                    shown(project, &path)
                )],
                ok: false,
            },
    };
    let name = named(kind, candidate.name());
    outcome.stdout.push_str(&swept(project, &done.swept));
    // Installed all the same, but not all of what was offered, which is
    // worth a look before an agent runs it.
    for left in &done.left_out {
        outcome
            .stderr
            .push(format!("warning: {name} in {into} {}", left_out(left)));
        outcome.ok = false;
    }
    // Installed, and what the project's rules came to on the way, which can
    // leave some of its agents reading none of them.
    if let Some(rendered) = &done.rendered {
        let (warned, notes) = render_said(rendered, candidate.name(), None);
        if !warned.is_empty() {
            outcome.ok = false;
        }
        outcome.stderr.extend(warned);
        if let Some(reason) = &rendered.still_excluded {
            outcome.stderr.push(format!(
                "warning: {into}: the exclude file still keeps out of git a file that is no \
                 longer Mindflayer's: {reason}"
            ));
            outcome.ok = false;
        }
        outcome.stderr.extend(notes);
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

/// How a line about installing names what it is about: a skill by its name
/// alone, as every such line did before a rule could be installed, and a
/// rule after its kind, as `add` and `remove` name one, since a route reads
/// like a path and one name can be a skill's and a rule's.
pub(crate) fn named(kind: Kind, name: &str) -> String {
    match kind {
        Kind::Skill => name.to_owned(),
        Kind::Rule => format!("{kind} {name}"),
    }
}

/// What rendering a project's rules for a change of the rule `changed` has
/// to say ([`Rendered::warnings_of`] and [`Rendered::notes`]), as `warning:`
/// and `note:` lines: each warning said of `project` when one is given, as
/// the install screen says every line of the project it is about. A note
/// names its project already.
pub(crate) fn render_said(
    rendered: &Rendered,
    changed: &str,
    project: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let of = project.map_or_else(String::new, |project| format!("{project}: "));
    (
        rendered
            .warnings_of(Some(changed))
            .iter()
            .map(|warning| format!("warning: {of}{warning}"))
            .collect(),
        rendered
            .notes()
            .iter()
            .map(|note| format!("note: {note}"))
            .collect(),
    )
}

/// A warning for each folder `project`'s settings name for one of `kinds`
/// that no install uses.
pub(crate) fn misplaced(project: &MindProject, kinds: &[Kind]) -> Vec<String> {
    project
        .misplaced_targets()
        .iter()
        .filter(|misplaced| kinds.contains(&misplaced.kind))
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

/// Take what `typed` names, a skill or a rule Mindflayer installed, back out
/// of one project, handing core the agents' own folders, `homes`, as an
/// install does.
pub(crate) fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    typed: &str,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Removed};

    let reference = Reference::parse(typed);
    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    let from = project.name();
    let failed = |error: CliError, said: super::gather::Scrubbed| Failure {
        error: Box::new(error),
        warnings: said.lines(),
    };
    let (kind, taken) =
        match installing::uninstall_typed(workspace, &ledger, project, &reference, homes) {
            Ok(taken) => taken,
            Err(error) => return Err(failed(error.into(), said)),
        };
    match taken {
        Removed::Removed {
            name,
            still_excluded,
            rendered,
            ..
        } => {
            // What the rules left come to without it.
            let (warned, notes) = rendered
                .map(|rendered| render_said(&rendered, &name, None))
                .unwrap_or_default();
            let name = named(kind, &name);
            let mut outcome = Outcome::plain(format!("removed {name} from {from}\n"));
            outcome.ok = warned.is_empty();
            outcome.stderr.extend(warned);
            outcome.stderr.extend(notes);
            if let Some(reason) = still_excluded {
                outcome.stderr.push(still(&name, from, &reason));
                outcome.ok = false;
            }
            Ok(said.before(outcome))
        }
        Removed::Foreign { name, path } => Ok(said.before(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {} at {} in {from} was not installed by Mindflayer, so it was left alone",
                named(kind, &name),
                shown(project, &path)
            )],
            ok: false,
        })),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(failed(
            CliError::NotInstalled {
                name: named(kind, &name),
                project: from.to_owned(),
            },
            said,
        )),
    }
}
