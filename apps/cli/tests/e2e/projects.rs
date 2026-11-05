//! Making a repository a mind project, giving the folder it sits in a
//! workspace, and linking the two: the reference's "Getting started".

use crate::Scenario;

#[test]
fn mind_init_makes_a_repository_a_project_kept_out_of_git() {
    let scenario = Scenario::empty();
    scenario.repository("~/Projects/collapse");

    scenario
        .run("~/Projects/collapse", "mind init")
        .succeeds()
        .prints("initialized mind project `collapse` in /home/you/Projects/collapse/.mind\n")
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/collapse/.mind"),
        [".gitignore", "settings.toml"]
    );
    assert_eq!(
        scenario.read("~/Projects/collapse/.mind/.gitignore"),
        "# Mindweaver keeps this project's settings here, for this checkout alone.\n\
         # Nothing in this folder is meant to be committed.\n\
         *\n"
    );
    for folder in ["skills", "rules"] {
        assert!(
            scenario
                .at(&format!("~/Projects/collapse/{folder}"))
                .is_dir(),
            "mind init makes {folder}/ for the project's own"
        );
    }
}

#[test]
fn mind_init_run_again_says_so_and_changes_nothing() {
    let scenario = Scenario::empty();
    scenario
        .repository("~/Projects/collapse")
        .given("~/Projects/collapse", "mind init");
    let settings = scenario.read("~/Projects/collapse/.mind/settings.toml");

    scenario
        .run("~/Projects/collapse", "mind init")
        .succeeds()
        .prints(
            "mind project `collapse` already initialized in /home/you/Projects/collapse/.mind\n",
        )
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.mind/settings.toml"),
        settings
    );
}

#[test]
fn mindweaver_init_makes_the_folder_a_workspace() {
    let scenario = Scenario::empty();
    scenario.folder("~/Projects");

    scenario
        .run("~/Projects", "mindweaver init")
        .succeeds()
        .prints("initialized weaver workspace `Projects` in /home/you/Projects/.mindweaver\n")
        .quiet();

    assert!(scenario.exists("~/Projects/.mindweaver/weaver.toml"));
    assert!(
        !scenario.exists("~/.mindweaver"),
        "a workspace made elsewhere leaves the home alone"
    );
}

#[test]
fn mindweaver_link_registers_a_project_and_its_skills_are_listed() {
    let scenario = Scenario::empty();
    scenario
        .repository("~/Projects/collapse")
        .given("~/Projects/collapse", "mind init")
        .given(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        )
        .given("~/Projects", "mindweaver init");

    scenario
        .run("~/Projects", "mindweaver link collapse")
        .succeeds()
        .prints("linked collapse as collapse\n")
        .quiet();

    scenario
        .run("~/Projects", "mindweaver list")
        .succeeds()
        .prints("deploy  Ship the service to staging\n")
        .quiet();
}

#[test]
fn a_repository_not_made_a_project_is_not_linked() {
    let scenario = Scenario::empty();
    scenario
        .repository("~/Projects/archive")
        .given("~/Projects", "mindweaver init");

    scenario
        .run("~/Projects", "mindweaver link archive")
        .exits(1)
        .prints("")
        .says(
            "error: /home/you/Projects/archive: not a mind project \
             (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)\n",
        );
}

#[test]
fn mindweaver_scan_finds_what_could_be_linked_and_says_what_it_needs() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.repository("~/Projects/archive");

    scenario
        .run("~/Projects", "mindweaver scan")
        .succeeds()
        .prints(concat!(
            "archive   not linked  git repository, not a mind project yet\n",
            "collapse  linked\n",
            "\n",
            "  a repository needs `mind -C <path> init` before it can be linked\n",
        ))
        .quiet();
}

#[test]
fn mindweaver_unlink_forgets_a_project_and_leaves_it_as_it_is() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given(
        "~/Projects/collapse",
        "mind add skill deploy \"Ship the service to staging\"",
    );

    scenario
        .run("~/Projects", "mindweaver unlink collapse")
        .succeeds()
        .prints("unlinked collapse\n")
        .quiet();

    assert!(scenario.exists("~/Projects/collapse/.mind/settings.toml"));
    assert!(scenario.exists("~/Projects/collapse/skills/deploy/SKILL.md"));
    scenario
        .run("~/Projects", "mindweaver list")
        .succeeds()
        .prints("Projects manages no projects yet\n  link one with `mindweaver link <path>`\n");
}

// -------------------------------------------------------------------------
// A project that moved: the reference's "Following a project that moved".
// -------------------------------------------------------------------------

/// `~/Projects` linking `collapse`, with `commit-style` installed into it
/// from the library, moved by hand into `apps/`.
fn collapse_moved_into_apps() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .folder("~/Projects/apps");
    std::fs::rename(
        scenario.at("~/Projects/collapse"),
        scenario.at("~/Projects/apps/collapse"),
    )
    .expect("move the project by hand");
    scenario
}

#[test]
fn a_project_that_moved_is_stale_and_status_says_why() {
    let scenario = collapse_moved_into_apps();

    scenario
        .run("~/Projects", "mindweaver status")
        .exits(1)
        .prints("collapse  stale\n")
        .says(
            "warning: /home/you/Projects/collapse: not a mind project \
             (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)\n",
        );
}

#[test]
fn mindweaver_scan_finds_a_project_that_moved_by_its_id() {
    let scenario = collapse_moved_into_apps();

    scenario
        .run("~/Projects", "mindweaver scan")
        .succeeds()
        .prints(concat!(
            "apps/collapse  not linked  moved: collapse -> apps/collapse\n",
            "\n",
            "  follow one that moved with `mindweaver relink <old> <new>`\n",
        ))
        .quiet();
}

#[test]
fn mindweaver_relink_follows_a_project_and_keeps_what_was_installed() {
    let scenario = collapse_moved_into_apps();

    scenario
        .run("~/Projects", "mindweaver relink collapse apps/collapse")
        .succeeds()
        .prints(concat!(
            "relinked collapse as apps/collapse, from collapse\n",
            "  1 install kept with it\n",
        ))
        .quiet();

    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints("commit-style  current  claude,codex,opencode  workspace\n")
        .quiet();
}
