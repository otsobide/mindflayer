//! The workspaces this machine knows: remembered by `flayer init` in the
//! default workspace, listed by `flayer workspaces`, and forgotten by
//! `flayer workspaces forget`, each with a temporary home.

mod common;

use common::*;

/// A temporary directory holding `home`, a made default workspace.
fn made_home() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    flayer_at_home(&home, &home, &["init"]).unwrap();
    (dir, home)
}

fn known_in(home: &Path) -> Vec<PathBuf> {
    FlayerWorkspace::open(home).unwrap().config().known.clone()
}

// ---------------------------------------------------------------------------
// Remembering
// ---------------------------------------------------------------------------

#[test]
fn init_remembers_the_workspace_in_the_default_one() {
    let (_dir, home) = made_home();

    let outcome = flayer_at_home(&home, &home, &["init", "flayers/writing"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "initialized flayer workspace `writing` in {}\n  remembered as flayers/writing by the default workspace\n",
            home.join("flayers/writing").join(FLAYER_DIR).display()
        )
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    assert_eq!(known_in(&home), [PathBuf::from("flayers/writing")]);
}

#[test]
fn init_again_remembers_a_workspace_made_before_the_default_and_nothing_more() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    flayer_at_home(dir.path(), &home, &["init", "writing"]).unwrap();
    flayer_at_home(&home, &home, &["init"]).unwrap();
    let marker = fs::read_to_string(dir.path().join("writing/.mindflayer/flayer.toml")).unwrap();

    let first = flayer_at_home(dir.path(), &home, &["init", "writing"]).unwrap();
    let second = flayer_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    let already = format!(
        "flayer workspace `writing` already initialized in {}\n",
        dir.path().join("writing").join(FLAYER_DIR).display()
    );
    assert_eq!(
        first.stdout,
        format!("{already}  remembered as ../writing by the default workspace\n")
    );
    assert_eq!(second.stdout, already, "already known says nothing more");
    assert_eq!(known_in(&home), [PathBuf::from("../writing")]);
    assert_eq!(
        fs::read_to_string(dir.path().join("writing/.mindflayer/flayer.toml")).unwrap(),
        marker,
        "the workspace itself is left as it was"
    );
}

#[test]
fn init_remembers_nothing_with_no_default() {
    let (dir, home) = made_home();

    // `common::flayer` pins `--home=`, which names no default at all.
    let outcome = flayer(dir.path(), &["init", "writing"]).unwrap();

    assert!(!outcome.stdout.contains("remembered"), "{}", outcome.stdout);
    assert!(known_in(&home).is_empty());
}

#[test]
fn init_never_makes_the_default_to_remember_one() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let outcome = flayer_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    assert!(!outcome.stdout.contains("remembered"), "{}", outcome.stdout);
    assert!(!home.join(FLAYER_DIR).exists(), "the default was made");
}

#[test]
fn the_default_workspace_does_not_remember_itself() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let made = flayer_at_home(&home, &home, &["init"]).unwrap();
    let again = flayer_at_home(&home, &home, &["init"]).unwrap();

    assert!(!made.stdout.contains("remembered"), "{}", made.stdout);
    assert!(!again.stdout.contains("remembered"), "{}", again.stdout);
    assert!(!registry(&home).contains("known"), "{}", registry(&home));
}

#[test]
fn a_default_that_cannot_remember_is_a_warning_after_the_init() {
    let (dir, home) = made_home();
    let path = home.join(FLAYER_DIR).join(FLAYER_CONFIG);
    let newer = registry(&home).replace("version = 2", "version = 99");
    fs::write(&path, &newer).unwrap();

    let outcome = flayer_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    assert!(
        outcome
            .stdout
            .starts_with("initialized flayer workspace `writing`"),
        "{}",
        outcome.stdout
    );
    assert_eq!(
        outcome.stderr,
        [format!(
            "warning: the default workspace could not remember it: {}: written by a newer Mindflayer (workspace marker version 99, this one reads 2)",
            path.display()
        )]
    );
    assert!(!outcome.ok);
    assert!(dir.path().join("writing").join(FLAYER_DIR).is_dir());
    assert_eq!(fs::read_to_string(&path).unwrap(), newer);
}
