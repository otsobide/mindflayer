//! Removing one of the workspace's own from every project it reached, and
//! taking out what nothing offers any more: the reference's "Removing one
//! everywhere".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse` and `tanukeys`, `commit-style` from the
/// library installed into both, and `tanukeys`'s copy changed since.
fn installed_twice_one_edited() -> Scenario {
    let scenario = Scenario::projects(&["collapse", "tanukeys"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given(
            "~/Projects",
            "mindweaver install --all-projects commit-style",
        )
        .file(
            "~/Projects/tanukeys/.agents/skills/commit-style/SKILL.md",
            &skill("commit-style", "How tanukeys writes commits"),
        );
    scenario
}

#[test]
fn removing_one_still_installed_says_what_it_would_leave_orphaned() {
    let scenario = installed_twice_one_edited();

    scenario
        .run("~/Projects", "mindweaver remove commit-style")
        .exits(1)
        .prints("")
        .says(concat!(
            "warning: commit-style is installed in collapse and tanukeys, which removing it alone \
             would leave orphaned there; --everywhere takes it out of them too\n",
            "error: nothing removed: this deletes \
             /home/you/Projects/.mindweaver/library/skills/commit-style and the 1 file in it; \
             add --yes to go ahead\n",
        ));
}

#[test]
fn removing_everywhere_without_yes_says_how_each_project_holds_it() {
    let scenario = installed_twice_one_edited();

    scenario
        .run("~/Projects", "mindweaver remove commit-style --everywhere")
        .succeeds()
        .prints(concat!(
            "would remove commit-style from collapse, where it is current\n",
            "would leave commit-style in tanukeys, where it is edited, at .agents/skills/commit-style\n",
            "would remove skill commit-style from \
             /home/you/Projects/.mindweaver/library/skills/commit-style and the 1 file in it\n",
            "nothing removed: add --yes to go ahead\n",
        ))
        .quiet();
    assert!(scenario.exists("~/Projects/collapse/.claude/skills/commit-style"));
}

#[test]
fn removing_everywhere_takes_out_every_untouched_copy_then_the_librarys() {
    let scenario = installed_twice_one_edited();

    scenario
        .run(
            "~/Projects",
            "mindweaver remove commit-style --everywhere --yes",
        )
        .succeeds()
        .prints(concat!(
            "removed commit-style from collapse\n",
            "left commit-style in tanukeys, where it is edited, at .agents/skills/commit-style\n",
            "removed skill commit-style from \
             /home/you/Projects/.mindweaver/library/skills/commit-style\n",
        ))
        .says(
            "note: nothing offers commit-style in tanukeys any more; \
             `mindweaver uninstall -p tanukeys commit-style --force` takes it out\n",
        );
    assert!(!scenario.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(!scenario.exists("~/Projects/.mindweaver/library/skills/commit-style"));
    assert!(scenario.exists("~/Projects/tanukeys/.agents/skills/commit-style"));
}

#[test]
fn uninstall_orphaned_takes_out_what_nothing_offers_and_leaves_what_was_edited() {
    let scenario = installed_twice_one_edited();
    // Removed from the library alone: it warns of what it left, which a
    // given step may not.
    scenario
        .run("~/Projects", "mindweaver remove commit-style --yes")
        .exits(1)
        .prints(
            "removed skill commit-style from \
             /home/you/Projects/.mindweaver/library/skills/commit-style\n",
        )
        .says(
            "warning: commit-style is still installed in collapse and tanukeys, orphaned now \
             that nothing offers it; `mindweaver uninstall --orphaned --all-projects` takes out \
             what nothing offers\n",
        );

    scenario
        .run(
            "~/Projects",
            "mindweaver uninstall --orphaned --all-projects",
        )
        .exits(1)
        .prints("removed commit-style from collapse\n")
        .says(
            "warning: commit-style in tanukeys was changed since it was installed, at \
             .agents/skills/commit-style, so it was left alone; --force takes it out\n",
        );
    assert!(!scenario.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(scenario.exists("~/Projects/tanukeys/.agents/skills/commit-style"));
}

#[test]
fn mind_uninstall_takes_a_copy_out_of_the_project_it_runs_in() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style");

    scenario
        .run("~/Projects/collapse", "mind uninstall commit-style")
        .succeeds()
        .prints("removed commit-style from collapse\n")
        .quiet();
    assert!(!scenario.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(!scenario.exists("~/Projects/collapse/.agents/skills/commit-style"));
}
