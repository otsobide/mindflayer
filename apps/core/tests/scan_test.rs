//! Looking under a workspace for what it could manage.

use std::fs;
use std::path::Path;

use mindflayer_core::scan::MAX_DEPTH;
use mindflayer_core::{scan, FlayerWorkspace, Found, MindProject};
use tempfile::TempDir;

/// A git repository, as far as a scan can tell: a directory with a `.git`.
fn git_repository(root: &Path) {
    fs::create_dir_all(root.join(".git")).unwrap();
}

fn routes(found: &[Found]) -> Vec<&str> {
    found.iter().map(|found| found.route.as_str()).collect()
}

#[test]
fn a_scan_finds_projects_and_repositories_and_says_which_are_linked() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        MindProject::init(dir.path().join(name)).unwrap();
    }
    git_repository(&dir.path().join("gamma"));
    fs::create_dir(dir.path().join("plain")).unwrap();
    let alpha = MindProject::open(dir.path().join("alpha")).unwrap();
    workspace.link(&alpha).unwrap();

    let scan = scan(&workspace);

    assert_eq!(routes(&scan.found), vec!["alpha", "beta", "gamma"]);
    let flags: Vec<(bool, bool)> = scan.found.iter().map(|f| (f.mind, f.linked)).collect();
    assert_eq!(flags, vec![(true, true), (true, false), (false, false)]);
    assert!(scan.failures.is_empty());
    assert!(!scan.truncated);
}

#[test]
fn linked_is_decided_by_where_an_entry_points() {
    let dir = TempDir::new().unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    fs::create_dir(dir.path().join("beta")).unwrap();
    MindProject::init(dir.path().join("beta")).unwrap();
    fs::write(
        dir.path().join(".mindflayer/flayer.toml"),
        "version = 2\nname = \"w\"\nprojects = [\"./beta\"]\n",
    )
    .unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();

    let scan = scan(&workspace);

    assert!(scan.found[0].linked, "{:?}", scan.found);
}

#[test]
fn a_repository_is_not_looked_inside() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("mono"));
    git_repository(&dir.path().join("mono/vendor/lib"));

    assert_eq!(routes(&scan(&workspace).found), vec!["mono"]);
}

#[test]
fn hidden_folders_and_build_output_are_not_entered() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join(".cache/repo"));
    git_repository(&dir.path().join("node_modules/package"));
    git_repository(&dir.path().join("kept"));

    assert_eq!(routes(&scan(&workspace).found), vec!["kept"]);
}

#[test]
fn repositories_are_found_a_few_levels_down_and_no_further() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("github.com/acme/skills"));
    git_repository(&dir.path().join("a/b/c/too-deep"));
    assert_eq!(MAX_DEPTH, 3, "the fixture above is built for three levels");

    assert_eq!(
        routes(&scan(&workspace).found),
        vec!["github.com/acme/skills"]
    );
}

#[test]
fn the_workspace_root_is_a_candidate_too() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    MindProject::init(dir.path()).unwrap();
    git_repository(&dir.path().join("beside"));

    // A workspace can manage the project it sits in, and the root is still
    // where the other repositories are kept.
    assert_eq!(routes(&scan(&workspace).found), vec![".", "beside"]);
}
