//! The two levels: `mind` for one project, `flayer` for the workspace and
//! what it owns itself.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// The two levels
// ---------------------------------------------------------------------------

#[test]
fn mind_init_creates_a_project_and_flayer_init_a_workspace() {
    let project = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let one = mind(project.path(), &["init"]).unwrap();
    let two = flayer(workspace.path(), &["init"]).unwrap();

    assert!(project.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    // `.mind` is the marker and the configuration; the skills sit beside the
    // code, where the agents that read them look.
    assert!(project.path().join("skills").is_dir());
    assert!(!project.path().join(FLAYER_DIR).exists());
    assert!(one.stdout.starts_with("initialized mind project"));

    assert!(workspace
        .path()
        .join(FLAYER_DIR)
        .join(FLAYER_CONFIG)
        .is_file());
    assert!(!workspace.path().join(MIND_DIR).exists());
    assert!(two.stdout.starts_with("initialized flayer workspace"));
}

#[test]
fn flayer_is_a_shortcut_for_mind_flayer() {
    let dir = workspace_with_two();

    for args in [
        vec!["list"],
        vec!["validate"],
        vec!["show", "alpha"],
        vec!["link", "alpha"],
        vec!["scan"],
    ] {
        let direct = flayer(dir.path(), &args).unwrap();
        let nested = {
            let mut nested = vec!["flayer"];
            nested.extend(args.iter().copied());
            mind(dir.path(), &nested).unwrap()
        };
        assert_eq!(
            direct, nested,
            "`flayer {args:?}` differs from `mind flayer`"
        );
    }
}

#[test]
fn mind_init_no_longer_takes_a_kind() {
    // The old surface was `mind init flayer`; that lives at `flayer init` now,
    // and leaving both spellings alive would be two ways to do one thing.
    assert!(Cli::try_parse_from(["mind", "init", "flayer"]).is_err());
    assert!(Cli::try_parse_from(["mind", "init", "mind"]).is_err());
}

#[test]
fn mind_list_sees_only_its_own_project_inside_a_workspace() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project_level = mind(&alpha, &["list"]).unwrap();
    let workspace_level = flayer(&alpha, &["list"]).unwrap();

    assert_eq!(project_level.stdout.lines().count(), 1);
    assert!(project_level.stdout.contains("alpha"));
    assert!(
        !project_level.stdout.contains("beta"),
        "the project level leaked its neighbour:\n{}",
        project_level.stdout
    );
    assert_eq!(workspace_level.stdout.lines().count(), 2);
    assert!(workspace_level.stdout.contains("beta"));
}

#[test]
fn only_the_workspace_level_names_the_project_each_skill_came_from() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project = mind(&alpha, &["validate"]).unwrap();
    let workspace = flayer(&alpha, &["validate"]).unwrap();

    assert!(project.stdout.contains("alpha: ok"), "{}", project.stdout);
    assert!(
        workspace.stdout.contains("alpha (alpha): ok"),
        "{}",
        workspace.stdout
    );
}

#[test]
fn each_level_names_the_command_that_creates_what_is_missing() {
    let dir = TempDir::new().unwrap();

    let project = mind(dir.path(), &["list"]).unwrap_err();
    let workspace = flayer(dir.path(), &["list"]).unwrap_err();

    assert!(matches!(*project.error, CliError::NotInProject(_)));
    assert!(project.error.to_string().contains("mind init"));
    assert!(matches!(*workspace.error, CliError::NotInWorkspace(_)));
    assert!(workspace.error.to_string().contains("flayer init"));
}

#[test]
fn ls_is_an_alias_at_both_levels() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    assert_eq!(
        mind(&alpha, &["list"]).unwrap(),
        mind(&alpha, &["ls"]).unwrap()
    );
    assert_eq!(
        flayer(dir.path(), &["list"]).unwrap(),
        flayer(dir.path(), &["ls"]).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Both levels: the workspace's own artifacts, and every command at either
// level
// ---------------------------------------------------------------------------

#[test]
fn flayer_add_writes_into_the_workspace_and_listings_name_it_so() {
    let dir = workspace_with_own();

    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    let listed = flayer(dir.path(), &["list"]).unwrap().stdout;
    assert!(
        listed.contains("workspace  commit-style  How every repo here writes commits"),
        "{listed}"
    );
    assert!(listed.contains("alpha      alpha"), "{listed}");
    let checked = flayer(dir.path(), &["validate"]).unwrap().stdout;
    assert!(
        checked.contains("commit-style (workspace): ok"),
        "{checked}"
    );
    // A project holds what is in its own folders; the workspace's reach it
    // by being installed.
    assert!(!mind(&dir.path().join("alpha"), &["list"])
        .unwrap()
        .stdout
        .contains("commit-style"));
}

#[test]
fn a_workspace_can_be_told_where_to_keep_its_own() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init", "--skills", "shared/skills"]).unwrap();

    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    assert!(dir.path().join("shared/skills/deploy/SKILL.md").is_file());
    let marker = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(marker.contains("skills = \"shared/skills\""), "{marker}");
}

#[test]
fn every_change_at_the_workspace_level_acts_on_its_own_unless_p_names_a_project() {
    let dir = workspace_with_own();

    flayer(dir.path(), &["rename", "commit-style", "commits"]).unwrap();
    flayer(
        dir.path(),
        &["add", "-p", "alpha", "rule", "style", "Plainly"],
    )
    .unwrap();
    flayer(dir.path(), &["remove", "alpha", "-p", "alpha", "--yes"]).unwrap();

    assert!(dir.path().join("skills/commits/SKILL.md").is_file());
    assert!(dir.path().join("alpha/rules/style.md").is_file());
    assert!(!dir.path().join("alpha/skills/alpha").exists());
    // `-p` at the workspace level is `mind` in that project: the same
    // function, so the same answer.
    assert_eq!(
        flayer(dir.path(), &["list", "-p", "alpha"]).unwrap(),
        mind(&dir.path().join("alpha"), &["list"]).unwrap()
    );
    assert_eq!(
        flayer(dir.path(), &["templates"]).unwrap().stdout,
        "full  skill  built in\n"
    );
}

#[test]
fn p_says_which_projects_there_are_when_it_names_none_of_them() {
    let dir = workspace_with_two();

    let failure = flayer(dir.path(), &["add", "-p", "gamma", "skill", "x", "y"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::UnknownProject { .. }));
    assert!(failure.error.to_string().contains("it manages alpha, beta"));
}

#[test]
fn mind_link_and_unlink_act_on_the_workspace_above() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();

    assert_eq!(
        mind(&alpha, &["link"]).unwrap().stdout,
        "linked alpha as alpha\n"
    );
    assert!(flayer(dir.path(), &["scan"])
        .unwrap()
        .stdout
        .contains("alpha  linked"));
    assert_eq!(
        mind(&alpha, &["unlink"]).unwrap().stdout,
        "unlinked alpha\n"
    );

    let alone = TempDir::new().unwrap();
    mind(alone.path(), &["init"]).unwrap();
    let failure = mind(alone.path(), &["link"]).unwrap_err();
    assert!(matches!(*failure.error, CliError::NoWorkspaceAbove(_)));
}

#[test]
fn a_skill_of_the_workspaces_installs_at_either_level() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");

    let from_the_project = mind(&alpha, &["install", "commit-style"]).unwrap();
    let from_the_workspace =
        flayer(dir.path(), &["install", "-p", "beta", "commit-style"]).unwrap();

    assert_eq!(
        from_the_project.stdout,
        "installed commit-style into alpha, from workspace\n"
    );
    assert_eq!(
        from_the_workspace.stdout,
        "installed commit-style into beta, from workspace\n"
    );
    assert!(alpha.join("skills/commit-style/SKILL.md").is_file());
    assert!(dir
        .path()
        .join("beta/skills/commit-style/SKILL.md")
        .is_file());

    assert_eq!(
        mind(&alpha, &["uninstall", "commit-style"]).unwrap().stdout,
        "removed commit-style from alpha\n"
    );
    assert!(!alpha.join("skills/commit-style").exists());
    let again = mind(&alpha, &["uninstall", "commit-style"]).unwrap_err();
    assert!(matches!(*again.error, CliError::NotInstalled { .. }));
}

#[test]
fn installing_without_a_screen_needs_a_project_and_a_skill() {
    let dir = workspace_with_own();

    let nowhere = flayer(dir.path(), &["install", "commit-style"]).unwrap_err();
    let nothing = flayer(dir.path(), &["install", "-p", "alpha"]).unwrap_err();

    assert!(matches!(*nowhere.error, CliError::InstallWhere));
    assert!(matches!(*nothing.error, CliError::InstallWhat));
}

#[test]
fn installing_needs_the_project_to_be_managed() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let loose = dir.path().join("loose");
    fs::create_dir(&loose).unwrap();
    mind(&loose, &["init"]).unwrap();

    let failure = mind(&loose, &["install", "anything"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotManaged { .. }));
    assert!(failure.error.to_string().contains("mind link"));
}

#[test]
fn one_name_from_two_places_is_installed_from_the_one_named() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    flayer(dir.path(), &["add", "skill", "deploy", "Our own deploy"]).unwrap();
    let project = dir.path().join("collapse");

    let twice = mind(&project, &["install", "deploy"]).unwrap_err();
    let ours = mind(&project, &["install", "deploy", "--from", "workspace"]).unwrap();

    assert!(
        twice.error.to_string().contains("--from"),
        "{}",
        twice.error
    );
    assert!(ours.stdout.contains("from workspace"), "{}", ours.stdout);
    let installed = fs::read_to_string(project.join("skills/deploy/SKILL.md")).unwrap();
    assert!(installed.contains("Our own deploy"));
}

#[test]
fn mind_on_its_own_is_the_tui_too() {
    let bare = Cli::try_parse_from(["mind"]).unwrap();
    let named = Cli::try_parse_from(["mind", "tui"]).unwrap();

    assert!(bare.command.is_none());
    assert!(matches!(named.command, Some(mindflayer_cli::Command::Tui)));
}
