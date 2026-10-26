//! The commands that decide which projects a workspace manages: `init` at
//! either level, `link`, `unlink`, and `scan`, which only finds what there is
//! to link.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindflayer_core::scan::MAX_LISTED;
use mindflayer_core::{
    scan, Directories, FlayerWorkspace, Initialization, Kind, MindProject, Registration, Scan,
    WorkspaceError,
};

use super::{all_or_nothing, project_here, targets, workspace_above, workspace_here};
use crate::render::{as_stored, printable, table};
use crate::{CliError, Failure, Outcome};

/// `mind init`: the directory made a mind project.
pub(crate) fn init_project(
    directory: &Path,
    skills: Option<&Path>,
    rules: Option<&Path>,
) -> Result<Outcome, Failure> {
    let directories = asked_directories(skills, rules);
    let (project, outcome) = MindProject::init_with(directory, &directories)?;
    Ok(Outcome::plain(initialized(
        "mind project",
        project.name(),
        &project.mind_dir(),
        outcome,
    )))
}

/// `flayer init`: the directory made a flayer workspace.
pub(crate) fn init_workspace(
    directory: &Path,
    skills: Option<&Path>,
    rules: Option<&Path>,
) -> Result<Outcome, Failure> {
    let directories = asked_directories(skills, rules);
    let (workspace, outcome) = FlayerWorkspace::init_with(directory, &directories)?;
    Ok(Outcome::plain(initialized(
        "flayer workspace",
        workspace.name(),
        &workspace.flayer_dir(),
        outcome,
    )))
}

/// What `init` was asked to put where, at either level.
///
/// Only what was asked for is set. A kind nobody mentioned is left to the
/// default rather than pinned to it here, so the default lives in one place.
fn asked_directories(skills: Option<&Path>, rules: Option<&Path>) -> Directories {
    let mut directories = Directories::default();
    if let Some(skills) = skills {
        directories = directories.with(Kind::Skill, skills);
    }
    if let Some(rules) = rules {
        directories = directories.with(Kind::Rule, rules);
    }
    directories
}

/// What `init` says it did, at either level.
fn initialized(kind: &str, name: &str, marker: &Path, outcome: Initialization) -> String {
    let marker = marker.display();
    match outcome {
        Initialization::Created => format!("initialized {kind} `{name}` in {marker}\n"),
        Initialization::AlreadyInitialized => {
            format!("{kind} `{name}` already initialized in {marker}\n")
        }
    }
}

/// `mind link`: the project here, registered with the workspace above it.
pub(crate) fn link_here(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    let project = project_here(directory)?;
    let mut workspace = workspace_above(&project, home)?;
    let (entry, registration) = workspace.link(&project)?;
    Ok(Outcome::plain(linked(project.name(), &entry, registration)))
}

/// `mind unlink`: the project here, dropped from the workspace above it.
pub(crate) fn unlink_here(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    let project = project_here(directory)?;
    let mut workspace = workspace_above(&project, home)?;
    let removed = workspace.unlink(project.root())?;
    Ok(Outcome::plain(unlinked(&removed)))
}

/// `flayer link`: every path given, registered with the workspace here.
pub(crate) fn link_projects(
    directory: &Path,
    projects: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, home)?;
    // All or nothing: every path is opened before any is registered,
    // so a typo in the third does not leave the first two linked by a
    // command that reports failure. Opening is the check — a
    // directory with no `.mind` is not a project, and registering one
    // would only fail later, further from the command that caused it.
    let mut opened: Vec<MindProject> = Vec::new();
    let mut refused = Vec::new();
    for target in targets(directory, projects)? {
        match MindProject::open(&target) {
            Ok(project) => opened.push(project),
            Err(error) => refused.push(CliError::from(error)),
        }
    }
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut text = String::new();
    for project in &opened {
        let (entry, registration) = workspace.link(project)?;
        text.push_str(&linked(project.name(), &entry, registration));
    }
    Ok(Outcome::plain(text))
}

/// `flayer unlink`: every path given, dropped from the workspace here.
pub(crate) fn unlink_projects(
    directory: &Path,
    projects: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, home)?;
    let targets = targets(directory, projects)?;
    // All or nothing, for the reason `link` is: nothing is dropped
    // unless everything named is registered.
    let refused = targets
        .iter()
        .filter(|target| !workspace.is_linked(target))
        .map(|target| {
            CliError::Workspace(WorkspaceError::NotRegistered {
                path: target.clone(),
                workspace: workspace.root().to_path_buf(),
            })
        })
        .collect();
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    // One rewrite for all of them: two spellings of one project are
    // one project, not a second one to fail on halfway.
    Ok(Outcome::plain(unlinked(&workspace.unlink_all(&targets)?)))
}

/// What `link` says it did, at either level.
fn linked(name: &str, entry: &Path, registration: Registration) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!("linked {name} as {entry}\n"),
        Registration::AlreadyRegistered => format!("{name} is already linked as {entry}\n"),
    }
}

/// What `unlink` says it did: every spelling that pointed at the project, not
/// just the first. Reporting "unlinked" while one entry still registers it is
/// the failure this reports its way out of.
fn unlinked(removed: &[PathBuf]) -> String {
    removed.iter().fold(String::new(), |mut text, entry| {
        let _ = writeln!(text, "unlinked {}", as_stored(entry));
        text
    })
}

/// `flayer scan`: every repository under the workspace here.
pub(crate) fn scan_workspace(directory: &Path, home: Option<&Path>) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, home)?;
    Ok(scanned(&workspace, &scan(&workspace)))
}

/// Every repository under a workspace, and whether it is linked yet.
fn scanned(workspace: &FlayerWorkspace, scan: &Scan) -> Outcome {
    let mut stdout = String::new();
    if scan.found.is_empty() {
        let _ = writeln!(
            stdout,
            "no mind projects or git repositories under {}",
            workspace.root().display()
        );
    } else {
        let rows: Vec<Vec<String>> = scan
            .found
            .iter()
            .map(|found| {
                let what = match (found.linked, found.mind) {
                    (true, _) => "",
                    (false, true) => "mind project",
                    (false, false) => "git repository, not a mind project yet",
                };
                vec![
                    printable(&found.route),
                    String::from(if found.linked { "linked" } else { "not linked" }),
                    what.to_owned(),
                ]
            })
            .collect();
        stdout.push_str(&table(&rows));

        // Only the advice that applies: a hint about a case the listing does
        // not contain is one more line to read past.
        let mut hints = Vec::new();
        if scan.found.iter().any(|found| !found.linked && found.mind) {
            hints.push("link one with `flayer link <path>`");
        }
        if scan.found.iter().any(|found| !found.linked && !found.mind) {
            hints.push("a repository needs `mind -C <path> init` before it can be linked");
        }
        if !hints.is_empty() {
            stdout.push('\n');
            for hint in hints {
                let _ = writeln!(stdout, "  {hint}");
            }
        }
    }

    let mut stderr: Vec<String> = scan
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    if scan.truncated {
        stderr.push(format!(
            "warning: stopped after listing {MAX_LISTED} folders; anything further down can still be linked by path"
        ));
    }
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}
