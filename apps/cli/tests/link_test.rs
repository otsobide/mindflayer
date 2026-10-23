//! `flayer link` and `unlink`, and the workspace in your home that they fall
//! back to.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// link and unlink
// ---------------------------------------------------------------------------

#[test]
fn link_registers_a_project_and_says_how_it_was_stored() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    mind(&root, &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["link", "alpha"]).unwrap();

    assert!(outcome.ok);
    assert_eq!(outcome.stdout, "linked alpha as alpha\n");
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("projects = [\"alpha\"]"), "{config}");
    assert!(config.contains("# Mindflayer workspace."), "comments kept");
}

#[test]
fn a_fresh_workspace_says_it_manages_nothing_yet() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("manages no projects yet"));
    assert!(outcome.stdout.contains("flayer link"));
}

#[test]
fn linking_the_same_project_twice_is_not_an_error() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["link", "alpha"]).unwrap();

    assert!(outcome.ok);
    assert_eq!(outcome.stdout, "alpha is already linked as alpha\n");
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert_eq!(config.matches("\"alpha\"").count(), 1, "{config}");
}

#[test]
fn link_refuses_a_directory_that_is_not_a_mind_project() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    fs::create_dir(dir.path().join("not-a-project")).unwrap();

    let error = flayer(dir.path(), &["link", "not-a-project"]).unwrap_err();

    assert!(
        error.error.to_string().contains(MIND_DIR),
        "{}",
        error.error
    );
}

#[test]
fn link_resolves_the_path_against_the_directory_it_runs_in() {
    let dir = workspace_with_two();
    let deep = dir.path().join("alpha/nested/deeper");
    fs::create_dir_all(&deep).unwrap();
    let gamma = dir.path().join("gamma");
    fs::create_dir(&gamma).unwrap();
    mind(&gamma, &["init"]).unwrap();

    // Typed relative to where the command runs, stored relative to the
    // workspace, which is three directories up.
    let outcome = flayer(&deep, &["link", "../../../gamma"]).unwrap();

    assert_eq!(outcome.stdout, "linked gamma as gamma\n");
    assert_eq!(
        flayer(dir.path(), &["list"])
            .unwrap()
            .stdout
            .lines()
            .count(),
        2
    );
}

#[test]
fn a_project_outside_the_workspace_is_stored_as_a_route_out() {
    let outer = TempDir::new().unwrap();
    let inner = outer.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    flayer(&inner, &["init"]).unwrap();
    let sibling = outer.path().join("collapse");
    fs::create_dir(&sibling).unwrap();
    mind(&sibling, &["init"]).unwrap();
    write_skill(&sibling, "deploy", &skill_file("deploy", "Ship it"));

    let outcome = flayer(&inner, &["link", "../collapse"]).unwrap();

    // Forward slashes on every platform, and — the invariant that matters —
    // the entry the message names is the entry in the file, verbatim. On
    // Windows `Path::display` would report `..\collapse` for a line that
    // reads `../collapse`, and what a command says it wrote has to be what
    // someone opening the file finds.
    assert_eq!(outcome.stdout, "linked collapse as ../collapse\n");
    let config = fs::read_to_string(inner.join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("\"../collapse\""), "{config}");
    assert!(flayer(&inner, &["list"]).unwrap().stdout.contains("deploy"));
}

#[test]
fn unlink_removes_one_entry_and_leaves_the_rest() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\n");
    let listed = flayer(dir.path(), &["list"]).unwrap();
    assert!(listed.stdout.contains("beta"));
    assert!(!listed.stdout.contains("alpha"));
}

#[test]
fn unlink_says_so_when_nothing_was_registered_under_that_path() {
    let dir = workspace_with_two();

    let error = flayer(dir.path(), &["unlink", "gamma"]).unwrap_err();

    assert!(
        error.error.to_string().contains("not registered"),
        "{}",
        error.error
    );
}

#[test]
fn unlink_works_on_a_project_whose_directory_has_gone() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    // The stale entry is exactly the one worth removing, and it warns until
    // it is gone.
    assert!(!flayer(dir.path(), &["list"]).unwrap().ok);

    let outcome = flayer(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\n");
    assert!(flayer(dir.path(), &["list"]).unwrap().ok);
}

#[test]
fn link_needs_a_workspace_not_just_a_project() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let error = flayer(dir.path(), &["link", "."]).unwrap_err();

    assert!(matches!(*error.error, CliError::NotInWorkspace(_)));
}

#[test]
fn a_workspace_can_manage_the_project_it_sits_in() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let outcome = flayer(dir.path(), &["link", "."]).unwrap();

    assert!(outcome.stdout.starts_with("linked"));
    assert!(flayer(dir.path(), &["list"])
        .unwrap()
        .stdout
        .contains("deploy"));
}

#[test]
fn link_takes_several_projects_at_once() {
    let dir = workspace_with_two_unlinked();

    let outcome = flayer(dir.path(), &["link", "gamma", "delta"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "linked gamma as gamma\nlinked delta as delta\n"
    );
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("\"gamma\""), "{config}");
    assert!(config.contains("\"delta\""), "{config}");
}

#[test]
fn link_links_none_when_one_of_them_is_refused() {
    let dir = workspace_with_two_unlinked();
    fs::create_dir(dir.path().join("plain")).unwrap();

    let error = flayer(dir.path(), &["link", "gamma", "plain", "delta"]).unwrap_err();

    assert!(
        error.error.to_string().contains(MIND_DIR),
        "{}",
        error.error
    );
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(
        !config.contains("\"gamma\""),
        "half a link happened: {config}"
    );
    assert!(
        !config.contains("\"delta\""),
        "half a link happened: {config}"
    );
}

#[test]
fn unlink_takes_several_and_refuses_all_for_one_unknown() {
    let dir = workspace_with_two();

    let error = flayer(dir.path(), &["unlink", "alpha", "gamma"]).unwrap_err();
    assert!(
        error.error.to_string().contains("not registered"),
        "{}",
        error.error
    );
    assert_eq!(
        flayer(dir.path(), &["list"])
            .unwrap()
            .stdout
            .lines()
            .count(),
        2
    );

    let outcome = flayer(dir.path(), &["unlink", "alpha", "beta"]).unwrap();
    assert_eq!(outcome.stdout, "unlinked alpha\nunlinked beta\n");
}

#[test]
fn link_and_unlink_need_at_least_one_path() {
    assert!(FlayerCli::try_parse_from(["flayer", "link"]).is_err());
    assert!(FlayerCli::try_parse_from(["flayer", "unlink"]).is_err());
}

// ---------------------------------------------------------------------------
// The workspace in your home
// ---------------------------------------------------------------------------

#[test]
fn with_no_workspace_above_flayer_uses_the_one_in_home() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let project = elsewhere.path().join("alpha");
    fs::create_dir(&project).unwrap();
    mind(&project, &["init"]).unwrap();

    let outcome = flayer_at_home(elsewhere.path(), home.path(), &["link", "alpha"]).unwrap();

    assert!(
        outcome.stdout.starts_with("linked alpha as "),
        "{}",
        outcome.stdout
    );
    assert!(home.path().join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
    // Nothing was made where the command ran.
    assert!(!elsewhere.path().join(FLAYER_DIR).exists());
}

#[test]
fn a_workspace_above_wins_over_the_one_in_home() {
    let home = TempDir::new().unwrap();
    let dir = workspace_with_two_unlinked();

    flayer_at_home(dir.path(), home.path(), &["link", "gamma"]).unwrap();

    assert!(!home.path().join(FLAYER_DIR).exists());
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("\"gamma\""), "{config}");
}

#[test]
fn an_empty_home_turns_the_fallback_off() {
    let dir = TempDir::new().unwrap();

    let error = flayer(dir.path(), &["list"]).unwrap_err();

    assert!(matches!(*error.error, CliError::NotInWorkspace(_)));
}
