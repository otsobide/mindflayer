//! Which workspace a front end gets: core's one function that decides it
//! (`resolve::resolve`), from what the command line names, what the shell
//! activated, the folders up the tree and the default in the home, and the
//! rule that chose it.

use mindflayer_core::resolve::{Intent, Reason};
use serde_json::json;

use crate::{at, World};

/// `~/Projects`, linking `api`, and `~/Notes`, a workspace of its own.
fn two_workspaces() -> World {
    let world = World::projects(&["api"]);
    world.folder("~/Notes").init_workspace("~/Notes");
    world
}

#[test]
fn the_workspace_a_command_line_names_is_used_over_the_one_above() {
    let world = two_workspaces();

    let resolved = world
        .resolve("~/Projects/api", Intent::Read, Some("~/Notes"), None)
        .expect("a workspace named")
        .expect("one to use");

    assert_eq!(resolved.reason, Reason::Explicit);
    assert_eq!(world.spelled(resolved.workspace.root()), "/home/you/Notes");
}

#[test]
fn an_activated_workspace_is_used_where_the_line_names_none() {
    let world = two_workspaces();

    let resolved = world
        .resolve("~/Projects/api", Intent::Read, None, Some("~/Notes"))
        .expect("a workspace activated")
        .expect("one to use");

    assert_eq!(resolved.reason, Reason::Activated);
    assert_eq!(world.spelled(resolved.workspace.root()), "/home/you/Notes");
}

#[test]
fn a_workspace_named_wins_over_the_one_activated() {
    let world = two_workspaces();

    let resolved = world
        .resolve("~", Intent::Read, Some("~/Projects"), Some("~/Notes"))
        .expect("a workspace named")
        .expect("one to use");

    assert_eq!(resolved.reason, Reason::Explicit);
    assert_eq!(
        world.spelled(resolved.workspace.root()),
        "/home/you/Projects"
    );
}

#[test]
fn the_first_workspace_up_the_tree_is_used_where_none_is_named_or_activated() {
    let world = two_workspaces();

    let resolved = world
        .resolve("~/Projects/api", Intent::Read, None, None)
        .expect("a workspace above")
        .expect("one to use");

    assert_eq!(resolved.reason, Reason::Above);
    assert_eq!(
        world.spelled(resolved.workspace.root()),
        "/home/you/Projects"
    );
}

#[test]
fn reading_where_no_workspace_is_reads_the_default_one_without_making_it() {
    let world = World::empty();

    let resolved = world
        .resolve("~", Intent::Read, None, None)
        .expect("the default")
        .expect("one to read");

    assert_eq!(resolved.reason, Reason::Home);
    assert_eq!(world.spelled(resolved.workspace.root()), "/home/you");
    assert!(!resolved.workspace.is_made());
    assert!(!world.exists("~/.mindflayer"));
}

#[test]
fn a_change_where_no_workspace_is_makes_the_default_one_in_the_home() {
    let world = World::empty();

    let resolved = world
        .resolve("~", Intent::Write, None, None)
        .expect("the default")
        .expect("one to change");

    assert_eq!(resolved.reason, Reason::Home);
    assert!(resolved.workspace.is_made());
    assert!(world.exists("~/.mindflayer/flayer.toml"));
}

#[test]
fn a_named_folder_that_is_no_workspace_is_refused_and_never_made() {
    let world = two_workspaces();
    world.folder("~/Archive");

    let refused = world
        .resolve("~/Projects", Intent::Write, Some("~/Archive"), None)
        .expect_err("no workspace there");

    assert_eq!(refused.code(), "not_a_workspace");
    assert_eq!(
        world.said(&refused),
        "the named workspace cannot be used: /home/you/Archive: not a flayer workspace \
         (no .mindflayer/flayer.toml)"
    );
    assert!(!world.exists("~/Archive/.mindflayer"));
}

#[test]
fn an_activated_folder_that_is_no_workspace_is_refused_rather_than_walked_past() {
    let world = two_workspaces();
    world.folder("~/Archive");

    let refused = world
        .resolve("~/Projects/api", Intent::Read, None, Some("~/Archive"))
        .expect_err("no workspace there");

    assert_eq!(refused.code(), "not_a_workspace");
    assert_eq!(
        world.said(&refused),
        "the activated workspace cannot be used: /home/you/Archive: not a flayer workspace \
         (no .mindflayer/flayer.toml)"
    );
}

#[test]
fn the_default_workspace_remembers_each_one_made_once_it_is_made_itself() {
    let world = World::empty();
    world
        .init_workspace("~")
        .folder("~/Projects")
        .init_workspace("~/Projects");

    let snapshot = world.snapshot("~/Projects");

    assert_eq!(
        at(&snapshot, "/known"),
        &json!([
            {
                "default": true, "in_use": false, "loadable": true, "loaded_as": null,
                "name": "you", "path": "/home/you", "projects": 0, "remembered": false,
                "stale": null, "stale_projects": 0,
            },
            {
                "default": false, "in_use": true, "loadable": false, "loaded_as": null,
                "name": "Projects", "path": "/home/you/Projects", "projects": 0,
                "remembered": true, "stale": null, "stale_projects": 0,
            },
        ])
    );
}

#[test]
fn a_loaded_workspaces_own_are_offered_until_it_is_unloaded() {
    let world = World::projects(&["collapse"]);
    world.folder("~/src/team").init_workspace("~/src/team").add(
        "~/src/team",
        "~/src/team",
        mindflayer_core::Kind::Skill,
        "deploy",
        "Ship the service to staging",
    );
    let mut workspace = world.workspace("~/Projects");

    workspace
        .load(&world.workspace("~/src/team"))
        .expect("load team");

    let offers = world.snapshot("~/Projects")["offers"].clone();
    assert_eq!(offers[0]["name"], "deploy");
    assert_eq!(offers[0]["origin"], "load:../src/team");

    workspace
        .unload(&world.at("~/src/team"))
        .expect("unload team");

    assert_eq!(world.snapshot("~/Projects")["offers"], json!([]));
}
