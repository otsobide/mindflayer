//! Moving a workspace made before its library into it: what the plan says,
//! what migrating does, and what it never touches.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::migrate::{self, Left, Move, Refusal};
use mindflayer_core::{create, Catalog, FlayerWorkspace, Kind, MindProject};
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
    // Only unlinked did the workspace count them as its own, as `own_kinds`
    // does, and so stop.
    let unlinked = migrate::library_plan(&workspace);
    let mut linked = workspace.clone();
    linked.link(&project).unwrap();
    let linked = migrate::library_plan(&linked);

    for (plan, counted) in [(unlinked, true), (linked, false)] {
        assert_eq!(
            plan.left,
            [Left {
                kind: Kind::Skill,
                path: root.join("skills/deploy"),
                project: root.to_path_buf(),
                counted,
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
    // Allowed by `flayer init`, unlike the root, which is not inside the
    // workspace, or `.mindflayer`, which holds the shelf.
    let text =
        marker(&workspace).replace("skills = \"skills\"", "skills = \".mindflayer/library\"");
    fs::write(workspace.config_path(), text).unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();

    let plan = migrate::library_plan(&workspace);

    let holding = dir.path().join(".mindflayer/library");
    assert!(
        matches!(
            plan.refusals.as_slice(),
            [Refusal::Tangled { kind: Kind::Skill, folder, .. }] if *folder == holding
        ),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn a_folder_outside_the_workspace_is_refused_and_left_as_it_is() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("ws");
    let (workspace, _) = FlayerWorkspace::init_with(&root, &common::beside()).unwrap();
    // Named by hand, as `flayer init` would refuse to.
    let text = marker(&workspace).replace("skills = \"skills\"", "skills = \"../team/skills\"");
    fs::write(workspace.config_path(), text).unwrap();
    let theirs = dir.path().join("team/skills/shared");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(
        theirs.join("SKILL.md"),
        "---\nname: shared\ndescription: Team\n---\n",
    )
    .unwrap();
    let mut workspace = FlayerWorkspace::open(&root).unwrap();

    let plan = migrate::library_plan(&workspace);
    let error = migrate::library(&mut workspace).unwrap_err();

    assert!(
        matches!(
            plan.refusals.as_slice(),
            [Refusal::Misplaced(
                mindflayer_core::WorkspaceError::OutsideWorkspace {
                    kind: Kind::Skill,
                    ..
                }
            )]
        ),
        "{:?}",
        plan.refusals
    );
    assert!(plan.moves.is_empty(), "{:?}", plan.moves);
    assert!(
        matches!(error, migrate::MigrateError::Refused(_)),
        "{error}"
    );
    assert!(theirs.join("SKILL.md").is_file());
}

#[test]
fn a_folder_on_the_shelf_is_refused_by_the_plan() {
    let (dir, workspace) = beside();
    let text = marker(&workspace).replace("skills = \"skills\"", "skills = \".mindflayer/skills\"");
    fs::write(workspace.config_path(), text).unwrap();
    let gathered = dir.path().join(".mindflayer/skills/gathered");
    fs::create_dir_all(&gathered).unwrap();
    fs::write(
        gathered.join("SKILL.md"),
        "---\nname: gathered\ndescription: From a source\n---\n",
    )
    .unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();

    let plan = migrate::library_plan(&workspace);

    assert!(
        matches!(
            plan.refusals.as_slice(),
            [Refusal::Misplaced(
                mindflayer_core::WorkspaceError::OverlapsGathered {
                    kind: Kind::Skill,
                    ..
                }
            )]
        ),
        "{:?}",
        plan.refusals
    );
    assert!(plan.moves.iter().all(|one| one.kind == Kind::Rule));
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

#[cfg(unix)]
#[test]
fn a_folder_reached_through_a_link_is_refused_and_left_as_it_is() {
    let (dir, workspace) = beside();
    let root = dir.path();
    // The skills folder is a link, as a dotfiles setup makes it; the rules
    // folder is reached through one, a step above it.
    fs::create_dir_all(root.join("dotfiles/config")).unwrap();
    fs::rename(root.join("skills"), root.join("dotfiles/skills")).unwrap();
    std::os::unix::fs::symlink("dotfiles/skills", root.join("skills")).unwrap();
    fs::rename(root.join("rules"), root.join("dotfiles/config/rules")).unwrap();
    std::os::unix::fs::symlink("dotfiles/config", root.join("config")).unwrap();
    let text = marker(&workspace).replace("rules = \"rules\"", "rules = \"config/rules\"");
    fs::write(workspace.config_path(), text).unwrap();
    let mut workspace = FlayerWorkspace::open(root).unwrap();

    let plan = migrate::library_plan(&workspace);
    let error = migrate::library(&mut workspace).unwrap_err();

    assert!(
        matches!(
            plan.refusals.as_slice(),
            [
                Refusal::Linked { kind: Kind::Skill, folder: skills, target: to_skills },
                Refusal::Linked { kind: Kind::Rule, folder: config, target: to_config },
            ] if *skills == root.join("skills")
                && to_skills == Path::new("dotfiles/skills")
                && *config == root.join("config")
                && to_config == Path::new("dotfiles/config")
        ),
        "{:?}",
        plan.refusals
    );
    assert!(plan.moves.is_empty(), "{:?}", plan.moves);
    assert!(
        matches!(error, migrate::MigrateError::Refused(_)),
        "{error}"
    );
    assert!(root.join("dotfiles/skills/commit-style/SKILL.md").is_file());
    assert!(root
        .join("dotfiles/config/rules/git/no-force-push.md")
        .is_file());
}

// ---------------------------------------------------------------------------
// Migrating
// ---------------------------------------------------------------------------

#[test]
fn migrating_moves_them_and_points_the_marker_at_the_library() {
    let (dir, mut workspace) = beside();
    let root = dir.path().to_path_buf();
    let before = marker(&workspace);

    let migrated = migrate::library(&mut workspace).unwrap();

    assert_eq!(migrated.plan.moves.len(), 2);
    assert!(library(&root, Kind::Skill)
        .join("commit-style/SKILL.md")
        .is_file());
    assert!(library(&root, Kind::Rule)
        .join("git/no-force-push.md")
        .is_file());
    // The folders they left are gone, so a home keeps no empty ~/skills.
    assert!(!root.join("skills").exists());
    assert!(!root.join("rules").exists());
    // Only the two values changed, the comments above them and every other
    // byte as they were.
    assert_eq!(
        marker(&workspace),
        before
            .replace(
                "skills = \"skills\"",
                "skills = \".mindflayer/library/skills\""
            )
            .replace("rules = \"rules\"", "rules = \".mindflayer/library/rules\"")
    );
    let backup = root.join(".mindflayer/flayer.toml.v2");
    assert_eq!(migrated.backup.as_deref(), Some(backup.as_path()));
    assert_eq!(fs::read_to_string(&backup).unwrap(), before);
    // The workspace handed back is the one on disk, and finds them there.
    assert_eq!(workspace, FlayerWorkspace::open(&root).unwrap());
    let catalog = Catalog::discover(&[workspace.own()]);
    let names: Vec<&str> = catalog.artifacts().iter().map(|a| a.name()).collect();
    assert_eq!(names, ["commit-style", "git/no-force-push"]);
    assert!(catalog.failures().is_empty());
}

#[test]
fn a_second_migration_changes_nothing() {
    let (dir, mut workspace) = beside();
    migrate::library(&mut workspace).unwrap();
    let after_first = marker(&workspace);
    let modified = fs::metadata(workspace.config_path())
        .unwrap()
        .modified()
        .unwrap();
    let backup = dir.path().join(".mindflayer/flayer.toml.v2");
    let kept = fs::read_to_string(&backup).unwrap();

    let again = migrate::library(&mut workspace).unwrap();

    assert!(again.plan.is_empty());
    assert_eq!(again.backup, None);
    assert_eq!(marker(&workspace), after_first);
    assert_eq!(
        fs::metadata(workspace.config_path())
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert_eq!(fs::read_to_string(&backup).unwrap(), kept);
}

#[test]
fn a_refused_migration_moves_nothing_and_writes_nothing() {
    let (dir, mut workspace) = beside();
    let before = marker(&workspace);
    fs::create_dir_all(library(dir.path(), Kind::Rule).join("git/no-force-push.md")).unwrap();

    let error = migrate::library(&mut workspace).unwrap_err();

    assert!(
        matches!(&error, migrate::MigrateError::Refused(refusals) if refusals.len() == 1),
        "{error}"
    );
    assert!(error.to_string().starts_with("nothing moved: "), "{error}");
    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    assert!(!library(dir.path(), Kind::Skill).exists());
    assert_eq!(marker(&workspace), before);
    assert!(!dir.path().join(".mindflayer/flayer.toml.v2").exists());
}

#[test]
fn a_move_that_fails_puts_back_what_had_moved() {
    let (dir, mut workspace) = beside();
    let before = marker(&workspace);
    // The skill moves first; the rule's folder cannot be made over a file.
    fs::create_dir_all(dir.path().join(".mindflayer/library")).unwrap();
    fs::write(library(dir.path(), Kind::Rule), "in the way\n").unwrap();

    let error = migrate::library(&mut workspace).unwrap_err();

    assert!(
        matches!(&error, migrate::MigrateError::Move { from, .. }
            if *from == dir.path().join("rules/git/no-force-push.md")),
        "{error}"
    );
    assert!(error.stranded().is_empty());
    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
    assert!(dir.path().join("rules/git/no-force-push.md").is_file());
    assert!(!library(dir.path(), Kind::Skill)
        .join("commit-style")
        .exists());
    assert_eq!(marker(&workspace), before);
    assert!(!dir.path().join(".mindflayer/flayer.toml.v2").exists());
}

#[test]
fn what_is_not_an_artifact_stays_and_keeps_its_folder() {
    let (dir, mut workspace) = beside();
    fs::write(dir.path().join("skills/README.md"), "Ours.\n").unwrap();

    migrate::library(&mut workspace).unwrap();

    assert!(dir.path().join("skills/README.md").is_file());
    assert!(!dir.path().join("skills/commit-style").exists());
    // A rule's folders went with the last rule in them.
    assert!(!dir.path().join("rules").exists());
}

#[test]
fn a_project_at_the_root_keeps_what_it_shares_and_the_marker_moves_on() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let (project, _) = MindProject::init(root).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init_with(root, &common::beside()).unwrap();
    workspace.link(&project).unwrap();
    create(&project, Kind::Skill, "deploy", "The project's").unwrap();

    let migrated = migrate::library(&mut workspace).unwrap();

    assert!(migrated.plan.moves.is_empty());
    assert!(root.join("skills/deploy/SKILL.md").is_file());
    assert_eq!(
        workspace.own().directory_for(Kind::Skill).unwrap(),
        library(root, Kind::Skill)
    );
    // No longer one folder for both, so the workspace counts it as its own
    // of every kind, and the project's skill is the project's.
    assert_eq!(workspace.own_kinds(), Kind::ALL);
    let catalog = Catalog::discover(&[workspace.own()]);
    assert!(catalog.is_empty());
}

#[test]
fn an_installed_skill_is_still_current_once_its_source_has_moved() {
    use mindflayer_core::install::{self, Installed, Standing};

    let (dir, mut workspace) = beside();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    let (project, _) = MindProject::init(&alpha).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    let offered = |workspace: &FlayerWorkspace| {
        install::offered(
            workspace,
            &ledger,
            &project,
            Kind::Skill,
            "commit-style",
            None,
        )
        .unwrap()
    };
    install::install(&workspace, &ledger, &project, &offered(&workspace)).unwrap();
    let recorded = ledger
        .installation("alpha", Kind::Skill, "commit-style")
        .unwrap();

    migrate::library(&mut workspace).unwrap();

    let candidate = offered(&workspace);
    assert_eq!(candidate.standing, Standing::Installed);
    assert_eq!(candidate.origin(), "workspace");
    assert!(install::is_current(&workspace, &project, &candidate));
    assert_eq!(
        ledger
            .installation("alpha", Kind::Skill, "commit-style")
            .unwrap(),
        recorded
    );
    let again = install::install(&workspace, &ledger, &project, &candidate).unwrap();
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{:?}",
        again.installed
    );
}

#[test]
fn an_unmade_workspace_has_nothing_to_move_and_is_not_made() {
    let dir = TempDir::new().unwrap();
    let mut workspace = FlayerWorkspace::unmade(dir.path()).unwrap();

    let migrated = migrate::library(&mut workspace).unwrap();

    assert!(migrated.plan.is_empty());
    assert!(!dir.path().join(".mindflayer").exists());
}
