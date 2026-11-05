//! Moving what an older build made to what this one writes, only when
//! asked and a dry run first: the reference's "Upgrading".

use crate::Scenario;

/// The home's workspace as `mindweaver init` made one before the library:
/// keeping its own in `~/skills` and `~/rules`, a skill and a rule there.
fn home_before_the_library() -> Scenario {
    let scenario = Scenario::empty();
    scenario
        .given("~", "mindweaver init --skills skills --rules rules")
        .given(
            "~",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given(
            "~",
            "mindweaver add rule git/no-force-push \"Never force-push a shared branch\"",
        );
    scenario
}

/// `~/Projects` linking `collapse`, a project an older build made,
/// marked by a `mind.toml`: `commit-style` installed from the library into
/// its `skills/`, beside `deploy`, its own.
fn collapse_marked_by_a_mind_toml() -> Scenario {
    let scenario = Scenario::empty();
    scenario
        .older_project("~/Projects/collapse")
        .given("~/Projects", "mindweaver init")
        .given("~/Projects", "mindweaver link collapse")
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .given(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        );
    scenario
}

#[test]
fn migrating_the_library_without_yes_says_what_would_move_and_moves_nothing() {
    let scenario = home_before_the_library();
    let marker = scenario.read("~/.mindweaver/weaver.toml");

    scenario
        .run("~", "mindweaver migrate --library")
        .succeeds()
        .prints(concat!(
            "would move /home/you/skills/commit-style to /home/you/.mindweaver/library/skills/commit-style\n",
            "would move /home/you/rules/git/no-force-push.md to /home/you/.mindweaver/library/rules/git/no-force-push.md\n",
            "would point skills and rules in /home/you/.mindweaver/weaver.toml at the library, \
             keeping it as it is in /home/you/.mindweaver/weaver.toml.v2\n",
            "nothing moved: add --yes to go ahead\n",
        ))
        .quiet();
    assert_eq!(scenario.read("~/.mindweaver/weaver.toml"), marker);
    assert!(scenario.exists("~/skills/commit-style/SKILL.md"));
}

#[test]
fn migrating_the_library_moves_each_of_its_own_and_keeps_the_old_marker() {
    let scenario = home_before_the_library();
    let marker = scenario.read("~/.mindweaver/weaver.toml");

    scenario
        .run("~", "mindweaver migrate --library --yes")
        .succeeds()
        .prints(concat!(
            "moved /home/you/skills/commit-style to /home/you/.mindweaver/library/skills/commit-style\n",
            "moved /home/you/rules/git/no-force-push.md to /home/you/.mindweaver/library/rules/git/no-force-push.md\n",
            "pointed skills and rules in /home/you/.mindweaver/weaver.toml at the library, \
             kept as it was in /home/you/.mindweaver/weaver.toml.v2\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/skills"));
    assert!(!scenario.exists("~/rules"));
    assert!(scenario.exists("~/.mindweaver/library/skills/commit-style/SKILL.md"));
    assert!(scenario.exists("~/.mindweaver/library/rules/git/no-force-push.md"));
    assert_eq!(scenario.read("~/.mindweaver/weaver.toml.v2"), marker);
}

#[test]
fn migrating_the_library_again_has_nothing_to_move() {
    let scenario = home_before_the_library();
    scenario.given("~", "mindweaver migrate --library --yes");

    scenario
        .run("~", "mindweaver migrate --library --yes")
        .succeeds()
        .prints("nothing to move: the workspace keeps its own in its library already\n")
        .quiet();
}

#[test]
fn migrating_a_project_without_yes_says_where_each_copy_would_go() {
    let scenario = collapse_marked_by_a_mind_toml();

    scenario
        .run("~/Projects/collapse", "mind migrate")
        .succeeds()
        .prints(concat!(
            "would write .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n",
            "would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is\n",
            "would copy commit-style to .agents/skills/commit-style, for codex and opencode\n",
            "would copy commit-style to .claude/skills/commit-style, for claude\n",
            "would remove commit-style from skills/commit-style\n",
            "would leave skills/deploy where it is: the project's own\n",
            "nothing changed: add --yes to go ahead\n",
        ))
        .quiet();
    assert!(scenario.exists("~/Projects/collapse/.mind/mind.toml"));
    assert!(!scenario.exists("~/Projects/collapse/.mind/settings.toml"));
}

#[test]
fn migrating_a_project_moves_its_copies_where_each_agent_reads() {
    let scenario = collapse_marked_by_a_mind_toml();

    scenario
        .run("~/Projects/collapse", "mind migrate --yes")
        .succeeds()
        .prints(concat!(
            "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n",
            "moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n",
            "copied commit-style to .agents/skills/commit-style, for codex and opencode\n",
            "copied commit-style to .claude/skills/commit-style, for claude\n",
            "removed commit-style from skills/commit-style\n",
            "left skills/deploy where it is: the project's own\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/Projects/collapse/skills/commit-style"));
    assert!(scenario.exists("~/Projects/collapse/skills/deploy/SKILL.md"));
    scenario
        .run("~/Projects/collapse", "mind status")
        .succeeds()
        .prints(concat!(
            "commit-style  current   claude,codex,opencode  workspace\n",
            "deploy        authored  -\n",
        ));
}

#[test]
fn migrating_a_project_again_has_nothing_to_do() {
    let scenario = collapse_marked_by_a_mind_toml();
    scenario.given("~/Projects/collapse", "mind migrate --yes");

    scenario
        .run("~/Projects/collapse", "mind migrate --yes")
        .succeeds()
        .prints("nothing to migrate: collapse is marked by its settings.toml already\n")
        .quiet();
}
