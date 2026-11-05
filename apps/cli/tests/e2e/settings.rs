//! Changing how a project is set up, and what is installed in it following:
//! the reference's "Changing its agents, or how it shares" and "Installing
//! somewhere else".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse`, a project for Claude alone, holding
/// `commit-style` from the library and `notes`, kept by hand where Claude
/// reads: the reference's.
fn collapse_for_claude() -> Scenario {
    let scenario = Scenario::empty();
    scenario
        .repository("~/Projects/collapse")
        .given("~/Projects/collapse", "mind init --agents claude")
        .given("~/Projects", "mindweaver init")
        .given("~/Projects", "mindweaver link collapse")
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .file(
            "~/Projects/collapse/.claude/skills/notes/SKILL.md",
            &skill("notes", "My own notes"),
        );
    scenario
}

/// [`collapse_for_claude`], with Codex added to its agents.
fn collapse_for_claude_and_codex() -> Scenario {
    let scenario = collapse_for_claude();
    scenario.given("~/Projects/collapse", "mind agents add codex --yes");
    scenario
}

#[test]
fn agents_without_yes_says_what_a_change_would_carry_and_changes_nothing() {
    let scenario = collapse_for_claude();
    let settings = scenario.read("~/Projects/collapse/.mind/settings.toml");

    scenario
        .run("~/Projects/collapse", "mind agents add codex")
        .succeeds()
        .prints(concat!(
            "would set agents to claude,codex in collapse, where they were claude\n",
            "would copy commit-style to .agents/skills/commit-style, for codex, kept out of git\n",
            "nothing changed: add --yes to go ahead\n",
        ))
        .quiet();
    assert_eq!(
        scenario.read("~/Projects/collapse/.mind/settings.toml"),
        settings
    );
    assert!(!scenario.exists("~/Projects/collapse/.agents"));
}

#[test]
fn an_agent_added_gets_a_copy_of_what_is_installed_where_it_reads() {
    let scenario = collapse_for_claude();

    scenario
        .run("~/Projects/collapse", "mind agents add codex --yes")
        .succeeds()
        .prints(concat!(
            "set agents to claude,codex in collapse, where they were claude\n",
            "copied commit-style to .agents/skills/commit-style, for codex, kept out of git\n",
        ))
        .quiet();
    assert_eq!(
        scenario.read("~/Projects/collapse/.agents/skills/commit-style/SKILL.md"),
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md")
    );
    scenario
        .run("~/Projects/collapse", "mind status")
        .succeeds()
        .prints(concat!(
            "commit-style  current  claude,codex  workspace\n",
            "notes         foreign  claude\n",
        ));
}

#[test]
fn an_agent_removed_loses_mindweavers_copy_and_somebodys_skill_stays() {
    let scenario = collapse_for_claude_and_codex();

    scenario
        .run("~/Projects/collapse", "mind agents remove claude --yes")
        .succeeds()
        .prints(concat!(
            "set agents to codex in collapse, where they were claude,codex\n",
            "removed commit-style from .claude/skills/commit-style\n",
            "left .claude/skills/notes where it is: Mindweaver did not install it\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(scenario.exists("~/Projects/collapse/.claude/skills/notes/SKILL.md"));
}

#[test]
fn turning_shared_hands_mind_and_the_copies_to_the_team_to_commit() {
    let scenario = collapse_for_claude_and_codex();

    scenario
        .run("~/Projects/collapse", "mind share shared --yes")
        .succeeds()
        .prints(concat!(
            "set share to shared in collapse, where it was local\n",
            "removed .mind/.gitignore, so .mind can be committed\n",
            "stopped keeping /.claude/skills/commit-style/ out of git\n",
            "stopped keeping /.agents/skills/commit-style/ out of git\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/Projects/collapse/.mind/.gitignore"));
    let exclude = scenario.read("~/Projects/collapse/.git/info/exclude");
    assert!(!exclude.contains("commit-style"), "{exclude}");
}

#[test]
fn mindweaver_agents_changes_a_project_from_the_workspace() {
    let scenario = collapse_for_claude();

    scenario
        .run(
            "~/Projects",
            "mindweaver agents -p collapse add codex --yes",
        )
        .succeeds()
        .prints(concat!(
            "set agents to claude,codex in collapse, where they were claude\n",
            "copied commit-style to .agents/skills/commit-style, for codex, kept out of git\n",
        ))
        .quiet();
}

#[test]
fn the_last_agent_is_not_removed() {
    let scenario = collapse_for_claude();
    let settings = scenario.read("~/Projects/collapse/.mind/settings.toml");

    scenario
        .run("~/Projects/collapse", "mind agents remove claude --yes")
        .exits(1)
        .prints("")
        .says(
            "error: collapse would name no agent, and what is installed into it would reach \
             none, so its last agent is not removed; `mindweaver uninstall` takes out what it has\n",
        );
    assert_eq!(
        scenario.read("~/Projects/collapse/.mind/settings.toml"),
        settings
    );
}

#[test]
fn targets_send_every_agents_copies_to_one_folder_and_warn_none_reads_it() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style");

    scenario
        .run("~/Projects/collapse", "mind targets set skills .skills --yes")
        .succeeds()
        .prints(concat!(
            "set the targets of skills to .skills in collapse, where they were the folders its agents read\n",
            "copied commit-style to .skills/commit-style, for claude, codex and opencode, kept out of git\n",
            "removed commit-style from .agents/skills/commit-style\n",
            "removed commit-style from .claude/skills/commit-style\n",
        ))
        .says(
            "note: claude, codex and opencode read none of the folders collapse installs skills \
             in (.skills), so none of them sees what is installed there until pointed at one\n",
        );
    assert!(scenario
        .read("~/Projects/collapse/.mind/settings.toml")
        .ends_with("\n[targets]\nskills = [\".skills\"]\n"));
    scenario
        .run("~/Projects/collapse", "mind status")
        .exits(1)
        .prints("commit-style  current  -  workspace\n")
        .says(
            "warning: claude, codex and opencode read none of the folders collapse installs \
             skills in (.skills), so none of them sees what is installed there until pointed at one\n",
        );
}

#[test]
fn targets_reset_sends_the_copies_back_where_each_agent_reads() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style");
    // Set as a step: it says a note, which a given step may not.
    scenario
        .run(
            "~/Projects/collapse",
            "mind targets set skills .skills --yes",
        )
        .succeeds();
    scenario.file(
        "~/Projects/collapse/.skills/notes/SKILL.md",
        &skill("notes", "My own notes"),
    );

    scenario
        .run("~/Projects/collapse", "mind targets reset skills --yes")
        .succeeds()
        .prints(concat!(
            "set the targets of skills to the folders its agents read in collapse, where they were .skills\n",
            "copied commit-style to .agents/skills/commit-style, for codex and opencode, kept out of git\n",
            "copied commit-style to .claude/skills/commit-style, for claude, kept out of git\n",
            "removed commit-style from .skills/commit-style\n",
            "left .skills/notes where it is: Mindweaver did not install it\n",
        ))
        .quiet();
    let settings = scenario.read("~/Projects/collapse/.mind/settings.toml");
    // The table taken out; the template's comments still say what one is.
    assert!(!settings.contains("\n[targets]\n"), "{settings}");
}
