//! Unit tests for `workspace.rs`, kept beside it rather than inside it.

use super::*;

use crate::agent::Agent;
use crate::settings::Share;

#[test]
fn templates_round_trip_through_the_parser() {
    let mind: MindConfig =
        toml::from_str(&mind_template("collapse", &Directories::default())).unwrap();
    // Not the newest a project's marker holds, which is a settings file's.
    assert_eq!(mind.version, MIND_TOML_VERSION);
    assert_eq!(mind.name, "collapse");
    // Spelled out rather than left to the default, so the file answers
    // where a project's skills go without anyone reading the source.
    assert_eq!(mind.directories.get(Kind::Skill), Some(Path::new("skills")));
    assert_eq!(mind.directories.get(Kind::Rule), Some(Path::new("rules")));

    let flayer: FlayerConfig =
        toml::from_str(&flayer_template("projects", &Directories::default())).unwrap();
    assert_eq!(flayer.version, FLAYER_FORMAT_VERSION);
    assert_eq!(flayer.name, "projects");
    assert!(flayer.projects.is_empty());
    // A workspace's own default is its library, not a project's.
    assert_eq!(
        flayer.directories.get(Kind::Skill),
        Some(Path::new(".mindflayer/library/skills"))
    );
    assert_eq!(
        flayer.directories.get(Kind::Rule),
        Some(Path::new(".mindflayer/library/rules"))
    );
}

#[test]
fn a_name_needing_quotes_still_round_trips() {
    let awkward = "quote\" and \\ backslash";
    let mind: MindConfig =
        toml::from_str(&mind_template(awkward, &Directories::default())).unwrap();
    assert_eq!(mind.name, awkward);
}

#[test]
fn a_name_holding_control_characters_still_round_trips() {
    // A basic string takes no control character as it is but a tab: each of
    // these, written unescaped, makes a marker the parser refuses.
    let awkward = "nul\u{0} esc\u{1b} cr\r del\u{7f} c1\u{85} nl\n tab\t";
    let mind: MindConfig =
        toml::from_str(&mind_template(awkward, &Directories::default())).unwrap();
    assert_eq!(mind.name, awkward);
    let flayer: FlayerConfig =
        toml::from_str(&flayer_template(awkward, &Directories::default())).unwrap();
    assert_eq!(flayer.name, awkward);
}

/// What `flayer init PATH` reaches with a typed argument: a directory whose
/// name holds a control character is made a workspace that opens again.
/// Unix only, as Windows takes no control character in a file name.
#[cfg(unix)]
#[test]
fn a_workspace_whose_folder_name_holds_a_control_character_opens_again() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("cr\r esc\u{1b}");
    FlayerWorkspace::init(&root).unwrap();
    assert_eq!(
        FlayerWorkspace::open(&root).unwrap().name(),
        "cr\r esc\u{1b}"
    );
}

/// Rewrite only the `version` line of the marker at `path`, TOML or JSON,
/// rather than writing a two-line stand-in, so what is read back is the
/// marker that was written with nothing changed but its version.
fn with_version(path: &Path, version: u32) {
    let text = fs::read_to_string(path).unwrap();
    let (prefix, line) = if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        ("  \"version\": ", format!("  \"version\": {version},"))
    } else {
        ("version = ", format!("version = {version}"))
    };
    assert!(
        text.lines().any(|line| line.starts_with(prefix)),
        "{} has no version line",
        path.display()
    );
    let rewritten: String = text
        .lines()
        .map(|written| {
            if written.starts_with(prefix) {
                format!("{line}\n")
            } else {
                format!("{written}\n")
            }
        })
        .collect();
    fs::write(path, rewritten).unwrap();
}

/// A project marked by settings, written at `root` as a new project's are.
fn settings_project(root: &Path, directories: Directories) -> PathBuf {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let path = mind.join(SETTINGS_FILE);
    let settings = Settings::new("both", Agent::ALL.to_vec(), Share::Local, directories);
    fs::write(&path, settings.text().unwrap()).unwrap();
    path
}

/// A directory that is both a mind project, marked by settings at the newest
/// version a project's marker has, and a flayer workspace, so each marker
/// can be made newer while the other is read beside it.
fn both_markers() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let mind = settings_project(dir.path(), Directories::default());
    FlayerWorkspace::init(dir.path()).unwrap();
    let flayer = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    (dir, mind, flayer)
}

#[test]
fn a_refusal_names_the_marker_that_is_newer() {
    let (dir, mind, flayer) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION + 1);
    with_version(&flayer, FLAYER_FORMAT_VERSION + 1);

    let project = MindProject::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            project,
            WorkspaceError::Version {
                marker: Marker::Project,
                ..
            }
        ),
        "{project}"
    );
    assert!(project.to_string().contains("project marker"), "{project}");

    let workspace = FlayerWorkspace::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            workspace,
            WorkspaceError::Version {
                marker: Marker::Workspace,
                ..
            }
        ),
        "{workspace}"
    );
    assert!(
        workspace.to_string().contains("workspace marker"),
        "{workspace}"
    );
}

// Written against each marker's own constant, so that a check holding one
// marker to the other's version fails: the second does, both ways round, now
// that a project's marker reads up to 3 and a workspace's up to 2.

#[test]
fn a_newer_project_marker_is_refused_while_a_current_workspace_marker_is_read() {
    let (dir, mind, flayer) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION + 1);
    with_version(&flayer, FLAYER_FORMAT_VERSION);

    let error = MindProject::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            error,
            WorkspaceError::Version { marker: Marker::Project, found, .. }
                if found == MIND_FORMAT_VERSION + 1
        ),
        "{error}"
    );
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    assert_eq!(workspace.config().version, FLAYER_FORMAT_VERSION);
}

#[test]
fn a_newer_workspace_marker_is_refused_while_a_current_project_marker_is_read() {
    let (dir, mind, flayer) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION);
    with_version(&flayer, FLAYER_FORMAT_VERSION + 1);

    let error = FlayerWorkspace::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            error,
            WorkspaceError::Version { marker: Marker::Workspace, found, .. }
                if found == FLAYER_FORMAT_VERSION + 1
        ),
        "{error}"
    );
    let project = MindProject::open(dir.path()).unwrap();
    assert_eq!(project.config().version(), MIND_FORMAT_VERSION);
}

#[test]
fn a_mind_toml_claiming_the_settings_version_is_refused_rather_than_guessed_at() {
    let dir = tempfile::TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    let mind = dir.path().join(MIND_DIR).join(MIND_CONFIG);
    with_version(&mind, SETTINGS_VERSION);

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(
        matches!(&error, WorkspaceError::Malformed { path, .. } if *path == mind),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("version 3 is written as settings.json"),
        "{error}"
    );
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

#[test]
fn a_replacement_builds_in_a_temporary_of_its_own() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("flayer.toml");
    fs::write(&file, "old").unwrap();
    // The one name every writer used to build in. Somebody's now, or another
    // writer's: either way not this one's to write into or take away.
    let shared = dir.path().join("flayer.toml.tmp");
    fs::write(&shared, "somebody's").unwrap();

    replace_file(&file, "new").unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    assert_eq!(fs::read_to_string(&shared).unwrap(), "somebody's");
    assert_eq!(names_in(dir.path()), ["flayer.toml", "flayer.toml.tmp"]);
}

#[cfg(unix)]
#[test]
fn an_unresolved_replacement_puts_a_file_where_a_link_was_and_leaves_its_target() {
    let dir = tempfile::TempDir::new().unwrap();
    let target = dir.path().join("elsewhere.json");
    fs::write(&target, "theirs").unwrap();
    let link = dir.path().join("settings.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    replace_bytes_unresolved(&link, b"new").unwrap();

    assert!(fs::symlink_metadata(&link).unwrap().is_file());
    assert_eq!(fs::read_to_string(&link).unwrap(), "new");
    assert_eq!(fs::read_to_string(&target).unwrap(), "theirs");
}

#[test]
fn a_replacement_that_cannot_be_put_in_place_leaves_no_temporary_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    // A folder where the file should be: the temporary is written whole, and
    // then no rename can put a file in its place.
    let file = dir.path().join("flayer.toml");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("inside"), "kept").unwrap();

    let (path, _) = replace_bytes(&file, b"new").unwrap_err();

    assert_eq!(path, fs::canonicalize(&file).unwrap());
    assert_eq!(names_in(dir.path()), ["flayer.toml"]);
    assert_eq!(fs::read_to_string(file.join("inside")).unwrap(), "kept");
}

/// What a version 1 build wrote, and what `init` writes as version 2.
const V1: &str = include_str!("../tests/fixtures/mind-v1.toml");
const V2: &str = include_str!("../tests/fixtures/mind-v2.toml");

/// A project at a new directory, marked by `marker` as its `mind.toml`.
fn toml_project(marker: &str) -> (tempfile::TempDir, MindProject) {
    let dir = tempfile::TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();
    fs::write(dir.path().join(MIND_DIR).join(MIND_CONFIG), marker).unwrap();
    let project = MindProject::open(dir.path()).unwrap();
    (dir, project)
}

#[test]
fn init_still_writes_the_version_2_marker_byte_for_byte() {
    assert_eq!(mind_template("collapse", &Directories::default()), V2);
}

#[test]
fn a_version_1_marker_still_keeps_every_kind_inside_mind() {
    let (dir, project) = toml_project(V1);

    assert_eq!(project.config().version(), 1);
    assert_eq!(project.settings(), None);
    for kind in Kind::ALL {
        assert_eq!(
            project.directory_for(kind),
            Some(dir.path().join(MIND_DIR).join(kind.folder())),
            "{kind}"
        );
    }
}

#[test]
fn a_version_2_marker_still_keeps_each_kind_where_it_says_or_beside_the_code() {
    let (dir, project) = toml_project(V2);
    assert_eq!(project.config().version(), 2);
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join("skills"))
    );
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("rules"))
    );

    // One that names a folder of its own, and leaves the other kind out.
    let named = V2.replace(
        "skills = \"skills\"\nrules = \"rules\"\n",
        "skills = \".claude/skills\"\n",
    );
    let (dir, project) = toml_project(&named);
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join(".claude/skills"))
    );
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("rules"))
    );
}

#[test]
fn a_project_marked_by_settings_without_directories_has_no_folder_for_any_kind() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = settings_project(dir.path(), Directories::default());
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace(",\n  \"directories\": {}", "")).unwrap();

    let project = MindProject::open(dir.path()).unwrap();

    assert!(!fs::read_to_string(&path).unwrap().contains("directories"));
    for kind in Kind::ALL {
        assert_eq!(project.directory_for(kind), None, "{kind}");
        let error = project.required_directory(kind).unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::NoDirectory { kind: refused, .. } if *refused == kind),
            "{error}"
        );
    }
}

#[test]
fn a_project_marked_by_settings_keeps_each_kind_it_names_where_it_says() {
    let dir = tempfile::TempDir::new().unwrap();
    settings_project(
        dir.path(),
        Directories::default().with(Kind::Skill, "docs/skills"),
    );

    let project = MindProject::open(dir.path()).unwrap();

    assert_eq!(project.config().version(), SETTINGS_VERSION);
    assert_eq!(project.name(), "both");
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join("docs/skills"))
    );
    assert_eq!(project.directory_for(Kind::Rule), None);
}
