//! `flayer migrate` and `mind migrate`: bringing what an older Mindflayer
//! made into the layout a new one has. `--library` moves a workspace's own
//! into its library; `mind migrate` moves a project from its `mind.toml` to
//! settings. Core's [`migrate`] decides everything about each; here it is
//! said.

use std::fmt::Write as _;

use mindflayer_core::migrate::{self, MigrateError, Plan, ProjectPlan, Stranded};
use mindflayer_core::{Agent, AgentHomes, FlayerWorkspace, MindProject, Share};

use super::all_or_nothing;
use super::consulted;
use super::gather::scrubbed;
use super::install::{cleared, shown};
use super::settings::{agents, moved, not_copied, outside_note};
use crate::{CliError, Failure, Lookup, Outcome};

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

/// What a project's settings are to say once it is moved, and whether to
/// move it or only say what moving it would do.
pub(crate) struct Asked<'a> {
    pub agents: &'a [Agent],
    pub share: Share,
    pub yes: bool,
}

/// `mind migrate`: the project at or above here moved to settings, carrying
/// what the workspace `mind` consults about it records, when that one links
/// it.
///
/// A project nothing links is moved all the same, since its marker is its
/// own, with a hint saying nothing installed was carried along, as `mind
/// agents` says it. A workspace found that cannot be read is a warning
/// beside the move; one `-W` or the activation names that is no workspace
/// refuses it, as it refuses every command.
pub(crate) fn migrate_here(
    project: &MindProject,
    asked: &Asked,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (None, Some(format!("warning: {error}"))),
    };
    let result = laid_out(workspace.as_ref(), project, asked, &lookup.agents);
    let carried = if asked.yes {
        "was carried along"
    } else {
        "would be carried along"
    };
    // Only beside a move made or laid out, and not when the workspace could
    // not be read, whose warning says why.
    let hint = match (&result, &workspace, &unread) {
        (Ok(Some(_)), Some(workspace), _) if !workspace.is_linked(project.root()) => Some(format!(
            "hint: the workspace at {} does not link this project, so nothing installed in \
                 it {carried}; `mind link` links it",
            workspace.root().display()
        )),
        (Ok(Some(_)), None, None) => Some(format!(
            "hint: no workspace links this project, so nothing installed in it {carried}"
        )),
        _ => None,
    };
    let unreadable = unread.is_some();
    let said: Vec<String> = unread.into_iter().chain(hint).collect();
    match result {
        Ok(outcome) => {
            let mut outcome = finished(outcome, project, asked);
            outcome.ok &= !unreadable;
            outcome.stderr.extend(said);
            Ok(outcome)
        }
        Err(stopped) => {
            let mut failure = stopped.failure();
            failure.warnings.extend(said);
            Err(failure)
        }
    }
}

/// What a dry run of a project's move ends with.
const UNCHANGED: &str = "nothing changed: add --yes to go ahead\n";

/// One project's move as a single command says it: [`laid_out`], ended as a
/// dry run is, or what a project with nothing to move says.
fn finished(outcome: Option<Outcome>, project: &MindProject, asked: &Asked) -> Outcome {
    match outcome {
        Some(mut outcome) => {
            if !asked.yes {
                outcome.stdout.push_str(UNCHANGED);
            }
            outcome
        }
        None => Outcome::plain(format!(
            "nothing to migrate: {} is marked by its settings already\n",
            project.name()
        )),
    }
}

/// Why one project's move did not happen: refused before anything was
/// written, or stopped by a failure, which may have put back less than all.
enum Stopped {
    Refused(Vec<migrate::Refusal>),
    Failed(Failure),
}

impl Stopped {
    /// As a command given one project fails: every refusal at once, the
    /// first as the error, as `flayer migrate --library` says them.
    fn failure(self) -> Failure {
        match self {
            // Never without a reason, so the second is not reached.
            Stopped::Refused(refusals) => refused(refusals)
                .err()
                .unwrap_or_else(|| CliError::Migrate(MigrateError::Refused(Vec::new())).into()),
            Stopped::Failed(failure) => failure,
        }
    }
}

/// One project's move, planned without `yes` and made with it, laid out;
/// `None` when its settings mark it already.
///
/// The dry run is a report, as `flayer migrate --library`'s is: a line for
/// each thing that would change, to read before saying yes. Made, a copy
/// left out of a folder an agent the project names reads is a warning, as
/// it is for `mind agents`.
fn laid_out(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    asked: &Asked,
    homes: &AgentHomes,
) -> Result<Option<Outcome>, Stopped> {
    let failed = |error: MigrateError| Stopped::Failed(CliError::Migrate(error).into());
    if !asked.yes {
        let Some(mut plan) =
            migrate::project_plan(workspace, project, asked.agents, asked.share, homes)
                .map_err(failed)?
        else {
            return Ok(None);
        };
        if !plan.refusals.is_empty() {
            return Err(Stopped::Refused(std::mem::take(&mut plan.refusals)));
        }
        let mut outcome = Outcome::plain(project_report(project, &plan, false));
        outcome.stderr.extend(beside(project, &plan));
        return Ok(Some(outcome));
    }

    let migrated = match migrate::project(workspace, project, asked.agents, asked.share, homes) {
        Ok(Some(migrated)) => migrated,
        Ok(None) => return Ok(None),
        Err(MigrateError::Refused(refusals)) => return Err(Stopped::Refused(refusals)),
        Err(error) => {
            // Said with the error, being what the move left that it could
            // not put back.
            let warnings = error.left_behind().iter().map(stranded).collect();
            return Err(Stopped::Failed(Failure {
                error: Box::new(CliError::Migrate(error)),
                warnings,
            }));
        }
    };
    let said = scrubbed(&migrated.scrub);
    let plan = &migrated.plan;
    let mut outcome = Outcome::plain(project_report(project, plan, true));
    for line in cleared(project, &migrated.swept) {
        let _ = writeln!(outcome.stdout, "  {line}");
    }
    for retarget in &plan.installations {
        let warnings = not_copied(project, retarget);
        outcome.ok &= warnings.is_empty();
        outcome.stderr.extend(warnings);
    }
    outcome.stderr.extend(beside(project, plan));
    Ok(Some(said.before(outcome)))
}

/// A line for each thing a project's move does, as what would happen in a
/// dry run, and as what did once it is made: the settings and the marker
/// first, then what keeps `.mind` out of git, each installation's copies,
/// what is left where it is, and what is kept out of git.
fn project_report(project: &MindProject, plan: &ProjectPlan, done: bool) -> String {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    let mut text = String::new();
    let _ = writeln!(
        text,
        "{} {} for {}: agents {}, share {}",
        would("wrote", "would write"),
        shown(project, &plan.settings_file),
        project.name(),
        agents(&plan.settings.agents),
        plan.settings.share
    );
    let _ = writeln!(
        text,
        "{} {} to {}, as it {}",
        would("moved", "would move"),
        shown(project, &plan.marker),
        shown(project, &plan.backup),
        would("was", "is")
    );
    if let Some(path) = &plan.ignore {
        let _ = writeln!(
            text,
            "{} {}, keeping .mind out of git",
            would("wrote", "would write"),
            shown(project, path)
        );
    }
    let local = plan.settings.share == Share::Local;
    for retarget in &plan.installations {
        moved(&mut text, project, retarget, done, local);
    }
    for path in &plan.authored {
        let _ = writeln!(
            text,
            "{} {} where it is: the project's own",
            would("left", "would leave"),
            shown(project, path)
        );
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
    text
}

/// What a project's move says on stderr beside its lines: that nothing
/// keeps a local project's copies out of git outside a work tree, and how
/// to stop git tracking a `.mind` that a local project's `.gitignore` keeps
/// none of out, which no ignore rule does and Mindflayer runs no git to do.
fn beside(project: &MindProject, plan: &ProjectPlan) -> Vec<String> {
    let mut said: Vec<String> = outside_note(project, plan.outside).into_iter().collect();
    if plan.tracked {
        said.push(format!(
            "hint: git tracks {}, and a .gitignore keeps out only what git does not track; \
             `git rm -r --cached .mind` in {} stops that, and leaves the files where they are",
            project.mind_dir().display(),
            project.root().display()
        ));
    }
    said
}

/// What a project's move that failed says of something it could not put
/// back.
fn stranded(left: &Stranded) -> String {
    match left {
        Stranded::Copy(path) => format!(
            "warning: {} was copied and could not be taken back out; remove it yourself",
            path.display()
        ),
        Stranded::Retired { name, path } => format!(
            "warning: {} was moved out of the way and could not be put back; `mind install \
             {name}` puts it back",
            path.display()
        ),
        Stranded::Settings(path) => format!(
            "warning: {} was written and could not be removed, so the project is read from it; \
             remove it to read the project from its mind.toml again",
            path.display()
        ),
        Stranded::Marker { marker, backup } => format!(
            "warning: {} could not be moved back to {}; move it yourself",
            backup.display(),
            marker.display()
        ),
        Stranded::Ignore(path) => format!(
            "warning: {} was written and could not be removed, so .mind is kept out of git; \
             remove it yourself",
            path.display()
        ),
    }
}
