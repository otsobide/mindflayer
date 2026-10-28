//! Finding a project again by its id: giving a copy an id of its own, and
//! following a project that moved to where it went, with what it had.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::exclude::{self, Followed};
use mindflayer_core::install;
use mindflayer_core::relink::{self, RelinkError};
use mindflayer_core::{
    Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, ProjectId, Share,
    WorkspaceError,
};
use tempfile::TempDir;

/// A workspace keeping `deploy` as its own and linking `collapse`, a
/// project marked by its settings in a repository of its own, shared as
/// `share` says, with `deploy` installed into it.
fn installed(share: Share) -> (TempDir, FlayerWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path().join("Projects")).unwrap();
    mindflayer_core::create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let root = dir.path().join("Projects/collapse");
    fs::create_dir_all(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) =
        MindProject::init_asked(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    let (ledger, _) = workspace.ledger_to_write().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    (dir, workspace, project)
}

fn id_of(root: &Path) -> ProjectId {
    MindProject::open(root)
        .unwrap()
        .settings()
        .unwrap()
        .id
        .clone()
}

fn exclude_file(repository: &Path) -> PathBuf {
    repository.join(".git").join("info").join("exclude")
}

#[test]
fn a_new_id_changes_the_id_line_alone_and_keeps_the_exclude_group_and_records_with_it() {
    let (_dir, workspace, project) = installed(Share::Local);
    let root = project.root().to_path_buf();
    let was = id_of(&root);
    let settings = fs::read_to_string(project.marker_path()).unwrap();

    let renewed = relink::new_id(Some(&workspace), &project).unwrap();

    assert_eq!(renewed.was, was);
    assert_ne!(renewed.id, was);
    assert_eq!(id_of(&root), renewed.id);
    assert_eq!(
        fs::read_to_string(project.marker_path()).unwrap(),
        settings.replace(was.as_str(), renewed.id.as_str())
    );
    assert_eq!(
        renewed.excluded,
        Some(Followed::Written {
            file: exclude_file(&root)
        })
    );
    let listed = exclude::listed(&root, renewed.id.as_str())
        .unwrap()
        .unwrap();
    assert_eq!(
        listed.lines,
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
    assert!(exclude::listed(&root, was.as_str())
        .unwrap()
        .unwrap()
        .lines
        .is_empty());
    let ledger = workspace.ledger_for_reading().unwrap();
    assert_eq!(
        ledger.project_at("collapse").unwrap(),
        Some(renewed.id.clone())
    );
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["deploy"]
    );
}

#[test]
fn a_shared_project_or_one_no_workspace_links_is_given_a_new_id_all_the_same() {
    let (_dir, workspace, project) = installed(Share::Shared);
    let renewed = relink::new_id(Some(&workspace), &project).unwrap();
    assert_eq!(renewed.excluded, None);
    assert_eq!(id_of(project.root()), renewed.id);

    let alone = TempDir::new().unwrap();
    let (project, _) = MindProject::init_asked(
        alone.path(),
        &Agent::ALL,
        Share::Shared,
        &Directories::default(),
    )
    .unwrap();
    let renewed = relink::new_id(None, &project).unwrap();
    assert_eq!(id_of(alone.path()), renewed.id);
}

#[test]
fn a_project_an_older_marker_marks_is_pointed_at_the_migration_for_a_new_id() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();

    let refused = relink::new_id(None, &project).unwrap_err();

    assert!(
        matches!(
            refused,
            RelinkError::Workspace(WorkspaceError::NoSettings { .. })
        ),
        "{refused}"
    );
    assert!(refused.to_string().contains("`mind migrate`"), "{refused}");
}

/// Copy the directory `from` to `to`, `.mind` and all, as `cp -r` would.
fn copied(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copied(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scanned(workspace: &FlayerWorkspace) -> mindflayer_core::Scan {
    mindflayer_core::scan(workspace, &mindflayer_core::scan::Request::default())
}

#[test]
fn a_scan_finds_a_project_moved_from_a_stale_entry_by_its_id() {
    let (dir, workspace, project) = installed(Share::Local);
    let moved = dir.path().join("Projects/apps/collapse");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let scan = scanned(&workspace);

    let found = scan.found.iter().find(|found| found.path == moved).unwrap();
    assert_eq!(found.route, "apps/collapse");
    assert_eq!(found.moved.as_deref(), Some("collapse"));
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn a_copy_beside_its_project_is_a_duplicate_and_never_a_move() {
    let (dir, workspace, project) = installed(Share::Local);
    let copy = dir.path().join("Projects/copy");
    copied(project.root(), &copy);

    let scan = scanned(&workspace);

    assert!(scan.found.iter().all(|found| found.moved.is_none()));
    let said: Vec<String> = scan.failures.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [format!(
            "{} and {} are both project {}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own",
            project.root().display(),
            copy.display(),
            id_of(project.root())
        )]
    );
}

#[test]
fn two_copies_of_a_project_that_moved_are_a_duplicate_and_neither_is_its_move() {
    let (dir, workspace, project) = installed(Share::Local);
    let first = dir.path().join("Projects/first");
    let second = dir.path().join("Projects/second");
    copied(project.root(), &second);
    fs::rename(project.root(), &first).unwrap();

    let scan = scanned(&workspace);

    assert!(scan.found.iter().all(|found| found.moved.is_none()));
    assert!(matches!(
        &scan.failures[..],
        [mindflayer_core::scan::ScanFailure::Duplicate(duplicate)]
            if duplicate.paths == [first.clone(), second.clone()]
    ));
}

#[test]
fn two_linked_copies_are_a_failure_of_the_status_naming_both() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let copy = dir.path().join("Projects/copy");
    copied(project.root(), &copy);
    workspace.link(&MindProject::open(&copy).unwrap()).unwrap();

    let (status, failures) = mindflayer_core::Status::of_workspace(&workspace);

    assert_eq!(status.projects.len(), 2);
    let said: Vec<String> = failures.iter().map(ToString::to_string).collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with(&format!(
            "{} and {} are both project {}",
            project.root().display(),
            copy.display(),
            id_of(&copy)
        )),
        "{said:?}"
    );
}
