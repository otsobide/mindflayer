//! A project marked by its settings, `.mind/settings.json`: found and read
//! as any project is, beside the ones a `mind.toml` still marks.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::{
    create, scan, Agent, Catalog, CreateError, Directories, FlayerWorkspace, Initialization, Kind,
    MindProject, ProjectWarning, Settings, Share, WorkspaceError, MIND_CONFIG, MIND_DIR,
    SETTINGS_FILE,
};
use tempfile::TempDir;

/// Mark `root` a project by its settings, naming `directories`, as a new
/// project's would be written.
fn settings_project(root: &Path, directories: Directories) -> PathBuf {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let path = mind.join(SETTINGS_FILE);
    let settings = Settings::new("collapse", Agent::ALL.to_vec(), Share::Local, directories);
    fs::write(&path, settings.text().unwrap()).unwrap();
    path
}

fn skills_beside() -> Directories {
    Directories::default().with(Kind::Skill, "skills")
}

#[test]
fn a_project_marked_by_its_settings_is_found_from_any_directory_below_it() {
    let dir = TempDir::new().unwrap();
    settings_project(dir.path(), skills_beside());
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    let found = MindProject::locate(&deep)
        .unwrap()
        .expect("a project above");

    assert_eq!(found.root(), dir.path());
    assert!(found.settings().is_some());
    assert!(found.warnings().is_empty());
}

#[test]
fn a_project_holding_both_markers_is_read_from_its_settings_and_warns_of_the_other() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    settings_project(dir.path(), Directories::default());
    let leftover = dir.path().join(MIND_DIR).join(MIND_CONFIG);

    let project = MindProject::open(dir.path()).unwrap();

    // Read from the settings: they name no folder, where the `mind.toml`
    // `init` wrote names one for every kind.
    assert!(project.settings().is_some());
    assert_eq!(project.directory_for(Kind::Skill), None);
    let warnings = project.warnings();
    assert_eq!(
        warnings,
        vec![ProjectWarning::Leftover {
            path: leftover.clone()
        }]
    );
    assert_eq!(
        warnings[0].to_string(),
        format!(
            "{}: ignored, since the project is read from the settings.json beside it",
            leftover.display()
        )
    );
}

#[test]
fn init_leaves_a_project_marked_by_its_settings_without_a_mind_toml() {
    let dir = TempDir::new().unwrap();
    settings_project(dir.path(), skills_beside());

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert!(!dir.path().join(MIND_DIR).join(MIND_CONFIG).exists());
    assert!(project.warnings().is_empty());
    // What the settings name is made, and nothing they leave out.
    assert!(dir.path().join("skills").is_dir());
    assert!(!dir.path().join("rules").exists());
}

#[test]
fn a_scan_takes_a_project_marked_by_its_settings_for_a_project() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    fs::create_dir(dir.path().join("alpha")).unwrap();
    settings_project(&dir.path().join("alpha"), skills_beside());

    let scan = scan(&workspace);

    assert_eq!(scan.found.len(), 1);
    assert_eq!(scan.found[0].route, "alpha");
    assert!(scan.found[0].mind);
}

#[test]
fn a_workspace_links_and_lists_a_project_marked_by_its_settings() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    settings_project(&root, skills_beside());
    let skill = root.join("skills/deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: deploy\ndescription: Ship it.\n---\n",
    )
    .unwrap();
    // A rules folder the settings do not name: nothing of the project's.
    fs::create_dir_all(root.join("rules")).unwrap();
    fs::write(root.join("rules/style.md"), "Be brief.\n").unwrap();

    workspace.link(&MindProject::open(&root).unwrap()).unwrap();
    let (projects, failures) = workspace.projects();
    let catalog = Catalog::discover(&projects);

    assert!(failures.is_empty());
    let names: Vec<(Kind, &str)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.kind(), artifact.name()))
        .collect();
    assert_eq!(names, vec![(Kind::Skill, "deploy")]);
    assert!(catalog.failures().is_empty());
}

#[test]
fn adding_a_kind_the_settings_name_no_folder_for_is_refused_before_anything_is_made() {
    let dir = TempDir::new().unwrap();
    settings_project(dir.path(), skills_beside());
    let project = MindProject::open(dir.path()).unwrap();

    let error = create(&project, Kind::Rule, "style", "Be brief.").unwrap_err();

    assert!(
        matches!(
            error,
            CreateError::Workspace(WorkspaceError::NoDirectory {
                kind: Kind::Rule,
                ..
            })
        ),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "{}: keeps no rules of its own, since its marker names no folder for them",
            dir.path().display()
        )
    );
    assert!(!dir.path().join("rules").exists());
}
