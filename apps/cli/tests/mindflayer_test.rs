//! What a Mindflayer build made keeps working (D-26): a workspace in a
//! `.mindflayer`, found walking up and as the default in home, read and
//! written under its own names, never given a `.mindweaver` beside it.
//!
//! Built by core's mother of what such a build made
//! (`mindweaver_core::mother::MindflayerMother`), never by hand.

mod common;

use common::*;
use mindweaver_core::mother::MindflayerMother;

/// Everything under `root` that says Mindweaver's names: none of it should
/// be there for a workspace under Mindflayer's.
fn made_new(root: &Path) -> bool {
    root.join(WEAVER_DIR).exists()
}

#[test]
fn a_workspace_a_mindflayer_build_made_links_adds_and_lists_under_its_own_names() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());
    new_project(&dir.path().join("app"));

    weaver(dir.path(), &["link", "app"]).unwrap();
    let added = weaver(dir.path(), &["add", "skill", "review", "Read it first"]).unwrap();
    let listed = weaver(dir.path(), &["list"]).unwrap();

    assert_eq!(
        added.stdout,
        format!(
            "created skill review at {}\n",
            dir.path()
                .join(".mindflayer/library/skills/review/SKILL.md")
                .display()
        )
    );
    assert!(listed.stdout.contains("review"), "{}", listed.stdout);
    assert!(listed.ok, "{:?}", listed.stderr);
    let marker = std::fs::read_to_string(dir.path().join(".mindflayer/flayer.toml")).unwrap();
    assert!(marker.contains("projects = [\"app\"]"), "{marker}");
    assert!(!made_new(dir.path()));
}

#[test]
fn an_install_from_a_mindflayer_workspace_keeps_its_record_in_mindflayer_db() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());
    new_project(&dir.path().join("app"));
    weaver(dir.path(), &["link", "app"]).unwrap();
    weaver(dir.path(), &["add", "skill", "review", "Read it first"]).unwrap();

    weaver(dir.path(), &["install", "-p", "app", "review"]).unwrap();
    let status = weaver(dir.path(), &["status"]).unwrap();

    assert!(dir.path().join(".mindflayer/mindflayer.db").is_file());
    assert!(status.stdout.contains("current"), "{}", status.stdout);
    assert!(!made_new(dir.path()));
}

#[test]
fn the_default_workspace_a_mindflayer_build_made_in_home_is_the_one_a_write_uses() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    MindflayerMother::create(home.path());

    let added = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["add", "skill", "review", "Read it first"],
    )
    .unwrap();

    assert!(
        added.stdout.contains(".mindflayer/library/skills/review"),
        "{}",
        added.stdout
    );
    assert!(!made_new(home.path()));
}

#[test]
fn init_where_a_mindflayer_build_made_a_workspace_makes_nothing_new() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());

    let outcome = weaver(dir.path(), &["init"]).unwrap();

    assert!(
        outcome.stdout.contains("already initialized"),
        "{}",
        outcome.stdout
    );
    assert!(!made_new(dir.path()));
}
