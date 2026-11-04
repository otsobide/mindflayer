//! Which workspace a command is about: the one up the tree, the one in the
//! home, one named with `-W` or activated with `$MINDFLAYER_WORKSPACE`. The
//! reference's "The workspace in your home" and "Choosing a workspace".

use crate::Scenario;

/// `~/Projects` linking `collapse`, beside `~/flayers/writing`, a workspace
/// of its own holding `prose-style`.
fn two_flayers() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .folder("~/flayers/writing")
        .given("~/flayers/writing", "flayer init")
        .given(
            "~/flayers/writing",
            "flayer add skill prose-style \"How every essay here is written\"",
        );
    scenario
}

#[test]
fn a_read_outside_every_workspace_answers_as_the_default_would_and_makes_nothing() {
    let scenario = Scenario::empty();

    scenario
        .run("~", "flayer list")
        .succeeds()
        .prints("you manages no projects yet\n  link one with `flayer link <path>`\n")
        .says(
            "hint: no workspace here or above, and the default one is not made yet; the first \
             command that writes to it, such as `flayer link <path>`, makes it in \
             /home/you/.mindflayer\n",
        );

    assert!(
        scenario.files("~").is_empty(),
        "a read leaves the home alone"
    );
}

#[test]
fn the_first_write_outside_every_workspace_makes_the_default_one_in_the_home() {
    let scenario = Scenario::empty();

    scenario
        .run(
            "~",
            "flayer add skill commit-style \"How every repo here writes commits\"",
        )
        .succeeds()
        .prints(
            "created skill commit-style at \
             /home/you/.mindflayer/library/skills/commit-style/SKILL.md\n",
        )
        .quiet();

    assert!(scenario.exists("~/.mindflayer/flayer.toml"));
}

#[test]
fn which_says_the_workspace_a_command_uses_and_why() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run("~/Projects/collapse", "flayer which")
        .succeeds()
        .prints(
            "Projects at /home/you/Projects, found walking up from /home/you/Projects/collapse\n",
        )
        .quiet();
}

#[test]
fn w_names_a_workspace_beside_the_one_above() {
    let scenario = two_flayers();

    scenario
        .run("~/Projects/collapse", "flayer -W ~/flayers/writing list")
        .succeeds()
        .prints("prose-style  How every essay here is written\n")
        .quiet();
}

#[test]
fn an_activated_workspace_is_the_one_every_command_uses() {
    let scenario = two_flayers();

    scenario
        .run_with(
            "~/Projects/collapse",
            "flayer list",
            &[("MINDFLAYER_WORKSPACE", "~/flayers/writing")],
        )
        .succeeds()
        .prints("prose-style  How every essay here is written\n")
        .quiet();
}

#[test]
fn mind_link_links_the_project_into_the_activated_workspace() {
    let scenario = two_flayers();

    scenario
        .run_with(
            "~/Projects/collapse",
            "mind link",
            &[("MINDFLAYER_WORKSPACE", "~/flayers/writing")],
        )
        .succeeds()
        .prints("linked collapse as ../../Projects/collapse\n")
        .quiet();
}

#[test]
fn w_naming_a_folder_that_is_no_workspace_is_refused_and_not_made_one() {
    let scenario = Scenario::empty();
    scenario.folder("~/notes");

    scenario
        .run("~", "flayer -W ~/notes list")
        .exits(1)
        .prints("")
        .says(
            "error: -W names a workspace that cannot be used: /home/you/notes: \
             not a flayer workspace (no .mindflayer/flayer.toml)\n",
        );

    assert!(!scenario.exists("~/notes/.mindflayer"));
    assert!(!scenario.exists("~/.mindflayer"));
}

/// A home whose default workspace is made, knowing `~/Projects`, linking
/// `collapse`, and `~/flayers/writing`, each remembered when `flayer init`
/// made it.
fn three_known() -> Scenario {
    let scenario = Scenario::empty();
    scenario
        .given("~", "flayer init")
        .repository("~/Projects/collapse")
        .given("~/Projects/collapse", "mind init")
        .folder("~/Projects")
        .given("~/Projects", "flayer init")
        .given("~/Projects", "flayer link collapse")
        .folder("~/flayers/writing")
        .given("~/flayers/writing", "flayer init");
    scenario
}

#[test]
fn workspaces_lists_every_flayer_this_machine_knows_and_marks_the_one_in_use() {
    let scenario = three_known();

    scenario
        .run("~/Projects/collapse", "flayer workspaces")
        .succeeds()
        .prints(concat!(
            "  you       /home/you                  no projects; the default one\n",
            "* Projects  /home/you/Projects         1 project\n",
            "  writing   /home/you/flayers/writing  no projects\n",
        ))
        .quiet();
}

#[test]
fn a_flayer_made_before_the_default_is_listed_in_use_and_not_remembered() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given("~", "flayer init");

    scenario
        .run("~/Projects", "flayer workspaces")
        .succeeds()
        .prints(concat!(
            "  you       /home/you           no projects; the default one\n",
            "* Projects  /home/you/Projects  1 project; not remembered\n",
            "\n",
            "  `flayer init <path>` remembers the one in use, and changes nothing else in it\n",
        ))
        .quiet();
}

#[test]
fn flayer_init_again_remembers_the_one_in_use_and_changes_nothing_else() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given("~", "flayer init");
    let marker = scenario.read("~/Projects/.mindflayer/flayer.toml");

    scenario
        .run("~", "flayer init ~/Projects")
        .succeeds()
        .prints(concat!(
            "flayer workspace `Projects` already initialized in /home/you/Projects/.mindflayer\n",
            "  remembered as Projects by the default workspace\n",
        ))
        .quiet();
    assert_eq!(scenario.read("~/Projects/.mindflayer/flayer.toml"), marker);
}

#[test]
fn workspaces_forget_takes_one_off_the_list_and_leaves_the_flayer_as_it_is() {
    let scenario = three_known();

    scenario
        .run("~", "flayer workspaces forget ~/flayers/writing")
        .succeeds()
        .prints("forgot flayers/writing\n")
        .quiet();
    assert!(scenario.exists("~/flayers/writing/.mindflayer/flayer.toml"));
    scenario
        .run("~", "flayer workspaces")
        .succeeds()
        .prints(concat!(
            "* you       /home/you           no projects; the default one\n",
            "  Projects  /home/you/Projects  1 project\n",
        ));
}
