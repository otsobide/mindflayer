//! Unit tests for `workspace.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn templates_round_trip_through_the_parser() {
    let mind: MindConfig =
        toml::from_str(&mind_template("collapse", &Directories::default())).unwrap();
    assert_eq!(mind.version, FORMAT_VERSION);
    assert_eq!(mind.name, "collapse");
    // Spelled out rather than left to the default, so the file answers
    // where a project's skills go without anyone reading the source.
    assert_eq!(mind.directories.get(Kind::Skill), Some(Path::new("skills")));
    assert_eq!(mind.directories.get(Kind::Rule), Some(Path::new("rules")));

    let flayer: FlayerConfig =
        toml::from_str(&flayer_template("projects", &Directories::default())).unwrap();
    assert_eq!(flayer.version, FORMAT_VERSION);
    assert_eq!(flayer.name, "projects");
    assert!(flayer.projects.is_empty());
    assert_eq!(
        flayer.directories.get(Kind::Skill),
        Some(Path::new("skills"))
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
    assert_ne!(rewritten, text, "{} has no version line", path.display());
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
    with_version(&mind, FORMAT_VERSION + 1);
    with_version(&flayer, FORMAT_VERSION + 1);

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
