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
commit-style  current   claude,codex,opencode  workspace
deploy        missing   codex,opencode         load:../team
notes         foreign   claude,opencode
review        authored  -
";

/// What a report says of `legacy`, which the `mind.toml` an older
/// Mindflayer wrote marks.
const LEGACY: &str =
    "hint: legacy is marked by a mind.toml, which `mind migrate` moves to a settings.toml";

#[test]
fn status_says_what_every_project_holds_from_where_and_for_which_agents() {
    let (_dir, projects) = workspace_with_everything();

    let outcome = flayer(&projects, &["status"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
collapse  commit-style  current   claude,codex,opencode  workspace
collapse  deploy        missing   codex,opencode         load:../team
collapse  notes         foreign   claude,opencode
collapse  review        authored  -
legacy    commit-style  current   -                      workspace
"
    );
    assert_eq!(outcome.stderr, [LEGACY]);
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

    assert_eq!(outcome.stdout, "commit-style  current  -  workspace\n");
}

#[test]
fn a_project_a_settings_json_marks_is_reported_with_a_hint_to_move_it() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let json = dir.path().join("json");
    fs::create_dir(&json).unwrap();
    json_project(&json);
    flayer(dir.path(), &["link", "json"]).unwrap();
    mind(&json, &["add", "skill", "review", "Review a change"]).unwrap();
    let hint =
        "hint: json is marked by a settings.json, which `mind migrate` moves to a settings.toml";

    let across = flayer(dir.path(), &["status"]).unwrap();
    let here = mind(&json, &["status"]).unwrap();
    let as_json = flayer(dir.path(), &["status", "-p", "json", "--json"]).unwrap();

    // Its agents say nothing of it: a settings.json names them as a
    // settings.toml does.
    assert_eq!(across.stdout, "review  authored  -\n");
    assert_eq!(across.stderr, [hint]);
    assert!(across.ok);
    assert_eq!(here, across);
    let (report, warnings): (Report, _) = enveloped(&as_json);
    assert_eq!(report.projects[0].marker.as_deref(), Some("settings.json"));
    assert_eq!(report.projects[0].agents, ["claude", "codex", "opencode"]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(as_json.stderr, [hint]);
    assert!(as_json.ok);
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
collapse  commit-style  current   claude,codex,opencode  workspace
collapse  deploy        missing   codex,opencode         load:../team
collapse  notes         foreign   claude,opencode
collapse  review        authored  -
legacy    commit-style  current   -                      workspace
old-api                 stale
"
    );
    assert_eq!(outcome.stderr.len(), 2, "{:?}", outcome.stderr);
    assert!(
        outcome.stderr[0].text.starts_with("warning: ")
            && outcome.stderr[0].text.contains("old-api"),
        "{:?}",
        outcome.stderr
    );
    assert_eq!(outcome.stderr[1], LEGACY);
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
fn a_workspace_above_that_cannot_be_read_is_a_warning_beside_the_report() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    new_project(&alpha);
    flayer(dir.path(), &["link", "alpha"]).unwrap();
    mind(&alpha, &["add", "skill", "review", "Review a change"]).unwrap();
    let marker = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    let mut text = fs::read_to_string(&marker).unwrap();
    text.push_str("garbage = [\n");
    fs::write(&marker, text).unwrap();

    let outcome = mind(&alpha, &["status"]).unwrap();
    let json = mind(&alpha, &["status", "--json"]).unwrap();

    assert_eq!(outcome.stdout, "review  authored  -\n");
    // The warning says why nothing reads as installed: no hint guesses.
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    let said = marker.display().to_string();
    assert!(
        outcome.stderr[0].text.starts_with("warning: ") && outcome.stderr[0].text.contains(&said),
        "{:?}",
        outcome.stderr
    );
    assert!(!outcome.ok);
    let (report, warnings): (Report, _) = enveloped(&json);
    assert_eq!(words(&report), ["alpha review authored -"]);
    assert_eq!(warnings, [&outcome.stderr[0].text["warning: ".len()..]]);
    assert!(json.stderr.is_empty(), "{:?}", json.stderr);
    assert!(!json.ok);
}

#[test]
fn a_workspace_named_that_is_none_still_refuses_mind_status() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    let nowhere = TempDir::new().unwrap();

    let named = nowhere.path().to_string_lossy();
    let failure = mind(dir.path(), &["status", "-W", &named]).unwrap_err();

    assert!(
        matches!(*failure.error, CliError::Chosen { .. }),
        "{}",
        failure.error
    );
}

#[test]
fn a_ledger_that_will_not_open_is_a_warning_beside_the_report() {
    let (_dir, projects) = workspace_with_everything();
    let ledger = projects.join(FLAYER_DIR).join("mindflayer.db");
    fs::write(&ledger, "not a database\n").unwrap();

    let text = flayer(&projects, &["status", "-p", "collapse"]).unwrap();
    let outcome = flayer(&projects, &["status", "--json"]).unwrap();

    // Nothing reads as installed, so nothing reads as Mindflayer's to change.
    assert_eq!(
        text.stdout,
        "\
commit-style  foreign   claude,codex,opencode
deploy        foreign   codex,opencode
notes         foreign   claude,opencode
review        authored  -
"
    );
    assert_eq!(text.stderr.len(), 1, "{:?}", text.stderr);
    let said = ledger.display().to_string();
    assert!(
        text.stderr[0].text.starts_with("warning: ") && text.stderr[0].text.contains(&said),
        "{:?}",
        text.stderr
    );
    assert!(!text.ok);
    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert_eq!(report.projects.len(), 2);
    assert_eq!(report.projects[0].entry.as_deref(), Some("collapse"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(&said), "{warnings:?}");
    assert!(!outcome.ok);
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
    assert!(unmade.stderr[0]
        .text
        .starts_with("hint: no workspace here or above"));
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
    marker: Option<String>,
    agents: Vec<String>,
    stale: Option<String>,
    unreached: Vec<Unreached>,
    missed: Vec<Missed>,
    artifacts: Vec<Held>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Unreached {
    kind: String,
    targets: Vec<String>,
    agents: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Missed {
    kind: String,
    name: String,
    agents: Vec<String>,
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
    // A hint, which is no warning, so on stderr beside the JSON as well.
    assert_eq!(outcome.stderr, [LEGACY]);
    assert!(outcome.ok);

    let collapse = &report.projects[0];
    assert_eq!(collapse.name.as_deref(), Some("collapse"));
    assert_eq!(collapse.entry.as_deref(), Some("collapse"));
    assert_eq!(
        collapse.path,
        projects.join("collapse").display().to_string()
    );
    assert_eq!(collapse.marker.as_deref(), Some("settings.toml"));
    assert_eq!(collapse.agents, ["claude", "codex", "opencode"]);
    assert_eq!(collapse.stale, None);
    assert_eq!(collapse.unreached, []);
    assert_eq!(collapse.missed, []);
    let kinds: Vec<&str> = collapse
        .artifacts
        .iter()
        .map(|held| held.kind.as_str())
        .collect();
    assert_eq!(kinds, ["skill"; 4]);
    let legacy = &report.projects[1];
    assert_eq!(legacy.marker.as_deref(), Some("mind.toml"));
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
    assert_eq!(stale.marker, None);
    let reason = stale.stale.as_deref().unwrap();
    assert_eq!(warnings, [reason]);
    assert_eq!(
        text.stderr,
        [format!("warning: {reason}"), LEGACY.to_owned()]
    );
    // The rest are there all the same, and the exit says something was not.
    assert_eq!(report.projects.len(), 3);
    assert_eq!(report.projects[0].artifacts.len(), 4);
    assert_eq!(outcome.stderr, [LEGACY]);
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
    assert_eq!(words(&report), ["legacy commit-style current - workspace"]);
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

/// A workspace linking `collapse`, a project naming every agent whose
/// settings, written by hand, install skills in `folders`, with `deploy`
/// installed there.
fn installing_in(folders: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    new_project(&root);
    let path = root.join(MIND_DIR).join(SETTINGS_FILE);
    let text = fs::read_to_string(&path).unwrap() + &format!("\n[targets]\nskills = [{folders}]\n");
    fs::write(&path, text).unwrap();
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    (dir, root)
}

#[test]
fn status_warns_of_a_kind_installed_where_none_of_the_projects_agents_reads() {
    let (dir, root) = installing_in("\".skills\"");
    let unread = "claude, codex and opencode read none of the folders collapse installs skills \
                  in (.skills), so none of them sees what is installed there until pointed at \
                  one";

    let here = mind(&root, &["status"]).unwrap();
    let across = flayer(dir.path(), &["status"]).unwrap();
    let json = flayer(dir.path(), &["status", "--json"]).unwrap();

    assert_eq!(here.stdout, "deploy  current  -  workspace\n");
    for outcome in [&here, &across] {
        assert_eq!(outcome.stderr, [format!("warning: {unread}")]);
        assert!(!outcome.ok);
    }
    let (report, warnings): (Report, _) = enveloped(&json);
    assert_eq!(warnings, [unread]);
    assert_eq!(
        report.projects[0].unreached,
        [Unreached {
            kind: String::from("skill"),
            targets: vec![String::from(".skills")],
            agents: vec![
                String::from("claude"),
                String::from("codex"),
                String::from("opencode")
            ],
        }]
    );
    assert!(!json.ok);
}

#[test]
fn status_says_a_target_no_install_may_use_and_the_one_agent_its_others_miss() {
    let (_dir, root) = installing_in("\".claude/skills\", \"../shared\"");
    let path = root.join(MIND_DIR).join(SETTINGS_FILE);

    let status = mind(&root, &["status"]).unwrap();

    assert_eq!(
        status.stdout,
        "deploy  current  claude,opencode  workspace\n"
    );
    assert_eq!(
        status.stderr,
        [
            format!(
                "warning: {}: ../shared is no folder to install skills in, because it is not \
                 inside the project, so nothing is installed there",
                path.display()
            ),
            String::from(
                "warning: codex reads none of the folders collapse installs skills in \
                 (.claude/skills), so it does not see what is installed there until pointed at \
                 one"
            ),
        ]
    );
    assert!(!status.ok);
}

#[test]
fn status_lists_the_rules_a_project_received_and_warns_of_one_some_agents_read_nowhere() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    new_project(&alpha);
    flayer(dir.path(), &["link", "alpha"]).unwrap();
    flayer(
        dir.path(),
        &[
            "add",
            "rule",
            "git/commits",
            "Commit one coherent step at a time",
        ],
    )
    .unwrap();
    let scoped = own_directory_for(dir.path(), Kind::Rule).join("style/comments.md");
    fs::create_dir_all(scoped.parent().unwrap()).unwrap();
    fs::write(
        &scoped,
        "---\npaths:\n  - \"**/*.rs\"\n---\n\n# Comments say why, not what\n",
    )
    .unwrap();
    flayer(dir.path(), &["install", "-p", "alpha", "git/commits"]).unwrap();
    flayer(dir.path(), &["install", "-p", "alpha", "style/comments"]).unwrap();
    let unseen = "codex and opencode do not read rule `style/comments` in alpha";

    let here = mind(&alpha, &["status"]).unwrap();
    let json = flayer(dir.path(), &["status", "-p", "alpha", "--json"]).unwrap();

    assert_eq!(
        here.stdout,
        "git/commits     current  claude,codex,opencode  workspace\n\
         style/comments  current  claude                 workspace\n"
    );
    assert_eq!(here.stderr, [format!("warning: {unseen}")]);
    assert!(!here.ok);
    let (report, warnings): (Report, _) = enveloped(&json);
    assert_eq!(warnings, [unseen]);
    assert_eq!(
        report.projects[0].missed,
        [Missed {
            kind: String::from("rule"),
            name: String::from("style/comments"),
            agents: vec![String::from("codex"), String::from("opencode")],
        }]
    );
    assert!(!json.ok);
}

#[cfg(unix)]
#[test]
fn status_says_an_installed_skill_edited_with_mind_edit_is_edited() {
    // A project that authors its skills where Claude reads them, so the copy
    // installed there is one `mind edit` reaches.
    let dir = TempDir::new().unwrap();
    let collapse = dir.path().join("collapse");
    fs::create_dir(&collapse).unwrap();
    gix::init(&collapse).unwrap();
    mind(&collapse, &["init", "--skills", ".claude/skills"]).unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(
        dir.path(),
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "commit-style"]).unwrap();
    let before = flayer(dir.path(), &["status", "-p", "collapse"]).unwrap();
    assert_eq!(
        before.stdout,
        "commit-style  current  claude,codex,opencode  workspace\n"
    );

    mind(
        &collapse,
        &["edit", "commit-style", "--editor", "printf 'Ours.\\n' >>"],
    )
    .unwrap();

    let after = flayer(dir.path(), &["status", "-p", "collapse"]).unwrap();
    assert_eq!(
        after.stdout,
        "commit-style  edited  claude,codex,opencode  workspace\n"
    );
    // Discovery says the same of the two copies, from where it reads them.
    assert_eq!(after.stderr.len(), 1, "{:?}", after.stderr);
    assert!(after.stderr[0]
        .text
        .ends_with("differ, so the agents reading them do not all load the same skill"));
    let json = flayer(dir.path(), &["status", "-p", "collapse", "--json"]).unwrap();
    assert!(
        json.stdout.contains("\"state\": \"edited\""),
        "{}",
        json.stdout
    );
}
