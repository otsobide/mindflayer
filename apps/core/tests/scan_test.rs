//! Looking under a workspace, or under folders given, for what it could
//! manage.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::scan::{Request, DEFAULT_DEPTH, MAX_LISTED};
use mindflayer_core::{scan, FlayerWorkspace, Found, MindProject, FLAYER_CONFIG, FLAYER_DIR};
use tempfile::TempDir;

/// A git repository, as far as a scan can tell: a directory with a `.git`.
fn git_repository(root: &Path) {
    fs::create_dir_all(root.join(".git")).unwrap();
}

fn routes(found: &[Found]) -> Vec<&str> {
    found.iter().map(|found| found.route.as_str()).collect()
}

fn paths(found: &[Found]) -> Vec<&Path> {
    found.iter().map(|found| found.path.as_path()).collect()
}

/// What a scan of `roots` to `depth` finds.
fn under(workspace: &FlayerWorkspace, roots: &[PathBuf], depth: usize) -> mindflayer_core::Scan {
    scan(
        workspace,
        &Request {
            roots: roots.to_vec(),
            depth,
            ..Request::default()
        },
    )
}

/// A workspace in `ws` beside the folders a test scans, so that what they
/// hold is found only by naming them.
fn workspace_beside(dir: &TempDir) -> FlayerWorkspace {
    FlayerWorkspace::init(dir.path().join("ws")).unwrap().0
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

    let scan = scan(&workspace, &Request::default());

    assert_eq!(routes(&scan.found), vec!["alpha", "beta", "gamma"]);
    let flags: Vec<(bool, bool)> = scan.found.iter().map(|f| (f.mind, f.linked)).collect();
    assert_eq!(flags, vec![(true, true), (true, false), (false, false)]);
    assert_eq!(scan.roots, vec![dir.path().to_path_buf()]);
    assert_eq!(scan.depth, DEFAULT_DEPTH);
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

    let scan = scan(&workspace, &Request::default());

    assert!(scan.found[0].linked, "{:?}", scan.found);
}

#[test]
fn a_repository_is_not_looked_inside() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("mono"));
    git_repository(&dir.path().join("mono/vendor/lib"));

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec!["mono"]
    );
}

#[test]
fn hidden_folders_and_build_output_are_not_entered() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join(".cache/repo"));
    git_repository(&dir.path().join("node_modules/package"));
    git_repository(&dir.path().join("kept"));

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec!["kept"]
    );
}

#[test]
fn repositories_are_found_a_few_levels_down_and_no_further() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("github.com/acme/skills"));
    git_repository(&dir.path().join("a/b/c/too-deep"));
    assert_eq!(
        DEFAULT_DEPTH, 3,
        "the fixture above is built for three levels"
    );

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
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
    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec![".", "beside"]
    );
}

#[test]
fn a_workspace_whose_root_is_not_there_yet_holds_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("nobody-made-this");
    let workspace = FlayerWorkspace::unmade(&home).unwrap();

    let scan = scan(&workspace, &Request::default());

    assert!(scan.found.is_empty());
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
    assert!(!home.exists());
}

#[test]
fn a_scan_looks_under_every_root_it_is_given_and_not_under_the_workspace() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    git_repository(&dir.path().join("ws/inside"));
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    git_repository(&one.join("alpha"));
    fs::create_dir_all(two.join("acme/beta")).unwrap();
    MindProject::init(two.join("acme/beta")).unwrap();

    let scan = under(&workspace, &[two.clone(), one.clone()], DEFAULT_DEPTH);

    let mut found = paths(&scan.found);
    found.sort();
    assert_eq!(found, vec![one.join("alpha"), two.join("acme/beta")]);
    assert_eq!(scan.roots, vec![two, one], "in the order given");
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn the_depth_a_scan_is_given_is_how_far_below_each_root_it_looks() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    git_repository(&root.join("top"));
    git_repository(&root.join("org/deep"));
    git_repository(&root.join("a/b/c/d/deeper"));

    let found = |depth| {
        let mut found: Vec<PathBuf> = paths(&under(&workspace, &[root.clone()], depth).found)
            .into_iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();
        found.sort();
        found
    };

    assert!(found(0).is_empty());
    assert_eq!(found(1), vec![PathBuf::from("top")]);
    assert_eq!(
        found(2),
        vec![PathBuf::from("org/deep"), PathBuf::from("top")]
    );
    assert_eq!(found(5).len(), 3);
}

#[test]
fn a_root_is_a_candidate_and_is_looked_inside_even_when_it_is_a_repository() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("mono");
    git_repository(&root);
    git_repository(&root.join("vendor/lib"));

    let scan = under(&workspace, &[root.clone()], DEFAULT_DEPTH);

    assert_eq!(
        paths(&scan.found),
        vec![root.clone(), root.join("vendor/lib")]
    );
}

#[test]
fn a_folder_two_roots_reach_is_found_once() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let outer = dir.path().join("work");
    let inner = outer.join("acme");
    git_repository(&inner.join("api"));
    git_repository(&inner);

    let scan = under(
        &workspace,
        &[outer.clone(), inner.clone(), outer.join("./")],
        DEFAULT_DEPTH,
    );

    let mut found = paths(&scan.found);
    found.sort();
    assert_eq!(found, vec![inner.clone(), inner.join("api")]);
    assert_eq!(scan.roots, vec![outer, inner], "each root once");
}

#[test]
fn a_root_named_that_is_not_there_is_a_failure_beside_what_the_others_hold() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let missing = dir.path().join("wrok");
    let root = dir.path().join("work");
    git_repository(&root.join("alpha"));

    for depth in [0, DEFAULT_DEPTH] {
        let scan = under(&workspace, &[missing.clone(), root.clone()], depth);

        assert_eq!(scan.failures.len(), 1, "{:?}", scan.failures);
        assert_eq!(scan.failures[0].path, missing);
        assert!(
            scan.failures[0].to_string().contains("cannot be listed"),
            "{}",
            scan.failures[0]
        );
        if depth > 0 {
            assert_eq!(paths(&scan.found), vec![root.join("alpha")]);
        }
    }
}

#[test]
fn below_a_root_hidden_folders_and_build_output_are_not_entered() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    // Named, so entered whatever it is called.
    let root = dir.path().join(".work");
    git_repository(&root.join(".cache/repo"));
    git_repository(&root.join("node_modules/package"));
    git_repository(&root.join("target/debug/build"));
    git_repository(&root.join("kept"));

    let scan = under(&workspace, &[root.clone()], DEFAULT_DEPTH);

    assert_eq!(paths(&scan.found), vec![root.join("kept")]);
}

#[cfg(unix)]
#[test]
fn below_a_root_a_symlinked_folder_is_not_entered() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let elsewhere = dir.path().join("elsewhere");
    git_repository(&elsewhere.join("repo"));
    let root = dir.path().join("work");
    git_repository(&root.join("kept"));
    std::os::unix::fs::symlink(&elsewhere, root.join("link")).unwrap();
    // A loop, which entering links would never leave.
    std::os::unix::fs::symlink(&root, root.join("kept-loop")).unwrap();

    let scan = under(&workspace, &[root.clone()], 10);

    assert_eq!(paths(&scan.found), vec![root.join("kept")]);
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn a_scan_that_runs_out_of_budget_says_so_and_still_sees_a_second_root() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let crowded = dir.path().join("crowded");
    // The root and every folder in it but the last are listed, which is
    // the whole budget.
    for index in 0..MAX_LISTED {
        fs::create_dir_all(crowded.join(format!("d{index:04}"))).unwrap();
    }
    let other = dir.path().join("other");
    git_repository(&other.join("alpha"));

    let crowded_only = under(&workspace, &[crowded.clone()], DEFAULT_DEPTH);
    let both = under(&workspace, &[crowded, other.clone()], DEFAULT_DEPTH);

    assert!(crowded_only.truncated);
    assert!(crowded_only.found.is_empty());
    // Breadth first across both roots: the second is looked under before
    // the first's folders use up what is left.
    assert!(both.truncated);
    assert_eq!(paths(&both.found), vec![other.join("alpha")]);
}

#[test]
fn a_scan_links_nothing() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    fs::create_dir_all(root.join("alpha")).unwrap();
    MindProject::init(root.join("alpha")).unwrap();
    git_repository(&root.join("beta"));
    let marker = dir.path().join("ws").join(FLAYER_DIR).join(FLAYER_CONFIG);
    let before = fs::read_to_string(&marker).unwrap();

    let scan = under(&workspace, &[root.clone()], DEFAULT_DEPTH);

    assert_eq!(scan.found.len(), 2);
    assert!(scan.found.iter().all(|found| !found.linked));
    assert_eq!(fs::read_to_string(&marker).unwrap(), before);
    assert!(FlayerWorkspace::open(dir.path().join("ws"))
        .unwrap()
        .config()
        .projects
        .is_empty());
}

#[test]
fn a_route_is_spelled_from_where_the_scan_is_asked_to_spell_it() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("work/acme/api"));
    fs::create_dir(dir.path().join("other")).unwrap();

    let spelled = |from: Option<PathBuf>| {
        let request = Request {
            from,
            ..Request::default()
        };
        scan(&workspace, &request).found[0].route.clone()
    };

    assert_eq!(spelled(None), "work/acme/api", "from the workspace root");
    assert_eq!(spelled(Some(dir.path().join("work"))), "acme/api");
    // Not `../work/acme/api`, which reads right from one directory only.
    let elsewhere = spelled(Some(dir.path().join("other")));
    assert_eq!(
        elsewhere,
        dir.path().join("work/acme/api").display().to_string()
    );
}

#[test]
fn a_repository_outside_the_workspace_is_spelled_by_its_path() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    git_repository(&root.join("alpha"));

    let scan = under(&workspace, &[root.clone()], DEFAULT_DEPTH);

    assert_eq!(
        routes(&scan.found),
        vec![root.join("alpha").display().to_string()]
    );
}
