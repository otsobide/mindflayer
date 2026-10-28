//! The commands that put one skill or rule into one project and take it
//! back out: `install` and `uninstall`, at either level. The screen `flayer
//! install` opens on its own is [`crate::install`].

use std::path::Path;

use mindflayer_core::assign::{self, Offered};
use mindflayer_core::install::rules::Rendered;
use mindflayer_core::{AgentHomes, FlayerWorkspace, Kind, MindProject, Reference};

use crate::commands::gather::{scrubbed, Scrubbed};
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome};

/// Put what `typed` names into one project, from wherever it is offered: a
/// skill or a rule, by its name alone or qualified (`rule:git/commits`), as
/// core finds it ([`offered_typed`](mindflayer_core::install::offered_typed)),
/// through core's batch of one ([`assign::install`]).
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
/// refuses to install into. `force` writes over a copy edited since it was
/// installed, which core otherwise leaves alone.
pub(crate) fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    typed: &str,
    from: Option<&str>,
    directory: &Path,
    homes: &AgentHomes,
    force: bool,
) -> Result<Outcome, Failure> {
    let reference = Reference::parse(typed);
    let loads = load_warnings(workspace);
    let assigned = assign::install(
        workspace,
        std::slice::from_ref(project),
        &reference,
        from,
        directory,
        homes,
        edits(force),
    )
    .map_err(|error| {
        let mut warnings = loads.clone();
        warnings.extend(misplaced(
            project,
            &mindflayer_core::install::kinds_named(&reference),
        ));
        Failure {
            error: Box::new(error.into()),
            warnings,
        }
    })?;
    let said = scrubbed(&assigned.scrub);
    let mut parts = assigned.parts.into_iter();
    let Some(part) = parts.next() else {
        // A batch of one project hands back its part.
        return Ok(said.before(Outcome::plain(String::new())));
    };
    // What the scrub said goes first, with an error too: it happened all the
    // same.
    alone(
        said,
        install_said(&part.project, part.done, &reference, &loads),
    )
}

/// One project's part of an install, `done`, as a command that installs into
/// it alone says it: a line of what it did and what there is to say beside
/// it, or why it was refused. `loads` are what the loaded workspaces could
/// not offer, said before what is said of the project's folders.
fn install_said(
    project: &MindProject,
    done: assign::Install,
    reference: &Reference,
    loads: &[String],
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};
    use mindflayer_core::Excluded;

    let Offered {
        candidate,
        installed,
    } = done.map_err(|error| {
        let mut told = loads.to_vec();
        told.extend(misplaced(project, &installing::kinds_named(reference)));
        Failure {
            error: Box::new(error.into()),
            warnings: told,
        }
    })?;
    let kind = candidate.kind();
    let mut warnings = loads.to_vec();
    warnings.extend(misplaced(project, &[kind]));
    let origin = candidate.origin();
    let into = project.name();
    let done = installed.map_err(|error| {
        // What it cleared before it failed comes first, being what it did:
        // on stderr with the error, the only place a failure prints.
        let mut told = cleared(project, error.swept());
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
        // And so is a copy somebody changed since, until forced.
        Installed::Edited { name, paths } => Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {} in {into} was changed since it was installed, at {}, so it was left alone; --force writes over it",
                named(kind, &name),
                listed(project, &paths)
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
    Ok(outcome)
}

/// One project's part, `said`, as a command given that project alone ends:
/// with what opening the ledger scrubbed before it, an error included.
fn alone(scrub: Scrubbed, said: Result<Outcome, Failure>) -> Result<Outcome, Failure> {
    match said {
        Ok(outcome) => Ok(scrub.before(outcome)),
        Err(mut failure) => {
            failure.warnings.splice(0..0, scrub.lines());
            Err(failure)
        }
    }
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

/// Whether core may write over, or take out, a copy edited since it was
/// installed: only when `--force` says so.
pub(crate) fn edits(force: bool) -> mindflayer_core::install::Edits {
    use mindflayer_core::install::Edits;

    if force {
        Edits::Forced
    } else {
        Edits::Kept
    }
}

/// Each of `paths` as [`shown`] spells it, joined into one list.
pub(crate) fn listed(project: &MindProject, paths: &[std::path::PathBuf]) -> String {
    paths
        .iter()
        .map(|path| shown(project, path))
        .collect::<Vec<_>>()
        .join(", ")
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
/// install does, through core's batch of one ([`assign::uninstall`]).
/// `force` takes out a copy edited since it was installed, which core
/// otherwise leaves alone with the rest.
pub(crate) fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    typed: &str,
    homes: &AgentHomes,
    force: bool,
) -> Result<Outcome, Failure> {
    let reference = Reference::parse(typed);
    let assigned = assign::uninstall(
        workspace,
        std::slice::from_ref(project),
        &reference,
        homes,
        edits(force),
    )
    .map_err(CliError::from)?;
    let said = scrubbed(&assigned.scrub);
    let mut parts = assigned.parts.into_iter();
    let Some(part) = parts.next() else {
        // A batch of one project hands back its part.
        return Ok(said.before(Outcome::plain(String::new())));
    };
    alone(said, uninstall_said(&part.project, part.done))
}

/// One project's part of an uninstall, `done`, as a command that takes it
/// out of that project alone says it: a line of what it took out and what
/// there is to say beside it, or why it was not.
fn uninstall_said(project: &MindProject, done: assign::Uninstall) -> Result<Outcome, Failure> {
    use mindflayer_core::install::Removed;

    let from = project.name();
    let (kind, taken) = done.map_err(CliError::from)?;
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
            Ok(outcome)
        }
        Removed::Foreign { name, path } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {} at {} in {from} was not installed by Mindflayer, so it was left alone",
                named(kind, &name),
                shown(project, &path)
            )],
            ok: false,
        }),
        Removed::Edited { name, paths } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {} in {from} was changed since it was installed, at {}, so it was left alone; --force takes it out",
                named(kind, &name),
                listed(project, &paths)
            )],
            ok: false,
        }),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(CliError::NotInstalled {
            name: named(kind, &name),
            project: from.to_owned(),
        }
        .into()),
    }
}
