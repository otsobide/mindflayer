//! `flayer update` and `mind update`: what Mindflayer installed brought up
//! to date with where it came from, the copies that have gone put back, an
//! edited copy left alone and said, and an orphan said without failing.

mod common;

use common::*;

/// A workspace with a skill of its own, `commit-style`, and a rule,
/// `git/commits`, linking `alpha`, `beta` and `gamma`, each given the
/// skill, and `alpha` the rule too.
fn workspace_with_three() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta", "gamma"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        flayer(dir.path(), &["link", name]).unwrap();
    }
    flayer(
        dir.path(),
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    flayer(
        dir.path(),
        &["add", "rule", "git/commits", "Commit one step at a time"],
    )
    .unwrap();
    for name in ["alpha", "beta", "gamma"] {
        flayer(dir.path(), &["install", "-p", name, "commit-style"]).unwrap();
    }
    flayer(dir.path(), &["install", "-p", "alpha", "git/commits"]).unwrap();
    dir
}

/// The workspace's own `commit-style`, its manifest.
fn offered(dir: &Path) -> PathBuf {
    own_directory_for(dir, Kind::Skill).join("commit-style/SKILL.md")
}

/// The workspace's own `git/commits`, its file.
fn offered_rule(dir: &Path) -> PathBuf {
    own_directory_for(dir, Kind::Rule).join("git/commits.md")
}

/// What somebody's editor does: add a line to the file.
fn type_into(file: &Path) {
    let mut text = fs::read_to_string(file).unwrap();
    text.push_str("Keep the subject under 50 characters.\n");
    fs::write(file, text).unwrap();
}

#[test]
fn an_update_rewrites_every_outdated_copy_and_leaves_an_edited_one_alone_saying_so() {
    let dir = workspace_with_three();
    type_into(&offered(dir.path()));
    let edited = copies(&dir.path().join("gamma"), "commit-style")[1].join("SKILL.md");
    type_into(&edited);
    let held = fs::read(&edited).unwrap();

    let before = flayer(dir.path(), &["status"]).unwrap();
    let updated = flayer(dir.path(), &["update", "--all", "--yes"]).unwrap();
    let after = flayer(dir.path(), &["status"]).unwrap();

    assert_eq!(
        before.stdout,
        "\
alpha  skill  commit-style  outdated  claude,codex,opencode  workspace
alpha  rule   git/commits   current   claude,codex,opencode  workspace
beta   skill  commit-style  outdated  claude,codex,opencode  workspace
gamma  skill  commit-style  edited    claude,codex,opencode  workspace
"
    );
    assert_eq!(
        updated.stdout,
        "\
updated commit-style in alpha, from workspace
updated commit-style in beta, from workspace
"
    );
    assert_eq!(
        updated.stderr,
        [
            "warning: commit-style in gamma was changed since it was installed, at \
          .claude/skills/commit-style, so it was left alone; --force writes over it"
        ]
    );
    assert!(!updated.ok);
    let offer = fs::read(offered(dir.path())).unwrap();
    for project in ["alpha", "beta"] {
        for copy in copies(&dir.path().join(project), "commit-style") {
            assert_eq!(
                fs::read(copy.join("SKILL.md")).unwrap(),
                offer,
                "{}",
                copy.display()
            );
        }
    }
    assert_eq!(fs::read(&edited).unwrap(), held);
    assert_eq!(
        after.stdout,
        "\
alpha  skill  commit-style  current  claude,codex,opencode  workspace
alpha  rule   git/commits   current  claude,codex,opencode  workspace
beta   skill  commit-style  current  claude,codex,opencode  workspace
gamma  skill  commit-style  edited   claude,codex,opencode  workspace
"
    );
}

#[test]
fn without_yes_an_update_says_what_it_would_do_and_writes_nothing() {
    let dir = workspace_with_three();
    type_into(&offered(dir.path()));
    let copy = copies(&dir.path().join("alpha"), "commit-style")[0].join("SKILL.md");
    let held = fs::read(&copy).unwrap();

    let planned = flayer(dir.path(), &["update", "-p", "alpha", "-p", "beta"]).unwrap();

    assert_eq!(
        planned.stdout,
        "\
would update commit-style in alpha, from workspace
would update commit-style in beta, from workspace
nothing changed: add --yes to go ahead
"
    );
    assert!(planned.stderr.is_empty(), "{:?}", planned.stderr);
    assert!(planned.ok);
    assert_eq!(fs::read(&copy).unwrap(), held);
}

#[test]
fn a_deleted_copy_is_put_back_byte_for_byte() {
    let dir = workspace_with_three();
    let alpha = dir.path().join("alpha");
    let agents = copies(&alpha, "commit-style")[0].clone();
    let written = fs::read(agents.join("SKILL.md")).unwrap();
    fs::remove_dir_all(&agents).unwrap();

    let updated = mind(&alpha, &["update", "--yes"]).unwrap();

    assert_eq!(
        updated.stdout,
        "put back commit-style in alpha at .agents/skills/commit-style, from workspace\n"
    );
    assert!(updated.stderr.is_empty(), "{:?}", updated.stderr);
    assert!(updated.ok);
    assert_eq!(fs::read(agents.join("SKILL.md")).unwrap(), written);
    assert_eq!(names_in(&agents), ["SKILL.md"]);
    // Kept out of git as it was when it was installed: the project is local.
    assert!(untracked(&alpha).is_empty(), "{:?}", untracked(&alpha));
}

#[test]
fn an_orphaned_installation_is_reported_and_is_no_failure() {
    let dir = workspace_with_three();
    fs::remove_dir_all(offered(dir.path()).parent().unwrap()).unwrap();

    let planned = flayer(dir.path(), &["update", "-p", "beta"]).unwrap();
    let updated = flayer(dir.path(), &["update", "-p", "beta", "--yes"]).unwrap();
    let status = flayer(dir.path(), &["status", "-p", "beta"]).unwrap();

    assert_eq!(planned.stdout, "nothing to update in beta\n");
    assert_eq!(
        planned.stderr,
        [
            "note: commit-style in beta is no longer offered by workspace, so it would be left as \
          it is; uninstalling it takes it out"
        ]
    );
    assert!(planned.ok);
    assert_eq!(updated.stdout, "nothing to update in beta\n");
    assert_eq!(
        updated.stderr,
        [
            "note: commit-style in beta is no longer offered by workspace, so it was left as it \
          is; uninstalling it takes it out"
        ]
    );
    assert!(updated.ok);
    assert_eq!(
        status.stdout,
        "commit-style  orphaned  claude,codex,opencode  workspace\n"
    );
}

#[test]
fn force_writes_over_an_edited_copy_after_saying_where_it_was() {
    let dir = workspace_with_three();
    let gamma = dir.path().join("gamma");
    let edited = copies(&gamma, "commit-style")[1].join("SKILL.md");
    type_into(&edited);

    let planned = mind(&gamma, &["update", "--force"]).unwrap();
    let forced = mind(&gamma, &["update", "--force", "--yes"]).unwrap();

    assert_eq!(
        planned.stdout,
        "would update commit-style in gamma, writing over the change at \
         .claude/skills/commit-style, from workspace\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert_eq!(
        forced.stdout,
        "updated commit-style in gamma, writing over the change at .claude/skills/commit-style, \
         from workspace\n"
    );
    assert!(forced.ok, "{:?}", forced.stderr);
    assert_eq!(
        fs::read(&edited).unwrap(),
        fs::read(offered(dir.path())).unwrap()
    );
}

#[test]
fn a_rule_is_updated_with_the_rest_of_its_projects_rules() {
    let dir = workspace_with_three();
    type_into(&offered_rule(dir.path()));

    let updated = flayer(dir.path(), &["update", "-p", "alpha", "--yes"]).unwrap();

    assert_eq!(
        updated.stdout,
        "updated rule git/commits in alpha, from workspace\n"
    );
    assert!(updated.ok, "{:?}", updated.stderr);
    let block = fs::read_to_string(dir.path().join("alpha/AGENTS.md")).unwrap();
    assert!(
        block.contains("Keep the subject under 50 characters."),
        "{block}"
    );
}

#[test]
fn names_pick_what_is_updated_and_one_that_names_nothing_is_said() {
    let dir = workspace_with_three();
    type_into(&offered(dir.path()));
    type_into(&offered_rule(dir.path()));

    let updated = flayer(
        dir.path(),
        &[
            "update",
            "--all",
            "rule:git/commits",
            "comit-style",
            "--yes",
        ],
    )
    .unwrap();

    assert_eq!(
        updated.stdout,
        "updated rule git/commits in alpha, from workspace\n"
    );
    assert_eq!(
        updated.stderr,
        ["warning: nothing named `comit-style` is installed in any project asked"]
    );
    assert!(!updated.ok);
}

#[test]
fn nothing_to_update_says_everything_asked_is_current() {
    let dir = workspace_with_three();

    let updated = flayer(dir.path(), &["update", "--all", "--yes"]).unwrap();

    assert_eq!(
        updated.stdout,
        "nothing to update: everything installed in alpha, beta and gamma is current\n"
    );
    assert!(updated.stderr.is_empty(), "{:?}", updated.stderr);
    assert!(updated.ok);
}

#[test]
fn mind_update_is_flayer_update_p_of_its_project() {
    let dir = workspace_with_three();
    type_into(&offered(dir.path()));

    let here = mind(&dir.path().join("beta"), &["update"]).unwrap();
    let named = flayer(dir.path(), &["update", "-p", "beta"]).unwrap();
    let through_mind = mind(dir.path(), &["flayer", "update", "-p", "beta"]).unwrap();

    assert_eq!(
        here.stdout,
        "would update commit-style in beta, from workspace\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert_eq!(named, here);
    assert_eq!(through_mind, here);
}

#[test]
fn flayer_update_needs_projects_and_takes_all_or_names_not_both() {
    let dir = workspace_with_three();

    let nowhere = flayer(dir.path(), &["update"]).unwrap_err();
    let both = FlayerCli::try_parse_from(line(
        "flayer",
        &["update", "-p", "alpha", "--all"],
        dir.path(),
    ));

    assert!(matches!(*nowhere.error, CliError::UpdateWhere));
    assert!(both.is_err());
}

#[test]
fn a_stale_entry_is_said_beside_what_the_rest_update() {
    let dir = workspace_with_three();
    type_into(&offered(dir.path()));
    let old = dir.path().join("old-api");
    fs::create_dir(&old).unwrap();
    new_project(&old);
    flayer(dir.path(), &["link", "old-api"]).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let planned = flayer(dir.path(), &["update", "--all"]).unwrap();

    assert!(
        planned
            .stdout
            .starts_with("would update commit-style in alpha, from workspace\n"),
        "{}",
        planned.stdout
    );
    assert_eq!(planned.stderr.len(), 1, "{:?}", planned.stderr);
    assert!(
        planned.stderr[0].starts_with("warning: ") && planned.stderr[0].contains("old-api"),
        "{:?}",
        planned.stderr
    );
    assert!(!planned.ok);
}

#[test]
fn every_entry_stale_is_not_said_as_nothing_linked() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let old = dir.path().join("old-api");
    fs::create_dir(&old).unwrap();
    new_project(&old);
    flayer(dir.path(), &["link", "old-api"]).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let planned = flayer(dir.path(), &["update", "--all"]).unwrap();

    assert_eq!(
        planned.stdout,
        "nothing to update: no project linked could be read\n"
    );
    assert_eq!(planned.stderr.len(), 1, "{:?}", planned.stderr);
    assert!(
        planned.stderr[0].starts_with("warning: ") && planned.stderr[0].contains("old-api"),
        "{:?}",
        planned.stderr
    );
    assert!(!planned.ok);
}

#[test]
fn an_update_of_a_default_workspace_nothing_has_made_makes_nothing() {
    let home = TempDir::new().unwrap();
    let here = TempDir::new().unwrap();

    let updated = flayer_at_home(here.path(), home.path(), &["update", "--all", "--yes"]).unwrap();

    assert_eq!(updated.stdout, "nothing to update: no project is linked\n");
    assert!(updated.ok, "{:?}", updated.stderr);
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn mind_update_where_no_workspace_manages_the_project_makes_none_in_home() {
    let home = TempDir::new().unwrap();
    let here = TempDir::new().unwrap();
    let project = here.path().join("collapse");
    fs::create_dir(&project).unwrap();
    new_project(&project);

    for args in [&["update"][..], &["update", "--yes"]] {
        let refused = mind_at_home(&project, home.path(), args).unwrap_err();

        assert!(
            matches!(*refused.error, CliError::NotManaged { .. }),
            "{args:?}: {}",
            refused.error
        );
    }
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn a_project_with_nothing_installed_has_nothing_to_update() {
    let dir = workspace_with_three();
    let delta = dir.path().join("delta");
    fs::create_dir(&delta).unwrap();
    new_project(&delta);
    flayer(dir.path(), &["link", "delta"]).unwrap();

    let updated = mind(&delta, &["update", "--yes"]).unwrap();

    assert_eq!(updated.stdout, "nothing to update in delta\n");
    assert!(updated.ok, "{:?}", updated.stderr);
}
