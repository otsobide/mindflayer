//! `doctor`: what keeps a project's agents from what it gives them, which
//! no agent says.
//!
//! Core examines each project and says what it found ([`Report`]), from
//! file reads alone, told the agent homes and the names of the variables
//! set by the lookup; this only lays it out, each finding with its fix,
//! and what could not be read as a warning.

use mindweaver_core::doctor::{Examined, Report};
use mindweaver_core::{MindProject, WeaverWorkspace};

use super::consulted;
use crate::render::{json_or_text, printable};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// `mindweaver doctor`: every project the workspace links.
pub(crate) fn doctor_of_workspace(
    workspace: &WeaverWorkspace,
    lookup: &Lookup,
    as_json: bool,
) -> Outcome {
    // As `mindweaver status` says it, and for the same reason; in JSON, a
    // workspace with nothing linked is one with no projects in its data.
    if workspace.config().projects.is_empty() && !as_json {
        return Outcome::plain(format!(
            "{} manages no projects yet\n  link one with `mindweaver link <path>`\n",
            workspace.name()
        ));
    }
    let report = Report::of_workspace(workspace, &lookup.machine());
    report_of(&report, true, Vec::new(), as_json)
}

/// `mindweaver doctor -p`: one project the workspace links.
pub(crate) fn doctor_of_project(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    lookup: &Lookup,
    as_json: bool,
) -> Outcome {
    let report = Report::of_project(project, Some(workspace), &lookup.machine());
    report_of(&report, false, Vec::new(), as_json)
}

/// `mind doctor`: the project at or above here, against the workspace `mind`
/// consults about it when that one links it, which says what was installed
/// into it; alone otherwise, which leaves out only the checks of what was
/// installed.
///
/// A workspace found that cannot be read is a warning beside the report,
/// as `mind status` says one, rather than refusing it: a read has nothing
/// to protect by refusing. Not one `-W` or the activation names that is no
/// workspace, which refuses this as it refuses every other command.
pub(crate) fn doctor_here(
    project: MindProject,
    lookup: &Lookup,
    as_json: bool,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(&project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (
            None,
            Some(Warning::coded(error.code(), format!("warning: {error}"))),
        ),
    };
    let linking = workspace
        .as_ref()
        .filter(|workspace| workspace.is_linked(project.root()));
    let report = Report::of_project(&project, linking, &lookup.machine());
    Ok(report_of(
        &report,
        false,
        unread.into_iter().collect(),
        as_json,
    ))
}

/// `report` laid out: each finding, then how many projects were checked and
/// what was found, and what could not be read, beside `warnings`, as
/// warnings, on stderr and in the envelope under `--json`. `across` when it
/// is about every project a workspace links, where each finding names its
/// project.
///
/// Not ok when anything was found, as `validate` is not when something
/// does not pass, so that it works in a script or CI, nor when anything
/// could not be read, as every reading command.
fn report_of(report: &Report, across: bool, mut warnings: Vec<Warning>, as_json: bool) -> Outcome {
    let mut stdout = String::new();
    for examined in &report.projects {
        let of = if across {
            format!("{}: ", printable(&named(examined)))
        } else {
            String::new()
        };
        for finding in &examined.findings {
            stdout.push_str(&format!(
                "{of}{}\n  fix: {}\n",
                printable(&finding.message),
                printable(&finding.fix)
            ));
        }
        warnings.extend(
            examined.failures.iter().map(|failure| {
                Warning::coded(failure.code, format!("warning: {}", failure.message))
            }),
        );
    }
    let found = report.found();
    if found > 0 {
        stdout.push('\n');
    }
    let checked = report.projects.len();
    let projects = if checked == 1 { "project" } else { "projects" };
    let said = match found {
        0 => String::from("nothing found"),
        1 => String::from("1 finding"),
        found => format!("{found} findings"),
    };
    stdout.push_str(&format!("{checked} {projects} checked, {said}\n"));
    let text = Outcome {
        ok: found == 0 && warnings.is_empty(),
        stdout,
        stderr: warnings,
    };
    json_or_text(as_json.then_some(report), text)
}

/// What a finding names its project by: its name, or its entry when it
/// opens none.
fn named(examined: &Examined) -> String {
    examined
        .name
        .clone()
        .or_else(|| examined.entry.clone())
        .unwrap_or_else(|| examined.path.display().to_string())
}
