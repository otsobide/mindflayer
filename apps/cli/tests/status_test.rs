//! `flayer status` and `mind status`: what each project holds, where each
//! thing in it came from, which agents read it, and whether it is all there.

mod common;

use common::*;

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
