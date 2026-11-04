//! Changing things from the screen: each form shows the line it is about to
//! run as it is filled in, runs it through the same parser as a line typed,
//! and the screen, the disk and the transcript printed on quitting all say
//! what it did.

use crate::{collapse_and_tanukeys, Key, Scenario};

#[test]
fn the_add_form_shows_the_line_as_it_is_typed_and_runs_it() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("a")
        .shows("Add to collapse")
        .shows("runs         mind -C collapse add skill '' ''")
        .types("review")
        .presses(Key::Tab)
        .types("Read a change")
        .shows("runs         mind -C collapse add skill review 'Read a change'");
    terminal
        .presses(Key::Enter)
        .shows(">   skill review        authored           Read a change")
        .shows("created skill review at /home/you/Projects/collapse/skills/review/SKILL.md")
        .shows("> collapse  3");

    terminal.types("q");
    let closed = terminal.exits();
    assert!(closed.status.success());
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse add skill review 'Read a change'\n",
            "created skill review at /home/you/Projects/collapse/skills/review/SKILL.md",
        )
    );
    assert!(scenario.exists("~/Projects/collapse/skills/review/SKILL.md"));
}

#[test]
fn the_add_form_runs_its_line_and_says_why_the_command_wrote_nothing() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("a")
        .shows("Add to collapse")
        .types("review")
        .presses(Key::Enter)
        .shows("a skill needs a description: one line saying what it is");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    // The line ran, and is handed back with what refused it, as every line
    // of a task that writes is, however it went.
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse add skill review ''\n",
            "error: a skill needs a description: one line saying what it is",
        )
    );
    assert!(!scenario.exists("~/Projects/collapse/skills/review"));
}

#[test]
fn unticking_agents_runs_the_agents_line_and_takes_their_copies_out() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("t")
        .shows("Agents and sharing for collapse")
        .presses(Key::Down)
        .types(" ")
        .presses(Key::Down)
        .types(" ")
        .shows("runs         mind -C collapse agents --yes claude")
        .presses(Key::Enter)
        .shows("set agents to claude in collapse, where they were claude,codex,opencode")
        .shows("removed commit-style from .agents/skills/commit-style")
        .presses(Key::Escape)
        .shows(">   skill commit-style  current  CC  How every repo here writes commits");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse agents --yes claude\n",
            "set agents to claude in collapse, where they were claude,codex,opencode\n",
            "removed commit-style from .agents/skills/commit-style",
        )
    );
    assert!(!scenario.exists("~/Projects/collapse/.agents/skills/commit-style"));
}

#[test]
fn update_brings_an_outdated_copy_current() {
    let scenario = collapse_and_tanukeys();
    scenario.file(
        "~/Projects/.mindflayer/library/skills/commit-style/SKILL.md",
        &crate::skill("commit-style", "How every repo here writes commits, now"),
    );
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows(">   skill commit-style  outdated CC CX OC");

    terminal
        .types("u")
        .shows("mind -C collapse update --yes")
        .shows("updated commit-style in collapse, from workspace")
        .presses(Key::Escape)
        .shows(">   skill commit-style  current  CC CX OC");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse update --yes\n",
            "updated commit-style in collapse, from workspace",
        )
    );
}

#[test]
fn deleting_an_artifact_asks_first_and_then_runs_remove() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .shows(">   skill deploy")
        .types("d")
        .shows("mind -C collapse remove --yes skill:deploy");
    terminal
        .types("y")
        .no_longer_shows("skill deploy        authored")
        .shows("removed skill deploy from /home/you/Projects/collapse/skills/deploy")
        .shows("> collapse  1");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse remove --yes skill:deploy\n",
            "removed skill deploy from /home/you/Projects/collapse/skills/deploy",
        )
    );
    assert!(!scenario.exists("~/Projects/collapse/skills/deploy"));
}

#[test]
fn linking_a_repository_asks_before_mind_init_writes_into_it() {
    let scenario = collapse_and_tanukeys();
    scenario.repository("~/Projects/site");
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  2");

    terminal
        .types("l")
        .shows("Link a project to Projects")
        .shows("> site  git repository (mind init first)")
        .shows("runs         mind -C site init && flayer link site")
        .presses(Key::Enter)
        .shows("site is not a mind project yet.")
        .shows("runs  mind -C site init && flayer link site");
    terminal
        .types("y")
        .shows("> site  0")
        .shows("linked site as site");

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
    scenario
        .run("~/Projects", "flayer status -p site")
        .succeeds()
        .quiet();
}

#[test]
fn saying_no_to_mind_init_writes_nothing_into_the_repository() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.repository("~/Projects/site");
    let mut terminal = scenario.open("~/Projects", "flayer");
    terminal.shows("collapse  0");

    terminal
        .types("l")
        .shows("> site  git repository (mind init first)")
        .presses(Key::Enter)
        .shows("site is not a mind project yet.")
        .types("n")
        .no_longer_shows("site is not a mind project yet.");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(closed.shown.trim_end(), "");
    assert!(!scenario.exists("~/Projects/site/.mind"));
}
