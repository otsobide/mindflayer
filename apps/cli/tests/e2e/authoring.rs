//! Writing skills and rules, in a project and in the workspace's library:
//! the reference's "Creating one", "Changing one" and "The workspace's own
//! skills and rules".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse`, which holds a skill of its own, `deploy`.
fn collapse_with_deploy() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given(
        "~/Projects/collapse",
        "mind add skill deploy \"Ship the service to staging\"",
    );
    scenario
}

#[test]
fn mind_add_writes_the_least_skill_validate_passes() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        )
        .succeeds()
        .prints("created skill deploy at /home/you/Projects/collapse/skills/deploy/SKILL.md\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        "---\nname: deploy\ndescription: Ship the service to staging\n---\n\n# deploy\n"
    );
    scenario
        .run("~/Projects/collapse", "mind validate")
        .succeeds()
        .prints("deploy: ok\n\n1 skill checked, 0 invalid\n");
}

#[test]
fn a_name_the_skill_format_refuses_is_never_written() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects/collapse",
            "mind add skill Deploy_It \"Ship it\"",
        )
        .exits(1)
        .prints("")
        .says(
            "error: `Deploy_It` is not usable as a name: \
             it has to be lowercase letters and digits, joined by single hyphens\n",
        );

    assert!(scenario.files("~/Projects/collapse/skills").is_empty());
}

#[test]
fn flayer_add_keeps_a_skill_in_the_workspaces_library() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "flayer add skill commit-style \"How every repo here writes commits\"",
        )
        .succeeds()
        .prints(
            "created skill commit-style at \
             /home/you/Projects/.mindflayer/library/skills/commit-style/SKILL.md\n",
        )
        .quiet();

    assert!(
        !scenario.exists("~/Projects/collapse/.claude"),
        "the library reaches a project only by an install"
    );
}

#[test]
fn flayer_list_lists_the_library_beside_each_projects_own() {
    let scenario = collapse_with_deploy();
    scenario
        .given(
            "~/Projects",
            "flayer add skill commit-style \"How every repo here writes commits\"",
        )
        .given(
            "~/Projects",
            "flayer add rule git/no-force-push \"Never force-push a shared branch\"",
        );

    scenario
        .run("~/Projects", "flayer list")
        .succeeds()
        .prints(concat!(
            "workspace  skill  commit-style       How every repo here writes commits\n",
            "collapse   skill  deploy             Ship the service to staging\n",
            "workspace  rule   git/no-force-push  Never force-push a shared branch\n",
        ))
        .quiet();
}

#[test]
fn edit_takes_a_skills_new_contents_from_standard_input() {
    let scenario = collapse_with_deploy();
    let draft = skill("deploy", "Ship the service to staging, then to production");

    scenario
        .run_fed(
            "~/Projects/collapse",
            "mind edit deploy --contents -",
            &draft,
        )
        .succeeds()
        .prints("edited skill deploy: ok\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        draft
    );
}

#[test]
fn contents_naming_another_skill_are_refused_and_nothing_is_written() {
    let scenario = collapse_with_deploy();
    let before = scenario.read("~/Projects/collapse/skills/deploy/SKILL.md");
    scenario.file(
        "~/renamed.md",
        &skill("ship", "Ship the service to staging"),
    );

    scenario
        .run(
            "~/Projects/collapse",
            "mind edit deploy --contents ~/renamed.md",
        )
        .exits(1)
        .prints("")
        .says(
            "error: the contents given for `deploy` call it `ship`: a skill's name is its \
             folder's too, so it is changed with `mind rename`, which moves both, and nothing \
             was written\n",
        );

    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        before
    );
}

#[test]
fn rename_moves_the_folder_and_the_name_together() {
    let scenario = collapse_with_deploy();

    scenario
        .run("~/Projects/collapse", "mind rename deploy ship")
        .succeeds()
        .prints("renamed skill deploy to ship at /home/you/Projects/collapse/skills/ship\n")
        .quiet();

    assert!(!scenario.exists("~/Projects/collapse/skills/deploy"));
    assert_eq!(
        scenario.read("~/Projects/collapse/skills/ship/SKILL.md"),
        "---\nname: ship\ndescription: Ship the service to staging\n---\n\n# deploy\n",
        "only the `name:` line is rewritten"
    );
}

#[test]
fn remove_without_yes_says_what_it_would_delete_and_deletes_nothing() {
    let scenario = collapse_with_deploy();

    scenario
        .run("~/Projects/collapse", "mind remove deploy")
        .exits(1)
        .prints("")
        .says(
            "error: nothing removed: this deletes /home/you/Projects/collapse/skills/deploy \
             and the 1 file in it; add --yes to go ahead\n",
        );

    assert!(scenario.exists("~/Projects/collapse/skills/deploy/SKILL.md"));
}

#[test]
fn remove_with_yes_deletes_the_skill() {
    let scenario = collapse_with_deploy();

    scenario
        .run("~/Projects/collapse", "mind remove deploy --yes")
        .succeeds()
        .prints("removed skill deploy from /home/you/Projects/collapse/skills/deploy\n")
        .quiet();

    assert!(!scenario.exists("~/Projects/collapse/skills/deploy"));
}

#[test]
fn templates_lists_what_add_can_start_from_and_where_each_comes_from() {
    let scenario = collapse_with_deploy();

    scenario
        .run("~/Projects/collapse", "mind templates")
        .succeeds()
        .prints("full  skill  built in\n")
        .quiet();
}

#[test]
fn a_skill_added_from_the_full_template_gets_its_sections_and_folders() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service\" --template full",
        )
        .succeeds()
        .prints("created skill deploy at /home/you/Projects/collapse/skills/deploy/SKILL.md\n")
        .quiet();
    assert_eq!(
        scenario.files("~/Projects/collapse/skills/deploy"),
        ["SKILL.md", "references/.gitkeep", "scripts/.gitkeep"]
    );
    let manifest = scenario.read("~/Projects/collapse/skills/deploy/SKILL.md");
    assert!(
        manifest.starts_with("---\nname: deploy\ndescription: Ship the service\n---\n"),
        "{manifest}"
    );
    scenario
        .run("~/Projects/collapse", "mind validate")
        .succeeds()
        .prints("deploy: ok\n\n1 skill checked, 0 invalid\n");
}

#[test]
fn a_template_of_the_projects_own_is_listed_and_copied_whole() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .file(
            "~/Projects/collapse/.mind/templates/runbook/SKILL.md",
            "---\nname: runbook\ndescription: A runbook\n---\n\n# {{name}}\n\n{{description}}\n\n## Steps\n",
        )
        .file(
            "~/Projects/collapse/.mind/templates/runbook/checklist.md",
            "- [ ] page someone\n",
        );

    scenario
        .run(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service\" --template runbook",
        )
        .succeeds()
        .quiet();
    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/SKILL.md"),
        "---\nname: deploy\ndescription: Ship the service\n---\n\n# deploy\n\nShip the service\n\n## Steps\n"
    );
    assert_eq!(
        scenario.read("~/Projects/collapse/skills/deploy/checklist.md"),
        "- [ ] page someone\n"
    );
}
