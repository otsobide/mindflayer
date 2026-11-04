//! `flayer migrate --library`: a workspace made before its library, moved
//! into it when asked, and said first when not.

mod common;

use common::*;

/// `root` joined with each part in turn, spelled as core spells the paths it
/// reports, a separator of the platform's between each.
fn at(root: &Path, parts: &[&str]) -> String {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
        .display()
        .to_string()
}

/// A workspace keeping its own beside its projects, as `flayer init` made
/// every one before the library, with a skill and a rule of its own.
fn beside(root: &Path) {
    weaver(root, &["init", "--skills", "skills", "--rules", "rules"]).unwrap();
    weaver(
        root,
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    weaver(
        root,
        &["add", "rule", "git/no-force-push", "Never force-push"],
    )
    .unwrap();
}

/// What the dry run and the migration say of the two in [`beside`].
fn report(root: &Path, moving: &str, pointing: &str) -> String {
    let marker = at(root, &[WEAVER_DIR, WEAVER_CONFIG]);
    format!(
        "{moving} {} to {}\n\
         {moving} {} to {}\n\
         {pointing} skills and rules in {marker} at the library, ",
        at(root, &["skills", "commit-style"]),
        at(root, &[WEAVER_DIR, "library", "skills", "commit-style"]),
        at(root, &["rules", "git", "no-force-push.md"]),
        at(
            root,
            &[WEAVER_DIR, "library", "rules", "git", "no-force-push.md"]
        ),
    )
}

#[test]
fn the_dry_run_says_what_would_move_and_moves_nothing() {
    let dir = TempDir::new().unwrap();
    beside(dir.path());
    let marker = fs::read_to_string(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap();

    let outcome = weaver(dir.path(), &["migrate", "--library"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "{}keeping it as it is in {}\nnothing moved: add --yes to go ahead\n",
            report(dir.path(), "would move", "would point"),
            at(dir.path(), &[WEAVER_DIR, "flayer.toml.v2"]),
        )
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap(),
        marker
    );
    assert!(!dir.path().join(WEAVER_DIR).join("flayer.toml.v2").exists());
}

#[test]
fn migrating_moves_them_and_a_second_run_has_nothing_to_move() {
    let dir = TempDir::new().unwrap();
    beside(dir.path());

    let migrated = weaver(dir.path(), &["migrate", "--library", "--yes"]).unwrap();
    let again = weaver(dir.path(), &["migrate", "--library", "--yes"]).unwrap();
    let dry = weaver(dir.path(), &["migrate", "--library"]).unwrap();
    let listed = weaver(dir.path(), &["list"]).unwrap();

    assert_eq!(
        migrated.stdout,
        format!(
            "{}kept as it was in {}\n",
            report(dir.path(), "moved", "pointed"),
            at(dir.path(), &[WEAVER_DIR, "flayer.toml.v2"]),
        )
    );
    let nothing = "nothing to move: the workspace keeps its own in its library already\n";
    assert_eq!(again.stdout, nothing);
    assert_eq!(dry.stdout, nothing);
    assert!(again.ok && dry.ok);
    assert_eq!(
        listed.stdout,
        "skill  commit-style       How commits are written\n\
         rule   git/no-force-push  Never force-push\n"
    );
    assert!(!dir.path().join("skills").exists());
    assert!(!dir.path().join("rules").exists());
}

#[test]
fn a_home_workspace_migrates_and_what_it_installed_stays_current() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let home_path = home.path();
    // Made as `flayer init` made the one in a home before the library, and
    // used from outside it, as the default.
    weaver(
        home_path,
        &["init", "--skills", "skills", "--rules", "rules"],
    )
    .unwrap();
    let alpha = home_path.join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let run = |args: &[&str]| weaver_at_home(elsewhere.path(), home_path, args);
    run(&["link", alpha.to_str().unwrap()]).unwrap();
    run(&["add", "skill", "x", "Ours"]).unwrap();
    run(&["install", "-p", "alpha", "x"]).unwrap();
    assert!(home_path.join("skills/x/SKILL.md").is_file());

    let migrated = run(&["migrate", "--library", "--yes"]).unwrap();
    let status = run(&["status", "-p", "alpha"]).unwrap();
    let installed = run(&["install", "-p", "alpha", "x"]).unwrap();

    assert!(
        migrated.stdout.starts_with(&format!(
            "moved {} to {}\n",
            at(home_path, &["skills", "x"]),
            at(home_path, &[WEAVER_DIR, "library", "skills", "x"]),
        )),
        "{}",
        migrated.stdout
    );
    assert!(home_path
        .join(".mindflayer/library/skills/x/SKILL.md")
        .is_file());
    assert!(!home_path.join("skills").exists());
    // Recorded, from the workspace, in every folder alpha's agents read; and
    // still the copy of what the workspace offers, which only `install` says
    // until status tells an outdated copy from a current one (#47).
    assert_eq!(
        status.stdout,
        "x  current  claude,codex,opencode  workspace\n"
    );
    assert!(status.ok, "{status:?}");
    assert_eq!(installed.stdout, "x in alpha was already current\n");
    assert!(installed.ok, "{installed:?}");
}

#[test]
fn a_destination_already_there_refuses_the_dry_run_and_the_migration() {
    let dir = TempDir::new().unwrap();
    beside(dir.path());
    let taken = dir.path().join(".mindflayer/library/skills/commit-style");
    fs::create_dir_all(&taken).unwrap();

    let dry = weaver(dir.path(), &["migrate", "--library"]).unwrap_err();
    let yes = weaver(dir.path(), &["migrate", "--library", "--yes"]).unwrap_err();

    let said = format!(
        "nothing moved: {}: already there, so {} cannot be moved onto it",
        at(
            dir.path(),
            &[WEAVER_DIR, "library", "skills", "commit-style"]
        ),
        at(dir.path(), &["skills", "commit-style"]),
    );
    assert_eq!(dry.error.to_string(), said);
    assert_eq!(yes.error.to_string(), said);
    assert!(dry.warnings.is_empty() && yes.warnings.is_empty());
    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    assert!(dir.path().join("rules/git/no-force-push.md").is_file());
}

#[test]
fn a_project_at_the_root_keeps_what_it_shares_and_is_said_to() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    weaver(
        dir.path(),
        &["init", "--skills", "skills", "--rules", "rules"],
    )
    .unwrap();
    weaver(dir.path(), &["link", "."]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "The project's"]).unwrap();

    let outcome = weaver(dir.path(), &["migrate", "--library", "--yes"]).unwrap();

    assert!(
        outcome.stdout.starts_with(&format!(
            "left {} where it is: the project at {} holds it\n",
            at(dir.path(), &["skills", "deploy"]),
            dir.path().display(),
        )),
        "{}",
        outcome.stdout
    );
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
}

#[test]
fn what_an_unlinked_project_at_the_root_keeps_is_said_to_be_the_workspaces_no_more() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // A home made a project and never linked: the workspace counts what is
    // in `skills` as its own, and installs it from there.
    mind(root, &["init"]).unwrap();
    weaver(root, &["init", "--skills", "skills", "--rules", "rules"]).unwrap();
    let alpha = root.join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    weaver(root, &["link", "alpha"]).unwrap();
    weaver(root, &["add", "skill", "x", "Ours"]).unwrap();
    weaver(root, &["install", "-p", "alpha", "x"]).unwrap();

    let dry = weaver(root, &["migrate", "--library"]).unwrap();
    let migrated = weaver(root, &["migrate", "--library", "--yes"]).unwrap();
    let installed = weaver(root, &["install", "-p", "alpha", "x"]).unwrap_err();
    let uninstalled = weaver(root, &["uninstall", "-p", "alpha", "x"]).unwrap();

    let skill = at(root, &["skills", "x"]);
    let project = root.display();
    assert!(
        dry.stdout.starts_with(&format!(
            "would leave {skill} where it is: the project at {project} holds it, \
             and the workspace would no longer count it as its own\n"
        )),
        "{}",
        dry.stdout
    );
    assert!(
        migrated.stdout.starts_with(&format!(
            "left {skill} where it is: the project at {project} holds it, \
             and the workspace no longer counts it as its own\n"
        )),
        "{}",
        migrated.stdout
    );
    assert!(root.join("skills/x/SKILL.md").is_file());
    // Still recorded, so it can be taken out, but offered by nothing.
    assert!(
        installed
            .error
            .to_string()
            .starts_with("nothing called `x` can be installed"),
        "{}",
        installed.error
    );
    assert!(uninstalled.ok, "{uninstalled:?}");
}

#[test]
fn migrate_is_told_what_to_migrate() {
    let dir = TempDir::new().unwrap();
    beside(dir.path());

    let failure = weaver(dir.path(), &["migrate"]).unwrap_err();

    let said = failure.error.to_string();
    assert!(
        said.starts_with("say what to migrate: --library "),
        "{said}"
    );
    assert!(dir.path().join("skills").is_dir(), "nothing moved");
}

#[test]
fn mind_weaver_migrate_is_the_same_command() {
    let dir = TempDir::new().unwrap();
    beside(dir.path());

    let long = mind(dir.path(), &["flayer", "migrate", "--library"]).unwrap();
    let short = weaver(dir.path(), &["migrate", "--library"]).unwrap();

    assert_eq!(long, short);
}

#[test]
fn the_default_workspace_unmade_has_nothing_to_move_and_is_not_made() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let outcome = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["migrate", "--library", "--yes"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "nothing to move: the workspace keeps its own in its library already\n"
    );
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].text.starts_with("hint: "), "{outcome:?}");
    assert!(!home.path().join(WEAVER_DIR).exists());
}
