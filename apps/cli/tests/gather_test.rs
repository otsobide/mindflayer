//! `flayer gather`: taking skills from a git repository onto the shelf.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// gather
//
// The repository is built here rather than fetched, for the reason the core
// suite builds one: a test that needs the network fails for reasons that have
// nothing to do with the code.
// ---------------------------------------------------------------------------

#[test]
fn gather_reports_the_source_and_what_it_took() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it")),
        (
            "skills/commit-style/SKILL.md",
            &skill_file("commit-style", "How we commit"),
        ),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stdout.contains("added"), "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains("commit-style  How we commit"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("2 skills: 2 added, 0 updated, 0 unchanged"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn gathering_again_says_nothing_moved() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let again = flayer(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        again
            .stdout
            .contains("1 skill: 0 added, 0 updated, 1 unchanged"),
        "{}",
        again.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_a_warning_and_a_non_zero_exit() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill_file("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter\n"),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn gather_list_names_where_each_skill_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("deploy"));
    assert!(outcome.stdout.contains(&url), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("Ship it"));
}

#[test]
fn an_empty_shelf_says_how_to_fill_it() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.starts_with("nothing gathered yet"));
    assert!(outcome.stdout.contains("flayer gather git"));
}

#[test]
fn gathering_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = flayer(dir.path(), &["gather", "list"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("flayer init"));
}

#[test]
fn gather_is_reachable_the_long_way_round_too() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let short = flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let long = mind(dir.path(), &["flayer", "gather", "list"]).unwrap();

    assert!(short.ok);
    assert!(long.stdout.contains("deploy"), "{}", long.stdout);
}

#[test]
fn gather_list_tells_two_branches_of_one_repository_apart() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    // The same URL at its default branch and at a named one: two sources, and
    // a listing that printed the URL alone would show one origin twice.
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    flayer(dir.path(), &["gather", "git", &url, "--ref", &branch]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert_eq!(outcome.stdout.lines().count(), 2, "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains(&format!("{url}#{branch}")),
        "{}",
        outcome.stdout
    );
}
