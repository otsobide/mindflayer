//! The two screens the main one opens: installing (`i`, or `flayer
//! install` straight from the shell) and the minds (`m`, or `flayer
//! minds`), each applying its marks as the lines they amount to.

use crate::{collapse_and_tanukeys, Key};

#[test]
fn the_install_screen_installs_what_was_marked_once_it_is_said_yes_to() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("i")
        .shows("Skills in collapse")
        .presses(Key::Down)
        .presses(Key::Right)
        .shows("Skills in tanukeys")
        .types(" ")
        .shows("> [x] commit-style  + install")
        .shows("1 to install, 0 to remove")
        .types("a")
        .shows("Install 1, remove 0?");
    terminal
        .types("y")
        .shows("flayer install -p tanukeys --from workspace skill:commit-style")
        .shows("installed commit-style into tanukeys, from workspace")
        .presses(Key::Escape)
        .shows("tanukeys  1");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ flayer install -p tanukeys --from workspace skill:commit-style\n",
            "installed commit-style into tanukeys, from workspace",
        )
    );
    assert!(scenario.exists("~/Projects/tanukeys/.claude/skills/commit-style/SKILL.md"));
}

#[test]
fn flayer_install_opens_the_install_screen_straight_from_the_shell() {
    let scenario = collapse_and_tanukeys();

    let mut terminal = scenario.open("~/Projects", "flayer install");

    terminal
        .shows("Skills in collapse")
        .shows("> [x] commit-style")
        .shows("nothing marked");
    terminal.types("q");
    let closed = terminal.exits();
    assert!(closed.status.success());
    assert_eq!(closed.shown.trim_end(), "nothing applied");
}

#[test]
fn the_minds_screen_makes_a_ticked_repository_a_project_and_links_it() {
    let scenario = collapse_and_tanukeys();
    scenario.repository("~/Projects/site");
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("m")
        .shows("Minds in Projects")
        .shows("> [ ] site      git repository (mind init first)")
        .shows("[x] collapse  linked")
        .types(" ")
        .shows("> [x] site      git repository (mind init first)  + mind init, then link")
        .types("a")
        .shows("Link 1?")
        .shows("runs  mind -C site init && flayer link site");
    terminal
        .types("y")
        .shows("linked site as site")
        .presses(Key::Escape)
        .shows("site  0");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C site init\n",
            "initialized mind project `site` in /home/you/Projects/site/.mind\n",
            "$ flayer link site\n",
            "linked site as site",
        )
    );
}
