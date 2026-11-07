//! `mindweaver gather`: collecting artifacts onto the workspace's shelf, and
//! listing everything that can be installed from elsewhere.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::gather::{self, Gathered, Preview, Report, Request, Scrub, Watch};
use mindweaver_core::install::{offered_by_loads, Offer};
use mindweaver_core::ledger::{Ledger, SourceListing};
use mindweaver_core::marketplace::{self, Holding, Listed, Pick, Plugin};
use mindweaver_core::sources::{self, Again, Regathered, Removal, Renamed, Which};
use mindweaver_core::url::redacted;
use mindweaver_core::{Kind, SourceKind, WeaverWorkspace};

use super::{reading_workspace, workspace_here};
use crate::render::{indent, json_or_text, plural, printable, table, utc};
use crate::{CliError, Failure, GatherCommand, Lookup, Outcome, Warning};

/// Collect artifacts onto the workspace shelf, or say what is already on it.
///
/// A gather reports to `watch` and stops when it is cancelled; what it then
/// prints is the same either way, so a front end that draws the progress
/// shows the same report afterwards as one that does not.
pub(crate) fn run_gather(
    command: &GatherCommand,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    match command {
        GatherCommand::Git {
            url,
            path,
            kind,
            reference,
            name,
            pin,
            dry_run,
        } => {
            // Read before the workspace is: an address its options
            // contradict is refused with nothing opened, let alone made.
            let request = Request::addressed(url, reference.clone(), path.clone())
                .map_err(CliError::from)?
                .gathering(kind.unwrap_or(Kind::Skill))
                .named(name.clone())
                .pinned(pin.clone())
                .watched_by(watch.clone());
            // Resolved as a read, which makes no workspace, the default one
            // in home included, as a dry run of `gather remove` is: one that
            // wrote a marker there would not be a dry run.
            if *dry_run {
                return reading_workspace(directory, lookup, |workspace| {
                    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                    match gather::preview(workspace, &ledger, &request) {
                        Ok(preview) => Ok(previewed(request.source.url(), &preview)),
                        Err(error) => Err(Failure::from(CliError::from(error))),
                    }
                });
            }
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
            let said = scrubbed(&scrub);
            match gather::gather(&workspace, &ledger, &request) {
                // By the repository the address names, which is what was
                // gathered: a tree URL's branch and folder are no part of it.
                Ok(report) => Ok(said.before(gathered(request.source.url(), &report))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        GatherCommand::Marketplace {
            source,
            path,
            reference,
            name,
            list,
            plugins,
            all,
            yes,
        } => {
            // Read before the workspace is, as `gather git`'s address is.
            let catalog = Request::addressed(source, reference.clone(), path.clone())
                .map_err(CliError::from)?
                .named(name.clone())
                .watched_by(watch.clone());
            let pick = match (*list, *all, *yes) {
                (true, _, _) => Pick::Listing,
                (_, true, true) => Pick::AllConfirmed,
                (_, true, false) => Pick::All,
                _ => Pick::Plugins(plugins.clone()),
            };
            let request = marketplace::Request::new(catalog, pick);
            // Looked at first, as a read that makes no workspace and writes
            // nothing, as a dry run of `gather git` is: one with more plugins
            // than a yes is waited for beyond stops there, and one with
            // fewer is gathered whole without one. What it prints is all this
            // decides: the gather refuses a catalog grown beyond them since.
            if *all && !*yes {
                let mut waits = None;
                let said = reading_workspace(directory, lookup, |workspace| {
                    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                    let listed =
                        marketplace::look(workspace, &ledger, &request).map_err(CliError::from)?;
                    let outcome = unconfirmed(&listed);
                    if listed.needs_confirming() {
                        waits = Some(());
                    }
                    Ok(outcome)
                })?;
                if waits.is_some() {
                    return Ok(said);
                }
            }
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
            let said = scrubbed(&scrub);
            match marketplace::gather(&workspace, &ledger, &request) {
                Ok(report) => Ok(said.before(marketplace_gathered(&report, &request.pick))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        GatherCommand::List { json } => {
            let mut found = None;
            let text =
                reading_workspace(directory, lookup, |workspace| shelf(workspace, &mut found))?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        GatherCommand::Rename { from, to } => {
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, said) = ledger_to_change(&workspace)?;
            match sources::rename(&workspace, &ledger, from, to) {
                Ok(renamed) => Ok(said.before(renamed_source(&renamed))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        GatherCommand::Update { names, all } => {
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, said) = ledger_to_change(&workspace)?;
            let which = if *all {
                Which::Every
            } else {
                Which::Named(names)
            };
            match sources::update(&workspace, &ledger, which, watch) {
                Ok(updated) => Ok(said.before(regathered(&updated))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        // Without --yes it deletes nothing, so it is resolved as a read and
        // makes no workspace, as `remove` without it is: a dry run that
        // wrote the default one's marker into home would not be one.
        GatherCommand::Remove { name, yes } => {
            if !*yes {
                return reading_workspace(directory, lookup, |workspace| {
                    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                    let removal =
                        sources::plan_removal(workspace, &ledger, name).map_err(CliError::from)?;
                    Ok(removing(&removal, false))
                });
            }
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, said) = ledger_to_change(&workspace)?;
            match sources::remove(&workspace, &ledger, name) {
                Ok(removal) => Ok(said.before(removing(&removal, true))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        GatherCommand::Sources { json } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                sources(workspace, &mut found)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        GatherCommand::Mcp(command) => super::registry::run_mcp(command, directory, lookup),
        GatherCommand::Search {
            query,
            finders,
            token_env,
            at,
            take,
            json,
        } => {
            let asked = super::search::Asked {
                query,
                finders,
                token_env: token_env.as_deref(),
                at,
            };
            match take {
                Some(number) => super::search::run_take(&asked, *number, directory, lookup, watch),
                None => super::search::run_search(&asked, *json, directory, lookup),
            }
        }
    }
}

/// The ledger a change to a source is made in: opened to write, scrubbing
/// it, with what that took out to be said, when there is one, and read
/// without being made when there is none, as for removing one of the
/// workspace's own everywhere: a workspace that never gathered has no
/// source to change, and no reason to be given a ledger to say so.
fn ledger_to_change(workspace: &WeaverWorkspace) -> Result<(Ledger, Scrubbed), CliError> {
    if workspace.ledger_path().is_file() {
        let (ledger, scrub) = workspace.ledger_to_write()?;
        Ok((ledger, scrubbed(&scrub)))
    } else {
        Ok((workspace.ledger_for_reading()?, Scrubbed::default()))
    }
}

/// What a rename did: the name the source had and the one it has, and its
/// URL, which every install from it still records as its origin.
fn renamed_source(renamed: &Renamed) -> Outcome {
    let (was, now) = (printable(&renamed.was), printable(&renamed.source.name));
    let stdout = if renamed.unchanged() {
        format!("{now} is called that already: nothing changed\n")
    } else {
        format!(
            "renamed source {was} to {now}, gathered from {}\n",
            printable(&renamed.source.origin())
        )
    };
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok: true,
    }
}

/// What gathering each source again came to, a source after another, an
/// empty line apart: each as `gather git` says one, by its name and the URL
/// it was gathered from, a marketplace as `gather marketplace` says the
/// plugins it gathers, and one that could not be gathered as a warning,
/// by its name, beside the rest, failing the command as a gather that
/// failed would.
fn regathered(updated: &[Regathered]) -> Outcome {
    if updated.is_empty() {
        return Outcome {
            stdout: String::from(
                "no sources yet\n  take some with `mindweaver gather git <url>`\n",
            ),
            stderr: Vec::new(),
            ok: true,
        };
    }
    let mut outcome = Outcome {
        stdout: String::new(),
        stderr: Vec::new(),
        ok: true,
    };
    for each in updated {
        let said = match &each.outcome {
            Again::Gathered(Ok(report)) => gathered(each.source.url.as_str(), report),
            Again::Marketplace(Ok(report)) => {
                let plugins: Vec<String> = report
                    .children
                    .iter()
                    .map(|child| child.entry.clone())
                    .collect();
                marketplace_gathered(report, &Pick::Recorded(plugins))
            }
            Again::Gathered(Err(error)) => {
                failed_again(&each.source, error.code(), &error.to_string())
            }
            Again::Marketplace(Err(error)) => {
                failed_again(&each.source, error.code(), &error.to_string())
            }
        };
        if !said.stdout.is_empty() && !outcome.stdout.is_empty() {
            outcome.stdout.push('\n');
        }
        outcome.stdout.push_str(&said.stdout);
        outcome.stderr.extend(said.stderr);
        outcome.ok &= said.ok;
    }
    outcome
}

/// A source that could not be gathered again, said as a warning by its
/// name, `error` being what kept it.
fn failed_again(
    source: &mindweaver_core::ledger::Source,
    code: &'static str,
    error: &str,
) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: vec![Warning::coded(
            code,
            format!("warning: {}: {error}", printable(&source.name)),
        )],
        ok: false,
    }
}

/// What removing a source would do, `done` or not: the source, by its name
/// and where it was gathered from, and the skills or rules it holds on the
/// shelf, then each folder deleted, then each copy a project the workspace
/// links installed from it, left there with its record and orphaned, by the
/// name it is installed as and its project.
///
/// Not refused without `--yes`, as `remove` of an artifact is: the dry run
/// says what would go and what would be left orphaned, which is what it is
/// asked for, and exits as a dry run of `remove --everywhere` does. What is
/// left orphaned once it is done is no failure either, being what removing
/// a source does, and a note says what takes them out. A project that
/// cannot be opened is a warning, since what it installed from the source
/// cannot be told.
fn removing(removal: &Removal, done: bool) -> Outcome {
    let (would, deleted, left) = if done {
        ("removed", "deleted", "left")
    } else {
        ("would remove", "would delete", "would leave")
    };
    let held = if done { "held" } else { "holds" };
    let mut outcome = Outcome::plain(if removal.source.kind == SourceKind::Marketplace {
        format!(
            "{would} the marketplace {}, gathered from {}, and the {} of it gathered\n",
            printable(&removal.source.name),
            printable(&removal.source.origin()),
            plural(removal.children.len(), "plugin"),
        )
    } else {
        format!(
            "{would} the source {}, gathered from {}, and the {} it {held} on the shelf\n",
            printable(&removal.source.name),
            printable(&removal.source.origin()),
            plural(removal.artifacts, removal.source.gathers.noun()),
        )
    });
    // Each plugin of a marketplace as a source of its own goes, before it,
    // as removing goes about it.
    for each in removal.children.iter().chain(std::iter::once(removal)) {
        if !std::ptr::eq(each, removal) {
            let _ = writeln!(
                outcome.stdout,
                "{would} its plugin {}, and the {} it {held} on the shelf",
                printable(&each.source.name),
                plural(each.artifacts, "skill"),
            );
        }
        for folder in &each.folders {
            let _ = writeln!(outcome.stdout, "{deleted} {}", folder.display());
        }
    }
    let installed = removal
        .children
        .iter()
        .chain(std::iter::once(removal))
        .flat_map(|each| &each.installed);
    for copy in installed.clone() {
        let _ = writeln!(
            outcome.stdout,
            "{left} {} in {}, installed from it, orphaned",
            printable(&super::install::named(copy.kind, &copy.name)),
            printable(copy.project.name()),
        );
    }
    if !done {
        outcome
            .stdout
            .push_str("nothing removed: add --yes to go ahead\n");
    } else if installed.clone().next().is_some() {
        outcome.stderr.push(Warning::from(String::from(
            "note: nothing offers what was installed from it any more; `mindweaver uninstall \
             --orphaned --all-projects` takes out what nothing offers",
        )));
    }
    for failure in &removal.unread {
        outcome.stderr.push(Warning::coded(
            failure.code(),
            format!("warning: {failure}"),
        ));
        outcome.ok = false;
    }
    outcome
}

/// `mindweaver gather sources`: every source the shelf is filled from, from
/// a ledger read without being made, as `gather list` reads it, each kept
/// in `found` for `--json` to print in place of the text.
fn sources(
    workspace: &WeaverWorkspace,
    found: &mut Option<Vec<SourceListing>>,
) -> Result<Outcome, Failure> {
    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
    let listed = ledger.source_listing().map_err(CliError::from)?;
    let stdout = sources_listing(&listed);
    *found = Some(listed);
    Ok(Outcome {
        stdout,
        stderr: Vec::new(),
        ok: true,
    })
}

/// Each source by the name `--from` takes and `gather update` and `gather
/// remove` are given, then its URL, the branch or tag asked for and the
/// commit it is pinned to, each in a column that earns its place only once
/// a source has one, how many skills or rules it holds, as it gathers one
/// or the other, and when it was last gathered, in UTC, the one zone a
/// time can be told in without a database of them.
///
/// A marketplace says how many plugins of it are gathered where a source
/// says its skills, and each of them is under it, its name indented, in
/// the order the ledger lists them (#103).
///
/// Empty, it says how to fill it, as an empty shelf does.
fn sources_listing(listed: &[SourceListing]) -> String {
    if listed.is_empty() {
        return String::from("no sources yet\n  take some with `mindweaver gather git <url>`\n");
    }
    let name_reference = listed.iter().any(|it| it.source.reference.is_some());
    let name_pin = listed.iter().any(|it| it.source.pin.is_some());
    let rows: Vec<Vec<String>> = listed
        .iter()
        .map(|it| {
            let name = match &it.parent {
                Some(_) => format!("  {}", printable(&it.source.name)),
                None => printable(&it.source.name),
            };
            let mut row = vec![name, printable(&it.source.url)];
            if name_reference {
                row.push(
                    it.source
                        .reference
                        .as_deref()
                        .map_or_else(String::new, |at| format!("ref {}", printable(at))),
                );
            }
            if name_pin {
                row.push(
                    it.source
                        .pin
                        .as_deref()
                        .map_or_else(String::new, |pin| format!("pinned {}", short(pin))),
                );
            }
            row.push(match it.source.kind {
                SourceKind::Marketplace => plural(
                    listed
                        .iter()
                        .filter(|other| other.source.parent_id == Some(it.source.id))
                        .count(),
                    "plugin",
                ),
                SourceKind::Git => plural(it.artifacts, it.source.gathers.noun()),
            });
            row.push(it.gathered_at.map_or_else(
                || String::from("never gathered"),
                |at| format!("gathered {}", utc(at)),
            ));
            row
        })
        .collect();
    table(&rows)
}

/// `mindweaver gather list`: what can be installed from elsewhere, from a
/// ledger read without being made, with each offer kept in `found`, for
/// `--json` to print in place of the text. A workspace that has gathered
/// nothing has no database, and listing that is no reason to create one.
fn shelf(workspace: &WeaverWorkspace, found: &mut Option<Vec<Offer>>) -> Result<Outcome, Failure> {
    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
    // What the workspaces it loads offer first, as the install screen
    // orders them; what they cannot offer is a warning beside the rest, not
    // a reason to show nothing.
    let loads = offered_by_loads(workspace);
    let stderr: Vec<Warning> = loads
        .failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    let mut offers: Vec<Offer> = loads.offers;
    let mut stderr = stderr;
    for gathered in ledger.gathered().map_err(CliError::from)? {
        // Listed only if it could be installed: a skill's name that is not
        // one folder name, or a rule's route no name a rule can have, never
        // can be, wherever it came from.
        if mindweaver_core::install::is_installable_name(gathered.kind, &gathered.name) {
            offers.push(Offer::Shelf(Box::new(gathered)));
        } else {
            stderr.push(
                format!(
                    "warning: {}: declares the name `{}`, which is not one folder name, so it is not offered",
                    gathered.path, gathered.name
                )
                .into(),
            );
        }
    }
    let ok = stderr.is_empty();
    let stdout = shelf_listing(&offers, workspace.loads().is_empty());
    *found = Some(offers);
    Ok(Outcome { stdout, stderr, ok })
}

/// What a scrub of the ledger has to tell, for a command that writes to it.
///
/// Notes rather than warnings, for what it took out: the command did what it
/// was asked, and an exit code that failed once because of a fix would be a
/// script broken by nothing it did. What it could not take out is a warning,
/// since the credential is still on disk.
#[derive(Debug, Default, Clone)]
pub(crate) struct Scrubbed {
    notes: Vec<Warning>,
    warnings: Vec<Warning>,
}

impl Scrubbed {
    /// Every line, notes first, for a command that stopped before it had
    /// anything else to say: the scrub happened all the same, and says so
    /// only this once.
    pub(crate) fn lines(self) -> Vec<Warning> {
        let mut lines = self.notes;
        lines.extend(self.warnings);
        lines
    }

    /// Put in front of what the command went on to say.
    pub(crate) fn before(self, mut outcome: Outcome) -> Outcome {
        outcome.ok = outcome.ok && self.warnings.is_empty();
        outcome.stderr.splice(0..0, self.lines());
        outcome
    }
}

/// What the scrub that opening the ledger to write runs took out
/// ([`mindweaver_core::WeaverWorkspace::ledger_to_write`]), said the way a
/// person can act on: every source it scrubbed now needs its credential from
/// a git credential helper.
pub(crate) fn scrubbed(scrub: &Scrub) -> Scrubbed {
    // What the step to schema 7 made one first, which came first: the
    // spelling that went is what a script may still name it by.
    let mut notes: Vec<Warning> = scrub
        .merged
        .iter()
        .map(|merged| {
            let went = match &merged.reference {
                Some(reference) => format!("{}#{reference}", merged.url),
                None => merged.url.to_string(),
            };
            format!(
                "note: {went}: the same repository as the source `{name}` ({kept}), so the two \
                 are one source now, and the shelf keeps what `{name}` gathered",
                name = merged.into.name,
                kept = merged.into.origin(),
            )
            .into()
        })
        .collect();
    notes.extend(scrub.sources.iter().map(|source| {
        format!(
            "note: {}: the user name, password or token an older Mindweaver stored in this \
                 URL is taken out; gathering it again needs a git credential helper that has it",
            source.url
        )
        .into()
    }));
    match scrub.actions {
        0 => {}
        1 => notes.push(
            String::from(
                "note: 1 action log entry: the user name, password or token an older Mindweaver \
                 stored in a URL in it is taken out",
            )
            .into(),
        ),
        entries => notes.push(
            format!(
                "note: {entries} action log entries: the user names, passwords or tokens an \
                 older Mindweaver stored in URLs in them are taken out"
            )
            .into(),
        ),
    }
    Scrubbed {
        notes,
        warnings: scrub
            .failures
            .iter()
            .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
            .chain(
                scrub.leftovers.iter().map(|leftover| {
                    Warning::coded(leftover.code(), format!("warning: {leftover}"))
                }),
            )
            .collect(),
    }
}

/// What one gather did, in the order it is worth reading: the source, by
/// the name `--from` takes and the URL it was gathered from, then every
/// artifact and what happened to it, then the count.
///
/// The source is named without credentials, as everywhere a URL is shown.
/// A gather that got this far was given none, since core refuses them, so
/// this costs nothing today and still holds if that ever changes. In
/// printable text, as its name is: a plugin's URL is its catalog's.
pub(super) fn gathered(url: &str, report: &Report) -> Outcome {
    let url = printable(&redacted(url));
    let mut stdout = String::new();
    let at = report
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let _ = writeln!(stdout, "{}: {url}{at}", printable(&report.name));

    // Added first, then updated, then removed, then unchanged: a second
    // gather is run to find out what moved, and what moved should not be at
    // the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
        ("removed", &report.removed),
        ("unchanged", &report.unchanged),
    ] {
        for artifact in harvested {
            rows.push(vec![
                label.to_owned(),
                printable(&artifact.name),
                printable(artifact.summary.as_deref().unwrap_or("")),
            ]);
        }
    }
    if rows.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        // What was taken off only when something was: what is on the shelf
        // is counted first, and nothing removed is nothing to say.
        let removed = match report.removed.len() {
            0 => String::new(),
            count => format!(", {count} removed"),
        };
        let _ = writeln!(
            stdout,
            "\n{}: {} added, {} updated, {} unchanged{removed}",
            plural(report.total(), report.kind.noun()),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest; so is what is wrong with one
    // that was taken anyway.
    let mut stderr = said_of(&report.failures, &report.warnings);
    let ok = stderr.is_empty();
    // What a package gathered whole holds beside its skills is a note, and
    // no reason to fail, as a marketplace's plugin's is (#104).
    stderr.extend(holds_note(&report.name, &report.holds, &report.mcp_files));
    Outcome { stdout, stderr, ok }
}

/// What a gather, or a dry run of one, could not take and what is wrong
/// with what it took, as the lines it warns of.
///
/// Printable, both: a warning quotes what the skill declares and a failure
/// the name two skills share and the folders they are in, which are the
/// source's to write, and an escape in one would reach the terminal.
fn said_of(failures: &[gather::Failure], warnings: &[gather::Warning]) -> Vec<Warning> {
    failures
        .iter()
        .map(|failure| {
            Warning::coded(
                failure.code(),
                format!("warning: {}", printable(&failure.to_string())),
            )
        })
        .chain(warnings.iter().map(|warning| {
            Warning::coded(
                warning.issue.code(),
                format!("warning: {}", printable(&warning.to_string())),
            )
        }))
        .collect()
}

/// What a dry run of a gather found: the source, by the URL it would be
/// gathered from and the commit it is at, then each skill or rule it would
/// take, by its name and where it is in the repository, then the count of
/// its kind, and that nothing was gathered.
///
/// Its warnings are a gather's: what could not be taken, and what is wrong
/// with what would be, each failing the command as it would fail a gather,
/// so a script can tell from a dry run's exit what the gather's would be.
fn previewed(url: &str, preview: &Preview) -> Outcome {
    let url = redacted(url);
    let at = preview
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let mut stdout = format!("{url}{at}\n");
    if preview.found.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        let rows: Vec<Vec<String>> = preview
            .found
            .iter()
            .map(|found| vec![printable(&found.name), printable(&found.upstream_path)])
            .collect();
        stdout.push_str(&indent(&table(&rows)));
        let _ = writeln!(
            stdout,
            "\n{} to gather; nothing gathered: run it again without --dry-run to gather them",
            plural(preview.found.len(), preview.kind.noun())
        );
    }
    let stderr = said_of(&preview.failures, &preview.warnings);
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}

/// What a gather of a marketplace did: its catalog, by the name it is
/// called and the URL it was fetched from, then, for `--list`, each plugin
/// it lists and that nothing was gathered, and otherwise each plugin
/// gathered as `gather git` says a source, an empty line apart.
///
/// Each entry that cannot be gathered is a warning, by the marketplace's
/// name, and fails the command, as a skill a gather cannot take does; so
/// does a plugin whose gather failed, by its entry, beside the rest. What a
/// plugin holds beside its skills, which is not taken, is a note.
fn marketplace_gathered(report: &marketplace::Report, pick: &Pick) -> Outcome {
    let catalog = &report.catalog;
    let mut outcome = Outcome::plain(catalog_line(catalog));
    if *pick == Pick::Listing {
        outcome.stdout.push_str(&entries(catalog));
        let _ = writeln!(
            outcome.stdout,
            "\n{} listed; nothing gathered: gather some with --plugin <name>, or every one with \
             --all",
            plural(catalog.listed, "plugin")
        );
    }
    outcome.stderr.extend(entry_failures(catalog));
    outcome.ok = catalog.failures.is_empty();
    for child in &report.children {
        match &child.outcome {
            Ok(report) => {
                let said = gathered(&gathered_url(catalog, child), report);
                outcome.stdout.push('\n');
                outcome.stdout.push_str(&said.stdout);
                outcome.stderr.extend(said.stderr);
                outcome.ok &= said.ok;
                outcome
                    .stderr
                    .extend(holds_note(&report.name, &child.holds, &child.mcp_files));
            }
            Err(error) => {
                outcome.stderr.push(Warning::coded(
                    error.code(),
                    format!(
                        "warning: {}: {}",
                        printable(&child.entry),
                        printable(&error.to_string())
                    ),
                ));
                outcome.ok = false;
            }
        }
    }
    outcome
}

/// The URL a plugin was gathered from, which its report's header says: the
/// marketplace's for one in its repository, and its own otherwise.
fn gathered_url(catalog: &Listed, child: &marketplace::Child) -> String {
    catalog
        .entries
        .iter()
        .find(|entry| entry.name == child.entry)
        .and_then(|entry| entry.plugin.url().map(|url| url.into_owned()))
        .map(|url| mindweaver_core::url::fetched(&url).into_owned())
        .unwrap_or_else(|| catalog.url.to_string())
}

/// The note saying what the plugin the source `name` was gathered from
/// holds beside its skills (`holds`), which is not taken, and the files its
/// MCP servers are in (`mcp_files`); none when it holds nothing.
fn holds_note(name: &str, holds: &[Holding], mcp_files: &[&str]) -> Option<Warning> {
    if holds.is_empty() {
        return None;
    }
    let holds: Vec<String> = holds
        .iter()
        .map(|holding| held(*holding, mcp_files))
        .collect();
    let holds: Vec<&str> = holds.iter().map(String::as_str).collect();
    Some(Warning::from(format!(
        "note: {}: its plugin holds {} too, which are not gathered",
        printable(name),
        and(&holds)
    )))
}

/// What a note says a plugin holds of `holding`: its words, and for its MCP
/// servers the files its folder keeps them in, `mcp_files`, when it keeps
/// any, so a person knows which to read (#104).
fn held(holding: Holding, mcp_files: &[&str]) -> String {
    match holding {
        Holding::McpServers if !mcp_files.is_empty() => {
            let files: Vec<String> = mcp_files.iter().map(|file| format!("`{file}`")).collect();
            let files: Vec<&str> = files.iter().map(String::as_str).collect();
            format!("{} in {}", holding.words(), and(&files))
        }
        _ => holding.words().to_owned(),
    }
}

/// `words` as a sentence lists them: `a`, `a and b`, `a, b and c`.
fn and(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A catalog's first line: the marketplace by its name, then the URL its
/// catalog was fetched from and the commit it was read at.
fn catalog_line(catalog: &Listed) -> String {
    let at = catalog
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let reference = catalog
        .reference
        .as_deref()
        .map_or_else(String::new, |at| format!("#{}", printable(at)));
    format!(
        "{}: {}{reference}{at}\n",
        printable(&catalog.name),
        printable(&catalog.url)
    )
}

/// Each plugin a catalog lists that can be gathered, by its name, its kind
/// of source and where it is, with the branch and the commit it is pinned
/// to when its entry names them, indented under the catalog's line.
fn entries(catalog: &Listed) -> String {
    if catalog.entries.is_empty() {
        return String::from("  no plugin it lists can be gathered\n");
    }
    let rows: Vec<Vec<String>> = catalog
        .entries
        .iter()
        .map(|entry| {
            let at = match &entry.plugin {
                Plugin::Relative { route } if route.is_empty() => String::from("."),
                Plugin::Relative { route } | Plugin::Local { route } => route.clone(),
                Plugin::Github { repo, .. } => repo.clone(),
                Plugin::Url { url, .. } => redacted(url).into_owned(),
                Plugin::GitSubdir { url, path, .. } => format!("{} {path}", redacted(url)),
            };
            let mut pins = Vec::new();
            if let Some(reference) = entry.plugin.reference() {
                pins.push(format!("ref {}", printable(reference)));
            }
            if let Some(sha) = entry.plugin.sha() {
                pins.push(format!("pinned {}", printable(short(sha))));
            }
            vec![
                printable(&entry.name),
                entry.plugin.kind().to_owned(),
                printable(&at),
                pins.join(" "),
            ]
        })
        .collect();
    indent(&table(&rows))
}

/// Each entry of `catalog` that cannot be gathered, as a warning by the
/// marketplace's name.
fn entry_failures(catalog: &Listed) -> Vec<Warning> {
    catalog
        .failures
        .iter()
        .map(|failure| {
            Warning::coded(
                failure.code(),
                format!(
                    "warning: {}: {}",
                    printable(&catalog.name),
                    printable(&failure.to_string())
                ),
            )
        })
        .collect()
}

/// What `--all` would gather of a catalog, a dry run: its line, each plugin
/// it lists, and, when it lists more than a yes is waited for beyond, that
/// nothing was gathered; with fewer, what follows is the gather itself.
fn unconfirmed(catalog: &Listed) -> Outcome {
    let mut outcome = Outcome::plain(catalog_line(catalog));
    outcome.stdout.push_str(&entries(catalog));
    let _ = writeln!(
        outcome.stdout,
        "\n{} to gather; nothing gathered: add --yes to gather every one",
        plural(catalog.entries.len(), "plugin")
    );
    outcome.stderr.extend(entry_failures(catalog));
    outcome.ok = outcome.stderr.is_empty();
    outcome
}

/// Everything that can be installed from elsewhere, with where each thing
/// comes from, spelled as `--from` takes it: a source by its name
/// ([`Offer::listed_from`]), then the commit it is pinned to, on a row
/// taken at it ([`pinned_at`]), in a column that earns its place only once
/// such a row is shown, and what was gathered's licence as it says it,
/// `unknown` when nothing does, in one that earns it once the shelf offers
/// anything.
///
/// A loaded workspace's own has no licence cell: nothing gathered it, and
/// a licence is what a gather records of where a skill came from, which an
/// `unknown` there would claim was looked for.
///
/// Empty, it says how to fill it, and suggests loading only when nothing is
/// loaded yet; `mindweaver load` on its own lists what is.
fn shelf_listing(shelf: &[Offer], nothing_loaded: bool) -> String {
    if shelf.is_empty() {
        return if nothing_loaded {
            String::from(
                "nothing gathered yet\n  take some with `mindweaver gather git <url>`, \
                 or offer a workspace's own with `mindweaver load <path>`\n",
            )
        } else {
            String::from(
                "nothing gathered yet, and the loaded workspaces offer nothing\n  \
                 take some with `mindweaver gather git <url>`; `mindweaver load` lists what is loaded\n",
            )
        };
    }

    // The same rule a catalog listing follows: a kind column earns its place
    // only once more than one kind is on the shelf.
    let mut kinds: Vec<Kind> = shelf.iter().map(|entry| entry.kind()).collect();
    kinds.sort();
    kinds.dedup();
    let name_kind = kinds.len() > 1;
    // As the kind column: a licence only once the shelf offers something,
    // a pin only once a row taken at one is shown.
    let name_license = shelf.iter().any(|entry| shelved(entry).is_some());
    let name_pin = shelf
        .iter()
        .filter_map(shelved)
        .any(|gathered| pinned_at(gathered).is_some());

    let rows: Vec<Vec<String>> = shelf
        .iter()
        .map(|entry| {
            let gathered = shelved(entry);
            let mut row = Vec::new();
            if name_kind {
                row.push(entry.kind().slug().to_owned());
            }
            row.push(printable(entry.name()));
            row.push(printable(&entry.listed_from()));
            if name_pin {
                let pin = gathered.and_then(pinned_at);
                row.push(pin.map_or_else(String::new, |pin| format!("pinned {}", short(pin))));
            }
            if name_license {
                row.push(gathered.map_or_else(String::new, |gathered| {
                    printable(gathered.license.as_deref().unwrap_or("unknown"))
                }));
            }
            row.push(printable(entry.summary().unwrap_or("")));
            row
        })
        .collect();
    table(&rows)
}

/// What was gathered of `entry`, when the shelf offers it.
fn shelved(entry: &Offer) -> Option<&Gathered> {
    match entry {
        Offer::Shelf(gathered) => Some(gathered),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    }
}

/// The commit `gathered`'s source is pinned to, when `gathered` was taken
/// at it.
///
/// A gather takes off what the commit it took lacks, but keeps a skill
/// whose folder is there and no longer reads, as an earlier gather took it
/// at another commit, so it stays on the shelf beside a pin, under its own
/// `revision`; saying it is pinned would claim the pin holds it. Its cell
/// is left empty rather than given its own commit, which `--json` holds: a
/// column of pins and commits would read as pins.
fn pinned_at(gathered: &Gathered) -> Option<&str> {
    let pin = gathered.source.pin.as_ref()?;
    (gathered.revision.as_ref() == Some(pin)).then_some(pin.as_str())
}

/// A revision as a person refers to it.
fn short(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
}

#[cfg(test)]
#[path = "gather_test.rs"]
mod tests;
