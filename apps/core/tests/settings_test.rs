//! A project marked by its settings, `.mind/settings.json`: found and read
//! as any project is, beside the ones a `mind.toml` still marks.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::{
    create, scan, Agent, Catalog, CreateError, Directories, FlayerWorkspace, Initialization, Kind,
    MindProject, ProjectWarning, Settings, Share, WorkspaceError, MIND_CONFIG, MIND_DIR,
    MIND_IGNORE, SETTINGS_FILE,
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

/// Every kind beside the code, as a new project names them by default.
fn both_beside() -> Directories {
    Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules")
}

/// The names in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Whether gix holds `path`, relative to the work tree, to be ignored. A
/// trailing `/` asks about a directory. Opened isolated, so nothing from the
/// machine's own git config takes part.
fn ignored(work_tree: &Path, path: &str) -> bool {
    let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
    let index = repository.index_or_empty().unwrap();
    let mut excludes = repository
        .excludes(&index, None, Default::default())
        .unwrap();
    let (path, mode) = match path.strip_suffix('/') {
        Some(directory) => (directory, Some(gix::index::entry::Mode::DIR)),
        None => (path, None),
    };
    excludes.at_path(path, mode).unwrap().is_excluded()
}

#[test]
fn a_new_project_writes_exactly_its_settings_and_a_gitignore() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();

    let (project, outcome) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &both_beside()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    let mind = root.join(MIND_DIR);
    assert_eq!(names_in(&mind), [".gitignore", "settings.json"]);
    let id = project.settings().unwrap().id.to_string();
    assert_eq!(
        fs::read_to_string(mind.join(SETTINGS_FILE)).unwrap(),
        format!(
            r#"{{
  "version": 3,
  "id": "{id}",
  "name": "collapse",
  "agents": ["claude", "codex", "opencode"],
  "share": "local",
  "edit_tracked": false,
  "directories": {{
    "skills": "skills",
    "rules": "rules"
  }}
}}
"#
        )
    );
    assert_eq!(
        fs::read_to_string(mind.join(".gitignore")).unwrap(),
        "# Mindflayer keeps this project's settings here, for this checkout alone.\n\
         # Nothing in this folder is meant to be committed.\n\
         *\n"
    );
    assert_eq!(names_in(&root), [".mind", "rules", "skills"]);
}

#[test]
fn two_new_projects_have_ids_of_their_own() {
    let (one, other) = (TempDir::new().unwrap(), TempDir::new().unwrap());

    let (one, _) =
        MindProject::init_settings(one.path(), &Agent::ALL, Share::Local, &both_beside()).unwrap();
    let (other, _) =
        MindProject::init_settings(other.path(), &Agent::ALL, Share::Local, &both_beside())
            .unwrap();

    assert_ne!(one.settings().unwrap().id, other.settings().unwrap().id);
}

#[test]
fn a_shared_project_writes_no_gitignore() {
    let dir = TempDir::new().unwrap();

    MindProject::init_settings(dir.path(), &[Agent::Claude], Share::Shared, &both_beside())
        .unwrap();

    assert_eq!(names_in(&dir.path().join(MIND_DIR)), ["settings.json"]);
    let text = fs::read_to_string(dir.path().join(MIND_DIR).join(SETTINGS_FILE)).unwrap();
    assert!(text.contains("  \"share\": \"shared\",\n"), "{text}");
    assert!(text.contains("  \"agents\": [\"claude\"],\n"), "{text}");
}

#[test]
fn a_new_project_naming_no_directories_makes_no_folder() {
    let dir = TempDir::new().unwrap();

    let (project, _) = MindProject::init_settings(
        dir.path(),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .unwrap();

    assert_eq!(names_in(dir.path()), [".mind"]);
    for kind in Kind::ALL {
        assert_eq!(project.directory_for(kind), None, "{kind}");
    }
}

#[test]
fn a_gitignore_somebody_wrote_is_left_as_they_left_it() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();
    fs::write(dir.path().join(MIND_DIR).join(".gitignore"), "mine\n").unwrap();

    MindProject::init_settings(dir.path(), &Agent::ALL, Share::Local, &both_beside()).unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join(MIND_DIR).join(".gitignore")).unwrap(),
        "mine\n"
    );
    assert!(dir.path().join(MIND_DIR).join(SETTINGS_FILE).is_file());
}

#[test]
fn initializing_twice_keeps_the_settings_and_makes_what_they_ask_for_again() {
    let dir = TempDir::new().unwrap();
    let (first, _) =
        MindProject::init_settings(dir.path(), &Agent::ALL, Share::Local, &both_beside()).unwrap();
    let settings = dir.path().join(MIND_DIR).join(SETTINGS_FILE);
    let written = fs::read_to_string(&settings).unwrap();
    fs::remove_file(dir.path().join(MIND_DIR).join(".gitignore")).unwrap();
    fs::remove_dir(dir.path().join("skills")).unwrap();

    let (second, outcome) = MindProject::init_settings(
        dir.path(),
        &[Agent::Codex],
        Share::Shared,
        &Directories::default().with(Kind::Skill, "elsewhere"),
    )
    .unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert_eq!(fs::read_to_string(&settings).unwrap(), written);
    assert_eq!(second.settings(), first.settings());
    // The marker says local, whatever this call said.
    assert_eq!(
        fs::read_to_string(dir.path().join(MIND_DIR).join(".gitignore")).unwrap(),
        MIND_IGNORE
    );
    assert!(dir.path().join("skills").is_dir());
    assert!(!dir.path().join("elsewhere").exists());
}

#[test]
fn a_project_a_mind_toml_marks_is_not_given_settings() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let (project, outcome) =
        MindProject::init_settings(dir.path(), &Agent::ALL, Share::Local, &both_beside()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert_eq!(project.settings(), None);
    assert_eq!(names_in(&dir.path().join(MIND_DIR)), ["mind.toml"]);
}

#[test]
fn a_directory_outside_the_project_is_refused_before_anything_is_made() {
    let dir = TempDir::new().unwrap();

    for outside in ["/etc/skills", "../elsewhere", ".", ""] {
        let directories = Directories::default().with(Kind::Skill, outside);
        let error = MindProject::init_settings(dir.path(), &Agent::ALL, Share::Local, &directories)
            .unwrap_err();

        assert!(
            matches!(error, WorkspaceError::OutsideProject { .. }),
            "{outside}: {error}"
        );
    }
    assert!(!dir.path().join(MIND_DIR).exists());
}

#[test]
fn git_ignores_everything_in_a_local_projects_mind() {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();

    MindProject::init_settings(dir.path(), &Agent::ALL, Share::Local, &both_beside()).unwrap();

    assert!(ignored(dir.path(), ".mind/settings.json"));
    assert!(ignored(dir.path(), ".mind/.gitignore"));
    // What the project authors is its own to commit.
    assert!(!ignored(dir.path(), "skills/"));
    assert!(!ignored(dir.path(), "skills/deploy/SKILL.md"));
}

#[test]
fn git_sees_a_shared_projects_settings() {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();

    MindProject::init_settings(dir.path(), &Agent::ALL, Share::Shared, &both_beside()).unwrap();

    assert!(!ignored(dir.path(), ".mind/settings.json"));
}
