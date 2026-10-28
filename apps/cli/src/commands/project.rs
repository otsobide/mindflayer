//! The commands that set projects and workspaces up: `init`, which makes
//! either one, `link` and `unlink`, which decide which projects a workspace
//! manages, and `scan`, which only finds what there is to link.
//!
//! Both levels of `init`, `link` and `unlink` sit side by side here, so a
//! command's two levels change together.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindflayer_core::relink;
use mindflayer_core::scan::{Order, Reason, MAX_LISTED};
use mindflayer_core::{
    known, scan, Agent, Directories, FlayerWorkspace, Initialization, Kind, MindProject,
    Registration, Scan, Share, WorkspaceError,
};

use super::gather::scrubbed;
use super::{
    all_or_nothing, consulted, project_here, targets, typed, workspace_for, workspace_here,
};
use crate::render::{as_stored, json, plural, printable, table};
use crate::{CliError, Failure, Lookup, Outcome};

/// `mind init`: the directory made a mind project, marked by its settings,
/// for `agents`, and shared with the team or kept out of git (D-01).
///
/// Only the folders it was asked for are passed: core fills in the rest,
/// since which folders a new project gets is a rule (D-06).
pub(crate) fn init_project(
    directory: &Path,
    agents: &[Agent],
    share: Share,
    skills: Option<&Path>,
    rules: Option<&Path>,
) -> Result<Outcome, Failure> {
    let directories = asked_directories(skills, rules);
    let (project, outcome) = MindProject::init_asked(directory, agents, share, &directories)?;
    Ok(Outcome::plain(initialized(
        "mind project",
        project.name(),
        &project.mind_dir(),
        outcome,
    )))
}

/// `flayer init`: the directory made a flayer workspace, and remembered by
/// the default one in `lookup`'s home, when that is made, so `flayer
/// workspaces` lists it.
///
/// One already made is remembered too, and otherwise left as it was: what
/// makes a workspace from before the default one, or from before the list,
/// known. A default that cannot remember it is a warning after the init,
/// which is done by then and is not undone for it.
pub(crate) fn init_workspace(
    directory: &Path,
    skills: Option<&Path>,
    rules: Option<&Path>,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let directories = asked_directories(skills, rules);
    let (workspace, outcome) = FlayerWorkspace::init_with(directory, &directories)?;
    let mut stdout = initialized(
        "flayer workspace",
        workspace.name(),
        &workspace.flayer_dir(),
        outcome,
    );
    let mut stderr = Vec::new();
    match known::remember(lookup.home.as_deref(), &workspace) {
        // Said only when it is news: a second init of one already known
        // changes nothing, and says nothing more than the first line.
        Ok(Some((entry, Registration::Added))) => {
            let _ = writeln!(
                stdout,
                "  remembered as {} by the default workspace",
                as_stored(&entry)
            );
        }
        Ok(_) => {}
        Err(error) => {
            stderr.push(format!(
                "warning: the default workspace could not remember it: {error}"
            ));
        }
    }
    Ok(Outcome {
        ok: stderr.is_empty(),
        stdout,
        stderr,
    })
}

/// `mind init --new-id`: the project here given an id of its own, and the
/// workspace consulted about it told, when it links the project.
///
/// The project at the directory itself rather than one found walking up, as
/// `init` makes one there: renewing a project somebody is standing below
/// would give the wrong one a new id. A workspace that cannot be read is a
/// warning after the id is given, as it is after a change to the settings,
/// whose ledger learns the new id the next time it is written.
pub(crate) fn renew_here(directory: &Path, lookup: &Lookup) -> Result<Outcome, Failure> {
    let project = MindProject::open(directory).map_err(CliError::from)?;
    let (workspace, unread) = match consulted(&project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (None, Some(format!("warning: {error}"))),
    };
    let renewed = relink::new_id(workspace.as_ref(), &project).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "gave {} a new id, {} (it was {})\n",
        printable(&renewed.name),
        renewed.id,
        renewed.was
    ));
    outcome.stderr.extend(
        renewed
            .excluded
            .as_ref()
            .and_then(|followed| followed.warning())
            .map(|warning| format!("note: {warning}")),
    );
    if let Some(warning) = unread {
        outcome.stderr.push(warning);
        outcome.ok = false;
    }
    Ok(scrubbed(&renewed.scrub).before(outcome))
}

/// `flayer relink`: the entry for `old` pointed at `new`, where the project
/// it was moved, with every record the workspace keeps of it.
pub(crate) fn relink_project(
    directory: &Path,
    old: &Path,
    new: &Path,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, lookup)?;
    let old = typed(directory, old)?;
    let new = typed(directory, new)?;
    let relinked = relink::relink(&mut workspace, &old, &new).map_err(CliError::from)?;
    let mut stdout = format!(
        "relinked {} as {}, from {}\n",
        printable(&relinked.name),
        as_stored(&relinked.entry),
        as_stored(&relinked.was)
    );
    if relinked.installations > 0 {
        let _ = writeln!(
            stdout,
            "  {} kept with it",
            plural(relinked.installations, "install")
        );
    }
    let mut outcome = Outcome::plain(stdout);
    outcome.stderr.extend(
        relinked
            .excluded
            .as_ref()
            .and_then(|followed| followed.warning())
            .map(|warning| format!("note: {warning}")),
    );
    Ok(scrubbed(&relinked.scrub).before(outcome))
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

/// `mind link`: the project here, registered with its workspace.
pub(crate) fn link_here(directory: &Path, lookup: &Lookup) -> Result<Outcome, Failure> {
    let project = project_here(directory)?;
    let mut workspace = workspace_for(&project, lookup)?;
    let (entry, registration) = workspace.link(&project)?;
    Ok(Outcome::plain(linked(project.name(), &entry, registration)))
}

/// `mind unlink`: the project here, dropped from its workspace.
pub(crate) fn unlink_here(directory: &Path, lookup: &Lookup) -> Result<Outcome, Failure> {
    let project = project_here(directory)?;
    let mut workspace = workspace_for(&project, lookup)?;
    let removed = workspace.unlink(project.root())?;
    Ok(Outcome::plain(unlinked(&removed)))
}

/// `flayer link`: every path given, registered with the workspace here.
pub(crate) fn link_projects(
    directory: &Path,
    projects: &[PathBuf],
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, lookup)?;
    // All or nothing: every path is opened before any is registered,
    // so a typo in the third does not leave the first two linked by a
    // command that reports failure. Opening is the check: a
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
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let mut workspace = workspace_here(directory, lookup)?;
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

/// `flayer scan`: every repository `request` asks for, under the workspace
/// here or the folders it names, the default workspace too before it is
/// made, since what is under it does not wait for that; as JSON when
/// `as_json`.
pub(crate) fn scan_workspace(
    workspace: &FlayerWorkspace,
    request: &scan::Request,
    as_json: bool,
) -> Outcome {
    let scan = scan(workspace, request);
    let warnings = scan_warnings(&scan);
    if as_json {
        return json(&scan, warnings);
    }
    scanned(&scan, warnings, request.order == Order::Suggested)
}

/// Why one found at `route` is worth linking, as `flayer scan --suggest`
/// says it and the minds screen and the link form show it, in these words
/// rather than their own: for one found moved, where from, since its reason
/// is its id but what it wants is a relink, not a link beside the entry it
/// left stale; otherwise the reason core ranked it by, and for a repository
/// the agent files that gave it that reason, by name.
pub(crate) fn worth_linking(
    route: &str,
    moved: Option<&str>,
    reason: Reason,
    agent_files: &[String],
) -> String {
    if let Some(old) = moved {
        return moved_to(old, route);
    }
    match reason {
        Reason::Id => String::from("mind project with an id"),
        Reason::Mind => String::from("mind project without an id"),
        Reason::Agents => format!("git repository with {}", agent_files.join(", ")),
        Reason::Git => String::from("git repository"),
    }
}

/// A project found at `route`, moved from the entry `old`, as a scan says it
/// whether or not it suggests.
fn moved_to(old: &str, route: &str) -> String {
    format!("moved: {} -> {}", printable(old), printable(route))
}

/// What a scan could not read, and that it stopped early, each as a warning
/// says it, without the prefix: on stderr beside a table, inside the
/// envelope beside JSON, and in a strip of the minds screen, which says them
/// in these words rather than its own.
pub(crate) fn scan_warnings(scan: &Scan) -> Vec<String> {
    let mut warnings: Vec<String> = scan.failures.iter().map(ToString::to_string).collect();
    if scan.truncated {
        warnings.push(format!(
            "stopped after listing {MAX_LISTED} folders; name a folder further down to scan past it, or link one by its path"
        ));
    }
    warnings
}

/// Every repository a scan found, and whether it is linked yet, in the
/// order core gave them; each not linked says why it is worth linking when
/// `suggested`.
///
/// No column for a project's id: the path already tells the rows apart,
/// and the id, 32 digits on every project's line, is for a script to match
/// a project by, which reads it from the JSON.
fn scanned(scan: &Scan, warnings: Vec<String>, suggested: bool) -> Outcome {
    let mut stdout = String::new();
    if scan.found.is_empty() {
        let roots: Vec<String> = scan
            .roots
            .iter()
            .map(|root| printable(&root.display().to_string()))
            .collect();
        let _ = writeln!(
            stdout,
            "no mind projects or git repositories under {}",
            roots.join(", ")
        );
    } else {
        let rows: Vec<Vec<String>> = scan
            .found
            .iter()
            .map(|found| {
                let what = match (found.linked, found.mind, &found.moved) {
                    (true, _, _) => String::new(),
                    _ if suggested => worth_linking(
                        &found.route,
                        found.moved.as_deref(),
                        found.reason,
                        &found.agent_files,
                    ),
                    (false, true, Some(old)) => moved_to(old, &found.route),
                    (false, true, None) => String::from("mind project"),
                    (false, false, _) => String::from("git repository, not a mind project yet"),
                };
                vec![
                    printable(&found.route),
                    String::from(if found.linked { "linked" } else { "not linked" }),
                    what,
                ]
            })
            .collect();
        stdout.push_str(&table(&rows));

        // Only the advice that applies: a hint about a case the listing does
        // not contain is one more line to read past.
        let mut hints: Vec<String> = Vec::new();
        if scan.found.iter().any(|found| found.moved.is_some()) {
            hints.push(String::from(
                "follow one that moved with `flayer relink <old> <new>`",
            ));
        }
        // Before the link, which would leave the entry a project moved from
        // stale beside a second one for it.
        if let [entry] = &scan.untraced[..] {
            let entry = printable(entry);
            hints.push(format!(
                "follow one that moved from {entry} with `flayer relink {entry} <path>`"
            ));
        } else if !scan.untraced.is_empty() {
            let entries: Vec<String> = scan.untraced.iter().map(|entry| printable(entry)).collect();
            hints.push(format!(
                "follow one that moved from {} with `flayer relink <old> <path>`",
                entries.join(" or ")
            ));
        }
        if scan
            .found
            .iter()
            .any(|found| !found.linked && found.mind && found.moved.is_none())
        {
            hints.push(String::from("link one with `flayer link <path>`"));
        }
        if scan.found.iter().any(|found| !found.linked && !found.mind) {
            hints.push(String::from(
                "a repository needs `mind -C <path> init` before it can be linked",
            ));
        }
        if !hints.is_empty() {
            stdout.push('\n');
            for hint in hints {
                let _ = writeln!(stdout, "  {hint}");
            }
        }
    }

    let stderr: Vec<String> = warnings
        .iter()
        .map(|warning| format!("warning: {warning}"))
        .collect();
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}
