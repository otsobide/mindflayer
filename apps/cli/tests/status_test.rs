//! `flayer status` and `mind status`: what each project holds, where each
//! thing in it came from, which agents read it, and whether it is all there.

mod common;

use common::*;
use serde::Deserialize;

/// Under one directory: `Projects`, a workspace with a skill of its own,
/// `commit-style`, linking `collapse`, marked by its settings, and `legacy`,
/// marked by the `mind.toml` an older Mindflayer wrote; and `team`, a
/// workspace beside it with `deploy`, which `Projects` loads.
///
/// `commit-style` is installed into both projects and `deploy` into
/// `collapse`, whose Claude copy of it is then deleted. `collapse` authors
/// `review`, and holds somebody's `notes` in Claude's folder.
fn workspace_with_everything() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    let team = dir.path().join("team");
    for folder in [
        projects.join("collapse"),
        projects.join("legacy"),
        team.clone(),
    ] {
        fs::create_dir_all(folder).unwrap();
    }
    flayer(&team, &["init"]).unwrap();
    flayer(&team, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    new_project(&projects.join("collapse"));
    older_project(&projects.join("legacy"));
    flayer(&projects, &["init"]).unwrap();
    flayer(&projects, &["link", "collapse", "legacy"]).unwrap();
    flayer(
        &projects,
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    flayer(&projects, &["load", "../team"]).unwrap();
    for (project, skill) in [
        ("collapse", "commit-style"),
        ("legacy", "commit-style"),
        ("collapse", "deploy"),
    ] {
        flayer(&projects, &["install", "-p", project, skill]).unwrap();
    }
    let collapse = projects.join("collapse");
    mind(&collapse, &["add", "skill", "review", "Review a change"]).unwrap();
    let notes = collapse.join(".claude/skills/notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill_file("notes", "My own notes")).unwrap();
    fs::remove_dir_all(collapse.join(".claude/skills/deploy")).unwrap();
    (dir, projects)
}

/// What `collapse` holds, as a report about it alone lays it out.
const COLLAPSE: &str = "\
commit-style  installed  claude,codex,opencode  workspace
deploy        partial    codex,opencode         load:../team
notes         foreign    claude,opencode
review        authored   -
";

#[test]
fn status_says_what_every_project_holds_from_where_and_for_which_agents() {
    let (_dir, projects) = workspace_with_everything();

    let outcome = flayer(&projects, &["status"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
collapse  commit-style  installed  claude,codex,opencode  workspace
collapse  deploy        partial    codex,opencode         load:../team
collapse  notes         foreign    claude,opencode
collapse  review        authored   -
legacy    commit-style  installed  -                      workspace
"
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);
}

#[test]
fn mind_status_says_of_its_project_what_flayer_status_p_does() {
    let (_dir, projects) = workspace_with_everything();

    let here = mind(&projects.join("collapse"), &["status"]).unwrap();
    let named = flayer(&projects, &["status", "-p", "collapse"]).unwrap();
    let through_mind = mind(&projects, &["flayer", "status", "-p", "collapse"]).unwrap();

    assert_eq!(here.stdout, COLLAPSE);
    assert!(here.stderr.is_empty(), "{:?}", here.stderr);
    assert_eq!(named, here);
    assert_eq!(through_mind, here);
}

#[test]
fn a_project_an_older_mindflayer_made_reaches_no_agent_it_names() {
    let (_dir, projects) = workspace_with_everything();

    let outcome = flayer(&projects, &["status", "-p", "legacy"]).unwrap();

    assert_eq!(outcome.stdout, "commit-style  installed  -  workspace\n");
}

#[test]
fn a_stale_entry_is_listed_as_stale_and_the_rest_still_print() {
    let (_dir, projects) = workspace_with_everything();
    let old = projects.join("old-api");
    fs::create_dir(&old).unwrap();
    new_project(&old);
    flayer(&projects, &["link", "old-api"]).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let outcome = flayer(&projects, &["status"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
collapse  commit-style  installed  claude,codex,opencode  workspace
collapse  deploy        partial    codex,opencode         load:../team
collapse  notes         foreign    claude,opencode
collapse  review        authored   -
legacy    commit-style  installed  -                      workspace
old-api                 stale
"
    );
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    assert!(
        outcome.stderr[0].starts_with("warning: ") && outcome.stderr[0].contains("old-api"),
        "{:?}",
        outcome.stderr
    );
    assert!(!outcome.ok);
}

#[test]
fn a_stale_entry_alone_is_still_named() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let old = dir.path().join("old-api");
    fs::create_dir(&old).unwrap();
    new_project(&old);
    flayer(dir.path(), &["link", "old-api"]).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let outcome = flayer(dir.path(), &["status"]).unwrap();

    assert_eq!(outcome.stdout, "old-api  stale\n");
    assert!(!outcome.ok);
}

#[test]
fn a_kind_column_appears_once_a_rule_is_in_play() {
    let dir = workspace_with_two();
    mind(
        &dir.path().join("alpha"),
        &["add", "rule", "git/no-force-push", "Never force-push"],
    )
    .unwrap();

    let outcome = flayer(dir.path(), &["status"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
alpha  skill  alpha              authored  -
alpha  rule   git/no-force-push  authored  -
beta   skill  beta               authored  -
"
    );
}

#[test]
fn a_project_no_workspace_links_reads_as_nothing_installed_and_says_why() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    mind(dir.path(), &["add", "skill", "review", "Review a change"]).unwrap();

    let outcome = mind(dir.path(), &["status"]).unwrap();

    assert_eq!(outcome.stdout, "review  authored  -\n");
    assert_eq!(
        outcome.stderr,
        ["hint: no workspace links this project, so nothing in it reads as installed"]
    );
    assert!(outcome.ok);
}

#[test]
fn a_project_the_workspace_above_does_not_link_says_so() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let loose = dir.path().join("loose");
    fs::create_dir(&loose).unwrap();
    new_project(&loose);
    mind(&loose, &["add", "skill", "review", "Review a change"]).unwrap();

    let outcome = mind(&loose, &["status"]).unwrap();

    assert_eq!(outcome.stdout, "review  authored  -\n");
    assert_eq!(
        outcome.stderr,
        [format!(
            "hint: the workspace at {} does not link this project, so nothing in it reads as \
             installed; `mind link` links it",
            dir.path().display()
        )]
    );
}

#[test]
fn status_makes_neither_a_ledger_nor_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let unmade = flayer_at_home(elsewhere.path(), home.path(), &["status"]).unwrap();
    let dir = workspace_with_two();
    let linked = flayer(dir.path(), &["status"]).unwrap();

    assert_eq!(
        unmade.stdout,
        format!(
            "{} manages no projects yet\n  link one with `flayer link <path>`\n",
            home.path().file_name().unwrap().to_string_lossy()
        )
    );
    assert!(unmade.stderr[0].starts_with("hint: no workspace here or above"));
    assert!(!home.path().join(FLAYER_DIR).exists());
    assert!(linked.ok, "{linked:?}");
    assert!(!dir.path().join(FLAYER_DIR).join("mindflayer.db").exists());
}

#[test]
fn projects_that_hold_nothing_say_where_status_looked() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    new_project(&alpha);
    flayer(dir.path(), &["link", "alpha"]).unwrap();

    let outcome = flayer(dir.path(), &["status"]).unwrap();

    let at = |folder: &str| alpha.join(folder).display().to_string();
    assert_eq!(
        outcome.stdout,
        format!(
            "nothing found\n  alpha: {}\n  alpha: {}\n  alpha: {}\n  alpha: {}\n",
            at(".agents/skills"),
            at(".claude/skills"),
            at("skills"),
            at("rules"),
        )
    );
}

/// What `status --json` holds, field by field: nothing else is allowed, so
/// a name changed in core fails here first. Every name is a contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    name: Option<String>,
    entry: Option<String>,
    path: String,
    agents: Vec<String>,
    stale: Option<String>,
    artifacts: Vec<Held>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    kind: String,
    name: String,
    state: String,
    agents: Vec<String>,
    origin: Option<String>,
}

/// Each artifact in a report, a line of words as the table spells them, so
/// the JSON and the text can be held to the same entries.
fn words(report: &Report) -> Vec<String> {
    report
        .projects
        .iter()
        .flat_map(|project| {
            project.artifacts.iter().map(move |held| {
                let agents = if held.agents.is_empty() {
                    String::from("-")
                } else {
                    held.agents.join(",")
                };
                [
                    project.name.as_deref().unwrap_or("?"),
                    &held.name,
                    &held.state,
                    &agents,
                    held.origin.as_deref().unwrap_or(""),
                ]
                .join(" ")
                .trim_end()
                .to_owned()
            })
        })
        .collect()
}

#[test]
fn status_as_json_holds_the_same_entries_as_the_table() {
    let (_dir, projects) = workspace_with_everything();

    let text = flayer(&projects, &["status"]).unwrap();
    let outcome = flayer(&projects, &["status", "--json"]).unwrap();

    let (report, warnings): (Report, _) = enveloped(&outcome);
    let table: Vec<String> = text
        .stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(words(&report), table);
    assert!(warnings.is_empty());
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);

    let collapse = &report.projects[0];
    assert_eq!(collapse.name.as_deref(), Some("collapse"));
    assert_eq!(collapse.entry.as_deref(), Some("collapse"));
    assert_eq!(
        collapse.path,
        projects.join("collapse").display().to_string()
    );
    assert_eq!(collapse.agents, ["claude", "codex", "opencode"]);
    assert_eq!(collapse.stale, None);
    let kinds: Vec<&str> = collapse
        .artifacts
        .iter()
        .map(|held| held.kind.as_str())
        .collect();
    assert_eq!(kinds, ["skill"; 4]);
    let legacy = &report.projects[1];
    assert!(legacy.agents.is_empty());
}

#[test]
fn a_stale_entry_in_json_says_why_in_its_data_and_its_warnings() {
    let (_dir, projects) = workspace_with_everything();
    let old = projects.join("old-api");
    fs::create_dir(&old).unwrap();
    new_project(&old);
    flayer(&projects, &["link", "old-api"]).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let text = flayer(&projects, &["status"]).unwrap();
    let outcome = flayer(&projects, &["status", "--json"]).unwrap();

    let (report, warnings): (Report, _) = enveloped(&outcome);
    let stale = &report.projects[2];
    assert_eq!(stale.name, None);
    assert_eq!(stale.entry.as_deref(), Some("old-api"));
    assert!(stale.artifacts.is_empty());
    let reason = stale.stale.as_deref().unwrap();
    assert_eq!(warnings, [reason]);
    assert_eq!(text.stderr, [format!("warning: {reason}")]);
    // The rest are there all the same, and the exit says something was not.
    assert_eq!(report.projects.len(), 3);
    assert_eq!(report.projects[0].artifacts.len(), 4);
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(!outcome.ok);
}

#[test]
fn mind_status_as_json_reports_its_one_project() {
    let (_dir, projects) = workspace_with_everything();

    let here = mind(&projects.join("legacy"), &["status", "--json"]).unwrap();
    let named = flayer(&projects, &["status", "-p", "legacy", "--json"]).unwrap();

    assert_eq!(here, named);
    let (report, _): (Report, _) = enveloped(&here);
    assert_eq!(report.projects.len(), 1);
    assert_eq!(
        words(&report),
        ["legacy commit-style installed - workspace"]
    );
}

#[test]
fn a_workspace_with_nothing_linked_is_an_empty_report_in_json() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["status", "--json"]).unwrap();

    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert!(report.projects.is_empty());
    assert!(warnings.is_empty());
    assert!(outcome.ok);
}
