//! Moving what an older build made to what this one writes, only when
//! asked: a project marked by a `mind.toml` (`migrate::project`), and a
//! workspace keeping its own beside its projects (`migrate::library`).

use mindweaver_core::migrate;
use mindweaver_core::{Agent, Directories, Kind, Share, WeaverWorkspace};

use crate::{at, World};

/// `~/Projects` linking `collapse`, a project an older build made:
/// `commit-style` installed from the library into its `skills/`, beside
/// `deploy`, its own.
fn collapse_marked_by_a_mind_toml() -> World {
    let world = World::empty();
    world
        .older_project("~/Projects/collapse")
        .init_workspace("~/Projects")
        .link("~/Projects", "~/Projects/collapse")
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "commit-style",
            "How every repo here writes commits",
        )
        .install("~/Projects", "~/Projects/collapse", "skill:commit-style")
        .add(
            "~/Projects",
            "~/Projects/collapse",
            Kind::Skill,
            "deploy",
            "Ship the service to staging",
        );
    world
}

#[test]
fn a_project_marked_by_a_mind_toml_moves_to_settings_and_its_copies_where_agents_read() {
    let world = collapse_marked_by_a_mind_toml();
    let workspace = world.workspace("~/Projects");

    let migrated = migrate::project(
        Some(&workspace),
        &world.project("~/Projects/collapse"),
        &Agent::ALL,
        Share::Shared,
        &world.homes(),
    )
    .expect("migrate the project");

    assert!(migrated.is_some());
    assert!(world.exists("~/Projects/collapse/.mind/settings.toml"));
    assert!(!world.exists("~/Projects/collapse/.mind/mind.toml"));
    assert!(world.exists("~/Projects/collapse/.mind/tmp/mind.toml.v2"));
    assert!(!world.exists("~/Projects/collapse/skills/commit-style"));
    assert!(world.exists("~/Projects/collapse/skills/deploy/SKILL.md"));
    assert!(world.exists("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"));
    assert!(world.exists("~/Projects/collapse/.agents/skills/commit-style/SKILL.md"));
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(at(&snapshot, "/projects/0/marker"), "settings.toml");
    assert_eq!(at(&snapshot, "/projects/0/migrates"), false);
    assert_eq!(
        at(&snapshot, "/projects/0/artifacts/0/name"),
        "commit-style"
    );
    assert_eq!(at(&snapshot, "/projects/0/artifacts/0/state"), "current");
    assert_eq!(at(&snapshot, "/projects/0/artifacts/1/state"), "authored");
}

#[test]
fn a_project_marked_by_a_mind_toml_says_it_migrates_until_it_does() {
    let world = collapse_marked_by_a_mind_toml();

    let snapshot = world.snapshot("~/Projects");

    assert_eq!(at(&snapshot, "/projects/0/marker"), "mind.toml");
    assert_eq!(at(&snapshot, "/projects/0/migrates"), true);
}

#[test]
fn a_project_marked_by_its_settings_already_has_nothing_to_migrate() {
    let world = World::projects(&["collapse"]);
    let settings = world.read("~/Projects/collapse/.mind/settings.toml");
    let workspace = world.workspace("~/Projects");

    let migrated = migrate::project(
        Some(&workspace),
        &world.project("~/Projects/collapse"),
        &Agent::ALL,
        Share::Local,
        &world.homes(),
    )
    .expect("nothing to migrate");

    assert!(migrated.is_none());
    assert_eq!(
        world.read("~/Projects/collapse/.mind/settings.toml"),
        settings
    );
}

#[test]
fn a_workspace_keeping_its_own_beside_its_projects_moves_them_into_its_library() {
    let world = World::empty();
    WeaverWorkspace::init_with(
        world.home(),
        &Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules"),
    )
    .expect("make the workspace as it was made before the library");
    world
        .add(
            "~",
            "~",
            Kind::Skill,
            "commit-style",
            "How every repo here writes commits",
        )
        .add(
            "~",
            "~",
            Kind::Rule,
            "git/no-force-push",
            "Never force-push a shared branch",
        );
    let marker = world.read("~/.mindweaver/weaver.toml");
    let mut workspace = world.workspace("~");

    let migrated = migrate::library(&mut workspace).expect("migrate the library");

    assert_eq!(
        migrated.backup.map(|backup| world.spelled(&backup)),
        Some("/home/you/.mindweaver/weaver.toml.v2".to_owned())
    );
    assert_eq!(world.read("~/.mindweaver/weaver.toml.v2"), marker);
    assert!(!world.exists("~/skills"));
    assert!(!world.exists("~/rules"));
    assert!(world.exists("~/.mindweaver/library/skills/commit-style/SKILL.md"));
    assert!(world.exists("~/.mindweaver/library/rules/git/no-force-push.md"));
    assert_eq!(
        at(&world.snapshot("~"), "/library/directories"),
        &serde_json::json!([
            {"kind": "skill", "path": ".mindweaver/library/skills"},
            {"kind": "rule", "path": ".mindweaver/library/rules"},
            {"kind": "mcp", "path": ".mindweaver/library/mcp"},
        ])
    );
}
