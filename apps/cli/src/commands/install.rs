//! The commands that put one skill or rule into projects and take it back
//! out: `install` and `uninstall`, at either level, into one project or, at
//! the workspace's, into each of several at once; and `uninstall --orphaned`,
//! which takes out whatever nothing offers any more. The screen `flayer
//! install` opens on its own is [`crate::install`].

use std::path::Path;

use mindflayer_core::assign::{self, Offered};
use mindflayer_core::install::rules::Rendered;
use mindflayer_core::{AgentHomes, FlayerWorkspace, Kind, MindProject, Reference};

use crate::commands::gather::{scrubbed, Scrubbed};
use crate::commands::update::listed as words_listed;
use crate::commands::workspace::load_warnings;
use crate::{CliError, Failure, Outcome, Warning};

/// Put what `typed` names into each of `projects`, from wherever it is
/// offered: a skill or a rule, by its name alone or qualified
/// (`rule:git/commits`), as core finds it for each
/// ([`offered_typed`](mindflayer_core::install::offered_typed)), through
/// core's batch ([`assign::install`]).
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
/// So is each folder a project's settings name for the kind that no install
/// uses ([`MindProject::misplaced_targets`]): no copy went there, and an
/// install refused for want of any other is refused for that. Every kind the
/// name could be, as core says ([`kinds_named`]), when it is refused before
/// it is known which.
///
/// [`kinds_named`]: mindflayer_core::install::kinds_named
///
/// One project is said as it always was, a refusal as the command's error.
/// Several are said as [`together`] says them, each one's lines in the
/// order asked, and what the loaded workspaces could not offer once, after
/// them all.
///
/// `homes` are the agents' own folders the command line decided, which core
/// refuses to install into. `force` writes over a copy edited since it was
/// installed, which core otherwise leaves alone.
pub(crate) fn install_named(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
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
        projects,
        &reference,
        from,
        directory,
        homes,
        edits(force),
    )
    .map_err(|error| {
        let kinds = mindflayer_core::install::kinds_named(&reference);
        let mut warnings = loads.clone();
        for project in projects {
            warnings.extend(misplaced(project, &kinds));
        }
        Failure {
            error: Box::new(error.into()),
            warnings,
        }
    })?;
    // What the scrub said goes first, with an error too: it happened all the
    // same.
    let said = scrubbed(&assigned.scrub);
    let mut parts = assigned.parts;
    if parts.len() == 1 {
        let part = parts.remove(0);
        return alone(
            said,
            install_said(&part.project, part.done, &reference, &loads, false),
        );
    }
    let each = parts
        .into_iter()
        .map(|part| {
            let into = part.project.name().to_owned();
            (
                into,
                install_said(&part.project, part.done, &reference, &[], true),
            )
        })
        .collect();
    together(said, each, loads)
}

/// One project's part of an install, `done`, as a command that installs into
/// it alone says it: a line of what it did and what there is to say beside
/// it, or why it was refused. `loads` are what the loaded workspaces could
/// not offer, said before what is said of the project's folders.
///
/// `among` others, what rendering its rules warns of is said of the project
/// ([`render_said`]): it names a rule or a file in the project and not the
/// project, which alone goes without saying, and beside another's would
/// leave no telling whose it is.
fn install_said(
    project: &MindProject,
    done: assign::Install,
    reference: &Reference,
    loads: &[Warning],
    among: bool,
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
        let mut told: Vec<Warning> = cleared(project, error.swept())
            .into_iter()
            .map(Warning::from)
            .collect();
        told.extend(warnings.iter().cloned());
        Failure {
            error: Box::new(error.into()),
            warnings: told,
        }
    })?;
    let unhashed = done.installed.unhashed().map(str::to_owned);
    // Why it was refused, when it was: what the panel tells somebody's copy
    // from one changed since by, as the line does in words.
    let refused = done.installed.refused().map(|refused| refused.code());
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
            stderr: vec![Warning {
                text: format!(
                    "warning: {into} already has a {} at {} that Mindflayer did not put there, so it was left alone",
                    named(kind, &name),
                    shown(project, &path)
                ),
                code: refused,
            }],
            ok: false,
        },
        // And so is a copy somebody changed since, until forced.
        Installed::Edited { name, paths } => Outcome {
            stdout: String::new(),
            stderr: vec![Warning {
                text: format!(
                    "warning: {} in {into} was changed since it was installed, at {}, so it was left alone; --force writes over it",
                    named(kind, &name),
                    listed(project, &paths)
                ),
                code: refused,
            }],
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
            .push(format!("warning: {name} in {into} {}", left_out(left)).into());
        outcome.ok = false;
    }
    // Installed, and what the project's rules came to on the way, which can
    // leave some of its agents reading none of them.
    if let Some(rendered) = &done.rendered {
        let (warned, notes) = render_said(rendered, candidate.name(), among.then_some(into));
        if !warned.is_empty() {
            outcome.ok = false;
        }
        outcome.stderr.extend(warned);
        if let Some(reason) = &rendered.still_excluded {
            outcome.stderr.push(
                format!(
                    "warning: {into}: the exclude file still keeps out of git a file that is no \
                     longer Mindflayer's: {reason}"
                )
                .into(),
            );
            outcome.ok = false;
        }
        outcome.stderr.extend(notes);
    }
    // A note rather than a warning: a project need not be in git, and
    // nothing went wrong, but what local mode keeps out of git is then kept
    // out of nothing.
    if let Some(note) = done.excluded.as_ref().and_then(Excluded::warning) {
        outcome.stderr.push(format!("note: {note}").into());
    }
    // Installed and recorded all the same, but without what tells a later
    // edit apart, which is worth a look at what is in the copy.
    if let Some(reason) = unhashed {
        outcome.stderr.push(
            format!(
                "warning: {name} in {into} was recorded without a hash of what it holds: {reason}"
            )
            .into(),
        );
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

/// Several projects' parts, `each` by the project's name, as one command
/// says them: what opening the ledger scrubbed first, then each project's
/// lines in the order asked, one refused ending in an `error:` that names
/// it, and `after`, said once for them all; failing when any part did,
/// while the others' lines stand, since what they did was done.
///
/// Every part refused alike, as a name nothing offers is refused for every
/// project, is that refusal once, as the command's error, with what each
/// part said before it: the same line for each project would bury what it
/// says under how often.
fn together(
    scrub: Scrubbed,
    each: Vec<(String, Result<Outcome, Failure>)>,
    after: Vec<Warning>,
) -> Result<Outcome, Failure> {
    let alike = each.split_first().is_some_and(|((_, first), rest)| {
        first.as_ref().err().is_some_and(|first| {
            rest.iter().all(|(_, said)| {
                said.as_ref()
                    .err()
                    .is_some_and(|other| other.error.to_string() == first.error.to_string())
            })
        })
    });
    if alike {
        let mut warnings = scrub.lines();
        let mut error = None;
        for (_, said) in each {
            if let Err(failure) = said {
                warnings.extend(failure.warnings);
                error.get_or_insert(failure.error);
            }
        }
        warnings.extend(after);
        return Err(Failure {
            error: error.expect("parts refused alike are refused, one at least"),
            warnings,
        });
    }
    let mut outcome = Outcome::plain(String::new());
    for (name, said) in each {
        match said {
            Ok(one) => {
                outcome.stdout.push_str(&one.stdout);
                outcome.stderr.extend(one.stderr);
                outcome.ok &= one.ok;
            }
            Err(failure) => {
                outcome.stderr.extend(failure.warnings);
                outcome.stderr.push(Warning::coded(
                    failure.error.code(),
                    format!("error: {name}: {}", failure.error),
                ));
                outcome.ok = false;
            }
        }
    }
    outcome.ok &= after.is_empty();
    outcome.stderr.extend(after);
    Ok(scrub.before(outcome))
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
/// it is among other projects' lines, where a warning that names a rule or
/// a file would not say whose it is. A note names its project already.
pub(crate) fn render_said(
    rendered: &Rendered,
    changed: &str,
    project: Option<&str>,
) -> (Vec<Warning>, Vec<Warning>) {
    let of = project.map_or_else(String::new, |project| format!("{project}: "));
    (
        rendered
            .warnings_of(Some(changed))
            .iter()
            .map(|warning| {
                Warning::coded(warning.code, format!("warning: {of}{}", warning.message))
            })
            .collect(),
        rendered
            .notes()
            .iter()
            .map(|note| format!("note: {note}").into())
            .collect(),
    )
}

/// A warning for each folder `project`'s settings name for one of `kinds`
/// that no install uses.
fn misplaced(project: &MindProject, kinds: &[Kind]) -> Vec<Warning> {
    project
        .misplaced_targets()
        .iter()
        .filter(|misplaced| kinds.contains(&misplaced.kind))
        .map(|misplaced| Warning::coded(misplaced.code(), format!("warning: {misplaced}")))
        .collect()
}

/// A line for each thing interrupted installs had left in `project` that
/// installing cleared: on stdout, under what the install did, because it is
/// part of what the install did rather than something wrong with it.
fn swept(project: &MindProject, swept: &[std::path::PathBuf]) -> String {
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
pub(crate) fn still(name: &str, project: &str, reason: &str) -> Warning {
    format!(
        "warning: {name} is gone from {project}, but the exclude file still keeps its place out \
         of git: {reason}"
    )
    .into()
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
/// of each of `projects`, handing core the agents' own folders, `homes`, as
/// an install does, through core's batch ([`assign::uninstall`]). `force`
/// takes out a copy edited since it was installed, which core otherwise
/// leaves alone with the rest.
///
/// One project is said as it always was, and one that has nothing of the
/// name is refused, as a typo far more often than not. Of several, one that
/// has nothing of it is passed over in silence, being as it was asked to
/// be, unless none of them has it, which is refused as that typo; the rest
/// are said as [`together`] says them.
pub(crate) fn uninstall_named(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    typed: &str,
    homes: &AgentHomes,
    force: bool,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::Removed;

    let reference = Reference::parse(typed);
    let assigned = assign::uninstall(workspace, projects, &reference, homes, edits(force))
        .map_err(CliError::from)?;
    let said = scrubbed(&assigned.scrub);
    let mut parts = assigned.parts;
    if parts.len() == 1 {
        let part = parts.remove(0);
        return alone(said, uninstall_said(&part.project, part.done, false));
    }
    let absent = |done: &assign::Uninstall| matches!(done, Ok((_, Removed::Absent { .. })));
    if parts.iter().all(|part| absent(&part.done)) {
        if let Some(Ok((kind, Removed::Absent { name }))) = parts.first().map(|part| &part.done) {
            return Err(Failure {
                error: Box::new(CliError::NotInstalledAnywhere {
                    name: named(*kind, name),
                }),
                warnings: said.lines(),
            });
        }
    }
    let each = parts
        .into_iter()
        .filter(|part| !absent(&part.done))
        .map(|part| {
            let from = part.project.name().to_owned();
            (from, uninstall_said(&part.project, part.done, true))
        })
        .collect();
    together(said, each, Vec::new())
}

/// Take every installation nothing offers any more where it came from back
/// out of each of `projects`, through core's batch
/// ([`assign::uninstall_orphaned`]), handing it the agents' own folders,
/// `homes`, as an uninstall by name does. `force` takes out one changed
/// since it was installed too, which core otherwise leaves alone with the
/// rest of its copies.
///
/// Each one taken out is said as an uninstall of it by name says it,
/// project by project in the order asked, and one that fails as an error of
/// its project while the rest stand, as [`together`] says them. With none
/// orphaned anywhere asked, that is said, and is no failure, as it was
/// asked to be. What could not be offered is said after it all, the
/// workspace's own first, as it is offered first, then the loaded
/// workspaces': an installation from an artifact that cannot be read, or a
/// loaded workspace that cannot be opened, is not taken for orphaned, and
/// this is why it stays.
pub(crate) fn uninstall_orphans(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    homes: &AgentHomes,
    force: bool,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::Removed;

    let assigned = assign::uninstall_orphaned(workspace, projects, homes, edits(force))
        .map_err(CliError::from)?;
    let said = scrubbed(&assigned.scrub);
    let mut unoffered: Vec<Warning> = mindflayer_core::install::own_failures(workspace, &Kind::ALL)
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    unoffered.extend(load_warnings(workspace));
    let among = assigned.parts.len() > 1;
    let mut asked: Vec<String> = Vec::new();
    let mut each: Vec<(String, Result<Outcome, Failure>)> = Vec::new();
    for part in assigned.parts {
        let from = part.project.name().to_owned();
        asked.push(from.clone());
        let orphans = match part.done {
            Ok(orphans) => orphans,
            Err(error) => {
                each.push((from, Err(CliError::from(error).into())));
                continue;
            }
        };
        for orphan in orphans {
            // Taken out by something else since it was found.
            if matches!(orphan.removed, Ok(Removed::Absent { .. })) {
                continue;
            }
            let done = orphan.removed.map(|removed| (orphan.kind, removed));
            each.push((from.clone(), uninstall_said(&part.project, done, among)));
        }
    }
    if each.is_empty() {
        let asked: Vec<&str> = asked.iter().map(String::as_str).collect();
        let mut outcome = Outcome::plain(format!(
            "nothing removed: nothing installed in {} is orphaned\n",
            words_listed(&asked)
        ));
        outcome.ok = unoffered.is_empty();
        outcome.stderr = unoffered;
        return Ok(said.before(outcome));
    }
    together(said, each, unoffered)
}

/// One project's part of an uninstall, `done`, as a command that takes it
/// out of that project alone says it: a line of what it took out and what
/// there is to say beside it, or why it was not; `among` others, with what
/// its rules warn of said of it, as [`install_said`] says it.
fn uninstall_said(
    project: &MindProject,
    done: assign::Uninstall,
    among: bool,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::Removed;

    let from = project.name();
    let (kind, taken) = done.map_err(CliError::from)?;
    let refused = taken.refused().map(|refused| refused.code());
    match taken {
        Removed::Removed {
            name,
            still_excluded,
            rendered,
            ..
        } => {
            // What the rules left come to without it.
            let (warned, notes) = rendered
                .map(|rendered| render_said(&rendered, &name, among.then_some(from)))
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
            stderr: vec![Warning {
                text: format!(
                    "warning: {} at {} in {from} was not installed by Mindflayer, so it was left alone",
                    named(kind, &name),
                    shown(project, &path)
                ),
                code: refused,
            }],
            ok: false,
        }),
        Removed::Edited { name, paths } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![Warning {
                text: format!(
                    "warning: {} in {from} was changed since it was installed, at {}, so it was left alone; --force takes it out",
                    named(kind, &name),
                    listed(project, &paths)
                ),
                code: refused,
            }],
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
