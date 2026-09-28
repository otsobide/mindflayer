//! `mind add`.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// mind add
// ---------------------------------------------------------------------------

#[test]
fn add_creates_a_skill_that_lists_and_validates() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let outcome = mind(
        dir.path(),
        &["add", "skill", "deploy", "Ship the service to staging"],
    )
    .unwrap();

    assert!(
        outcome.stdout.starts_with("created skill deploy at "),
        "{}",
        outcome.stdout
    );
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
    assert!(mind(dir.path(), &["list"])
        .unwrap()
        .stdout
        .contains("Ship the service to staging"));
    assert!(mind(dir.path(), &["validate"]).unwrap().ok);
}

#[test]
fn add_files_a_rule_by_its_route() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    mind(
        dir.path(),
        &[
            "add",
            "rule",
            "git/no-force-push",
            "Never force-push a shared branch",
        ],
    )
    .unwrap();

    assert!(dir.path().join("rules/git/no-force-push.md").is_file());
    let shown = mind(dir.path(), &["show", "rule:git/no-force-push"]).unwrap();
    assert!(shown.stdout.contains("Never force-push a shared branch"));
}

#[test]
fn add_refuses_a_name_validate_would_reject() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let failure = mind(dir.path(), &["add", "skill", "Deploy", "Ship it"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::Create(_)));
    assert!(failure.error.to_string().contains("`Deploy`"));
    assert_eq!(fs::read_dir(dir.path().join("skills")).unwrap().count(), 0);
}

#[test]
fn add_never_writes_over_what_is_there() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "The first"]).unwrap();

    let failure = mind(dir.path(), &["add", "skill", "deploy", "The second"]).unwrap_err();

    assert!(failure.error.to_string().contains("already exists"));
    let kept = fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap();
    assert!(kept.contains("The first"));
}

#[test]
fn add_needs_a_project_to_add_to() {
    let dir = TempDir::new().unwrap();

    let failure = mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInProject(_)));
}
