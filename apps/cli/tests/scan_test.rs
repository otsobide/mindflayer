//! `flayer scan`.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// flayer scan
// ---------------------------------------------------------------------------

#[test]
fn scan_says_what_is_under_the_workspace_and_what_is_linked() {
    let dir = workspace_with_two();
    flayer(dir.path(), &["unlink", "beta"]).unwrap();
    fs::create_dir_all(dir.path().join("gamma/.git")).unwrap();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    let lines: Vec<&str> = outcome.stdout.lines().collect();
    assert_eq!(lines[0], "alpha  linked");
    assert_eq!(lines[1], "beta   not linked  mind project");
    assert_eq!(
        lines[2],
        "gamma  not linked  git repository, not a mind project yet"
    );
    assert!(outcome.stdout.contains("flayer link <path>"));
    assert!(outcome.stdout.contains("mind -C <path> init"));
}

#[test]
fn scan_with_everything_linked_has_no_advice_to_give() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert_eq!(outcome.stdout, "alpha  linked\nbeta   linked\n");
}

#[test]
fn scan_of_an_empty_workspace_says_so() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert!(outcome
        .stdout
        .starts_with("no mind projects or git repositories under"));
}
