//! Unit tests for `migrate/project.rs`, kept beside it rather than inside it.

use super::*;

fn marker(version: u32, directories: Directories) -> MindConfig {
    MindConfig {
        version,
        name: String::from("collapse"),
        directories,
    }
}

#[test]
fn a_version_2_markers_folders_are_written_out_with_the_defaults_it_meant() {
    let written = authored(&marker(
        2,
        Directories::default()
            .with(Kind::Skill, ".claude/skills")
            .with_key("mcp", "servers"),
    ));

    assert_eq!(written.get(Kind::Skill), Some(Path::new(".claude/skills")));
    assert_eq!(written.get(Kind::Rule), Some(Path::new("rules")));
    assert_eq!(
        written.entries(),
        [
            ("skills", Path::new(".claude/skills")),
            ("rules", Path::new("rules")),
            ("mcp", Path::new("servers")),
        ]
    );
}

#[test]
fn a_version_1_marker_is_described_inside_mind_where_it_kept_every_kind() {
    let written = authored(&marker(1, Directories::default()));

    for kind in Kind::ALL {
        let folder = written.get(kind).unwrap();
        assert_eq!(folder, Path::new(MIND_DIR).join(kind.folder()));
        assert!(matches!(
            misplaced_authored(kind, folder),
            Some(crate::workspace::WorkspaceError::InsideMind { .. })
        ));
    }
}

#[test]
fn a_version_1_marker_keeps_the_folders_it_names_outside_mind() {
    let written = authored(&marker(
        1,
        Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules"),
    ));

    assert_eq!(written.get(Kind::Skill), Some(Path::new("skills")));
    assert_eq!(written.get(Kind::Rule), Some(Path::new("rules")));
    for kind in Kind::ALL {
        assert!(misplaced_authored(kind, written.get(kind).unwrap()).is_none());
    }
}
