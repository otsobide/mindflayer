//! Moving a workspace made before its library into it: what the plan says,
//! what migrating does, and what it never touches.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::migrate::{self, Left, Move, Refusal};
use mindflayer_core::{create, FlayerWorkspace, Kind, MindProject};
use tempfile::TempDir;

mod common;

/// A workspace keeping its own beside its projects, as `flayer init` made
/// every one before the library, holding a skill and a rule in a folder.
fn beside() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init_with(dir.path(), &common::beside()).unwrap();
    create(&workspace.own(), Kind::Skill, "commit-style", "Shared").unwrap();
    create(&workspace.own(), Kind::Rule, "git/no-force-push", "Never").unwrap();
    (dir, workspace)
}

fn library(root: &Path, kind: Kind) -> PathBuf {
    root.join(".mindflayer/library").join(kind.folder())
}

fn marker(workspace: &FlayerWorkspace) -> String {
    fs::read_to_string(workspace.config_path()).unwrap()
}

#[test]
fn a_workspace_made_with_its_library_plans_nothing() {
    let (_dir, workspace) = common::workspace();
    create(&workspace.own(), Kind::Skill, "commit-style", "Shared").unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(plan.is_empty());
    assert!(plan.moves.is_empty() && plan.refusals.is_empty());
}

#[test]
fn the_plan_names_each_own_artifact_and_where_it_goes_and_moves_nothing() {
    let (dir, workspace) = beside();
    let root = dir.path();
    // Broken, and the workspace's all the same: the plan reads no front matter.
    fs::create_dir_all(root.join("skills/broken")).unwrap();
    fs::write(root.join("skills/broken/SKILL.md"), "# No front matter\n").unwrap();

    let plan = migrate::library_plan(&workspace);

    let kinds: Vec<Kind> = plan.kinds.iter().map(|repoint| repoint.kind).collect();
    assert_eq!(kinds, [Kind::Skill, Kind::Rule]);
    assert_eq!(
        plan.moves,
        [
            Move {
                kind: Kind::Skill,
                from: root.join("skills/broken"),
                to: library(root, Kind::Skill).join("broken"),
            },
            Move {
                kind: Kind::Skill,
                from: root.join("skills/commit-style"),
                to: library(root, Kind::Skill).join("commit-style"),
            },
            Move {
                kind: Kind::Rule,
                from: root.join("rules/git/no-force-push.md"),
                to: library(root, Kind::Rule).join("git/no-force-push.md"),
            },
        ]
    );
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert_eq!(plan.backup, root.join(".mindflayer/flayer.toml.v2"));
    assert!(root.join("skills/commit-style/SKILL.md").is_file());
    assert!(!root.join(".mindflayer/library").exists());
    assert!(!plan.backup.exists());
}

#[test]
fn a_destination_already_there_is_refused_by_the_plan() {
    let (dir, workspace) = beside();
    let taken = library(dir.path(), Kind::Skill).join("commit-style");
    fs::create_dir_all(&taken).unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(
        matches!(
            plan.refusals.as_slice(),
            [Refusal::Taken { from, to }]
                if *from == dir.path().join("skills/commit-style") && *to == taken
        ),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn a_marker_kept_by_an_earlier_migration_is_refused_by_the_plan() {
    let (dir, workspace) = beside();
    let backup = dir.path().join(".mindflayer/flayer.toml.v2");
    fs::write(&backup, "kept\n").unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(
        matches!(plan.refusals.as_slice(), [Refusal::Backup { path }] if *path == backup),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn what_a_project_at_the_root_shares_with_the_workspace_is_left_to_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // A home that is a project, keeping its skills in `skills` as the
    // workspace does, and its rules apart.
    MindProject::init_with(
        root,
        &mindflayer_core::Directories::default().with(Kind::Rule, ".claude/rules"),
    )
    .unwrap();
    let (workspace, _) = FlayerWorkspace::init_with(root, &common::beside()).unwrap();
    let project = MindProject::open(root).unwrap();
    create(&project, Kind::Skill, "deploy", "The project's").unwrap();
    create(&workspace.own(), Kind::Rule, "shared", "The workspace's").unwrap();

    // Linked or not: a home made a project keeps its skills there either way.
    let unlinked = migrate::library_plan(&workspace);
    let mut linked = workspace.clone();
    linked.link(&project).unwrap();
    let linked = migrate::library_plan(&linked);

    for plan in [unlinked, linked] {
        assert_eq!(
            plan.left,
            [Left {
                kind: Kind::Skill,
                path: root.join("skills/deploy"),
                project: root.to_path_buf(),
            }]
        );
        let moved: Vec<&Path> = plan.moves.iter().map(|one| one.from.as_path()).collect();
        assert_eq!(moved, [root.join("rules/shared.md")]);
        // Its line still moves: the workspace's own skills start empty there.
        assert_eq!(plan.kinds.len(), 2);
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    }
}

#[test]
fn a_project_that_cannot_be_read_refuses_the_plan_only_where_it_could_hold_them() {
    let (dir, mut workspace) = beside();
    let elsewhere = dir.path().join("alpha");
    fs::create_dir(&elsewhere).unwrap();
    let (alpha, _) = MindProject::init(&elsewhere).unwrap();
    workspace.link(&alpha).unwrap();
    fs::write(elsewhere.join(".mind/mind.toml"), "not = [toml\n").unwrap();

    let apart = migrate::library_plan(&workspace);
    // A home made a project, with a marker that does not read: the
    // workspace's folders are inside it, so it could keep a kind in one.
    fs::create_dir(dir.path().join(".mind")).unwrap();
    fs::write(dir.path().join(".mind/mind.toml"), "not = [toml\n").unwrap();
    let around = migrate::library_plan(&workspace);

    assert!(apart.refusals.is_empty(), "{:?}", apart.refusals);
    assert_eq!(apart.moves.len(), 2);
    assert!(
        matches!(around.refusals.as_slice(), [Refusal::Unreadable(_)]),
        "{:?}",
        around.refusals
    );
}

#[test]
fn a_folder_holding_the_library_is_refused_by_the_plan() {
    let (dir, workspace) = beside();
    let text = marker(&workspace).replace("skills = \"skills\"", "skills = \".\"");
    fs::write(workspace.config_path(), text).unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(
        plan.refusals.iter().any(|refusal| matches!(
            refusal,
            Refusal::Tangled { kind: Kind::Skill, folder, .. } if folder == dir.path()
        )),
        "{:?}",
        plan.refusals
    );
}

#[cfg(unix)]
#[test]
fn a_link_aimed_by_a_relative_route_is_refused_by_the_plan() {
    let (dir, workspace) = beside();
    let shared = dir.path().join("dotfiles/shared.md");
    fs::create_dir_all(shared.parent().unwrap()).unwrap();
    fs::write(&shared, "Shared.\n").unwrap();
    let link = dir.path().join("rules/shared.md");
    std::os::unix::fs::symlink("../dotfiles/shared.md", &link).unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(
        matches!(
            plan.refusals.as_slice(),
            [Refusal::Link { path, target }]
                if *path == link && target == Path::new("../dotfiles/shared.md")
        ),
        "{:?}",
        plan.refusals
    );
}
