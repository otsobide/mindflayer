//! Unit tests for `view.rs`: what a snapshot never does, which only a test
//! inside the crate can count.

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::scan;

/// A workspace linking one project beside it, which holds a skill of its
/// own, and a project nobody linked, which a scan would find.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("api")).unwrap();
    let skill = project.directory_for(Kind::Skill).unwrap().join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: deploy\ndescription: Ship it\n---\n\n# deploy\n",
    )
    .unwrap();
    workspace.link(&project).unwrap();
    MindProject::init(dir.path().join("web")).unwrap();
    dir
}

/// Inputs naming the workspace at `root` and no default.
fn naming(root: &Path) -> Inputs<'_> {
    Inputs {
        flag: Some(root),
        activated: None,
        start: root,
        home: None,
        intent: Intent::Read,
    }
}

/// A live update reads a snapshot every time something changes, and a walk
/// of the disk is no price to pay for one: what could be linked is a query
/// of its own. Counted on this thread, where the snapshot is built.
#[test]
fn building_a_snapshot_runs_no_scan() {
    let dir = fixture();
    let before = scan::scans();

    let snapshot = Snapshot::read(&naming(dir.path())).unwrap().unwrap();

    assert_eq!(scan::scans(), before, "the snapshot scanned");
    assert_eq!(snapshot.projects.len(), 1);
    // The counter counts: a scan of the same workspace is one.
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let found = scan::scan(&workspace, &scan::Request::default());
    assert_eq!(scan::scans(), before + 1);
    assert!(found.found.iter().any(|found| !found.linked));
}
