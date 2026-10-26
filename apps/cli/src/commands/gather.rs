//! `flayer gather`: collecting artifacts onto the workspace's shelf, and
//! listing everything that can be installed from elsewhere.

use std::fmt::Write as _;
use std::path::Path;

use mindflayer_core::gather::{self, Report, Request};
use mindflayer_core::Kind;

use super::workspace_here;
use crate::render::{indent, plural, printable, table};
use crate::{CliError, Failure, GatherCommand, Outcome};

/// Collect artifacts onto the workspace shelf, or say what is already on it.
pub(crate) fn run_gather(
    command: &GatherCommand,
    directory: &Path,
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, home)?;
    let ledger = workspace.ledger().map_err(CliError::from)?;

    match command {
        GatherCommand::Git {
            url,
            path,
            reference,
        } => {
            let request = Request::git(url.clone())
                .from_subdirectory(path.clone())
                .at(reference.clone());
            let report = gather::gather(&workspace, &ledger, &request).map_err(CliError::from)?;
            Ok(gathered(url, &report))
        }
        GatherCommand::List => {
            use mindflayer_core::install::{offered_by_loads, Offer};
            // What the workspaces it loads offer first, as the install screen
            // orders them; what they cannot offer is a warning beside the
            // rest, not a reason to show nothing.
            let loads = offered_by_loads(&workspace);
            let stderr: Vec<String> = loads
                .failures
                .iter()
                .map(|failure| format!("warning: {failure}"))
                .collect();
            let mut offers: Vec<Offer> = loads.offers;
            let mut stderr = stderr;
            for gathered in ledger.gathered().map_err(CliError::from)? {
                // Listed only if it could be installed: a name that is not one
                // folder name never can be, wherever it came from.
                if mindflayer_core::install::is_folder_name(&gathered.name) {
                    offers.push(Offer::Shelf(gathered));
                } else {
                    stderr.push(format!(
                        "warning: {}: declares the name `{}`, which is not one folder name, so it is not offered",
                        gathered.path, gathered.name
                    ));
                }
            }
            let ok = stderr.is_empty();
            Ok(Outcome {
                stdout: shelf_listing(&offers, workspace.loads().is_empty()),
                stderr,
                ok,
            })
        }
    }
}

/// What one gather did, in the order it is worth reading: the source, then
/// every artifact and what happened to it, then the count.
fn gathered(url: &str, report: &Report) -> Outcome {
    let mut stdout = String::new();
    let at = report
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let _ = writeln!(stdout, "{url}{at}");

    // Added first, then updated, then unchanged: a second gather is run to
    // find out what moved, and what moved should not be at the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
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
        let _ = writeln!(
            stdout,
            "\n{}: {} added, {} updated, {} unchanged",
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest; so is what is wrong with one
    // that was taken anyway.
    let stderr: Vec<String> = report
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .chain(
            report
                .warnings
                .iter()
                .map(|warning| format!("warning: {warning}")),
        )
        .collect();
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}

/// Everything that can be installed from elsewhere, with where each thing
/// comes from — spelled as `--from` takes it.
///
/// Empty, it says how to fill it — and suggests loading only when nothing is
/// loaded yet; `flayer load` on its own lists what is.
fn shelf_listing(shelf: &[mindflayer_core::install::Offer], nothing_loaded: bool) -> String {
    if shelf.is_empty() {
        return if nothing_loaded {
            String::from(
                "nothing gathered yet\n  take some with `flayer gather git <url>`, \
                 or offer a workspace's own with `flayer load <path>`\n",
            )
        } else {
            String::from(
                "nothing gathered yet, and the loaded workspaces offer nothing\n  \
                 take some with `flayer gather git <url>`; `flayer load` lists what is loaded\n",
            )
        };
    }

    // The same rule a catalog listing follows: a kind column earns its place
    // only once more than one kind is on the shelf.
    let mut kinds: Vec<Kind> = shelf.iter().map(|entry| entry.kind()).collect();
    kinds.sort();
    kinds.dedup();
    let name_kind = kinds.len() > 1;

    let rows: Vec<Vec<String>> = shelf
        .iter()
        .map(|entry| {
            let mut row = Vec::new();
            if name_kind {
                row.push(entry.kind().slug().to_owned());
            }
            row.push(printable(entry.name()));
            row.push(printable(&entry.origin()));
            row.push(printable(entry.summary().unwrap_or("")));
            row
        })
        .collect();
    table(&rows)
}

/// A revision as a person refers to it.
fn short(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
}
