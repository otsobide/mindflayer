//! `flayer remove --everywhere` (#50): one of the workspace's own taken out
//! of every project it links, what was changed since it was installed and
//! somebody's own of that name left and said, and then the library copy.

mod common;

use std::collections::BTreeMap;

use common::*;

/// A workspace with a skill of its own, `commit-style`, linking `alpha`,
/// `beta`, `collapse` and `tanukeys`: installed into the first three,
/// changed by hand in `collapse`, and `tanukeys` holding somebody's own of
/// that name.
fn workspace_with_copies() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta", "collapse", "tanukeys"] {
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
    let installed = flayer(
        dir.path(),
        &[
            "install",
            "commit-style",
            "-p",
            "alpha",
            "-p",
            "beta",
            "-p",
            "collapse",
        ],
    )
    .unwrap();
    assert!(installed.ok, "{:?}", installed.stderr);
    fs::write(
        dir.path()
            .join("collapse/.claude/skills/commit-style/SKILL.md"),
        skill_file("commit-style", "Our way"),
    )
    .unwrap();
    let theirs = dir.path().join("tanukeys/.claude/skills/commit-style");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill_file("commit-style", "Mine")).unwrap();
    dir
}

/// Every file under `root`, by its path, with what it holds: what a dry run
/// must leave exactly as it found it.
fn every_file(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    files
}

#[test]
fn remove_everywhere_without_yes_says_each_copy_with_how_it_stands_then_the_library_copy() {
    let dir = workspace_with_copies();
    let library = own_directory_for(dir.path(), Kind::Skill).join("commit-style");

    let outcome = flayer(dir.path(), &["remove", "commit-style", "--everywhere"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "\
would remove commit-style from alpha, where it is current
would remove commit-style from beta, where it is current
would leave commit-style in collapse, where it is edited, at .claude/skills/commit-style
would leave commit-style in tanukeys, where it is foreign, at .claude/skills/commit-style
would remove skill commit-style from {} and the 1 file in it
nothing removed: add --yes to go ahead
",
            library.display()
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);
}

#[test]
fn the_dry_run_writes_nothing() {
    let dir = workspace_with_copies();
    let before = every_file(dir.path());

    flayer(dir.path(), &["remove", "commit-style", "--everywhere"]).unwrap();
    flayer(dir.path(), &["remove", "skill:commit-style"]).unwrap_err();

    assert_eq!(every_file(dir.path()), before);
}

#[test]
fn a_dry_run_makes_no_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    for args in [
        &["remove", "commit-style", "--everywhere"][..],
        &["remove", "commit-style"][..],
    ] {
        let failure = flayer_at_home(elsewhere.path(), home.path(), args).unwrap_err();
        assert!(
            failure
                .error
                .to_string()
                .contains("nothing named `commit-style`"),
            "{}",
            failure.error
        );
    }

    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn removing_it_everywhere_leaves_an_edited_copy_and_somebodys_and_says_so() {
    let dir = workspace_with_copies();
    let library = own_directory_for(dir.path(), Kind::Skill).join("commit-style");

    let outcome = flayer(
        dir.path(),
        &["remove", "commit-style", "--everywhere", "--yes"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "\
removed commit-style from alpha
removed commit-style from beta
left commit-style in collapse, where it is edited, at .claude/skills/commit-style
left commit-style in tanukeys, where it is foreign, at .claude/skills/commit-style
removed skill commit-style from {}
",
            library.display()
        )
    );
    assert_eq!(
        outcome.stderr,
        [
            "note: nothing offers commit-style in collapse any more; `flayer uninstall -p \
             collapse commit-style --force` takes it out"
        ]
    );
    assert!(outcome.ok);
    assert!(!library.exists());
    for name in ["alpha", "beta"] {
        for copy in copies(&dir.path().join(name), "commit-style") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
    for copy in copies(&dir.path().join("collapse"), "commit-style") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    let theirs = dir
        .path()
        .join("tanukeys/.claude/skills/commit-style/SKILL.md");
    assert!(fs::read_to_string(theirs).unwrap().contains("Mine"));

    // The edited copy is still Mindflayer's install, and the note's command
    // takes it out.
    let status = flayer(dir.path(), &["status", "-p", "collapse"]).unwrap();
    assert!(
        status.stdout.contains("commit-style  edited"),
        "{}",
        status.stdout
    );
    let forced = flayer(
        dir.path(),
        &["uninstall", "-p", "collapse", "commit-style", "--force"],
    )
    .unwrap();
    assert_eq!(forced.stdout, "removed commit-style from collapse\n");
}

#[test]
fn a_rule_of_the_workspaces_own_goes_from_every_project_that_received_it() {
    let dir = workspace_with_copies();
    flayer(dir.path(), &["add", "rule", "git/no-force-push", "Never"]).unwrap();
    flayer(
        dir.path(),
        &["install", "git/no-force-push", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    let would = flayer(dir.path(), &["remove", "git/no-force-push", "--everywhere"]).unwrap();
    assert!(
        would.stdout.starts_with(
            "\
would remove rule git/no-force-push from alpha, where it is current
would remove rule git/no-force-push from beta, where it is current
would remove rule git/no-force-push from "
        ),
        "{}",
        would.stdout
    );

    let outcome = flayer(
        dir.path(),
        &["remove", "rule:git/no-force-push", "--everywhere", "--yes"],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.starts_with(
            "\
removed rule git/no-force-push from alpha
removed rule git/no-force-push from beta
removed rule git/no-force-push from "
        ),
        "{}",
        outcome.stdout
    );
    for name in ["alpha", "beta"] {
        let block = fs::read_to_string(dir.path().join(name).join("AGENTS.md")).unwrap_or_default();
        assert!(!block.contains("git/no-force-push"), "{block}");
    }
}

#[test]
fn everywhere_is_for_the_workspaces_own_and_not_a_projects() {
    let dir = workspace_with_copies();

    let refused = FlayerCli::try_parse_from(line(
        "flayer",
        &["remove", "commit-style", "-p", "alpha", "--everywhere"],
        dir.path(),
    ))
    .unwrap_err();

    assert!(
        refused.to_string().contains("cannot be used with"),
        "{refused}"
    );
}
