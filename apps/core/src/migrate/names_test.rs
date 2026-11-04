//! Unit tests for `migrate/names.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_route_inside_the_old_folder_moves_with_it_and_no_other() {
    assert_eq!(
        moved_route(Path::new(".mindflayer/library/skills")),
        Some(PathBuf::from(".mindweaver/library/skills"))
    );
    assert_eq!(moved_route(Path::new("skills")), None);
    assert_eq!(moved_route(Path::new("a/.mindflayer/skills")), None);
    assert_eq!(moved_route(Path::new(".mindflayerish/skills")), None);
}

#[test]
fn only_what_is_named_after_the_product_is_renamed_in_the_folder() {
    let dir = tempfile::TempDir::new().unwrap();
    for name in [
        "flayer.toml",
        "flayer.toml.v1",
        "flayer.toml.12345-abc.tmp",
        "mindflayer.db",
        "mindflayer.db.v2.bak",
        "mindflayer.db.v2.bak.tmp",
        "lock",
    ] {
        fs::write(dir.path().join(name), "").unwrap();
    }

    let renamed = renamed_inside(dir.path()).unwrap();

    assert_eq!(
        renamed,
        [
            (String::from("flayer.toml"), String::from("weaver.toml")),
            (
                String::from("flayer.toml.v1"),
                String::from("weaver.toml.v1")
            ),
            (String::from("mindflayer.db"), String::from("mindweaver.db")),
            (
                String::from("mindflayer.db.v2.bak"),
                String::from("mindweaver.db.v2.bak")
            ),
        ]
    );
}
