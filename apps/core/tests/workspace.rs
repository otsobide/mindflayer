//! Creating, finding and reading mind projects and flayer workspaces.

use std::fs;

use mindflayer_core::{
    FlayerWorkspace, Initialization, MindProject, WorkspaceError, FLAYER_CONFIG, FLAYER_DIR,
    MIND_CONFIG, MIND_DIR,
};
use tempfile::TempDir;

#[test]
fn initializing_a_mind_project_creates_its_marker_and_its_skills_folder() {
    let dir = TempDir::new().unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(project.skills_dir().is_dir());
    assert_eq!(project.root(), dir.path());
    assert_eq!(project.config().version, 1);
}

#[test]
fn a_new_project_is_named_after_its_directory() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();

    let (project, _) = MindProject::init(&root).unwrap();

    assert_eq!(project.name(), "collapse");
}

#[test]
fn initializing_twice_never_rewrites_the_marker() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let marker = dir.path().join(MIND_DIR).join(MIND_CONFIG);
    fs::write(&marker, "version = 1\nname = \"renamed by hand\"\n").unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert_eq!(project.name(), "renamed by hand");
}

#[test]
fn initializing_fills_in_a_skills_folder_someone_deleted() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    fs::remove_dir(project.skills_dir()).unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert!(project.skills_dir().is_dir());
}

#[test]
fn a_project_is_found_from_any_directory_below_it() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    let found = MindProject::locate(&deep)
        .unwrap()
        .expect("a project above");

    assert_eq!(found.root(), dir.path());
}

#[test]
fn locate_finds_nothing_when_there_is_no_project() {
    let dir = TempDir::new().unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
    assert!(FlayerWorkspace::locate(dir.path()).unwrap().is_none());
}

#[test]
fn opening_a_directory_that_is_not_a_project_says_so() {
    let dir = TempDir::new().unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotAProject { .. }));
    assert!(error.to_string().contains(MIND_DIR));
}

#[test]
fn an_empty_mind_directory_is_not_a_project() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
}

#[test]
fn a_marker_from_a_newer_mindflayer_is_refused() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "version = 99\nname = \"future\"\n",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { found: 99, .. }));
}

#[test]
fn a_corrupt_marker_is_reported_as_such() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "not = toml = at all",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Parse { .. }));
}

#[test]
fn initializing_a_workspace_creates_an_empty_registry() {
    let dir = TempDir::new().unwrap();

    let (workspace, outcome) = FlayerWorkspace::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
    assert!(workspace.config().projects.is_empty());
    assert_eq!(workspace.projects().0, vec![]);
}

#[test]
fn a_workspace_opens_the_projects_it_references() {
    let dir = TempDir::new().unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        MindProject::init(&root).unwrap();
    }
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"beta\"]\n",
    )
    .unwrap();

    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert!(failures.is_empty());
    let names: Vec<&str> = projects.iter().map(MindProject::name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn a_stale_reference_is_reported_without_losing_the_rest() {
    let dir = TempDir::new().unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"moved-away\"]\n",
    )
    .unwrap();

    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert_eq!(projects.len(), 1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].to_string().contains("moved-away"));
}

#[test]
fn one_directory_can_be_both_a_workspace_and_a_project() {
    let dir = TempDir::new().unwrap();

    FlayerWorkspace::init(dir.path()).unwrap();
    MindProject::init(dir.path()).unwrap();

    assert!(FlayerWorkspace::locate(dir.path()).unwrap().is_some());
    assert!(MindProject::locate(dir.path()).unwrap().is_some());
}
