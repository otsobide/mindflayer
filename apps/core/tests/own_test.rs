//! The workspace's own artifacts: the skills and rules a flayer workspace
//! keeps for the projects it manages, beside those projects' own.

use std::fs;
use std::path::Path;

use mindflayer_core::lifecycle;
use mindflayer_core::template;
use mindflayer_core::{
    create, Catalog, Directories, FlayerWorkspace, Kind, MindProject, Reference, WorkspaceError,
};
use tempfile::TempDir;

mod common;

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
        marker.ends_with(
            "[directories]\n\
             skills = \".mindflayer/library/skills\"\n\
             rules = \".mindflayer/library/rules\"\n"
        ),
        "{marker}"
    );
    // In its library, where no agent looks, rather than beside the
    // repositories it manages.
    assert_eq!(
        workspace.own().directory_for(Kind::Skill),
        dir.path().join(".mindflayer/library/skills")
    );
    assert_eq!(
        workspace.own().directory_for(Kind::Rule),
        dir.path().join(".mindflayer/library/rules")
    );
    // And made by the first artifact added, as anywhere else.
    assert!(!dir.path().join(".mindflayer/library").exists());
    assert!(!dir.path().join("skills").exists());
    assert!(!dir.path().join("rules").exists());
}

#[test]
fn a_kind_the_marker_does_not_name_is_kept_in_the_library() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    // Every key taken out, and then one put back beside the projects.
    let marker = fs::read_to_string(workspace.config_path()).unwrap();
    let without = marker.split("[directories]").next().unwrap().to_owned();
    fs::write(workspace.config_path(), &without).unwrap();
    let none = FlayerWorkspace::open(dir.path()).unwrap();
    fs::write(
        workspace.config_path(),
        without + "[directories]\nskills = \"skills\"\n",
    )
    .unwrap();
    let one = FlayerWorkspace::open(dir.path()).unwrap();

    // Never a folder at the root, which for the default workspace is the
    // home directory: a kind added later lands in the library of every
    // workspace, whatever its marker says.
    for kind in Kind::ALL {
        assert_eq!(
            none.own().directory_for(kind),
            dir.path().join(".mindflayer/library").join(kind.folder())
        );
    }
    assert_eq!(
        one.own().directory_for(Kind::Skill),
        dir.path().join("skills")
    );
    assert_eq!(
        one.own().directory_for(Kind::Rule),
        dir.path().join(".mindflayer/library/rules")
    );
}

#[test]
fn a_workspace_told_to_keep_its_own_beside_its_projects_still_does() {
    // What every workspace made before the library was, and is until it is
    // migrated: the marker names the folders, and they are where it says.
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init_with(dir.path(), &common::beside()).unwrap();
    let own = workspace.own();

    create(&own, Kind::Skill, "commit-style", "Shared").unwrap();
    create(&own, Kind::Rule, "git/no-force-push", "Never force-push").unwrap();
    let target = lifecycle::find(&own, &Reference::parse("commit-style")).unwrap();
    lifecycle::rename(None, &own, &target, "commits").unwrap();

    assert!(dir.path().join("skills/commits/SKILL.md").is_file());
    assert!(dir.path().join("rules/git/no-force-push.md").is_file());
    assert_eq!(Catalog::discover(&[own]).artifacts().len(), 2);
    assert!(!dir.path().join(".mindflayer/library").exists());
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
fn a_workspace_directory_overlapping_what_is_reserved_for_gathering_is_refused() {
    // The shelf of either kind, somewhere inside it, the cache, and
    // `.mindflayer` itself, which holds both. Case aside: on macOS and
    // Windows `.Mindflayer/Skills` is the shelf.
    for (directory, gathered) in [
        (".mindflayer/skills", ".mindflayer/skills"),
        (".mindflayer/rules", ".mindflayer/rules"),
        (".mindflayer/skills/mine", ".mindflayer/skills"),
        ("./.mindflayer/cache", ".mindflayer/cache"),
        (".mindflayer", ".mindflayer/skills"),
        (".Mindflayer/Skills", ".mindflayer/skills"),
    ] {
        let dir = TempDir::new().unwrap();

        let error = FlayerWorkspace::init_with(
            dir.path(),
            &Directories::default().with(Kind::Rule, directory),
        )
        .unwrap_err();

        match &error {
            WorkspaceError::OverlapsGathered {
                kind, gathered: at, ..
            } => {
                assert_eq!(*kind, Kind::Rule);
                assert_eq!(at.as_path(), Path::new(gathered), "{directory}");
            }
            _ => panic!("{directory}: {error}"),
        }
        assert!(
            !dir.path().join(".mindflayer").exists(),
            "{directory}: refused before anything was made"
        );
    }
}

#[test]
fn a_workspace_directory_elsewhere_in_mindflayer_is_allowed() {
    let dir = TempDir::new().unwrap();

    let (workspace, _) = FlayerWorkspace::init_with(
        dir.path(),
        &Directories::default().with(Kind::Skill, ".mindflayer/mine/skills"),
    )
    .unwrap();

    assert_eq!(
        workspace.own().directory_for(Kind::Skill),
        dir.path().join(".mindflayer/mine/skills")
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

    let library = dir.path().join(".mindflayer/library");
    assert!(library.join("skills/commit-style/SKILL.md").is_file());
    assert!(library.join("rules/git/no-force-push.md").is_file());
    let catalog = Catalog::discover(std::slice::from_ref(&own));
    assert_eq!(catalog.artifacts().len(), 2);

    // And changed the way a project's are.
    let target = lifecycle::find(&own, &Reference::parse("commit-style")).unwrap();
    lifecycle::rename(None, &own, &target, "commits").unwrap();
    assert!(library.join("skills/commits/SKILL.md").is_file());
    // A rule removed takes the group it leaves empty with it, and stops at
    // the kind's folder in the library, as it stops at a project's.
    let rule = lifecycle::find(&own, &Reference::parse("git/no-force-push")).unwrap();
    lifecycle::remove(None, &own, &rule).unwrap();
    assert!(!library.join("rules/git").exists());
    assert!(library.join("rules").is_dir());
    // Nothing beside the workspace, and nothing an agent reads above it.
    let mut top: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    top.sort();
    assert_eq!(top, [".mindflayer"]);
}

#[test]
fn the_workspace_templates_serve_its_own_artifacts_too() {
    let (dir, workspace) = workspace();
    let template = dir.path().join(".mindflayer/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n").unwrap();

    assert!(template::find(Some(&workspace), &workspace.own(), Kind::Skill, "runbook").is_ok());
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
    let (_dir, both, _project) = workspace_at_a_project(&common::beside());
    assert_eq!(
        both.own_kinds(),
        vec![],
        "both kinds live where the project keeps them"
    );

    let (_dir, apart, _project) =
        workspace_at_a_project(&common::beside().with(Kind::Skill, "shared/skills"));
    assert_eq!(apart.own_kinds(), vec![Kind::Skill]);
}

#[test]
fn a_file_two_holders_can_see_is_one_artifact() {
    let (_dir, workspace, project) = workspace_at_a_project(&common::beside());
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
