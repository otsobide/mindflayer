//! `agents` and `share`: changing a project's agents or share mode, and
//! carrying what is installed into it along, at both levels.

mod common;

use common::*;

/// A workspace keeping `deploy` of its own, and `collapse`, a repository
/// made a project naming `agents` (`claude`, say) and linked, with `deploy`
/// installed into it.
fn installed(agents: &str, shared: bool) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let mut init = vec!["init", "--agents", agents];
    if shared {
        init.push("--shared");
    }
    mind(&root, &init).unwrap();
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    (dir, root)
}

fn claudes(root: &Path, name: &str) -> PathBuf {
    root.join(".claude").join("skills").join(name)
}

fn agents(root: &Path, name: &str) -> PathBuf {
    root.join(".agents").join("skills").join(name)
}

fn settings(root: &Path) -> String {
    fs::read_to_string(root.join(MIND_DIR).join(SETTINGS_FILE)).unwrap()
}

/// What git would offer to commit in the repository at `root`, in name
/// order.
fn offered(root: &Path) -> Vec<String> {
    let mut listed = untracked(root);
    listed.sort();
    listed
}

/// What the workspace at `dir` records of `deploy` in `collapse`, by path.
fn recorded(dir: &Path) -> Vec<String> {
    let workspace = FlayerWorkspace::open(dir).unwrap();
    workspace
        .ledger()
        .unwrap()
        .copies("collapse", Kind::Skill)
        .unwrap()
}

#[test]
fn adding_codex_says_what_it_would_copy_then_copies_it_where_codex_reads() {
    let (dir, root) = installed("claude", false);
    let before = settings(&root);

    let dry = mind(&root, &["agents", "add", "codex"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would set agents to claude,codex in collapse, where they were claude\n\
         would copy deploy to .agents/skills/deploy, for codex, kept out of git\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert!(dry.stderr.is_empty(), "{:?}", dry.stderr);
    assert!(dry.ok);
    // Nothing written: the settings, the copies and the record as they were.
    assert_eq!(settings(&root), before);
    assert!(!agents(&root, "deploy").exists());
    assert_eq!(recorded(dir.path()), [".claude/skills/deploy"]);

    let done = mind(&root, &["agents", "add", "codex", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "set agents to claude,codex in collapse, where they were claude\n\
         copied deploy to .agents/skills/deploy, for codex, kept out of git\n"
    );
    assert!(done.ok, "{:?}", done.stderr);
    assert!(settings(&root).contains(r#"agents = ["claude", "codex"]"#));
    assert_eq!(
        fs::read(agents(&root, "deploy").join("SKILL.md")).unwrap(),
        fs::read(claudes(&root, "deploy").join("SKILL.md")).unwrap()
    );
    assert_eq!(
        recorded(dir.path()),
        [".agents/skills/deploy", ".claude/skills/deploy"]
    );
    // Local, so git is offered neither copy.
    assert!(untracked(&root).is_empty(), "{:?}", untracked(&root));

    let again = mind(&root, &["agents", "add", "codex", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to change: collapse names claude,codex already\n"
    );
}

#[test]
fn removing_claude_takes_out_its_copies_and_says_what_somebody_left_there() {
    let (dir, root) = installed("claude,codex,opencode", false);
    let notes = claudes(&root, "notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill_file("notes", "Mine")).unwrap();

    let done = mind(&root, &["agents", "remove", "claude", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "set agents to codex,opencode in collapse, where they were claude,codex,opencode\n\
         removed deploy from .claude/skills/deploy\n\
         left .claude/skills/notes where it is: Mindflayer did not install it\n"
    );
    assert!(done.ok, "{:?}", done.stderr);
    assert!(!claudes(&root, "deploy").exists());
    assert!(notes.join("SKILL.md").is_file());
    assert!(agents(&root, "deploy").join("SKILL.md").is_file());
    assert_eq!(recorded(dir.path()), [".agents/skills/deploy"]);
}

#[test]
fn naming_exactly_some_agents_moves_the_copies_from_the_others() {
    let (dir, root) = installed("claude", false);

    let done = mind(&root, &["agents", "codex", "opencode", "-y"]).unwrap();

    assert_eq!(
        done.stdout,
        "set agents to codex,opencode in collapse, where they were claude\n\
         copied deploy to .agents/skills/deploy, for codex and opencode, kept out of git\n\
         removed deploy from .claude/skills/deploy\n"
    );
    assert_eq!(recorded(dir.path()), [".agents/skills/deploy"]);
}

#[test]
fn flayer_agents_changes_a_project_the_workspace_manages_and_needs_one_named() {
    let (dir, root) = installed("claude", false);

    let done = flayer(
        dir.path(),
        &["agents", "-p", "collapse", "add", "codex", "-y"],
    )
    .unwrap();
    assert_eq!(
        done.stdout,
        "set agents to claude,codex in collapse, where they were claude\n\
         copied deploy to .agents/skills/deploy, for codex, kept out of git\n"
    );
    assert!(agents(&root, "deploy").is_dir());

    // `-p` on the other side of `remove` names it all the same.
    let removed = flayer(
        dir.path(),
        &["agents", "remove", "codex", "--project", "collapse", "-y"],
    )
    .unwrap();
    assert!(removed
        .stdout
        .contains("removed deploy from .agents/skills/deploy"));

    let failure = flayer(dir.path(), &["agents", "add", "codex"]).unwrap_err();
    assert!(
        matches!(*failure.error, CliError::AgentsWhere),
        "{}",
        failure.error
    );
}

#[test]
fn a_project_a_mind_toml_marks_is_refused_and_sent_to_mind_migrate() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("old");
    fs::create_dir(&root).unwrap();
    older_project(&root);
    flayer(dir.path(), &["link", "old"]).unwrap();

    for args in [
        &["agents", "add", "codex"][..],
        &["share", "shared", "--yes"][..],
    ] {
        let failure = mind(&root, args).unwrap_err();
        let said = failure.error.to_string();
        assert!(said.contains("since a mind.toml marks it"), "{said}");
        assert!(said.ends_with("`mind migrate` moves it to one"), "{said}");
    }
    let failure = flayer(dir.path(), &["share", "-p", "old", "local"]).unwrap_err();
    assert!(failure.error.to_string().contains("`mind migrate`"));
    assert!(!root.join(MIND_DIR).join(SETTINGS_FILE).exists());
}

#[test]
fn somebodys_skill_where_a_copy_would_go_is_left_and_warned_about() {
    let (dir, root) = installed("claude", false);
    let theirs = agents(&root, "deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill_file("deploy", "Theirs")).unwrap();

    let dry = mind(&root, &["agents", "add", "codex"]).unwrap();
    assert!(
        dry.stdout.contains(
            "would leave .agents/skills/deploy as it is: Mindflayer did not put it there, \
             so deploy would not be copied there\n"
        ),
        "{}",
        dry.stdout
    );

    let done = mind(&root, &["agents", "add", "codex", "--yes"]).unwrap();
    assert_eq!(
        done.stdout,
        "set agents to claude,codex in collapse, where they were claude\n"
    );
    assert_eq!(
        done.stderr,
        [
            "warning: collapse already has a deploy at .agents/skills/deploy that Mindflayer did \
          not put there, so it was left alone, and deploy was not copied there"
        ]
    );
    assert!(!done.ok);
    assert_eq!(
        fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        skill_file("deploy", "Theirs")
    );
    assert_eq!(recorded(dir.path()), [".claude/skills/deploy"]);
}

#[test]
fn removing_the_last_agent_is_refused() {
    let (_dir, root) = installed("claude", false);
    let before = settings(&root);

    let failure = mind(&root, &["agents", "remove", "claude", "--yes"]).unwrap_err();

    assert!(
        failure
            .error
            .to_string()
            .starts_with("collapse would name no agent"),
        "{}",
        failure.error
    );
    assert_eq!(settings(&root), before);
}

#[test]
fn sharing_a_local_project_leaves_its_mind_and_its_copies_for_the_team() {
    let (_dir, root) = installed("claude", false);
    assert!(untracked(&root).is_empty());

    let dry = mind(&root, &["share", "shared"]).unwrap();
    assert_eq!(
        dry.stdout,
        "would set share to shared in collapse, where it was local\n\
         would remove .mind/.gitignore, so .mind can be committed\n\
         would stop keeping /.claude/skills/deploy/ out of git\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert!(untracked(&root).is_empty());

    let done = mind(&root, &["share", "shared", "--yes"]).unwrap();
    assert_eq!(
        done.stdout,
        "set share to shared in collapse, where it was local\n\
         removed .mind/.gitignore, so .mind can be committed\n\
         stopped keeping /.claude/skills/deploy/ out of git\n"
    );
    assert!(done.ok, "{:?}", done.stderr);
    // The settings and the copy are offered to git, for the team to commit.
    assert_eq!(offered(&root), [".claude", ".mind/settings.toml"]);
    assert!(!root.join(MIND_DIR).join(".gitignore").exists());
}

#[test]
fn making_a_shared_project_local_keeps_its_mind_and_its_copies_out_of_git() {
    let (_dir, root) = installed("claude", true);
    assert_eq!(offered(&root), [".claude", ".mind/settings.toml"]);

    let done = mind(&root, &["share", "local", "--edit-tracked", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "set share to local in collapse, where it was shared\n\
         set edit_tracked to true in collapse, where it was false\n\
         wrote .mind/.gitignore, keeping .mind out of git\n\
         kept .claude/skills/deploy out of git\n"
    );
    assert!(untracked(&root).is_empty(), "{:?}", untracked(&root));
    assert!(settings(&root).contains("\nedit_tracked = true\n"));

    let again = mind(&root, &["share", "local", "--edit-tracked"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to change: collapse is local already, with edit_tracked\n"
    );
}

#[test]
fn flayer_share_changes_a_project_the_workspace_manages() {
    let (dir, root) = installed("claude", false);

    let done = flayer(dir.path(), &["share", "-p", "collapse", "shared", "-y"]).unwrap();

    assert!(done
        .stdout
        .starts_with("set share to shared in collapse, where it was local\n"));
    assert!(
        MindProject::open(&root).unwrap().settings().unwrap().share
            == mindflayer_core::Share::Shared
    );
}

#[test]
fn a_project_no_workspace_links_changes_its_settings_and_is_told_what_was_not_carried() {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    mind(dir.path(), &["init", "--agents", "claude"]).unwrap();

    let dry = mind(dir.path(), &["agents", "add", "codex"]).unwrap();
    assert_eq!(
        dry.stderr,
        [
            "hint: no workspace links this project, so nothing installed in it would be carried \
          along"
        ]
    );
    let done = mind(dir.path(), &["agents", "add", "codex", "--yes"]).unwrap();

    let name = MindProject::open(dir.path()).unwrap().name().to_owned();
    assert_eq!(
        done.stdout,
        format!("set agents to claude,codex in {name}, where they were claude\n")
    );
    assert_eq!(
        done.stderr,
        ["hint: no workspace links this project, so nothing installed in it was carried along"]
    );
    assert!(done.ok);
    // A refusal changes nothing, so there is nothing it did not carry.
    let refused = mind(
        dir.path(),
        &["agents", "remove", "claude", "codex", "--yes"],
    )
    .unwrap_err();
    assert!(refused.warnings.is_empty(), "{:?}", refused.warnings);
}
