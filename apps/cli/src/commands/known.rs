//! The commands about the flayer workspaces this machine knows:
//! `workspaces`, which lists them, with the one in use marked, and
//! `workspaces forget`, which takes one out of the list.
//!
//! What is known, in which order, and which one is in use are core's
//! (`known::known`, fed by `resolve`); what is decided here is only how the
//! list reads. `flayer init` is what remembers one, in `project`.

use std::fmt::Write as _;
use std::path::Path;

use mindflayer_core::resolve::{resolve, Intent};
use mindflayer_core::{known, Known, KnownWorkspace};

use super::{reading, typed};
use crate::render::{as_stored, json, plural, printable, table};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

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
    let warnings: Vec<Warning> = failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    let outcome = if as_json {
        json(&known, &warnings)
    } else {
        listing(&known, warnings)
    };
    match &in_use {
        Some(workspace) => reading(workspace, |_| Ok(outcome)),
        None => Ok(outcome),
    }
}

/// `flayer workspaces forget`: every entry the default workspace in
/// `lookup`'s home keeps for the workspace at `path`, typed from
/// `directory` as `unlink`'s paths are, taken out, each said.
///
/// Refused when none points at it, and with no default at all: a typo far
/// more often than a no-op. Never makes the default to find that out.
pub(crate) fn forget_command(
    directory: &Path,
    path: &Path,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let root = typed(directory, path)?;
    let home = lookup
        .home
        .as_deref()
        .ok_or_else(|| CliError::NoDefault(root.clone()))?;
    let mut text = String::new();
    for entry in known::forget(home, &root)? {
        let _ = writeln!(text, "forgot {}", as_stored(&entry));
    }
    Ok(Outcome::plain(text))
}

/// The list as a table: a `*` before the one in use, as `git branch` marks
/// the branch checked out, then each one's name, where it is and what it
/// holds; and the advice that applies to what is in it.
fn listing(known: &Known, stderr: Vec<Warning>) -> Outcome {
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
        let gone = known
            .workspaces
            .iter()
            .any(|listed| listed.remembered && listed.stale.is_some());
        let mut hints = Vec::new();
        if default && unremembered {
            hints.push(
                "`flayer init <path>` remembers the one in use, and changes nothing else in it",
            );
        }
        if gone {
            hints.push("`flayer workspaces forget <path>` forgets one that cannot be opened");
        }
        if !hints.is_empty() {
            stdout.push('\n');
            for hint in hints {
                let _ = writeln!(stdout, "  {hint}");
            }
        }
    }
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
