//! A project marked by its settings, `.mind/settings.json`: found and read
//! as any project is, beside the ones a `mind.toml` still marks.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use mindflayer_core::{
    create, scan, Agent, AgentTarget, Catalog, CreateError, Directories, FlayerWorkspace,
    Initialization, Kind, MindProject, ProjectWarning, Settings, Share, WorkspaceError,
    MIND_CONFIG, MIND_DIR, MIND_IGNORE, SETTINGS_FILE,
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

#[test]
fn a_project_marked_by_its_settings_goes_where_the_table_sends_its_agents() {
    let dir = TempDir::new().unwrap();
    let claude = dir.path().join("claude");
    let (alone, _) =
        MindProject::init_settings(&claude, &[Agent::Claude], Share::Local, &skills_beside())
            .unwrap();
    settings_project(dir.path(), skills_beside());
    let every = MindProject::open(dir.path()).unwrap();

    assert_eq!(
        alone.targets(Kind::Skill),
        [AgentTarget {
            path: Path::new(".claude").join("skills"),
            agents: vec![Agent::Claude],
        }]
    );
    let paths: Vec<PathBuf> = every
        .targets(Kind::Skill)
        .into_iter()
        .map(|target| target.path)
        .collect();
    assert_eq!(
        paths,
        [
            Path::new(".agents").join("skills"),
            Path::new(".claude").join("skills")
        ]
    );
    assert_eq!(every.targets(Kind::Rule), []);
}

#[test]
fn a_project_a_mind_toml_marks_names_no_agents_to_go_to() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();

    for kind in Kind::ALL {
        assert_eq!(project.targets(kind), []);
    }
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
fn a_project_made_as_mind_init_makes_one_gives_each_kind_not_asked_for_its_default() {
    let dir = TempDir::new().unwrap();
    let asked = Directories::default().with(Kind::Skill, "docs/skills");

    let (project, _) =
        MindProject::init_asked(dir.path(), &Agent::ALL, Share::Local, &asked).unwrap();

    assert_eq!(
        project.settings().unwrap().directories,
        asked.with(Kind::Rule, "rules")
    );
    assert!(dir.path().join("docs").join("skills").is_dir());
    assert!(dir.path().join("rules").is_dir());
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
fn a_directory_inside_mind_is_refused_before_anything_is_made() {
    let dir = TempDir::new().unwrap();

    // Shared too, though git would see its `.mind`: a change may make it
    // local later.
    for share in [Share::Local, Share::Shared] {
        for inside in [
            ".mind",
            ".mind/skills",
            "skills/../.mind/skills",
            ".MIND/skills",
        ] {
            let directories = Directories::default().with(Kind::Skill, inside);
            let error = MindProject::init_settings(dir.path(), &Agent::ALL, share, &directories)
                .unwrap_err();

            assert!(
                matches!(
                    error,
                    WorkspaceError::InsideMind {
                        kind: Kind::Skill,
                        ..
                    }
                ),
                "{inside}: {error}"
            );
        }
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

/// Settings as somebody keeps them by hand: four spaces a level, the keys in
/// an order of their own, a key this build does not know among them and
/// another in `directories`, which is written on one line.
const BY_HAND: &str = r#"{
    "name": "collapse",
    "targets": {"skills": [".skills"]},
    "version": 3,
    "agents": ["claude"],
    "id": "0123456789abcdef0123456789abcdef",
    "share": "local",
    "directories": {"mcp": "mcp", "skills": "skills"}
}
"#;

/// A project at `root` marked by [`BY_HAND`], and where its settings are.
fn by_hand(root: &Path) -> (MindProject, PathBuf) {
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    let path = root.join(MIND_DIR).join(SETTINGS_FILE);
    fs::write(&path, BY_HAND).unwrap();
    (MindProject::open(root).unwrap(), path)
}

#[test]
fn an_edit_keeps_an_unknown_key_and_the_order_the_keys_are_in() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    project
        .change_settings(|settings| settings.agents = vec![Agent::Claude, Agent::Codex])
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        BY_HAND.replace(
            r#""agents": ["claude"]"#,
            r#""agents": ["claude", "codex"]"#
        )
    );
    assert_eq!(
        project.settings().unwrap().agents,
        [Agent::Claude, Agent::Codex]
    );
}

#[test]
fn an_edit_that_changes_nothing_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    let agents = project
        .change_settings(|settings| {
            settings.share = Share::Local;
            settings.agents.clone()
        })
        .unwrap();

    assert_eq!(agents, [Agent::Claude]);
    assert_eq!(fs::read_to_string(&path).unwrap(), BY_HAND);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), long_ago);
}

#[test]
fn an_edit_adds_a_key_the_file_left_out_only_when_it_has_something_to_say() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    // Left out, `edit_tracked` reads as off: renaming is no reason to say so.
    project
        .change_settings(|settings| settings.name = "renamed".to_owned())
        .unwrap();
    let renamed = BY_HAND.replace(r#""name": "collapse""#, r#""name": "renamed""#);
    assert_eq!(fs::read_to_string(&path).unwrap(), renamed);

    project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        renamed.replace(
            "\"skills\": \"skills\"}\n}",
            "\"skills\": \"skills\"},\n    \"edit_tracked\": true\n}"
        )
    );
    assert!(project.settings().unwrap().edit_tracked);
}

#[test]
fn an_edit_changes_directories_an_entry_at_a_time() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    project
        .change_settings(|settings| {
            settings.directories = settings
                .directories
                .clone()
                .without(Kind::Skill)
                .with(Kind::Rule, "docs/rules");
        })
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        BY_HAND.replace(
            r#""directories": {"mcp": "mcp", "skills": "skills"}"#,
            r#""directories": {"mcp": "mcp", "rules": "docs/rules"}"#
        )
    );
    assert_eq!(project.directory_for(Kind::Skill), None);
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("docs/rules"))
    );
}

#[test]
fn an_edit_names_a_folder_in_settings_that_named_none() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());
    let without = BY_HAND.replace(
        ",\n    \"directories\": {\"mcp\": \"mcp\", \"skills\": \"skills\"}",
        "",
    );
    fs::write(&path, &without).unwrap();

    project
        .change_settings(|settings| {
            settings.directories = Directories::default().with(Kind::Skill, ".claude/skills");
        })
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        without.replace(
            "\"share\": \"local\"\n}",
            "\"share\": \"local\",\n    \"directories\": {\n        \"skills\": \".claude/skills\"\n    }\n}"
        )
    );
}

#[test]
fn an_edit_of_the_version_or_the_id_is_refused_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    let version = project
        .change_settings(|settings| settings.version = 4)
        .unwrap_err();
    let id = project
        .change_settings(|settings| settings.id = mindflayer_core::ProjectId::generate())
        .unwrap_err();

    for error in [version, id] {
        assert!(matches!(error, WorkspaceError::Edit { .. }), "{error}");
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), BY_HAND);
}

#[test]
fn a_project_a_mind_toml_marks_has_no_settings_to_change() {
    let dir = TempDir::new().unwrap();
    let (mut project, _) = MindProject::init(dir.path()).unwrap();

    let error = project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap_err();

    assert!(
        matches!(error, WorkspaceError::NoSettings { .. }),
        "{error}"
    );
    // Sent to what gives it settings, since `mind init` leaves its marker.
    assert!(error.to_string().contains("`mind migrate`"), "{error}");
    assert!(!dir.path().join(MIND_DIR).join(SETTINGS_FILE).exists());
}

#[test]
fn settings_somebody_broke_since_they_were_read_are_refused_and_left_as_they_are() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());
    let broken = BY_HAND.replace("\"version\": 3,", "\"version\": 3, // three");
    fs::write(&path, &broken).unwrap();

    let error = project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::Malformed { .. }), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), broken);
}

#[test]
fn an_edit_leaves_nothing_else_beside_the_settings() {
    let dir = TempDir::new().unwrap();
    let (mut project, _) = by_hand(dir.path());

    project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();

    assert_eq!(names_in(&dir.path().join(MIND_DIR)), ["settings.json"]);
    assert_eq!(project.settings().unwrap().share, Share::Shared);
}

#[test]
fn an_edit_that_moves_a_folder_out_of_the_project_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    for outside in ["/etc/skills", "../elsewhere", "."] {
        let error = project
            .change_settings(|settings| {
                settings.directories = settings.directories.clone().with(Kind::Skill, outside);
            })
            .unwrap_err();

        assert!(
            matches!(
                error,
                WorkspaceError::OutsideProject {
                    kind: Kind::Skill,
                    ..
                }
            ),
            "{outside}: {error}"
        );
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), BY_HAND);
}

#[test]
fn an_edit_that_moves_a_folder_into_mind_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = by_hand(dir.path());

    let error = project
        .change_settings(|settings| {
            settings.directories = settings.directories.clone().with(Kind::Rule, ".mind/rules");
        })
        .unwrap_err();

    assert!(
        matches!(
            error,
            WorkspaceError::InsideMind {
                kind: Kind::Rule,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), BY_HAND);
}

#[test]
fn a_folder_written_outside_by_hand_does_not_stop_a_change_to_something_else() {
    let dir = TempDir::new().unwrap();
    let (_, path) = by_hand(dir.path());
    let outside = BY_HAND.replace("\"skills\": \"skills\"", "\"skills\": \"../shared\"");
    fs::write(&path, &outside).unwrap();
    let mut project = MindProject::open(dir.path()).unwrap();

    project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        outside.replace("\"share\": \"local\"", "\"share\": \"shared\"")
    );
}
