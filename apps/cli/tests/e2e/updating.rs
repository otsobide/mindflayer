//! Keeping what was installed up to date with where it came from: the
//! reference's "Keeping projects up to date".

use crate::{skill, Scenario};

const SOURCE: &str = "~/src/acme-skills";

/// What `commit-style` says once its source moved on.
fn moved_on() -> String {
    format!(
        "{}Keep the subject short.\n",
        skill("commit-style", "How this repo writes commit messages")
    )
}

/// `~/Projects` linking `collapse`, which holds `commit-style` as gathered
/// from `~/src/acme-skills`.
fn installed_from_acme() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .source(
            SOURCE,
            &[(
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            )],
        )
        .given("~/Projects", "flayer gather git ../src/acme-skills")
        .given("~/Projects", "flayer install -p collapse commit-style");
    scenario
}

/// [`installed_from_acme`], once the source moved on and was gathered again:
/// the shelf offers something newer than the project holds.
fn outdated() -> Scenario {
    let scenario = installed_from_acme();
    scenario
        .change(SOURCE, &[("skills/commit-style/SKILL.md", &moved_on())])
        .given("~/Projects", "flayer gather git ../src/acme-skills");
    scenario
}

#[test]
fn a_copy_behind_what_its_source_offers_is_outdated() {
    let scenario = outdated();

    scenario
        .run("~/Projects", "flayer status")
        .succeeds()
        .prints("commit-style  outdated  claude,codex,opencode  ../src/acme-skills\n")
        .quiet();
}

#[test]
fn update_without_yes_says_what_it_would_do_and_changes_nothing() {
    let scenario = outdated();
    let copy = "~/Projects/collapse/.claude/skills/commit-style/SKILL.md";
    let before = scenario.read(copy);

    scenario
        .run("~/Projects", "flayer update -p collapse")
        .succeeds()
        .prints(concat!(
            "would update commit-style in collapse, from ../src/acme-skills\n",
            "nothing changed: add --yes to go ahead\n",
        ))
        .quiet();

    assert_eq!(scenario.read(copy), before);
}

#[test]
fn update_with_yes_brings_every_copy_up_to_date() {
    let scenario = outdated();

    scenario
        .run("~/Projects", "flayer update -p collapse --yes")
        .succeeds()
        .prints("updated commit-style in collapse, from ../src/acme-skills\n")
        .quiet();

    for folder in [".claude/skills", ".agents/skills"] {
        assert_eq!(
            scenario.read(&format!(
                "~/Projects/collapse/{folder}/commit-style/SKILL.md"
            )),
            moved_on(),
            "{folder} holds what the source offers now"
        );
    }
    scenario
        .run("~/Projects", "flayer status")
        .succeeds()
        .prints("commit-style  current  claude,codex,opencode  ../src/acme-skills\n");
}

#[test]
fn update_puts_back_a_copy_that_was_deleted() {
    let scenario = installed_from_acme();
    std::fs::remove_dir_all(scenario.at("~/Projects/collapse/.claude/skills/commit-style"))
        .expect("delete Claude's copy by hand");

    scenario
        .run("~/Projects/collapse", "mind update --yes")
        .succeeds()
        .prints(
            "put back commit-style in collapse at .claude/skills/commit-style, \
             from ../src/acme-skills\n",
        )
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        scenario.read("~/Projects/collapse/.agents/skills/commit-style/SKILL.md")
    );
}

#[test]
fn update_leaves_a_copy_somebody_changed_and_says_how_to_force_it() {
    let scenario = outdated();
    let copy = "~/Projects/collapse/.agents/skills/commit-style/SKILL.md";
    let changed = format!("{}My own line.\n", scenario.read(copy));
    scenario.file(copy, &changed);

    scenario
        .run("~/Projects", "flayer update -p collapse --yes")
        .exits(1)
        .prints("nothing to update in collapse\n")
        .says(
            "warning: commit-style in collapse was changed since it was installed, at \
             .agents/skills/commit-style, so it was left alone; --force writes over it\n",
        );

    assert_eq!(scenario.read(copy), changed);
}

#[test]
fn a_change_to_the_library_reaches_a_project_by_an_update() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "flayer add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "flayer install -p collapse commit-style")
        .file(
            "~/draft.md",
            &skill(
                "commit-style",
                "How every repo here writes commits, briefly",
            ),
        )
        .given(
            "~/Projects",
            "flayer edit commit-style --contents ~/draft.md",
        );

    scenario
        .run("~/Projects", "flayer update -p collapse --yes")
        .succeeds()
        .prints("updated commit-style in collapse, from workspace\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        scenario.read("~/draft.md")
    );
}

#[test]
fn update_says_which_projects_it_needs_named() {
    let scenario = installed_from_acme();

    scenario
        .run("~/Projects", "flayer update --yes")
        .exits(1)
        .prints("")
        .says(
            "error: say which projects to update: -p <project> for each, or --all for every \
             one the workspace links\n",
        );
}
