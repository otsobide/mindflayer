//! Installing skills and rules into projects, and taking them out: the
//! reference's "Installing into a project", "Without the screen" and
//! "Loading another workspace".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse`, its library holding `commit-style`.
fn library_with_commit_style(projects: &[&str]) -> Scenario {
    let scenario = Scenario::projects(projects);
    scenario.given(
        "~/Projects",
        "flayer add skill commit-style \"How every repo here writes commits\"",
    );
    scenario
}

/// [`library_with_commit_style`], installed into `collapse`.
fn commit_style_installed() -> Scenario {
    let scenario = library_with_commit_style(&["collapse"]);
    scenario.given("~/Projects", "flayer install -p collapse commit-style");
    scenario
}

#[test]
fn a_skill_installed_gets_a_copy_in_every_folder_the_projects_agents_read() {
    let scenario = library_with_commit_style(&["collapse"]);

    scenario
        .run("~/Projects", "flayer install -p collapse commit-style")
        .succeeds()
        .prints("installed commit-style into collapse, from workspace\n")
        .quiet();

    let library = scenario.read("~/Projects/.mindflayer/library/skills/commit-style/SKILL.md");
    for folder in [".claude/skills", ".agents/skills"] {
        assert_eq!(
            scenario.read(&format!(
                "~/Projects/collapse/{folder}/commit-style/SKILL.md"
            )),
            library,
            "{folder} holds the library's copy"
        );
    }
}

#[test]
fn a_local_projects_copies_are_kept_out_of_git() {
    let scenario = library_with_commit_style(&["collapse"]);
    let id = scenario.project_id("~/Projects/collapse");

    scenario
        .run("~/Projects", "flayer install -p collapse commit-style")
        .succeeds();

    let exclude = scenario.read("~/Projects/collapse/.git/info/exclude");
    let block = exclude
        .split_once("# mindflayer:begin\n")
        .map(|(_, block)| block)
        .expect("a block of Mindflayer's in the exclude file");
    assert_eq!(
        block,
        format!(
            "# project {id}\n/.agents/skills/commit-style/\n/.claude/skills/commit-style/\n\
             # mindflayer:end\n"
        )
    );
}

#[test]
fn mind_install_installs_into_the_project_it_runs_in() {
    let scenario = library_with_commit_style(&["collapse"]);

    scenario
        .run("~/Projects/collapse", "mind install commit-style")
        .succeeds()
        .prints("installed commit-style into collapse, from workspace\n")
        .quiet();
}

#[test]
fn a_rule_installed_is_written_into_the_projects_agents_md() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given(
        "~/Projects",
        "flayer add rule git/commits \"Commit one coherent step at a time\"",
    );

    scenario
        .run("~/Projects", "flayer install -p collapse git/commits")
        .succeeds()
        .prints("installed rule git/commits into collapse, from workspace\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/AGENTS.md"),
        concat!(
            "<!-- mindflayer:begin -->\n",
            "<!-- mindflayer:rule git/commits -->\n",
            "# Commit one coherent step at a time\n",
            "<!-- mindflayer:end -->\n",
        )
    );
    assert!(
        !scenario.exists("~/Projects/collapse/CLAUDE.md"),
        "Claude reads AGENTS.md itself where no CLAUDE.md is, so none is made"
    );
}

#[test]
fn a_rule_about_some_files_reaches_claude_alone_and_the_install_says_so() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .file(
            "~/comments.md",
            "---\npaths:\n  - \"src/**\"\n---\n\n# Comments say why, not what\n",
        )
        .given(
            "~/Projects",
            "flayer add rule style/comments \"Comments say why, not what\"",
        )
        .given(
            "~/Projects",
            "flayer edit style/comments --contents ~/comments.md",
        );

    scenario
        .run("~/Projects", "flayer install -p collapse style/comments")
        .exits(1)
        .prints("installed rule style/comments into collapse, from workspace\n")
        .says(
            "warning: rule `style/comments` states `paths`, so it reaches claude only: \
             codex and opencode read no .claude/rules\n",
        );

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/rules/mindflayer/style/comments.md"),
        scenario.read("~/comments.md"),
        "Claude's rule file holds the rule as written, front matter included"
    );
    assert!(!scenario.exists("~/Projects/collapse/AGENTS.md"));
}

#[test]
fn a_skill_installed_from_a_loaded_workspace_says_where_it_came_from() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "flayer init")
        .given(
            "~/src/team",
            "flayer add skill deploy \"Ship the service to staging\"",
        )
        .given("~/Projects", "flayer load ../src/team");

    scenario
        .run("~/Projects", "flayer install -p collapse deploy")
        .succeeds()
        .prints("installed deploy into collapse, from load:../src/team\n")
        .quiet();
}

#[test]
fn loading_a_workspace_says_what_it_offers_and_where_it_went() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "flayer init")
        .given(
            "~/src/team",
            "flayer add skill deploy \"Ship the service to staging\"",
        );

    scenario
        .run("~/Projects", "flayer load ../src/team")
        .succeeds()
        .prints(concat!(
            "loaded team as ../src/team: 1 skill to install\n",
            "  into the workspace at /home/you/Projects\n",
        ))
        .quiet();
}

#[test]
fn somebodys_skill_of_the_same_name_is_left_alone() {
    let scenario = library_with_commit_style(&["collapse"]);
    let theirs = skill("commit-style", "How I write commits");
    scenario.file(
        "~/Projects/collapse/.claude/skills/commit-style/SKILL.md",
        &theirs,
    );

    scenario
        .run("~/Projects", "flayer install -p collapse commit-style")
        .exits(1)
        .prints("")
        .says(
            "warning: collapse already has a commit-style at .claude/skills/commit-style \
             that Mindflayer did not put there, so it was left alone\n",
        );

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        theirs
    );
    assert!(
        !scenario.exists("~/Projects/collapse/.agents/skills/commit-style"),
        "an install is every copy or none"
    );
}

#[test]
fn a_copy_changed_by_hand_is_not_written_over() {
    let scenario = commit_style_installed();
    let copy = "~/Projects/collapse/.agents/skills/commit-style/SKILL.md";
    let changed = format!(
        "{}Keep the subject under 50 characters.\n",
        scenario.read(copy)
    );
    scenario.file(copy, &changed);

    scenario
        .run("~/Projects/collapse", "mind install commit-style")
        .exits(1)
        .prints("")
        .says(
            "warning: commit-style in collapse was changed since it was installed, at \
             .agents/skills/commit-style, so it was left alone; --force writes over it\n",
        );

    assert_eq!(scenario.read(copy), changed);
}

#[test]
fn forcing_writes_over_a_copy_changed_by_hand() {
    let scenario = commit_style_installed();
    let copy = "~/Projects/collapse/.agents/skills/commit-style/SKILL.md";
    let installed = scenario.read(copy);
    scenario.file(
        copy,
        &format!("{installed}Keep the subject under 50 characters.\n"),
    );

    scenario
        .run("~/Projects/collapse", "mind install commit-style --force")
        .succeeds()
        .prints("updated commit-style in collapse, from workspace\n")
        .quiet();

    assert_eq!(scenario.read(copy), installed);
}

#[test]
fn an_install_into_every_project_says_each_and_goes_on_past_a_refusal() {
    let scenario = library_with_commit_style(&["collapse", "tanukeys"]);
    scenario.file(
        "~/Projects/tanukeys/.claude/skills/commit-style/SKILL.md",
        &skill("commit-style", "How I write commits"),
    );

    scenario
        .run("~/Projects", "flayer install commit-style --all-projects")
        .exits(1)
        .prints("installed commit-style into collapse, from workspace\n")
        .says(
            "warning: tanukeys already has a commit-style at .claude/skills/commit-style \
             that Mindflayer did not put there, so it was left alone\n",
        );
}

#[test]
fn uninstall_takes_out_every_copy_it_installed_and_nothing_else() {
    let scenario = commit_style_installed();
    scenario.file(
        "~/Projects/collapse/.claude/skills/notes/SKILL.md",
        &skill("notes", "My own notes"),
    );

    scenario
        .run("~/Projects", "flayer uninstall -p collapse commit-style")
        .succeeds()
        .prints("removed commit-style from collapse\n")
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/collapse/.claude/skills"),
        ["notes/SKILL.md"]
    );
    assert!(!scenario.exists("~/Projects/collapse/.agents/skills/commit-style"));
}

#[test]
fn uninstalling_the_last_rule_takes_away_the_agents_md_it_made() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "flayer add rule git/commits \"Commit one coherent step at a time\"",
        )
        .given("~/Projects", "flayer install -p collapse git/commits");

    scenario
        .run(
            "~/Projects",
            "flayer uninstall -p collapse rule:git/commits",
        )
        .succeeds()
        .prints("removed rule git/commits from collapse\n")
        .quiet();

    assert!(!scenario.exists("~/Projects/collapse/AGENTS.md"));
}
