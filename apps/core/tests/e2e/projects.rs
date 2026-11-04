//! Making projects and linking them: a project's marker as `mind init`
//! has core write it, a workspace's registry as `flayer link` changes it,
//! and a project found again by its id once it moved.

use mindflayer_core::relink;
use mindflayer_core::{Agent, Directories, Initialization, MindProject, Registration, Share};
use serde_json::json;

use crate::{at, World};

#[test]
fn a_new_project_is_marked_by_settings_written_from_their_template() {
    let world = World::empty();
    world.repository("~/Projects/api");

    let (_, made) = MindProject::init_asked(
        world.at("~/Projects/api"),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .expect("make the project");

    assert_eq!(made, Initialization::Created);
    let id = world.project_id("~/Projects/api");
    let settings = world.read("~/Projects/api/.mind/settings.toml");
    let (comments, values) = settings
        .split_once("version = ")
        .expect("the comments, then the values");
    assert!(
        comments.starts_with("# Mindflayer mind project: what Mindflayer needs to know"),
        "{comments}"
    );
    assert_eq!(
        values,
        format!(
            "5\nid = \"{id}\"\nname = \"api\"\nagents = [\"claude\", \"codex\", \"opencode\"]\n\
             share = \"local\"\nedit_tracked = false\n\n[directories]\nskills = \"skills\"\n\
             rules = \"rules\"\n"
        )
    );
    assert_eq!(id.len(), 32, "{id}");
    // Kept out of git for this checkout alone, by a file inside the folder
    // it is about.
    assert_eq!(
        world.read("~/Projects/api/.mind/.gitignore"),
        "# Mindflayer keeps this project's settings here, for this checkout alone.\n\
         # Nothing in this folder is meant to be committed.\n*\n"
    );
    assert!(world.exists("~/Projects/api/skills"));
    assert!(world.exists("~/Projects/api/rules"));
}

#[test]
fn a_project_made_again_keeps_its_marker_and_its_id() {
    let world = World::projects(&["api"]);
    let before = world.read("~/Projects/api/.mind/settings.toml");

    let (_, made) = MindProject::init_asked(
        world.at("~/Projects/api"),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .expect("make the project again");

    assert_eq!(made, Initialization::AlreadyInitialized);
    assert_eq!(world.read("~/Projects/api/.mind/settings.toml"), before);
}

#[test]
fn linking_a_project_registers_it_by_its_route_from_the_workspace() {
    let world = World::projects(&[]);
    world
        .repository("~/Projects/apps/api")
        .init_project("~/Projects/apps/api");
    let mut workspace = world.workspace("~/Projects");

    let (entry, registration) = workspace
        .link(&world.project("~/Projects/apps/api"))
        .expect("link it");

    assert_eq!(registration, Registration::Added);
    assert_eq!(entry.to_str(), Some("apps/api"));
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(at(&snapshot, "/projects/0/entry"), "apps/api");
    assert_eq!(at(&snapshot, "/projects/0/name"), "api");
    assert_eq!(at(&snapshot, "/projects/0/health"), "ok");
}

#[test]
fn linking_a_project_already_linked_changes_nothing_and_says_so() {
    let world = World::projects(&["api"]);
    let registry = world.read("~/Projects/.mindflayer/flayer.toml");
    let mut workspace = world.workspace("~/Projects");

    let (entry, registration) = workspace
        .link(&world.project("~/Projects/api"))
        .expect("link it again");

    assert_eq!(registration, Registration::AlreadyRegistered);
    assert_eq!(entry.to_str(), Some("api"));
    assert_eq!(world.read("~/Projects/.mindflayer/flayer.toml"), registry);
}

#[test]
fn a_project_moved_by_hand_reads_as_stale_where_it_was() {
    let world = World::projects(&["api"]);
    std::fs::rename(world.at("~/Projects/api"), world.at("~/Projects/service"))
        .expect("move the project by hand");

    let snapshot = world.snapshot("~/Projects");

    let project = at(&snapshot, "/projects/0");
    assert_eq!(project["entry"], "api");
    assert_eq!(project["health"], "stale");
    assert_eq!(
        project["stale"],
        json!({
            "code": "not_a_project",
            "message": "/home/you/Projects/api: not a mind project (no .mind/settings.toml, \
                        .mind/settings.json or .mind/mind.toml)",
        })
    );
}

#[test]
fn a_moved_project_is_relinked_by_its_id_with_what_was_installed_in_it() {
    let world = World::projects(&["api"]);
    world
        .add(
            "~/Projects",
            "~/Projects",
            mindflayer_core::Kind::Skill,
            "review",
            "Read a change before it lands",
        )
        .install("~/Projects", "~/Projects/api", "skill:review");
    let id = world.project_id("~/Projects/api");
    std::fs::rename(world.at("~/Projects/api"), world.at("~/Projects/service"))
        .expect("move the project by hand");
    let mut workspace = world.workspace("~/Projects");

    let relinked = relink::relink(
        &mut workspace,
        &world.at("~/Projects/api"),
        &world.at("~/Projects/service"),
    )
    .expect("relink it");

    assert_eq!(relinked.name, "api");
    assert_eq!(relinked.entry.to_str(), Some("service"));
    assert_eq!(relinked.id.to_string(), id);
    assert_eq!(relinked.installations, 1);
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(at(&snapshot, "/projects/0/entry"), "service");
    assert_eq!(at(&snapshot, "/projects/0/health"), "ok");
    assert_eq!(
        at(&snapshot, "/projects/0/artifacts/0"),
        &json!({
            "agents": ["claude", "codex", "opencode"],
            "issues": [],
            "kind": "skill",
            "name": "review",
            "origin": "workspace",
            "state": "current",
            "summary": "Read a change before it lands",
            "targets": [".agents/skills/review", ".claude/skills/review"],
            "unread": null,
        })
    );
}
