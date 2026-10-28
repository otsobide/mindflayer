//! `flayer update` and `mind update`: what Mindflayer installed into
//! projects brought up to date with where it came from, and the copies that
//! have gone put back. Core's [`update`] decides everything, what is
//! written, left or said; here it is laid out, as what would happen without
//! `--yes` and as what did with it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindflayer_core::install::{Installation, Installed};
use mindflayer_core::update::{self, Plan, ProjectPlan, Step, Update};
use mindflayer_core::{paths, AgentHomes, Excluded, FlayerWorkspace, MindProject, Reference};

use super::gather::scrubbed;
use super::install::{edits, left_out, named};
use crate::{CliError, Failure, Outcome};

/// What a dry run that would write something ends with.
const UNCHANGED: &str = "nothing changed: add --yes to go ahead\n";

/// `flayer update -p`, `flayer update --all` and `mind update`: `projects`,
/// each one `workspace` links, brought up to date, or, without `yes`, what
/// that would do. `typed` names the installations to update, as a command
/// line names them, every one when it names none; `force` writes over a
/// copy edited since it was installed. `warnings` are what choosing the
/// projects had to say, an entry that would not open, said beside the rest.
///
/// The dry run says what the real one does, in the same lines and with the
/// same exit status: a copy left alone because it was edited is a warning
/// either way, as an install refused over it is, and an orphaned one a
/// note, never a failure, since nothing could be done for it.
pub(crate) fn update_projects(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    typed: &[String],
    force: bool,
    yes: bool,
    homes: &AgentHomes,
    warnings: Vec<String>,
) -> Result<Outcome, Failure> {
    let references: Vec<Reference> = typed.iter().map(|typed| Reference::parse(typed)).collect();
    let failed = |error: CliError, warnings: &[String]| Failure {
        error: Box::new(error),
        warnings: warnings.to_vec(),
    };
    // Every project linked, and none of them would open: not a workspace
    // that links none, which is what an empty plan otherwise means.
    let unread = projects.is_empty() && !warnings.is_empty();
    if !yes {
        let plan = update::plan(workspace, projects, &references, edits(force))
            .map_err(|error| failed(error.into(), &warnings))?;
        let mut outcome = laid_out(&plan, false, unread);
        if !plan.is_empty() {
            outcome.stdout.push_str(UNCHANGED);
        }
        return Ok(beside(outcome, warnings));
    }
    let applied = update::apply(workspace, projects, &references, edits(force), homes)
        .map_err(|error| failed(error.into(), &warnings))?;
    let said = scrubbed(&applied.scrub);
    Ok(said.before(beside(laid_out(&applied.plan, true, unread), warnings)))
}

/// `outcome`, with `warnings` after what it says.
fn beside(mut outcome: Outcome, warnings: Vec<String>) -> Outcome {
    outcome.ok &= warnings.is_empty();
    outcome.stderr.extend(warnings);
    outcome
}

/// A line for each installation written, in plan order, and on stderr what
/// is left and why: as what would happen, or, once `done`, as what did.
/// `unread` says the plan holds no project because none of those linked
/// would open.
fn laid_out(plan: &Plan, done: bool, unread: bool) -> Outcome {
    let mut outcome = Outcome::plain(String::new());
    for project in &plan.projects {
        project_laid_out(&mut outcome, project, done);
    }
    for typed in &plan.unmatched {
        let projects = match &plan.projects[..] {
            [one] => one.name.clone(),
            _ => String::from("any project asked"),
        };
        outcome.stderr.push(format!(
            "warning: nothing named `{typed}` is installed in {projects}"
        ));
        outcome.ok = false;
    }
    if plan.is_empty() {
        outcome.stdout = nothing(plan, unread);
    }
    outcome
}

/// What an update that writes nothing says: that everything asked is
/// current, when it is, and otherwise only that nothing is written, with
/// why beside it. `unread` as [`laid_out`] takes it.
fn nothing(plan: &Plan, unread: bool) -> String {
    let names: Vec<&str> = plan
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect();
    // Only `--all` asks for none: in a workspace that links none, or one
    // whose every entry would not open, each said beside this.
    if names.is_empty() {
        return String::from(if unread {
            "nothing to update: no project linked could be read\n"
        } else {
            "nothing to update: no project is linked\n"
        });
    }
    // Said only of what there is: with nothing installed, or nothing the
    // names given match, there is nothing to call current.
    let mut asked = plan
        .projects
        .iter()
        .flat_map(|project| &project.installations)
        .peekable();
    let some = asked.peek().is_some();
    if some && asked.all(|update| update.step == Step::Keep) {
        format!(
            "nothing to update: everything installed in {} is current\n",
            listed(&names)
        )
    } else {
        format!("nothing to update in {}\n", listed(&names))
    }
}

/// `a`, `a and b`, `a, b and c`.
pub(crate) fn listed(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What one project's part says. Each rule of a project marked by its
/// settings is written by one render of them all, so its lines wait for
/// that render: one that failed writes none of them, and says why.
fn project_laid_out(outcome: &mut Outcome, project: &ProjectPlan, done: bool) {
    let into = project.name.as_str();
    let mut routes: Vec<&str> = Vec::new();
    let mut rule_lines = String::new();
    for update in &project.installations {
        match &update.step {
            Step::Keep => {}
            Step::Leave(paths) => {
                outcome.stderr.push(format!(
                    "warning: {} in {into} was changed since it was installed, at {}, so it {} \
                     left alone; --force writes over it",
                    named(update.kind, &update.name),
                    listed_at(&project.root, paths),
                    if done { "was" } else { "would be" }
                ));
                outcome.ok = false;
            }
            Step::Orphaned => outcome.stderr.push(format!(
                "note: {} in {into} is no longer offered {}, so it {} left as it is; \
                 uninstalling it takes it out",
                named(update.kind, &update.name),
                update.origin.as_deref().map_or_else(
                    || String::from("where it came from"),
                    |origin| format!("by {origin}")
                ),
                if done { "was" } else { "would be" }
            )),
            Step::Unsure(origins) => {
                outcome.stderr.push(format!(
                    "warning: {} in {into} is offered by {}, and was installed before \
                     Mindflayer kept where from, so it {} left as it is; `install --from` says \
                     which",
                    named(update.kind, &update.name),
                    listed(&origins.iter().map(String::as_str).collect::<Vec<_>>()),
                    if done { "was" } else { "would be" }
                ));
                outcome.ok = false;
            }
            Step::Refresh | Step::PutBack(_) | Step::WriteOver(_) => match &update.done {
                // A rule of a project marked by its settings: its render's.
                None if done => {
                    routes.push(&update.name);
                    rule_lines.push_str(&line(&project.root, into, update, true));
                }
                None => outcome
                    .stdout
                    .push_str(&line(&project.root, into, update, false)),
                Some(Ok(installation)) => {
                    installed(outcome, &project.root, into, update, installation);
                }
                Some(Err(error)) => {
                    for path in error.swept() {
                        outcome.stderr.push(format!(
                            "cleared {}, left by an install that did not finish",
                            at(&project.root, path)
                        ));
                    }
                    outcome.stderr.push(format!("error: {into}: {error}"));
                    outcome.ok = false;
                }
            },
        }
    }
    match &project.rendered {
        Some(Ok(rendered)) => {
            outcome.stdout.push_str(&rule_lines);
            let warnings = rendered.warnings_of_update(&routes);
            outcome.ok &= warnings.is_empty() && rendered.still_excluded.is_none();
            outcome.stderr.extend(
                warnings
                    .iter()
                    .map(|warning| format!("warning: {into}: {}", warning.message)),
            );
            if let Some(reason) = &rendered.still_excluded {
                outcome.stderr.push(format!(
                    "warning: {into}: the exclude file still keeps out of git a file that is no \
                     longer Mindflayer's: {reason}"
                ));
            }
            outcome
                .stderr
                .extend(rendered.notes().iter().map(|note| format!("note: {note}")));
        }
        Some(Err(error)) => {
            outcome.stderr.push(format!("error: {into}: {error}"));
            outcome.ok = false;
        }
        None => {}
    }
}

/// The line for an installation an update writes: as what would happen,
/// or, once `done`, as what did.
fn line(root: &Path, into: &str, update: &Update, done: bool) -> String {
    let what = named(update.kind, &update.name);
    let from = update
        .origin
        .as_deref()
        .map_or_else(String::new, |origin| format!(", from {origin}"));
    let would = |now: &str, then: &str| {
        if done {
            now.to_owned()
        } else {
            then.to_owned()
        }
    };
    match &update.step {
        Step::PutBack(paths) => format!(
            "{} {what} in {into} at {}{from}\n",
            would("put back", "would put back"),
            listed_at(root, paths)
        ),
        Step::WriteOver(paths) => format!(
            "{} {what} in {into}, writing over the change at {}{from}\n",
            would("updated", "would update"),
            listed_at(root, paths)
        ),
        _ => format!(
            "{} {what} in {into}{from}\n",
            would("updated", "would update")
        ),
    }
}

/// What writing one installation as an install does came to, said as an
/// install says it: its line, or why it was left after all, with what the
/// install cleared on the way and what it left out of the copy.
fn installed(
    outcome: &mut Outcome,
    root: &Path,
    into: &str,
    update: &Update,
    installation: &Installation,
) {
    let what = named(update.kind, &update.name);
    match &installation.installed {
        Installed::Added { .. } | Installed::Updated { .. } => {
            outcome.stdout.push_str(&line(root, into, update, true));
        }
        Installed::Unchanged { .. } => {
            let _ = writeln!(outcome.stdout, "{what} in {into} was already current");
        }
        // Looked at again under the lock, and somebody's after all.
        Installed::Foreign { path, .. } => {
            outcome.stderr.push(format!(
                "warning: {into} already has a {what} at {} that Mindflayer did not put there, \
                 so it was left alone",
                at(root, path)
            ));
            outcome.ok = false;
        }
        Installed::Edited { paths, .. } => {
            outcome.stderr.push(format!(
                "warning: {what} in {into} was changed since it was installed, at {}, so it was \
                 left alone; --force writes over it",
                listed_at(root, paths)
            ));
            outcome.ok = false;
        }
    }
    for path in &installation.swept {
        let _ = writeln!(
            outcome.stdout,
            "  cleared {}, left by an install that did not finish",
            at(root, path)
        );
    }
    for left in &installation.left_out {
        outcome
            .stderr
            .push(format!("warning: {what} in {into} {}", left_out(left)));
        outcome.ok = false;
    }
    if let Some(note) = installation.excluded.as_ref().and_then(Excluded::warning) {
        outcome.stderr.push(format!("note: {note}"));
    }
    if let Some(reason) = installation.installed.unhashed() {
        outcome.stderr.push(format!(
            "warning: {what} in {into} was recorded without a hash of what it holds: {reason}"
        ));
        outcome.ok = false;
    }
}

/// Each of `paths` as [`at`] spells it, joined into one list.
fn listed_at(root: &Path, paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| at(root, path))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A path in the project at `root` as somebody standing in it would spell
/// it, as an install's lines spell one
/// ([`shown`](super::install::shown)).
fn at(root: &Path, path: &Path) -> String {
    paths::relative_to(path, root)
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| path.display().to_string())
}
