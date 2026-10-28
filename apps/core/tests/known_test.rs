//! The workspaces this machine knows: remembered in the default workspace's
//! marker, never made for the occasion, and forgotten by any spelling.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::known;
use mindflayer_core::{
    FlayerWorkspace, MindProject, Registration, WorkspaceError, FLAYER_CONFIG, FLAYER_DIR,
};
use tempfile::TempDir;

/// A temporary directory holding `home`, a made default workspace, and
/// `home/flayers/writing`, a workspace somebody made under it.
fn home_and_writing() -> (TempDir, PathBuf, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    FlayerWorkspace::init(&home).unwrap();
    let (writing, _) = FlayerWorkspace::init(home.join("flayers/writing")).unwrap();
    (dir, home, writing)
}

fn marker(root: &Path) -> String {
    fs::read_to_string(root.join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap()
}

#[test]
fn remember_records_a_workspace_as_a_route_from_home_and_says_what_known_is() {
    let (_dir, home, writing) = home_and_writing();

    let remembered = known::remember(Some(&home), &writing).unwrap();

    assert_eq!(
        remembered,
        Some((PathBuf::from("flayers/writing"), Registration::Added))
    );
    let text = marker(&home);
    assert!(text.contains("known = [\"flayers/writing\"]"), "{text}");
    assert!(
        text.contains("# `known` lists the other flayer workspaces on this machine"),
        "{text}"
    );
    assert!(text.contains("# Mindflayer workspace."), "comments kept");
    assert_eq!(
        FlayerWorkspace::open(&home).unwrap().config().known,
        [PathBuf::from("flayers/writing")]
    );
}

#[test]
fn remembering_twice_changes_nothing_and_says_the_spelling_the_file_uses() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let before = marker(&home);

    let again = FlayerWorkspace::open(home.join("flayers/./writing/")).unwrap();
    let remembered = known::remember(Some(&home), &again).unwrap();

    assert_eq!(
        remembered,
        Some((
            PathBuf::from("flayers/writing"),
            Registration::AlreadyRegistered
        ))
    );
    assert_eq!(marker(&home), before);
}

#[test]
fn a_second_workspace_goes_after_the_first_under_one_comment() {
    let (_dir, home, writing) = home_and_writing();
    let (notes, _) = FlayerWorkspace::init(home.join("notes")).unwrap();

    known::remember(Some(&home), &writing).unwrap();
    known::remember(Some(&home), &notes).unwrap();

    let text = marker(&home);
    assert!(
        text.contains("known = [\"flayers/writing\", \"notes\"]"),
        "{text}"
    );
    assert_eq!(text.matches("# `known` lists").count(), 1, "{text}");
}

#[test]
fn a_workspace_outside_home_is_remembered_by_its_route_from_there() {
    let (dir, home, _) = home_and_writing();
    let (elsewhere, _) = FlayerWorkspace::init(dir.path().join("elsewhere")).unwrap();

    let remembered = known::remember(Some(&home), &elsewhere).unwrap();

    assert_eq!(
        remembered,
        Some((PathBuf::from("../elsewhere"), Registration::Added))
    );
}

#[test]
fn nothing_is_remembered_without_a_default() {
    let (_dir, home, writing) = home_and_writing();
    let before = marker(&home);

    assert_eq!(known::remember(None, &writing).unwrap(), None);
    assert_eq!(marker(&home), before);
}

#[test]
fn a_default_not_made_yet_is_not_made_to_remember_one() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let (writing, _) = FlayerWorkspace::init(dir.path().join("writing")).unwrap();

    assert_eq!(known::remember(Some(&home), &writing).unwrap(), None);
    assert!(!home.join(FLAYER_DIR).exists(), "the default was made");
}

#[test]
fn the_default_workspace_does_not_remember_itself() {
    let (_dir, home, _) = home_and_writing();
    let before = marker(&home);
    let default = FlayerWorkspace::open(&home).unwrap();

    assert_eq!(known::remember(Some(&home), &default).unwrap(), None);
    assert_eq!(marker(&home), before);
}

#[test]
fn a_default_written_by_a_newer_build_is_refused_and_left_alone() {
    let (_dir, home, writing) = home_and_writing();
    let newer = marker(&home).replace("version = 2", "version = 99");
    fs::write(home.join(FLAYER_DIR).join(FLAYER_CONFIG), &newer).unwrap();

    let error = known::remember(Some(&home), &writing).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { .. }), "{error}");
    assert_eq!(marker(&home), newer);
}

#[test]
fn forget_removes_every_spelling_of_a_workspace() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let absolute = writing.root().to_string_lossy().into_owned();
    let path = home.join(FLAYER_DIR).join(FLAYER_CONFIG);
    let text = marker(&home).replace(
        "known = [\"flayers/writing\"]",
        &format!("known = [\"flayers/writing\", \"./flayers/writing/\", \"{absolute}\"]"),
    );
    fs::write(&path, text).unwrap();

    let forgotten = known::forget(&home, &home.join("flayers/writing")).unwrap();

    assert_eq!(
        forgotten,
        [
            PathBuf::from("flayers/writing"),
            PathBuf::from("./flayers/writing/"),
            PathBuf::from(&absolute),
        ]
    );
    let text = marker(&home);
    assert!(text.contains("known = []"), "{text}");
    assert!(FlayerWorkspace::open(&home)
        .unwrap()
        .config()
        .known
        .is_empty());
}

#[test]
fn a_workspace_that_has_gone_is_forgotten_by_its_path() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    fs::remove_dir_all(writing.root()).unwrap();

    let forgotten = known::forget(&home, writing.root()).unwrap();

    assert_eq!(forgotten, [PathBuf::from("flayers/writing")]);
}

#[test]
fn forgetting_what_nothing_remembers_is_refused_and_writes_nothing() {
    let (_dir, home, writing) = home_and_writing();
    let before = marker(&home);

    let error = known::forget(&home, writing.root()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotKnown { .. }), "{error}");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: not known to the default workspace at {}",
            writing.root().display(),
            home.display()
        )
    );
    assert_eq!(marker(&home), before, "no empty `known` left behind");
}

#[test]
fn forgetting_with_no_default_made_is_refused_and_makes_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let error = known::forget(&home, &dir.path().join("writing")).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotKnown { .. }), "{error}");
    assert!(!home.join(FLAYER_DIR).exists(), "the default was made");
}

#[test]
fn linking_into_the_default_keeps_what_it_knows() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let (project, _) = MindProject::init(home.join("collapse")).unwrap();

    FlayerWorkspace::open(&home)
        .unwrap()
        .link(&project)
        .unwrap();

    let text = marker(&home);
    assert!(text.contains("known = [\"flayers/writing\"]"), "{text}");
    assert!(text.contains("projects = [\"collapse\"]"), "{text}");
}
