//! Unit tests for `workspace.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn templates_round_trip_through_the_parser() {
    let mind: MindConfig =
        toml::from_str(&mind_template("collapse", &Directories::default())).unwrap();
    assert_eq!(mind.version, MIND_FORMAT_VERSION);
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

/// Rewrite only the `version` line of the marker at `path`, rather than
/// writing a two-line stand-in, so what is read back is the marker `init`
/// wrote with nothing changed but its version.
fn with_version(path: &Path, version: u32) {
    let text = fs::read_to_string(path).unwrap();
    assert!(
        text.lines().any(|line| line.starts_with("version = ")),
        "{} has no version line",
        path.display()
    );
    let rewritten: String = text
        .lines()
        .map(|line| {
            if line.starts_with("version = ") {
                format!("version = {version}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(path, rewritten).unwrap();
}

/// A directory that is both a mind project and a flayer workspace, so each
/// marker can be made newer while the other is read beside it.
fn both_markers() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    let mind = dir.path().join(MIND_DIR).join(MIND_CONFIG);
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

// Both constants are 2 today, so these two cannot yet tell a swapped check
// from a right one. They are written against each marker's own constant so
// that the first bump of either makes them do exactly that.

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
    assert_eq!(project.config().version, MIND_FORMAT_VERSION);
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
