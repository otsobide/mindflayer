//! The workspace's own artifacts: the skills and rules a flayer workspace
//! keeps for the projects it manages, beside those projects' own.

use std::fs;

use mindflayer_core::lifecycle;
use mindflayer_core::template;
use mindflayer_core::{
    create, Catalog, Directories, FlayerWorkspace, Kind, MindProject, Reference, WorkspaceError,
};
use tempfile::TempDir;

fn workspace() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    (dir, workspace)
}

#[test]
fn a_new_workspace_says_where_its_own_artifacts_go_and_makes_nothing_yet() {
    let (dir, workspace) = workspace();

    let marker = fs::read_to_string(workspace.config_path()).unwrap();
    assert!(
        marker.contains("[directories]\nskills = \"skills\"\nrules = \"rules\"\n"),
        "{marker}"
    );
    assert_eq!(
        workspace.own().directory_for(Kind::Skill),
        dir.path().join("skills")
    );
    // A workspace sits among other people's work, so nothing appears beside
    // it until there is something to put there.
    assert!(!dir.path().join("skills").exists());
    assert!(!dir.path().join("rules").exists());
}

#[test]
fn a_workspace_can_be_told_where_to_keep_them_as_long_as_it_is_inside() {
    let dir = TempDir::new().unwrap();

    let (workspace, _) = FlayerWorkspace::init_with(
        dir.path(),
        &Directories::default().with(Kind::Skill, "shared/skills"),
    )
    .unwrap();
    let outside = TempDir::new().unwrap();
    let error = FlayerWorkspace::init_with(
        outside.path(),
        &Directories::default().with(Kind::Rule, "../rules"),
    )
    .unwrap_err();

    assert_eq!(
        workspace.own().directory_for(Kind::Skill),
        dir.path().join("shared/skills")
    );
    assert!(
        matches!(error, WorkspaceError::OutsideWorkspace { .. }),
        "{error}"
    );
    assert!(
        !outside.path().join(".mindflayer").exists(),
        "refused before anything was made"
    );
}

#[test]
fn a_workspace_holds_its_own_the_way_a_project_does() {
    let (dir, workspace) = workspace();
    let own = workspace.own();

    create(
        &own,
        Kind::Skill,
        "commit-style",
        "How every repo here writes commits",
    )
    .unwrap();
    create(&own, Kind::Rule, "git/no-force-push", "Never force-push").unwrap();

    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    assert!(dir.path().join("rules/git/no-force-push.md").is_file());
    let catalog = Catalog::discover(std::slice::from_ref(&own));
    assert_eq!(catalog.artifacts().len(), 2);

    // And changed the way a project's are.
    let target = lifecycle::find(&own, &Reference::parse("commit-style")).unwrap();
    lifecycle::rename(&own, &target, "commits").unwrap();
    assert!(dir.path().join("skills/commits/SKILL.md").is_file());
}

#[test]
fn the_workspace_templates_serve_its_own_artifacts_too() {
    let (dir, workspace) = workspace();
    let template = dir.path().join(".mindflayer/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n").unwrap();

    assert!(template::find(&workspace.own(), Kind::Skill, "runbook").is_ok());
}

/// A workspace whose root is also a project it manages.
fn workspace_at_a_project(directories: &Directories) -> (TempDir, FlayerWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init_with(dir.path(), directories).unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    workspace.link(&project).unwrap();
    (dir, workspace, project)
}

#[test]
fn a_folder_the_project_at_the_root_uses_is_the_projects_not_the_workspaces() {
    let (_dir, both, _project) = workspace_at_a_project(&Directories::default());
    assert_eq!(
        both.own_kinds(),
        vec![],
        "both kinds live where the project keeps them"
    );

    let (_dir, apart, _project) =
        workspace_at_a_project(&Directories::default().with(Kind::Skill, "shared/skills"));
    assert_eq!(apart.own_kinds(), vec![Kind::Skill]);
}

#[test]
fn a_file_two_holders_can_see_is_one_artifact() {
    let (_dir, workspace, project) = workspace_at_a_project(&Directories::default());
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let catalog = Catalog::discover(&[workspace.own(), project]);

    assert_eq!(catalog.artifacts().len(), 1);
}

#[test]
fn an_artifact_in_the_workspace_is_found_from_the_workspace_and_not_from_its_projects() {
    let (dir, mut workspace) = workspace();
    create(&workspace.own(), Kind::Skill, "commit-style", "Shared").unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    workspace.link(&project).unwrap();

    // A project holds what is in its own folders; the workspace's reach it
    // by being installed, not by being inherited out of sight.
    assert!(lifecycle::find(&project, &Reference::parse("commit-style")).is_err());
    assert!(lifecycle::find(&workspace.own(), &Reference::parse("commit-style")).is_ok());
}
