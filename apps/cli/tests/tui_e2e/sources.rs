//! What a workspace takes from elsewhere, from the screen: a repository
//! gathered onto its shelf, watched while it runs, and another workspace
//! loaded, then forgotten.

use crate::{collapse_and_tanukeys, skill, Key, Scenario};

/// `~/Projects` as [`collapse_and_tanukeys`] has it, and `~/src/team`, a
/// workspace of its own offering `review`.
fn beside_team() -> Scenario {
    let scenario = collapse_and_tanukeys();
    scenario
        .folder("~/src/team")
        .given("~/src/team", "mindweaver init")
        .given(
            "~/src/team",
            "mindweaver add skill review \"Read a change\"",
        );
    scenario
}

#[test]
fn g_gathers_from_a_repository_and_the_header_counts_what_it_put_on_the_shelf() {
    let scenario = collapse_and_tanukeys();
    scenario.source(
        "~/src/acme-skills",
        &[("skills/lint/SKILL.md", &skill("lint", "Lint the code"))],
    );
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("nothing on the shelf");

    terminal
        .types("g")
        .shows("Gather skills from a git repository")
        .types("../src/acme-skills")
        .shows("mindweaver gather git ../src/acme-skills")
        .presses(Key::Enter)
        .shows("lint")
        .shows("1 on the shelf");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        format!(
            "$ mindweaver gather git ../src/acme-skills\n\
             ../src/acme-skills at {}\n\
             \x20 added  lint  Lint the code\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged",
            scenario.head("~/src/acme-skills")
        )
    );
    scenario
        .run("~/Projects", "mindweaver install -p tanukeys lint")
        .succeeds();
}

#[test]
fn capital_l_loads_another_workspace_and_its_own_are_offered_from_then_on() {
    let scenario = beside_team();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("L")
        .shows("Load")
        .types("../src/team")
        .shows("mindweaver load ../src/team")
        .presses(Key::Enter)
        .shows("1 loaded");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver load ../src/team\n",
            "loaded team as ../src/team: 1 skill to install\n",
            "  into the workspace at /home/you/Projects",
        )
    );
    scenario
        .run("~/Projects", "mindweaver install -p tanukeys review")
        .succeeds()
        .prints("installed review into tanukeys, from load:../src/team\n");
}

#[test]
fn enter_on_a_loaded_workspace_in_the_load_form_unloads_it_once_asked() {
    let scenario = beside_team();
    scenario.given("~/Projects", "mindweaver load ../src/team");
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("1 loaded");

    terminal
        .types("L")
        .shows("team · 1 to install")
        .presses(Key::Enter)
        .shows("Unload");
    terminal.types("y").no_longer_shows("1 loaded");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver unload ../src/team\n",
            "unloaded ../src/team\n",
            "  from the workspace at /home/you/Projects",
        )
    );
    scenario
        .run("~/Projects", "mindweaver load")
        .succeeds()
        .prints(concat!(
            "nothing loaded\n",
            "  offer another workspace's own with `mindweaver load <path>`\n",
            "  in the workspace at /home/you/Projects\n",
        ));
}
