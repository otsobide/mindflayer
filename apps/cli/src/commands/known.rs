//! The command about the flayer workspaces this machine knows: `workspaces`,
//! which lists them, with the one in use marked.
//!
//! What is known, in which order, and which one is in use are core's
//! (`known::known`, fed by `resolve`); what is decided here is only how the
//! list reads. `flayer init` is what remembers one, in `project`.

use std::fmt::Write as _;
use std::path::Path;

use mindflayer_core::resolve::{resolve, Intent};
use mindflayer_core::{known, Known, KnownWorkspace};

use super::reading;
use crate::render::{json, plural, printable, table};
use crate::{Failure, Lookup, Outcome};

/// `flayer workspaces`: every workspace the default one in `lookup`'s home
/// knows, with the one a command run from `directory` would use marked, as
/// JSON when `as_json`.
///
/// A read, so the default workspace is never made for it: one not made yet
/// knows nothing, and the hint every read of it gives says where it will
/// be. A remembered workspace that has gone is listed, with a warning, and
/// never hides the rest.
pub(crate) fn workspaces_command(
    directory: &Path,
    lookup: &Lookup,
    as_json: bool,
) -> Result<Outcome, Failure> {
    let in_use = resolve(&lookup.inputs(directory, Intent::Read))?.map(|chosen| chosen.workspace);
    let (known, failures) = known::known(lookup.home.as_deref(), in_use.as_ref());
    let warnings: Vec<String> = failures.iter().map(ToString::to_string).collect();
    let outcome = if as_json {
        json(&known, warnings)
    } else {
        listing(&known, warnings)
    };
    match &in_use {
        Some(workspace) => reading(workspace, |_| Ok(outcome)),
        None => Ok(outcome),
    }
}

/// The list as a table: a `*` before the one in use, as `git branch` marks
/// the branch checked out, then each one's name, where it is and what it
/// holds; and the advice that applies to what is in it.
fn listing(known: &Known, warnings: Vec<String>) -> Outcome {
    let mut stdout = String::new();
    if known.workspaces.is_empty() {
        stdout.push_str("no workspaces known\n");
    } else {
        let rows: Vec<Vec<String>> = known
            .workspaces
            .iter()
            .map(|listed| {
                vec![
                    listed
                        .name
                        .as_deref()
                        .map_or_else(|| String::from("-"), printable),
                    printable(&listed.path.display().to_string()),
                    holds(listed),
                ]
            })
            .collect();
        // Marked outside the table, so a column of one character does not
        // take the two spaces every column is padded with.
        for (line, listed) in table(&rows).lines().zip(&known.workspaces) {
            let mark = if listed.in_use { '*' } else { ' ' };
            let _ = writeln!(stdout, "{mark} {line}");
        }

        // Only while there is a default to remember it in: before that,
        // `flayer init` would remember nothing either.
        let default = known
            .workspaces
            .iter()
            .any(|listed| listed.default && listed.stale.is_none());
        let unremembered = known
            .workspaces
            .iter()
            .any(|listed| !listed.default && !listed.remembered);
        if default && unremembered {
            stdout.push_str(
                "\n  `flayer init <path>` remembers the one in use, and changes nothing else in it\n",
            );
        }
    }
    let stderr: Vec<String> = warnings
        .iter()
        .map(|warning| format!("warning: {warning}"))
        .collect();
    Outcome {
        ok: stderr.is_empty(),
        stdout,
        stderr,
    }
}

/// What one workspace holds, and what it is to the list: "3 projects, 1
/// stale; the default one".
fn holds(listed: &KnownWorkspace) -> String {
    if listed.stale.is_some() {
        return String::from("cannot be opened");
    }
    let mut said = match (listed.projects, listed.stale_projects) {
        (0, _) => String::from("no projects"),
        (projects, 0) => plural(projects, "project"),
        (projects, stale) => format!("{}, {stale} stale", plural(projects, "project")),
    };
    if listed.default {
        said.push_str("; the default one");
    } else if !listed.remembered {
        said.push_str("; not remembered");
    }
    said
}
