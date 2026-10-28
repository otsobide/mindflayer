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
