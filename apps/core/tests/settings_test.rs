//! A project marked by its settings, `.mind/settings.toml`, or the
//! `.mind/settings.json` before it: found and read as any project is, beside
//! the ones a `mind.toml` still marks.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use mindflayer_core::{
    create, scan, Agent, AgentTarget, Catalog, CreateError, Directories, FlayerWorkspace,
    Initialization, Kind, MarkerFile, MindProject, Misplaced, MisplacedTarget, ProjectWarning,
    Settings, Share, Targets, WorkspaceError, MIND_CONFIG, MIND_DIR, MIND_IGNORE, SETTINGS_FILE,
    SETTINGS_JSON_FILE, SETTINGS_TOML_VERSION, SETTINGS_VERSION, TARGETS_VERSION,
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
            path: leftover.clone(),
            read: MarkerFile::SettingsToml,
        }]
    );
    assert_eq!(
        warnings[0].to_string(),
        format!(
            "{}: ignored, since the project is read from the settings.toml beside it",
            leftover.display()
        )
    );
}

/// Mark `root` a project the way a version 3 `mind init` did, by a
/// `settings.json`, written here as it wrote it, and as version 4 when
/// `targets` (a JSON object) is given.
fn json_project(root: &Path, targets: Option<&str>) -> PathBuf {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let path = mind.join(SETTINGS_JSON_FILE);
    let (version, targets) = match targets {
        Some(targets) => (4, format!(",\n  \"targets\": {targets}")),
        None => (3, String::new()),
    };
    fs::write(
        &path,
        format!(
            "{{\n  \"version\": {version},\n  \"id\": \"0123456789abcdef0123456789abcdef\",\n  \
             \"name\": \"collapse\",\n  \"agents\": [\"claude\", \"codex\"],\n  \
             \"share\": \"local\",\n  \"edit_tracked\": false,\n  \"directories\": {{\n    \
             \"skills\": \"skills\",\n    \"rules\": \"rules\"\n  }}{targets}\n}}\n"
        ),
    )
    .unwrap();
    path
}

/// Mark `root` a project by a `settings.toml` holding `settings`.
fn toml_project(root: &Path, settings: &Settings) -> PathBuf {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let path = mind.join(SETTINGS_FILE);
    fs::write(&path, settings.text().unwrap()).unwrap();
    path
}

#[test]
fn a_settings_json_of_version_3_or_4_is_read_as_it_was_written() {
    let three = TempDir::new().unwrap();
    json_project(three.path(), None);
    let four = TempDir::new().unwrap();
    json_project(four.path(), Some(r#"{"skills": [".skills"]}"#));

    let three = MindProject::open(three.path()).unwrap();
    let four = MindProject::open(four.path()).unwrap();

    for (project, version) in [(&three, SETTINGS_VERSION), (&four, TARGETS_VERSION)] {
        let settings = project.settings().unwrap();
        assert_eq!(settings.version, version);
        assert_eq!(settings.id.as_str(), "0123456789abcdef0123456789abcdef");
        assert_eq!(settings.agents, [Agent::Claude, Agent::Codex]);
        assert_eq!(settings.share, Share::Local);
        assert_eq!(project.marker_file(), MarkerFile::SettingsJson);
        assert_eq!(
            project.marker_path(),
            project.mind_dir().join(SETTINGS_JSON_FILE)
        );
        assert_eq!(
            project.directory_for(Kind::Skill),
            Some(project.root().join("skills"))
        );
    }
    assert!(three.settings().unwrap().targets.is_empty());
    assert_eq!(
        four.settings().unwrap().targets.get(Kind::Skill),
        Some(&[PathBuf::from(".skills")][..])
    );
}

#[test]
fn a_project_holding_a_settings_toml_is_read_from_it_and_warns_of_each_older_marker() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    json_project(dir.path(), None);
    let settings = Settings::new(
        "renamed",
        vec![Agent::OpenCode],
        Share::Shared,
        Directories::default(),
    );
    toml_project(dir.path(), &settings);
    let mind = dir.path().join(MIND_DIR);

    let project = MindProject::open(dir.path()).unwrap();

    assert_eq!(project.marker_file(), MarkerFile::SettingsToml);
    let read = project.settings().unwrap();
    assert_eq!(read.version, SETTINGS_TOML_VERSION);
    assert_eq!(read.name, "renamed");
    assert_eq!(read.agents, [Agent::OpenCode]);
    assert_eq!(project.directory_for(Kind::Skill), None);
    let warnings: Vec<String> = project.warnings().iter().map(ToString::to_string).collect();
    assert_eq!(
        warnings,
        [
            format!(
                "{}: ignored, since the project is read from the settings.toml beside it",
                mind.join(SETTINGS_JSON_FILE).display()
            ),
            format!(
                "{}: ignored, since the project is read from the settings.toml beside it",
                mind.join(MIND_CONFIG).display()
            ),
        ]
    );
}

/// What a build from before #29 leaves: its `mind init` asks only whether a
/// `mind.toml` is there, so it writes one beside a `settings.json`.
#[test]
fn a_project_holding_a_settings_json_and_a_mind_toml_is_read_from_the_json_and_warns_of_the_other()
{
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    json_project(dir.path(), None);
    let leftover = dir.path().join(MIND_DIR).join(MIND_CONFIG);

    let project = MindProject::open(dir.path()).unwrap();

    // Read from the settings.json: a mind.toml names no agents.
    assert_eq!(project.marker_file(), MarkerFile::SettingsJson);
    let read = project.settings().unwrap();
    assert_eq!(read.version, SETTINGS_VERSION);
    assert_eq!(read.agents, [Agent::Claude, Agent::Codex]);
    let warnings = project.warnings();
    assert_eq!(
        warnings,
        vec![ProjectWarning::Leftover {
            path: leftover.clone(),
            read: MarkerFile::SettingsJson,
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
fn a_project_marked_by_a_settings_toml_alone_is_found_and_says_nothing_else() {
    let dir = TempDir::new().unwrap();
    let settings = Settings::new(
        "collapse",
        Agent::ALL.to_vec(),
        Share::Local,
        skills_beside(),
    );
    toml_project(dir.path(), &settings);
    let deep = dir.path().join("src");
    fs::create_dir_all(&deep).unwrap();

    assert!(MindProject::is_marked(dir.path()));
    let found = MindProject::locate(&deep)
        .unwrap()
        .expect("a project above");

    assert_eq!(found.root(), dir.path());
    assert_eq!(found.marker_file(), MarkerFile::SettingsToml);
    assert!(found.warnings().is_empty());
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

    let scan = scan(&workspace, &scan::Request::default());

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

/// A project at `root` whose settings, written by hand, install skills in
/// `folders`, for Claude and Codex.
fn targeting(root: &Path, folders: &[&str]) -> MindProject {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let listed: Vec<String> = folders.iter().map(|folder| format!("{folder:?}")).collect();
    fs::write(
        mind.join(SETTINGS_FILE),
        format!(
            "version = 5\nid = \"0123456789abcdef0123456789abcdef\"\nname = \"collapse\"\n\
             agents = [\"claude\", \"codex\"]\nshare = \"local\"\n\n[targets]\nskills = [{}]\n",
            listed.join(", ")
        ),
    )
    .unwrap();
    MindProject::open(root).unwrap()
}

#[test]
fn targets_in_the_settings_send_a_kind_there_for_every_agent_in_place_of_their_folders() {
    let dir = TempDir::new().unwrap();

    let project = targeting(dir.path(), &["./.skills/", "tools/skills"]);

    assert_eq!(
        project.targets(Kind::Skill),
        [
            AgentTarget {
                path: PathBuf::from(".skills"),
                agents: vec![Agent::Claude, Agent::Codex],
            },
            AgentTarget {
                path: Path::new("tools").join("skills"),
                agents: vec![Agent::Claude, Agent::Codex],
            },
        ]
    );
    assert_eq!(project.targets(Kind::Rule), []);
    assert!(project.misplaced_targets().is_empty());
}

#[test]
fn a_target_outside_the_project_inside_mind_or_absolute_is_passed_over_and_said() {
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let absolute = elsewhere.path().join("skills");
    let absolute = absolute.to_str().unwrap();

    let project = targeting(
        dir.path(),
        &[
            ".skills",
            "../shared",
            ".mind/skills",
            absolute,
            ".",
            "nested/.mindflayer/skills",
            ".git",
            "vendor/lib/.GIT/skills",
        ],
    );

    assert_eq!(
        project.targets(Kind::Skill),
        [AgentTarget {
            path: PathBuf::from(".skills"),
            agents: vec![Agent::Claude, Agent::Codex],
        }]
    );
    let settings = dir.path().join(MIND_DIR).join(SETTINGS_FILE);
    let misplaced = |folder: &str, why| MisplacedTarget {
        settings: settings.clone(),
        kind: Kind::Skill,
        folder: PathBuf::from(folder),
        why,
    };
    let said = project.misplaced_targets();
    assert_eq!(
        said,
        [
            misplaced("../shared", Misplaced::Outside),
            misplaced(".mind/skills", Misplaced::InsideMind),
            misplaced(absolute, Misplaced::Outside),
            misplaced(".", Misplaced::Outside),
            misplaced("nested/.mindflayer/skills", Misplaced::InsideWorkspace),
            misplaced(".git", Misplaced::InsideGit),
            misplaced("vendor/lib/.GIT/skills", Misplaced::InsideGit),
        ]
    );
    assert_eq!(
        said[0].to_string(),
        format!(
            "{}: ../shared is no folder to install skills in, because it is not inside the \
             project, so nothing is installed there",
            settings.display()
        )
    );
    assert!(
        said[1]
            .to_string()
            .contains("because it is inside .mind, which holds Mindflayer's own files"),
        "{}",
        said[1]
    );
    assert!(
        said[5]
            .to_string()
            .contains("because it is inside .git, which holds a repository's own files"),
        "{}",
        said[5]
    );
}

#[test]
fn a_kind_every_target_of_which_is_passed_over_goes_nowhere() {
    let dir = TempDir::new().unwrap();

    let project = targeting(dir.path(), &["../shared"]);

    assert_eq!(project.targets(Kind::Skill), []);
}

#[test]
fn an_edit_sending_a_kind_where_no_install_may_go_is_refused_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    for (kind, folders, why) in [
        (
            Kind::Skill,
            vec![".skills", "../shared"],
            Misplaced::Outside,
        ),
        (Kind::Skill, vec![".mind/skills"], Misplaced::InsideMind),
        (Kind::Skill, vec![".git"], Misplaced::InsideGit),
        (
            Kind::Skill,
            vec![elsewhere.path().to_str().unwrap()],
            Misplaced::Outside,
        ),
        (Kind::Rule, vec![".rules"], Misplaced::NotInstalled),
    ] {
        let folders: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
        let error = project
            .change_settings(|settings| {
                settings.targets = Targets::default().with(kind, folders.clone());
            })
            .unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::Target { kind: refused, why: said, .. }
                if *refused == kind && *said == why),
            "{error}"
        );
    }
    let none = project
        .change_settings(|settings| {
            settings.targets = Targets::default().with(Kind::Skill, Vec::new());
        })
        .unwrap_err();
    assert!(matches!(none, WorkspaceError::Edit { .. }), "{none}");
    assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);

    let outside = project
        .change_settings(|settings| {
            settings.targets =
                Targets::default().with(Kind::Skill, vec![PathBuf::from("../shared")]);
        })
        .unwrap_err();
    assert_eq!(
        outside.to_string(),
        "../shared: cannot be where a project's skills are installed, because it is not inside \
         the project"
    );
}

#[test]
fn a_target_written_outside_by_hand_does_not_stop_a_change_to_something_else() {
    let dir = TempDir::new().unwrap();
    let mut project = targeting(dir.path(), &["../shared"]);

    project
        .change_settings(|settings| settings.agents = vec![Agent::Codex])
        .unwrap();

    assert_eq!(project.settings().unwrap().agents, [Agent::Codex]);
    assert_eq!(project.misplaced_targets().len(), 1);
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
    assert_eq!(names_in(&mind), [".gitignore", "settings.toml"]);
    let text = fs::read_to_string(mind.join(SETTINGS_FILE)).unwrap();
    assert_eq!(text, project.settings().unwrap().text().unwrap());
    let id = project.settings().unwrap().id.to_string();
    assert!(
        text.ends_with(&format!(
            "# this file is laid out: change neither.\n\
             version = 5\n\
             id = \"{id}\"\n\
             name = \"collapse\"\n\
             agents = [\"claude\", \"codex\", \"opencode\"]\n\
             share = \"local\"\n\
             edit_tracked = false\n\
             \n\
             [directories]\n\
             skills = \"skills\"\n\
             rules = \"rules\"\n"
        )),
        "{text}"
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

    assert_eq!(names_in(&dir.path().join(MIND_DIR)), ["settings.toml"]);
    let text = fs::read_to_string(dir.path().join(MIND_DIR).join(SETTINGS_FILE)).unwrap();
    assert!(text.contains("\nshare = \"shared\"\n"), "{text}");
    assert!(text.contains("\nagents = [\"claude\"]\n"), "{text}");
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

    assert!(ignored(dir.path(), ".mind/settings.toml"));
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

    assert!(!ignored(dir.path(), ".mind/settings.toml"));
}

// ---------------------------------------------------------------------------
// Changing a settings.toml
// ---------------------------------------------------------------------------

/// A `settings.toml` somebody laid out by hand: comments of their own, a key
/// this build does not know, and the keys in an order of their own.
const TOML_BY_HAND: &str = r#"# Somebody's own notes, kept by every edit.
name = "collapse"
servers = { github = ["npx"] }
version = 5
agents = ["claude"] # who works here
id = "0123456789abcdef0123456789abcdef"

# the team's call
share = "local"

[directories]
mcp = "mcp"
skills = "skills"
"#;

/// A project at `root` marked by [`TOML_BY_HAND`], and where its settings
/// are.
fn toml_by_hand(root: &Path) -> (MindProject, PathBuf) {
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    let path = root.join(MIND_DIR).join(SETTINGS_FILE);
    fs::write(&path, TOML_BY_HAND).unwrap();
    (MindProject::open(root).unwrap(), path)
}

/// `path`'s modification time, set long ago, so a write would show.
fn aged(path: &Path) -> SystemTime {
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    long_ago
}

#[test]
fn a_change_of_agents_in_toml_keeps_every_comment_an_unknown_key_and_the_key_order() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    project
        .change_settings(|settings| settings.agents = vec![Agent::Claude, Agent::Codex])
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        TOML_BY_HAND.replace(
            r#"agents = ["claude"] # who works here"#,
            r#"agents = ["claude", "codex"] # who works here"#
        )
    );
    assert_eq!(
        project.settings().unwrap().agents,
        [Agent::Claude, Agent::Codex]
    );
}

#[test]
fn a_change_of_share_mode_in_toml_keeps_the_comment_above_it() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        TOML_BY_HAND.replace(
            "# the team's call\nshare = \"local\"",
            "# the team's call\nshare = \"shared\""
        )
    );
}

#[test]
fn a_change_of_targets_in_toml_adds_a_table_last_at_version_5_and_a_reset_takes_it_out() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    project
        .change_settings(|settings| {
            settings.targets = Targets::default().with(Kind::Skill, vec![PathBuf::from(".skills")]);
        })
        .unwrap();

    let targeted = format!("{TOML_BY_HAND}\n[targets]\nskills = [\".skills\"]\n");
    assert_eq!(fs::read_to_string(&path).unwrap(), targeted);
    assert_eq!(project.settings().unwrap().version, SETTINGS_TOML_VERSION);

    project
        .change_settings(|settings| settings.targets = Targets::default())
        .unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);
    assert_eq!(project.settings().unwrap().version, SETTINGS_TOML_VERSION);
}

#[test]
fn a_reset_of_targets_keeps_a_comment_written_above_them() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());
    let commented = format!(
        "{TOML_BY_HAND}\n# one folder for every agent\n[targets]\nskills = [\".skills\"]\n"
    );
    fs::write(&path, &commented).unwrap();

    project
        .change_settings(|settings| settings.targets = Targets::default())
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{TOML_BY_HAND}\n# one folder for every agent\n[targets]\n")
    );
    assert!(project.settings().unwrap().targets.is_empty());
}

#[test]
fn a_change_in_toml_that_changes_nothing_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());
    let long_ago = aged(&path);

    let agents = project
        .change_settings(|settings| {
            settings.share = Share::Local;
            settings.targets = Targets::default();
            settings.agents.clone()
        })
        .unwrap();

    assert_eq!(agents, [Agent::Claude]);
    assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), long_ago);
}

#[test]
fn a_change_in_toml_adds_a_key_the_file_left_out_only_when_it_has_something_to_say() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    project
        .change_settings(|settings| settings.name = "renamed".to_owned())
        .unwrap();
    let renamed = TOML_BY_HAND.replace(r#"name = "collapse""#, r#"name = "renamed""#);
    assert_eq!(fs::read_to_string(&path).unwrap(), renamed);

    project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        renamed.replace(
            "share = \"local\"\n",
            "share = \"local\"\nedit_tracked = true\n"
        )
    );
    assert!(project.settings().unwrap().edit_tracked);
}

#[test]
fn a_change_in_toml_changes_directories_an_entry_at_a_time() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

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
        TOML_BY_HAND.replace(
            "mcp = \"mcp\"\nskills = \"skills\"\n",
            "mcp = \"mcp\"\nrules = \"docs/rules\"\n"
        )
    );
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("docs/rules"))
    );
}

#[test]
fn a_settings_toml_written_new_and_one_changed_since_read_alike() {
    let dir = TempDir::new().unwrap();
    let settings = Settings::new("collapse", Agent::ALL.to_vec(), Share::Local, both_beside());
    let path = toml_project(dir.path(), &settings);
    let mut project = MindProject::open(dir.path()).unwrap();

    project
        .change_settings(|settings| {
            settings.agents = vec![Agent::Claude];
            settings.share = Share::Shared;
            settings.edit_tracked = true;
            settings.targets = Targets::default().with(Kind::Skill, vec![PathBuf::from(".skills")]);
        })
        .unwrap();

    let changed = project.settings().unwrap();
    assert_eq!(changed.agents, [Agent::Claude]);
    assert_eq!(fs::read_to_string(&path).unwrap(), changed.text().unwrap());
}

#[test]
fn a_change_in_toml_of_the_version_or_the_id_is_refused_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

    for change in [
        (|settings: &mut Settings| settings.version = TARGETS_VERSION) as fn(&mut Settings),
        |settings: &mut Settings| settings.id = mindflayer_core::ProjectId::generate(),
    ] {
        let error = project.change_settings(change).unwrap_err();

        assert!(matches!(error, WorkspaceError::Edit { .. }), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);
    }
}

#[test]
fn a_change_in_toml_names_a_folder_in_settings_that_named_none() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());
    let start = TOML_BY_HAND.find("\n[directories]").unwrap();
    let without = &TOML_BY_HAND[..start];
    fs::write(&path, without).unwrap();

    project
        .change_settings(|settings| {
            settings.directories = Directories::default().with(Kind::Skill, ".claude/skills");
        })
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{without}\n[directories]\nskills = \".claude/skills\"\n")
    );
}

#[test]
fn a_change_of_something_else_leaves_targets_written_by_hand() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());
    let hand = TOML_BY_HAND.replace(
        "[directories]",
        "[targets] # one for all\nskills = [ \".skills\" ]\n\n[directories]",
    );
    fs::write(&path, &hand).unwrap();

    project
        .change_settings(|settings| settings.agents = vec![Agent::Codex])
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        hand.replace(r#"agents = ["claude"]"#, r#"agents = ["codex"]"#)
    );
}

#[test]
fn a_project_a_mind_toml_marks_has_no_settings_to_change() {
    let dir = TempDir::new().unwrap();
    let (mut project, _) = MindProject::init(dir.path()).unwrap();

    let error = project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap_err();

    assert!(
        matches!(
            error,
            WorkspaceError::NoSettings {
                marker: MarkerFile::MindToml,
                ..
            }
        ),
        "{error}"
    );
    // Sent to what gives it settings, since `mind init` leaves its marker.
    assert!(error.to_string().contains("`mind migrate`"), "{error}");
    assert!(!dir.path().join(MIND_DIR).join(SETTINGS_FILE).exists());
}

#[test]
fn a_settings_json_is_changed_by_nothing_but_its_move_to_a_settings_toml() {
    let dir = TempDir::new().unwrap();
    let path = json_project(dir.path(), None);
    let written = fs::read_to_string(&path).unwrap();
    let mut project = MindProject::open(dir.path()).unwrap();

    let error = project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap_err();

    assert!(
        matches!(
            error,
            WorkspaceError::NoSettings {
                marker: MarkerFile::SettingsJson,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "{}: has no settings.toml to change, since a settings.json marks it; \
             `mind migrate` moves it to one",
            dir.path().display()
        )
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), written);
    assert!(!dir.path().join(MIND_DIR).join(SETTINGS_FILE).exists());
}

#[test]
fn settings_somebody_broke_since_they_were_read_are_refused_and_left_as_they_are() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());
    let broken = TOML_BY_HAND.replace("version = 5\n", "version = 5,\n");
    fs::write(&path, &broken).unwrap();

    let error = project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::Malformed { .. }), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), broken);
}

#[test]
fn a_change_leaves_nothing_else_beside_the_settings() {
    let dir = TempDir::new().unwrap();
    let (mut project, _) = toml_by_hand(dir.path());

    project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();

    assert_eq!(names_in(&dir.path().join(MIND_DIR)), ["settings.toml"]);
    assert_eq!(project.settings().unwrap().share, Share::Shared);
}

#[test]
fn a_change_that_moves_a_folder_out_of_the_project_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

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
    assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);
}

#[test]
fn a_change_that_moves_a_folder_into_mind_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut project, path) = toml_by_hand(dir.path());

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
    assert_eq!(fs::read_to_string(&path).unwrap(), TOML_BY_HAND);
}

#[test]
fn a_folder_written_outside_by_hand_does_not_stop_a_change_to_something_else() {
    let dir = TempDir::new().unwrap();
    let (_, path) = toml_by_hand(dir.path());
    let outside = TOML_BY_HAND.replace("skills = \"skills\"", "skills = \"../shared\"");
    fs::write(&path, &outside).unwrap();
    let mut project = MindProject::open(dir.path()).unwrap();

    project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        outside.replace("share = \"local\"", "share = \"shared\"")
    );
}
