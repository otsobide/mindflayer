//! Changing how a project is set up, and what is installed in it following
//! (`reapply::apply`): its agents, how it shares and where a kind is
//! installed, each carrying only what Mindweaver installed and nobody has
//! changed since.

use mindweaver_core::reapply::{self, Change};
use mindweaver_core::{Agent, Kind, Share};
use serde_json::json;

use crate::{at, World};

/// What `commit-style` is said to do.
const COMMIT_STYLE: &str = "How every repo here writes commits";

/// `~/Projects` linking `collapse`, a project for Claude alone holding
/// `commit-style` from the library and `notes`, kept by hand where Claude
/// reads.
fn collapse_for_claude() -> World {
    let world = World::empty();
    world
        .repository("~/Projects/collapse")
        .init_project_for("~/Projects/collapse", &[Agent::Claude])
        .folder("~/Projects")
        .init_workspace("~/Projects")
        .link("~/Projects", "~/Projects/collapse")
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "commit-style",
            COMMIT_STYLE,
        )
        .install("~/Projects", "~/Projects/collapse", "skill:commit-style")
        .file(
            "~/Projects/collapse/.claude/skills/notes/SKILL.md",
            "---\nname: notes\ndescription: My own notes\n---\n",
        );
    world
}

/// `change` made to `collapse`, as `mind agents`, `mind share` or `mind
/// targets` makes it with `--yes`.
fn change(world: &World, change: Change) -> Result<reapply::Applied, reapply::ReapplyError> {
    let workspace = world.workspace("~/Projects");
    reapply::apply(
        Some(&workspace),
        &world.project("~/Projects/collapse"),
        &change,
        &world.homes(),
    )
}

#[test]
fn an_agent_added_gets_a_copy_of_each_installation_where_it_reads() {
    let world = collapse_for_claude();

    change(&world, Change::AddAgents(vec![Agent::Codex])).expect("add codex");

    assert_eq!(
        world.read("~/Projects/collapse/.agents/skills/commit-style/SKILL.md"),
        world.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md")
    );
    assert!(!world.exists("~/Projects/collapse/.agents/skills/notes"));
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(
        at(&snapshot, "/projects/0/agents"),
        &json!(["claude", "codex"])
    );
    assert_eq!(
        at(&snapshot, "/projects/0/artifacts/0/targets"),
        &json!([".agents/skills/commit-style", ".claude/skills/commit-style"])
    );
    assert_eq!(at(&snapshot, "/projects/0/artifacts/0/state"), "current");
}

#[test]
fn an_agent_removed_loses_mindweavers_copy_and_somebodys_skill_stays() {
    let world = collapse_for_claude();
    change(&world, Change::AddAgents(vec![Agent::Codex])).expect("add codex");

    change(&world, Change::RemoveAgents(vec![Agent::Claude])).expect("remove claude");

    assert!(!world.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(world.exists("~/Projects/collapse/.claude/skills/notes/SKILL.md"));
    assert!(world.exists("~/Projects/collapse/.agents/skills/commit-style/SKILL.md"));
}

#[test]
fn a_project_left_with_no_agent_is_refused_and_keeps_its_settings() {
    let world = collapse_for_claude();
    let settings = world.read("~/Projects/collapse/.mind/settings.toml");

    let refused =
        change(&world, Change::RemoveAgents(vec![Agent::Claude])).expect_err("no agent left");

    assert_eq!(refused.code(), "reapply_no_agent");
    assert_eq!(
        world.read("~/Projects/collapse/.mind/settings.toml"),
        settings
    );
    assert!(world.exists("~/Projects/collapse/.claude/skills/commit-style"));
}

#[test]
fn turning_shared_hands_mind_and_the_copies_to_the_team() {
    let world = collapse_for_claude();

    change(
        &world,
        Change::Share {
            share: Share::Shared,
            edit_tracked: false,
        },
    )
    .expect("turn shared");

    assert!(!world.exists("~/Projects/collapse/.mind/.gitignore"));
    let exclude = world.read("~/Projects/collapse/.git/info/exclude");
    assert!(!exclude.contains("commit-style"), "{exclude}");
    assert_eq!(
        at(&world.snapshot("~/Projects"), "/projects/0/settings/share"),
        "shared"
    );
}

#[test]
fn a_kind_sent_to_one_folder_moves_every_untouched_copy_there() {
    let world = collapse_for_claude();

    change(
        &world,
        Change::Targets {
            kind: Kind::Skill,
            folders: Some(vec![".skills".into()]),
        },
    )
    .expect("set the targets");

    assert!(world.exists("~/Projects/collapse/.skills/commit-style/SKILL.md"));
    assert!(!world.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(world.exists("~/Projects/collapse/.claude/skills/notes/SKILL.md"));
    assert!(world
        .read("~/Projects/collapse/.mind/settings.toml")
        .ends_with("\n[targets]\nskills = [\".skills\"]\n"));
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(
        at(&snapshot, "/projects/0/settings/targets"),
        &json!([{"kind": "skill", "folders": [".skills"]}])
    );
    // No agent reads `.skills` until it is pointed there.
    assert_eq!(at(&snapshot, "/projects/0/health"), "attention");
}
