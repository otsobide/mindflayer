//! Offering another workspace's own without copying it: the reference's
//! "Loading another workspace", what is loaded, forgetting one, and naming
//! where an install comes from when two places offer the name.

use crate::Scenario;

/// `~/Projects` linking `collapse`, loading `~/src/team`, a workspace of
/// its own offering `deploy`.
fn loading_team() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "flayer init")
        .given(
            "~/src/team",
            "flayer add skill deploy \"Ship the service to staging\"",
        )
        .given("~/Projects", "flayer load ../src/team");
    scenario
}

#[test]
fn load_alone_says_what_is_loaded_and_what_each_offers() {
    let scenario = loading_team();

    scenario
        .run("~/Projects", "flayer load")
        .succeeds()
        .prints(concat!(
            "../src/team  team  1 skill to install\n",
            "  in the workspace at /home/you/Projects\n",
        ))
        .quiet();
}

#[test]
fn unload_forgets_a_loaded_workspace_and_leaves_what_was_installed_from_it() {
    let scenario = loading_team();
    scenario.given("~/Projects", "flayer install -p collapse deploy");

    scenario
        .run("~/Projects", "flayer unload ../src/team")
        .succeeds()
        .prints("unloaded ../src/team\n  from the workspace at /home/you/Projects\n")
        .quiet();
    assert!(scenario.exists("~/Projects/collapse/.claude/skills/deploy/SKILL.md"));
    scenario
        .run("~/Projects", "flayer status -p collapse")
        .succeeds()
        .prints("deploy  orphaned  claude,codex,opencode  load:../src/team\n");
}

#[test]
fn mind_load_loads_into_the_workspace_above_the_project() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "flayer init")
        .given(
            "~/src/team",
            "flayer add skill deploy \"Ship the service to staging\"",
        );

    scenario
        .run("~/Projects/collapse", "mind load ../../src/team")
        .succeeds()
        .prints(concat!(
            "loaded team as ../src/team: 1 skill to install\n",
            "  into the workspace at /home/you/Projects\n",
        ))
        .quiet();
}

#[test]
fn a_name_two_places_offer_is_installed_from_the_one_from_names() {
    let scenario = loading_team();
    scenario.given(
        "~/Projects",
        "flayer add skill deploy \"Ship the service to production\"",
    );

    scenario
        .run(
            "~/Projects",
            "flayer install -p collapse deploy --from load:../src/team",
        )
        .succeeds()
        .prints("installed deploy into collapse, from load:../src/team\n")
        .quiet();
    assert!(scenario
        .read("~/Projects/collapse/.claude/skills/deploy/SKILL.md")
        .contains("description: Ship the service to staging\n"));
}

#[test]
fn a_name_two_places_offer_is_not_installed_until_from_says_which() {
    let scenario = loading_team();
    scenario.given(
        "~/Projects",
        "flayer add skill deploy \"Ship the service to production\"",
    );

    scenario
        .run("~/Projects", "flayer install -p collapse deploy")
        .exits(1)
        .prints("")
        .says(
            "error: `deploy` is offered by more than one place (workspace, load:../src/team), \
             so say which with --from\n",
        );
    assert!(!scenario.exists("~/Projects/collapse/.claude/skills/deploy"));
}
