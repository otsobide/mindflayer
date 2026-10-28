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

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// Each listed workspace as `(name, path, default, remembered, in use)`.
fn listed(known: &known::Known) -> Vec<(Option<&str>, &Path, bool, bool, bool)> {
    known
        .workspaces
        .iter()
        .map(|listed| {
            (
                listed.name.as_deref(),
                listed.path.as_path(),
                listed.default,
                listed.remembered,
                listed.in_use,
            )
        })
        .collect()
}

#[test]
fn the_default_comes_first_then_what_it_remembers_in_order() {
    let (_dir, home, writing) = home_and_writing();
    let (notes, _) = FlayerWorkspace::init(home.join("notes")).unwrap();
    known::remember(Some(&home), &writing).unwrap();
    known::remember(Some(&home), &notes).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, false),
            (Some("notes"), notes.root(), false, true, false),
        ]
    );
}

#[test]
fn the_workspace_in_use_is_marked_where_it_is_listed() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();

    let (known, _) = known::known(Some(&home), Some(&writing));

    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, true),
        ]
    );
}

#[test]
fn one_in_use_that_nothing_remembers_is_listed_last() {
    let (dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let (elsewhere, _) = FlayerWorkspace::init(dir.path().join("elsewhere")).unwrap();

    let (known, _) = known::known(Some(&home), Some(&elsewhere));

    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, false),
            (Some("elsewhere"), elsewhere.root(), false, false, true),
        ]
    );
}

#[test]
fn a_remembered_workspace_that_has_gone_is_listed_with_why_and_a_failure() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    fs::remove_dir_all(writing.root()).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    let gone = &known.workspaces[1];
    assert_eq!(gone.name, None);
    assert_eq!(gone.path, writing.root());
    assert!(gone.remembered);
    let why = format!(
        "{}: not a flayer workspace (no .mindflayer/flayer.toml)",
        writing.root().display()
    );
    assert_eq!(gone.stale.as_deref(), Some(why.as_str()));
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].to_string(), why);
}

#[test]
fn two_spellings_of_one_workspace_are_listed_once() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let path = home.join(FLAYER_DIR).join(FLAYER_CONFIG);
    let absolute = writing.root().to_string_lossy().into_owned();
    let text = marker(&home).replace(
        "known = [\"flayers/writing\"]",
        &format!("known = [\"flayers/writing\", \"{absolute}\", \".\"]"),
    );
    fs::write(&path, text).unwrap();

    let (known, _) = known::known(Some(&home), None);

    // `.` is the default itself, which is listed once, as remembered too.
    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, true, false),
            (Some("writing"), writing.root(), false, true, false),
        ]
    );
}

#[test]
fn each_workspace_counts_its_projects_and_the_stale_ones() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let mut writing = FlayerWorkspace::open(writing.root()).unwrap();
    for name in ["essays", "letters", "gone"] {
        let (project, _) = MindProject::init(writing.root().join(name)).unwrap();
        writing.link(&project).unwrap();
    }
    fs::remove_dir_all(writing.root().join("gone")).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert!(
        failures.is_empty(),
        "a stale project is the count's, not a failure"
    );
    assert_eq!(
        (
            known.workspaces[0].projects,
            known.workspaces[0].stale_projects
        ),
        (0, 0)
    );
    assert_eq!(
        (
            known.workspaces[1].projects,
            known.workspaces[1].stale_projects
        ),
        (3, 1)
    );
}

#[test]
fn without_a_default_only_the_one_in_use_is_listed() {
    let (_dir, _home, writing) = home_and_writing();

    let (nothing, _) = known::known(None, None);
    let (in_use, _) = known::known(None, Some(&writing));

    assert!(nothing.workspaces.is_empty());
    assert_eq!(
        listed(&in_use),
        [(Some("writing"), writing.root(), false, false, true)]
    );
}

#[test]
fn listing_never_makes_the_default_nor_marks_it_unmade() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let unmade = FlayerWorkspace::unmade(&home).unwrap();

    let (known, failures) = known::known(Some(&home), Some(&unmade));

    assert!(known.workspaces.is_empty());
    assert!(failures.is_empty());
    assert!(!home.join(FLAYER_DIR).exists(), "the default was made");
}

#[test]
fn a_default_that_will_not_open_is_listed_as_stale() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let newer = marker(&home).replace("version = 2", "version = 99");
    fs::write(home.join(FLAYER_DIR).join(FLAYER_CONFIG), newer).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert_eq!(known.workspaces.len(), 1, "what it knows cannot be read");
    assert!(known.workspaces[0].default);
    assert!(known.workspaces[0].stale.is_some());
    assert!(matches!(failures[..], [WorkspaceError::Version { .. }]));
}

#[test]
fn the_list_serializes_each_field_by_its_name() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();

    let (known, _) = known::known(Some(&home), Some(&writing));

    assert_eq!(
        serde_json::to_value(&known).unwrap(),
        serde_json::json!({
            "workspaces": [
                {
                    "name": "home",
                    "path": home.to_string_lossy(),
                    "default": true,
                    "remembered": false,
                    "in_use": false,
                    "projects": 0,
                    "stale_projects": 0,
                    "stale": null,
                },
                {
                    "name": "writing",
                    "path": writing.root().to_string_lossy(),
                    "default": false,
                    "remembered": true,
                    "in_use": true,
                    "projects": 0,
                    "stale_projects": 0,
                    "stale": null,
                },
            ]
        })
    );
}
