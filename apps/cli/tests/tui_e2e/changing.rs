//! Changing things from the screen: each form shows the line it is about to
//! run as it is filled in, runs it through the same parser as a line typed,
//! and the screen, the disk and the transcript printed on quitting all say
//! what it did.

use crate::{collapse_and_tanukeys, Key, Scenario};

#[test]
fn the_add_form_shows_the_line_as_it_is_typed_and_runs_it() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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
        "~/Projects/.mindweaver/library/skills/commit-style/SKILL.md",
        &crate::skill("commit-style", "How every repo here writes commits, now"),
    );
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("l")
        .shows("Link a project to Projects")
        .shows("> site  git repository (mind init first)")
        .shows("runs         mind -C site init && mindweaver link site")
        .presses(Key::Enter)
        .shows("site is not a mind project yet.")
        .shows("runs  mind -C site init && mindweaver link site");
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
            "$ mindweaver link site\n",
            "linked site as site",
        )
    );
    scenario
        .run("~/Projects", "mindweaver status -p site")
        .succeeds()
        .quiet();
}

#[test]
fn saying_no_to_mind_init_writes_nothing_into_the_repository() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.repository("~/Projects/site");
    let mut terminal = scenario.open("~/Projects", "mindweaver");
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

#[test]
fn r_renames_from_a_form_that_starts_out_holding_the_old_name() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .types("r")
        .shows("mind -C collapse rename skill:deploy deploy")
        .presses(Key::CtrlU)
        .types("ship")
        .shows("mind -C collapse rename skill:deploy ship")
        .presses(Key::Enter)
        .shows(">   skill ship          authored           Ship the service to staging");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse rename skill:deploy ship\n",
            "renamed skill deploy to ship at /home/you/Projects/collapse/skills/ship",
        )
    );
    assert!(scenario.exists("~/Projects/collapse/skills/ship/SKILL.md"));
    assert!(!scenario.exists("~/Projects/collapse/skills/deploy"));
}

#[test]
fn e_hands_the_terminal_to_the_editor_and_takes_it_back_once_it_is_closed() {
    let scenario = collapse_and_tanukeys();
    scenario.file(
        "~/draft.md",
        &crate::skill("deploy", "Ship the service to production"),
    );
    // An editor that asks before it saves, so the test sees the terminal
    // while it is the editor's: `sh` runs it, the file last, as git runs
    // an editor.
    let editor = format!(
        "printf 'save the draft? '; read answer; cp {}",
        scenario.at("~/draft.md").display()
    );
    let mut terminal = scenario.open_with("~/Projects", "mindweaver", &[("EDITOR", &editor)]);
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .types("e")
        .steps_aside()
        .shows("save the draft?");
    terminal
        .types("y\r")
        .comes_back()
        .shows("edited skill deploy: ok");
    terminal
        .presses(Key::Escape)
        .shows(">   skill deploy        authored           Ship the service to production");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert!(closed.status.success());
    // What the editor wrote while the terminal was its own stays where it
    // wrote it, above the transcript, as it does in a shell.
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "save the draft? y\n",
            "$ mind -C collapse edit skill:deploy\n",
            "edited skill deploy: ok",
        )
    );
    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        crate::skill("deploy", "Ship the service to production")
    );
}

#[test]
fn an_editor_that_fails_leaves_the_artifact_as_it_was_and_the_screen_says_why() {
    let scenario = collapse_and_tanukeys();
    let before = scenario.read("~/Projects/collapse/skills/deploy/SKILL.md");
    let mut terminal = scenario.open_with("~/Projects", "mindweaver", &[("EDITOR", "false")]);
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .types("e")
        .comes_back()
        .shows("`false` failed (exit status: 1), so the file is reported on no further");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mind -C collapse edit skill:deploy\n",
            "error: `false` failed (exit status: 1), so the file is reported on no further",
        )
    );
    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        before
    );
}

#[test]
fn capital_u_unlinks_the_project_once_asked_and_leaves_its_folder_as_it_is() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Down)
        .shows("> tanukeys  0")
        .types("U")
        .shows("Unlink tanukeys?");
    terminal
        .types("y")
        .no_longer_shows("tanukeys  0")
        .shows("Projects (1)");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        "$ mindweaver unlink tanukeys\nunlinked tanukeys"
    );
    assert!(scenario.exists("~/Projects/tanukeys/.mind/settings.toml"));
}

#[test]
fn capital_a_adopts_a_skill_somebody_keeps_once_asked() {
    let scenario = collapse_and_tanukeys();
    scenario.file(
        "~/Projects/collapse/.claude/skills/notes/SKILL.md",
        &crate::skill("notes", "My own notes"),
    );
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  3");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .presses(Key::Down)
        .shows(">   skill notes         foreign")
        .types("A")
        .shows("Adopt notes from collapse into the library?");
    terminal
        .types("y")
        .shows("copied notes from collapse into the library");

    terminal
        .presses(Key::Escape)
        .presses(Key::Escape)
        .types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver adopt -p collapse --yes skill:notes\n",
            "copied notes from collapse into the library, at /home/you/Projects/.mindweaver/library/skills/notes\n",
            "recorded notes in collapse as installed from workspace, at .claude/skills/notes\n",
            "recorded .agents/skills/notes missing: `mindweaver update -p collapse` puts it there\n",
            "kept .claude/skills/notes out of git",
        )
    );
    assert!(scenario.exists("~/Projects/.mindweaver/library/skills/notes/SKILL.md"));
}

#[test]
fn capital_t_sends_skills_to_the_folders_typed_and_the_row_says_no_agent_reads_them() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("T")
        .shows("Where collapse installs skills")
        .types(".skills")
        .shows("mind -C collapse targets set --yes skills .skills")
        .presses(Key::Enter)
        .shows("removed commit-style from .claude/skills/commit-style");
    terminal.presses(Key::Escape).shows("> collapse  2  ⚠");

    terminal.types("q");
    let closed = terminal.exits();
    assert!(
        closed
            .shown
            .starts_with("$ mind -C collapse targets set --yes skills .skills\n"),
        "{}",
        closed.shown
    );
    assert_eq!(
        scenario.files("~/Projects/collapse/.skills"),
        ["commit-style/SKILL.md"]
    );
}

#[test]
fn capital_m_follows_a_project_the_scan_found_moved_with_what_was_installed_in_it() {
    let scenario = collapse_and_tanukeys();
    scenario.folder("~/Projects/apps");
    std::fs::rename(
        scenario.at("~/Projects/collapse"),
        scenario.at("~/Projects/apps/collapse"),
    )
    .expect("move the project by hand");
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal
        .shows("! collapse")
        .shows("it moved to apps/collapse: M relinks it there");

    terminal
        .types("M")
        .shows("Relink collapse to apps/collapse?");
    terminal.types("y").shows("> collapse  2");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver relink collapse apps/collapse\n",
            "relinked collapse as apps/collapse, from collapse\n",
            "  1 install kept with it",
        )
    );
    scenario
        .run("~/Projects", "mindweaver status -p collapse")
        .succeeds()
        .prints(
            "commit-style  current   claude,codex,opencode  workspace\n\
             deploy        authored  -\n",
        );
}
