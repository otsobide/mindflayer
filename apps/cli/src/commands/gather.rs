//! `mindweaver gather`: collecting artifacts onto the workspace's shelf, and
//! listing everything that can be installed from elsewhere.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::gather::{self, Gathered, Report, Request, Scrub, Watch};
use mindweaver_core::install::{offered_by_loads, Offer};
use mindweaver_core::ledger::{Ledger, SourceListing};
use mindweaver_core::sources::{self, Regathered, Renamed, Which};
use mindweaver_core::url::redacted;
use mindweaver_core::{Kind, WeaverWorkspace};

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
            reference,
            name,
            pin,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
            let said = scrubbed(&scrub);
            let request = Request::git(url.clone())
                .from_subdirectory(path.clone())
                .at(reference.clone())
                .named(name.clone())
                .pinned(pin.clone())
                .watched_by(watch.clone());
            match gather::gather(&workspace, &ledger, &request) {
                Ok(report) => Ok(said.before(gathered(url, &report))),
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
        GatherCommand::Sources { json } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                sources(workspace, &mut found)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
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
/// it was gathered from, and one that could not be gathered as a warning,
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
        match &each.outcome {
            Ok(report) => {
                let said = gathered(each.source.url.as_str(), report);
                if !outcome.stdout.is_empty() {
                    outcome.stdout.push('\n');
                }
                outcome.stdout.push_str(&said.stdout);
                outcome.stderr.extend(said.stderr);
                outcome.ok &= said.ok;
            }
            Err(error) => {
                outcome.stderr.push(Warning::coded(
                    error.code(),
                    format!("warning: {}: {error}", printable(&each.source.name)),
                ));
                outcome.ok = false;
            }
        }
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
/// a source has one, how many skills it holds, and when it was last
/// gathered, in UTC, the one zone a time can be told in without a
/// database of them.
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
            let mut row = vec![printable(&it.source.name), printable(&it.source.url)];
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
            row.push(plural(it.artifacts, "skill"));
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
        // Listed only if it could be installed: a name that is not one folder
        // name never can be, wherever it came from.
        if mindweaver_core::install::is_folder_name(&gathered.name) {
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
/// this costs nothing today and still holds if that ever changes.
fn gathered(url: &str, report: &Report) -> Outcome {
    let url = redacted(url);
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
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest; so is what is wrong with one
    // that was taken anyway.
    let stderr: Vec<Warning> = report
        .failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        // Printable, because a warning quotes what the skill declares, which
        // is the source's to write.
        .chain(report.warnings.iter().map(|warning| {
            Warning::coded(
                warning.issue.code(),
                format!("warning: {}", printable(&warning.to_string())),
            )
        }))
        .collect();
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
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
