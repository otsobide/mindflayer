//! Which workspace a command uses: the one `-W` names, then the one
//! `$MINDFLAYER_WORKSPACE` activates, then the one above, then the default;
//! and `flayer which`, which says which one and why.
//!
//! What the environment says can only be set for a binary of its own, so
//! those tests spawn one (`common::spawn`); the rest run in process.

mod common;

use common::*;

/// Under one directory, none above another: `outer`, a workspace with a
/// skill of its own and `alpha` linked under it; `hub`, a workspace with a
/// skill of its own; and `collapse`, a project under neither, made as an
/// older Mindflayer made it, so a copy installed there is in the folder it
/// authors in, where a change from the project reaches it.
struct Fixture {
    _dir: TempDir,
    outer: PathBuf,
    alpha: PathBuf,
    hub: PathBuf,
    collapse: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let outer = dir.path().join("outer");
    let alpha = outer.join("alpha");
    let hub = dir.path().join("hub");
    let collapse = dir.path().join("collapse");
    for root in [&outer, &alpha, &hub, &collapse] {
        fs::create_dir_all(root).unwrap();
    }
    flayer(&outer, &["init"]).unwrap();
    flayer(&hub, &["init"]).unwrap();
    mind(&alpha, &["init"]).unwrap();
    older_project(&collapse);
    flayer(&outer, &["link", "alpha"]).unwrap();
    flayer(&outer, &["add", "skill", "outer-skill", "The outer one's"]).unwrap();
    flayer(&hub, &["add", "skill", "hub-skill", "The hub's"]).unwrap();
    Fixture {
        _dir: dir,
        outer,
        alpha,
        hub,
        collapse,
    }
}

fn spelled(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn a_named_workspace_wins_over_the_one_above() {
    let fixture = fixture();
    let hub = spelled(&fixture.hub);

    let outcome = flayer(&fixture.alpha, &["list", "-W", &hub]).unwrap();

    assert!(outcome.stdout.contains("hub-skill"), "{}", outcome.stdout);
    assert!(
        !outcome.stdout.contains("outer-skill"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn a_project_links_into_the_workspace_named_for_it() {
    let fixture = fixture();
    let hub = spelled(&fixture.hub);

    let outcome = mind(&fixture.alpha, &["link", "--workspace", &hub]).unwrap();

    assert_eq!(outcome.stdout, "linked alpha as ../outer/alpha\n");
    assert!(registry(&fixture.hub).contains("\"../outer/alpha\""));
}

#[test]
fn a_workspace_is_named_by_its_mindflayer_folder_as_well() {
    let fixture = fixture();
    let folder = spelled(&fixture.hub.join(FLAYER_DIR));

    let outcome = flayer(&fixture.alpha, &["list", &format!("-W{folder}")]).unwrap();

    assert!(outcome.stdout.contains("hub-skill"), "{}", outcome.stdout);
}

#[test]
fn naming_a_directory_that_is_no_workspace_is_refused_and_makes_nothing() {
    let fixture = fixture();
    let plain = fixture.outer.join("plain");
    fs::create_dir(&plain).unwrap();
    let before = registry(&fixture.outer);

    let failure = flayer(
        &fixture.alpha,
        &["add", "skill", "x", "An x", "-W", &spelled(&plain)],
    )
    .unwrap_err();

    // Never the workspace above instead, which nobody chose.
    assert!(
        matches!(*failure.error, CliError::Chosen { .. }),
        "{}",
        failure.error
    );
    assert_eq!(
        failure.error.to_string(),
        format!(
            "-W names a workspace that cannot be used: {}: not a flayer workspace (no .mindflayer/flayer.toml)",
            plain.display()
        )
    );
    assert!(!plain.join(FLAYER_DIR).exists());
    assert_eq!(registry(&fixture.outer), before);
    assert!(!own_directory_for(&fixture.outer, Kind::Skill)
        .join("x")
        .exists());
}

// ---------------------------------------------------------------------------
// What the environment says
// ---------------------------------------------------------------------------

#[test]
fn an_activated_workspace_wins_over_the_one_above() {
    let fixture = fixture();
    let hub = spelled(&fixture.hub);

    let listed = spawn(
        "flayer",
        &fixture.alpha,
        &["list"],
        &[("MINDFLAYER_WORKSPACE", &hub)],
    );

    assert!(listed.ok, "{}", listed.stderr);
    assert!(listed.stdout.contains("hub-skill"), "{}", listed.stdout);
    assert!(!listed.stdout.contains("outer-skill"), "{}", listed.stdout);
}

#[test]
fn an_empty_mindflayer_workspace_counts_as_unset() {
    let fixture = fixture();

    let listed = spawn(
        "flayer",
        &fixture.alpha,
        &["list"],
        &[("MINDFLAYER_WORKSPACE", "")],
    );

    assert!(listed.ok, "{}", listed.stderr);
    assert!(listed.stdout.contains("outer-skill"), "{}", listed.stdout);
}

#[test]
fn a_workspace_named_empty_keeps_the_activated_one_out() {
    let fixture = fixture();
    let hub = spelled(&fixture.hub);

    let listed = spawn(
        "flayer",
        &fixture.alpha,
        &["list", "-W", ""],
        &[("MINDFLAYER_WORKSPACE", &hub)],
    );

    assert!(listed.ok, "{}", listed.stderr);
    assert!(listed.stdout.contains("outer-skill"), "{}", listed.stdout);
}

#[test]
fn a_named_workspace_wins_over_the_activated_one() {
    let fixture = fixture();
    let (hub, outer) = (spelled(&fixture.hub), spelled(&fixture.outer));

    let listed = spawn(
        "mind",
        &fixture.collapse,
        &["flayer", "list", "-W", &outer],
        &[("MINDFLAYER_WORKSPACE", &hub)],
    );

    assert!(listed.ok, "{}", listed.stderr);
    assert!(listed.stdout.contains("outer-skill"), "{}", listed.stdout);
}

#[test]
fn activating_a_directory_that_is_no_workspace_is_refused() {
    let fixture = fixture();
    let gone = fixture.outer.join("gone");

    let listed = spawn(
        "flayer",
        &fixture.alpha,
        &["list"],
        &[("MINDFLAYER_WORKSPACE", &spelled(&gone))],
    );

    assert!(!listed.ok);
    assert!(
        listed
            .stderr
            .contains("$MINDFLAYER_WORKSPACE names a workspace that cannot be used"),
        "{}",
        listed.stderr
    );
    assert!(!gone.exists());
}

#[test]
fn installing_from_an_activated_workspace_not_above_records_and_disowns() {
    // `hub` is beside `collapse`, so only the activation leads to it: walking
    // up from the project never does.
    let fixture = fixture();
    flayer(&fixture.hub, &["link", "../collapse"]).unwrap();
    let hub = spelled(&fixture.hub);
    let activated = [("MINDFLAYER_WORKSPACE", hub.as_str())];
    let recorded = || {
        FlayerWorkspace::open(&fixture.hub)
            .unwrap()
            .ledger()
            .unwrap()
            .installations("../collapse", Kind::Skill)
            .unwrap()
    };

    let installed = spawn(
        "mind",
        &fixture.collapse,
        &["install", "hub-skill"],
        &activated,
    );

    assert!(installed.ok, "{}", installed.stderr);
    assert_eq!(
        installed.stdout,
        "installed hub-skill into collapse, from workspace\n"
    );
    assert_eq!(recorded(), vec![String::from("hub-skill")]);

    let removed = spawn(
        "mind",
        &fixture.collapse,
        &["remove", "hub-skill", "--yes"],
        &activated,
    );

    assert!(removed.ok, "{}", removed.stderr);
    assert!(
        removed.stdout.contains("`flayer install` can put it back"),
        "{}",
        removed.stdout
    );
    assert!(recorded().is_empty());
    assert!(!fixture.collapse.join("skills/hub-skill").exists());
}

#[test]
fn a_change_to_a_project_is_refused_when_the_named_workspace_will_not_open() {
    // Told to a workspace that is not there, the change would leave the
    // record of the one meant behind once the name was put right.
    let fixture = fixture();
    write_skill(
        &fixture.collapse,
        "deploy",
        &skill_file("deploy", "Ship it"),
    );
    let plain = fixture.outer.join("plain");

    let failure = mind(
        &fixture.collapse,
        &["remove", "deploy", "--yes", "-W", &spelled(&plain)],
    )
    .unwrap_err();

    assert!(
        matches!(*failure.error, CliError::Chosen { .. }),
        "{}",
        failure.error
    );
    assert!(fixture.collapse.join("skills/deploy").is_dir());
}

// ---------------------------------------------------------------------------
// flayer which: the workspace a command would use, and why
// ---------------------------------------------------------------------------

#[test]
fn which_says_a_workspace_named_with_w_is_used_because_it_was_named() {
    let fixture = fixture();

    let outcome = flayer(&fixture.alpha, &["which", "-W", &spelled(&fixture.hub)]).unwrap();

    assert_eq!(
        outcome,
        Outcome {
            stdout: format!("hub at {}, named by -W\n", fixture.hub.display()),
            stderr: Vec::new(),
            ok: true,
        }
    );
}

#[test]
fn which_says_an_activated_workspace_is_used_because_it_was_activated() {
    let fixture = fixture();

    let printed = spawn(
        "flayer",
        &fixture.alpha,
        &["which"],
        &[("MINDFLAYER_WORKSPACE", &spelled(&fixture.hub))],
    );

    assert!(printed.ok, "{}", printed.stderr);
    assert_eq!(
        printed.stdout,
        format!(
            "hub at {}, activated by $MINDFLAYER_WORKSPACE\n",
            fixture.hub.display()
        )
    );
    assert_eq!(printed.stderr, "");
}

#[test]
fn which_says_a_workspace_above_is_used_because_it_was_found_walking_up() {
    let fixture = fixture();

    let outcome = flayer(&fixture.alpha, &["which"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "outer at {}, found walking up from {}\n",
            fixture.outer.display(),
            fixture.alpha.display()
        )
    );
}

#[test]
fn which_says_the_default_workspace_is_used_because_there_is_no_other() {
    let fixture = fixture();
    let home = TempDir::new().unwrap();
    flayer(home.path(), &["init"]).unwrap();

    let outcome = flayer_at_home(&fixture.collapse, home.path(), &["which"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "{} at {}, the default one\n",
            home.path().file_name().unwrap().to_string_lossy(),
            home.path().display()
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
}

#[test]
fn which_says_the_default_workspace_is_not_made_yet_and_leaves_it_so() {
    let fixture = fixture();
    let home = TempDir::new().unwrap();

    let outcome = flayer_at_home(&fixture.collapse, home.path(), &["which"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "{} at {}, the default one, not made yet\n",
            home.path().file_name().unwrap().to_string_lossy(),
            home.path().display()
        )
    );
    assert!(
        outcome.stderr[0].starts_with("hint: no workspace here or above"),
        "{:?}",
        outcome.stderr
    );
    assert!(outcome.ok);
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn which_with_no_workspace_anywhere_says_so() {
    let fixture = fixture();

    let failure = flayer(&fixture.collapse, &["which"]).unwrap_err();

    assert!(
        matches!(*failure.error, CliError::NotInWorkspace(_)),
        "{}",
        failure.error
    );
}

#[test]
fn which_says_why_an_activated_workspace_cannot_be_used() {
    let fixture = fixture();
    let gone = fixture.outer.join("gone");

    let printed = spawn(
        "flayer",
        &fixture.alpha,
        &["which"],
        &[("MINDFLAYER_WORKSPACE", &spelled(&gone))],
    );

    assert!(!printed.ok);
    assert_eq!(printed.stdout, "");
    assert_eq!(
        printed.stderr,
        format!(
            "error: $MINDFLAYER_WORKSPACE names a workspace that cannot be used: {}: not a flayer workspace (no .mindflayer/flayer.toml)\n",
            gone.display()
        )
    );
}
