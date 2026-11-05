//! Keeping copies up to date with where they came from: what an update of
//! every project would do (`update::Updates::read`), and doing it
//! (`update::apply`), which writes what is outdated or missing and leaves
//! what somebody changed.

use mindweaver_core::install::Edits;
use mindweaver_core::update::{self, Updates};
use mindweaver_core::Kind;
use serde_json::json;

use crate::World;

/// A skill's file as `add` writes it.
fn added(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}

/// `~/Projects` linking `api` and `web`, `commit-style` installed into
/// `api` and since changed in the library, and `review` installed into
/// `web` and since changed in its copy.
fn one_outdated_one_edited() -> World {
    let world = World::projects(&["api", "web"]);
    world
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "commit-style",
            "How every repo here writes commits",
        )
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "review",
            "Read a change before it lands",
        )
        .install("~/Projects", "~/Projects/api", "skill:commit-style")
        .install("~/Projects", "~/Projects/web", "skill:review")
        .file(
            "~/Projects/.mindweaver/library/skills/commit-style/SKILL.md",
            &added("commit-style", "How every repo here writes commits, now"),
        )
        .file(
            "~/Projects/web/.claude/skills/review/SKILL.md",
            &added("review", "Mine now"),
        );
    world
}

/// Every project `~/Projects` links, as an update of them all takes them.
fn every_project(world: &World) -> Vec<mindweaver_core::MindProject> {
    world.workspace("~/Projects").projects().0
}

#[test]
fn what_an_update_would_do_is_said_with_nothing_written() {
    let world = one_outdated_one_edited();
    let library = world.read("~/Projects/api/.claude/skills/commit-style/SKILL.md");

    let updates = Updates::read(&world.workspace("~/Projects")).expect("survey");

    assert_eq!(
        world.json(&updates),
        json!({
            "projects": [
                {
                    "entry": "api",
                    "installations": [{
                        "copies": [],
                        "does": "refresh",
                        "forced": false,
                        "kind": "skill",
                        "left": null,
                        "name": "commit-style",
                        "origin": "workspace",
                        "state": "outdated",
                    }],
                    "name": "api",
                    "writes": true,
                },
                {
                    "entry": "web",
                    "installations": [{
                        "copies": [".claude/skills/review"],
                        "does": "leave",
                        "forced": true,
                        "kind": "skill",
                        "left": {
                            "code": "edited",
                            "message": "skill `review` in web was changed since it was installed, \
                                        at .claude/skills/review, so an update leaves it alone, \
                                        every copy of it; forcing the update writes over the change",
                        },
                        "name": "review",
                        "origin": "workspace",
                        "state": "edited",
                    }],
                    "name": "web",
                    "writes": false,
                },
            ],
            "states": [
                {"count": 1, "state": "outdated"},
                {"count": 1, "state": "edited"},
                {"count": 0, "state": "missing"},
                {"count": 0, "state": "orphaned"},
            ],
            "unread": [],
            "writes": true,
        })
    );
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/commit-style/SKILL.md"),
        library
    );
}

#[test]
fn an_update_writes_what_is_outdated_and_leaves_what_somebody_changed() {
    let world = one_outdated_one_edited();
    let workspace = world.workspace("~/Projects");

    update::apply(
        &workspace,
        &every_project(&world),
        &[],
        Edits::Kept,
        &world.homes(),
    )
    .expect("update");

    let now = added("commit-style", "How every repo here writes commits, now");
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/commit-style/SKILL.md"),
        now
    );
    assert_eq!(
        world.read("~/Projects/api/.agents/skills/commit-style/SKILL.md"),
        now
    );
    assert_eq!(
        world.read("~/Projects/web/.claude/skills/review/SKILL.md"),
        added("review", "Mine now")
    );
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(snapshot["projects"][0]["artifacts"][0]["state"], "current");
    assert_eq!(snapshot["projects"][1]["artifacts"][0]["state"], "edited");
}

#[test]
fn a_copy_taken_away_by_hand_stands_missing_and_an_update_puts_it_back() {
    let world = World::projects(&["api"]);
    world
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "review",
            "Read a change before it lands",
        )
        .install("~/Projects", "~/Projects/api", "skill:review");
    std::fs::remove_dir_all(world.at("~/Projects/api/.agents/skills/review"))
        .expect("take the copy away by hand");
    assert_eq!(
        world.snapshot("~/Projects")["projects"][0]["artifacts"][0]["state"],
        "missing"
    );
    let workspace = world.workspace("~/Projects");

    update::apply(
        &workspace,
        &every_project(&world),
        &[],
        Edits::Kept,
        &world.homes(),
    )
    .expect("update");

    assert_eq!(
        world.read("~/Projects/api/.agents/skills/review/SKILL.md"),
        added("review", "Read a change before it lands")
    );
    assert_eq!(
        world.snapshot("~/Projects")["projects"][0]["artifacts"][0]["state"],
        "current"
    );
}
