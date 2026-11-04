//! Bringing a skill somebody keeps in a project into the library: the
//! reference's "A repository that already has .claude/skills".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse`, which keeps `notes` by hand where
/// Claude reads, a file git does not track, so the adoption keeps it out of
/// git as the project's local share has everything installed there.
fn collapse_keeping_notes() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.file(
        "~/Projects/collapse/.claude/skills/notes/SKILL.md",
        &skill("notes", "My own notes"),
    );
    scenario
}

#[test]
fn adopt_without_yes_says_what_it_would_copy_and_record_and_writes_nothing() {
    let scenario = collapse_keeping_notes();

    scenario
        .run("~/Projects", "flayer adopt -p collapse notes")
        .succeeds()
        .prints(concat!(
            "would copy notes from collapse into the library, at /home/you/Projects/.mindflayer/library/skills/notes\n",
            "would record notes in collapse as installed from workspace, at .claude/skills/notes\n",
            "would record .agents/skills/notes missing, for `flayer update -p collapse` to put there\n",
            "would keep .claude/skills/notes out of git\n",
            "nothing adopted: add --yes to go ahead\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/Projects/.mindflayer/library/skills/notes"));
    scenario
        .run("~/Projects", "flayer status -p collapse")
        .succeeds()
        .prints("notes  foreign  claude,opencode\n");
}

#[test]
fn an_adopted_skill_is_the_librarys_and_its_copy_is_recorded_as_it_is() {
    let scenario = collapse_keeping_notes();
    let copy = scenario.read("~/Projects/collapse/.claude/skills/notes/SKILL.md");

    scenario
        .run("~/Projects", "flayer adopt -p collapse notes --yes")
        .succeeds()
        .prints(concat!(
            "copied notes from collapse into the library, at /home/you/Projects/.mindflayer/library/skills/notes\n",
            "recorded notes in collapse as installed from workspace, at .claude/skills/notes\n",
            "recorded .agents/skills/notes missing: `flayer update -p collapse` puts it there\n",
            "kept .claude/skills/notes out of git\n",
        ))
        .quiet();
    assert_eq!(
        scenario.read("~/Projects/.mindflayer/library/skills/notes/SKILL.md"),
        copy
    );
    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/notes/SKILL.md"),
        copy
    );
    assert!(!scenario.exists("~/Projects/collapse/.agents/skills/notes"));
}

#[test]
fn an_update_after_an_adoption_gives_every_agent_its_copy() {
    let scenario = collapse_keeping_notes();
    scenario.given("~/Projects", "flayer adopt -p collapse notes --yes");

    scenario
        .run("~/Projects", "flayer update -p collapse --yes")
        .succeeds()
        .prints("put back notes in collapse at .agents/skills/notes, from workspace\n")
        .quiet();
    scenario
        .run("~/Projects", "flayer status -p collapse")
        .succeeds()
        .prints("notes  current  claude,codex,opencode  workspace\n");
}

#[test]
fn a_name_the_library_holds_already_is_not_adopted() {
    let scenario = collapse_keeping_notes();
    scenario.given("~/Projects", "flayer add skill notes \"The team's notes\"");
    let library = scenario.read("~/Projects/.mindflayer/library/skills/notes/SKILL.md");

    scenario
        .run("~/Projects", "flayer adopt -p collapse notes --yes")
        .exits(1)
        .prints("")
        .says(
            "error: the library holds `notes` already, at \
             /home/you/Projects/.mindflayer/library/skills/notes, so \
             /home/you/Projects/collapse/.claude/skills/notes is not adopted: rename one of them \
             first\n",
        );
    assert_eq!(
        scenario.read("~/Projects/.mindflayer/library/skills/notes/SKILL.md"),
        library
    );
}
