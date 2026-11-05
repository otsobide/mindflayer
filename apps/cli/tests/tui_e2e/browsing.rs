//! Looking around the screen: what `mindweaver` and `mind` open on, moving
//! over the projects, and the keys and what each runs.

use crate::{collapse_and_tanukeys, Key};

#[test]
fn mindweaver_opens_on_the_workspaces_own_then_each_project_and_what_the_first_holds() {
    let scenario = collapse_and_tanukeys();

    let mut terminal = scenario.open("~/Projects", "mindweaver");

    terminal.draws(
        0,
        &[
            " mindweaver  Projects  · found above · nothing on the shelf  · /home/you/Projects",
            "┏ Projects (2) ━━━━━━━━━━━━━━━━━━━━┓┌ collapse ────────────────────────────────────────────────────────────────────────┐",
            "┃  ◆ Projects  1                   ┃│>   skill commit-style  current  CC CX OC  How every repo here writes commits     │",
            "┃> collapse  2                     ┃│    skill deploy        authored           Ship the service to staging            │",
            "┃  tanukeys  0                     ┃│                                                                                  │",
            "┃                                  ┃│                                                                                  │",
        ],
    );
    terminal.draws(
        21,
        &[
            "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└──────────────────────────────────────────────────────────────────────────────────┘",
            "q quit  ? keys  → open  a add  u update  m minds  l link  U unlink  v validate  g gather  L load  s shelf  i install  t",
            "every change is a command, printed when you quit; ? lists them",
        ],
    );
}

#[test]
fn moving_down_shows_what_the_next_project_holds_and_where_it_keeps_it() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.presses(Key::Down).draws(
        1,
        &[
            "┏ Projects (2) ━━━━━━━━━━━━━━━━━━━━┓┌ tanukeys ────────────────────────────────────────────────────────────────────────┐",
            "┃  ◆ Projects  1                   ┃│nothing here yet                                                                  │",
            "┃  collapse  2                     ┃│                                                                                  │",
            "┃> tanukeys  0                     ┃│skills go in skills/                                                              │",
            "┃                                  ┃│rules go in rules/                                                                │",
            "┃                                  ┃│                                                                                  │",
            "┃                                  ┃│a adds a skill or a rule                                                          │",
        ],
    );
}

#[test]
fn the_keys_say_what_each_runs_as_a_command_line() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.types("?").draws(
        1,
        &[
            "┏ keys ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓",
            "┃Every key here runs a command you could type instead.                                                                 ┃",
            "┃                                                                                                                      ┃",
            "┃  a      add a skill or a rule                                                                                        ┃",
            "┃         mind -C <project> add <kind> <name> <description>                                                            ┃",
            "┃  enter  show an artifact                                                                                             ┃",
            "┃         mind -C <project> show <kind:name>                                                                           ┃",
            "┃  e      edit it, in $VISUAL or $EDITOR                                                                               ┃",
            "┃         mind -C <project> edit <kind:name>                                                                           ┃",
            "┃  r      rename it                                                                                                    ┃",
            "┃         mind -C <project> rename <kind:name> <new-name>                                                              ┃",
        ],
    );
}

#[test]
fn validate_shows_what_mindweaver_validate_says_of_everything() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("v")
        .shows("3 skills checked, 0 invalid")
        .shows("commit-style (workspace): ok")
        .shows("commit-style (collapse): ok")
        .shows("deploy (collapse): ok");
}

#[test]
fn mind_opens_on_the_project_it_runs_in_with_its_artifacts_under_the_cursor() {
    let scenario = collapse_and_tanukeys();

    let mut terminal = scenario.open("~/Projects/collapse", "mind");

    terminal.shows("> collapse  2").draws(
        22,
        &["q quit  ? keys  enter show  e edit  r rename  d delete  u update  A adopt  a add  ← back  v validate"],
    );
}
