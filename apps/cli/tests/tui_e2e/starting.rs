//! Opening the screen and leaving it: where there is a workspace, where
//! there is only the default one, and where there is none, and the
//! terminal given back as it was, with the session's transcript.

use crate::{collapse_and_tanukeys, Key, Scenario};

#[test]
fn quitting_with_nothing_changed_prints_nothing_and_gives_the_terminal_back() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.types("q");
    let closed = terminal.exits();

    assert!(closed.status.success());
    assert_eq!(closed.shown.trim_end(), "");
}

#[test]
fn ctrl_c_quits_as_q_does_since_the_screen_keeps_it_for_itself() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.presses(Key::CtrlC);
    let closed = terminal.exits();

    assert!(closed.status.success());
    assert_eq!(closed.shown.trim_end(), "");
}

#[test]
fn where_no_workspace_is_it_opens_the_default_one_without_making_it() {
    let scenario = Scenario::empty();
    scenario.folder("~/notes");

    let mut terminal = scenario.open("~/notes", "mindweaver");

    terminal
        .draws(
            0,
            &[" mindweaver  you  · default workspace, not made yet · nothing on the shelf  · /home/you"],
        )
        .shows("> ◆ you  0")
        .shows("skills go in .mindweaver/library/skills/");
    terminal.types("q");
    let closed = terminal.exits();
    assert!(closed.status.success());
    assert!(!scenario.exists("~/.mindweaver"));
}

#[test]
fn with_no_default_either_it_offers_to_make_a_workspace_where_it_was_opened() {
    let scenario = Scenario::empty();
    scenario.folder("~/notes");
    let mut terminal = scenario.open("~/notes", "mindweaver --home=");

    terminal
        .shows("No weaver workspace here")
        .shows("Nothing at or above this directory is one:")
        .shows("/home/you/notes")
        .shows("enter  create one here  runs `mindweaver init`");
    terminal
        .presses(Key::Enter)
        .shows(" mindweaver  notes  · found above · nothing on the shelf  · /home/you/notes")
        .shows("initialized weaver workspace `notes` in /home/you/notes/.mindweaver");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver init\n",
            "initialized weaver workspace `notes` in /home/you/notes/.mindweaver",
        )
    );
    assert!(scenario.exists("~/notes/.mindweaver/weaver.toml"));
}

#[test]
fn quitting_where_nothing_was_made_leaves_the_folder_as_it_was() {
    let scenario = Scenario::empty();
    scenario.folder("~/notes");
    let mut terminal = scenario.open("~/notes", "mindweaver --home=");
    terminal.shows("No weaver workspace here");

    terminal.types("q");
    let closed = terminal.exits();

    assert!(closed.status.success());
    assert!(!scenario.exists("~/notes/.mindweaver"));
}
