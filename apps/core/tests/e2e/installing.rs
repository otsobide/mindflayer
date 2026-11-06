//! Putting skills and rules into projects and taking them out again, as
//! every front end does it (`assign::install`, `assign::uninstall`), and the
//! plan of a batch of boxes ticked and unticked (`install::plan`).

use mindweaver_core::assign;
use mindweaver_core::install::{self, Cell};
use mindweaver_core::install::{Confirmed, Edits, Installed, Removed};
use mindweaver_core::{Kind, Reference};
use serde_json::json;

use crate::{at, World};

/// What `review` is said to do.
const REVIEW: &str = "Read a change before it lands";

/// `~/Projects` linking `api`, with `review` in its library.
fn library_with_review() -> World {
    let world = World::projects(&["api"]);
    world.add("~/Projects", "~/Projects", Kind::Skill, "review", REVIEW);
    world
}

/// What installing `typed` into `api` came to, as one install into one
/// project does.
fn install(world: &World, typed: &str, edits: Edits) -> Installed {
    let workspace = world.workspace("~/Projects");
    let mut assigned = assign::install(
        &workspace,
        &[world.project("~/Projects/api")],
        &Reference::parse(typed),
        None,
        &world.at("~/Projects"),
        &world.homes(),
        edits,
        Confirmed::Yes,
    )
    .expect("open the ledger");
    let part = assigned.parts.remove(0);
    part.done
        .expect("offered")
        .installed
        .expect("installed")
        .installed
}

/// What taking `typed` out of `api` came to.
fn uninstall(world: &World, typed: &str) -> Removed {
    let workspace = world.workspace("~/Projects");
    let mut assigned = assign::uninstall(
        &workspace,
        &[world.project("~/Projects/api")],
        &Reference::parse(typed),
        &world.homes(),
        Edits::Kept,
    )
    .expect("open the ledger");
    assigned.parts.remove(0).done.expect("asked").1
}

/// The paths `installed` wrote, as the person spells them.
fn written(world: &World, installed: &Installed) -> Vec<String> {
    match installed {
        Installed::Added { paths, .. }
        | Installed::Updated { paths, .. }
        | Installed::Unchanged { paths, .. }
        | Installed::Edited { paths, .. }
        | Installed::Unconfirmed { paths, .. } => paths.iter().map(|p| world.spelled(p)).collect(),
        Installed::Foreign { path, .. } => vec![world.spelled(path)],
    }
}

#[test]
fn a_skill_installed_is_copied_where_each_agent_reads_and_kept_out_of_git() {
    let world = library_with_review();

    let installed = install(&world, "skill:review", Edits::Kept);

    assert!(
        matches!(installed, Installed::Added { .. }),
        "{installed:?}"
    );
    assert_eq!(
        written(&world, &installed),
        [
            "/home/you/Projects/api/.agents/skills/review",
            "/home/you/Projects/api/.claude/skills/review",
        ]
    );
    let copy = format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n");
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/review/SKILL.md"),
        copy
    );
    assert_eq!(
        world.read("~/Projects/api/.agents/skills/review/SKILL.md"),
        copy
    );
    let id = world.project_id("~/Projects/api");
    let exclude = world.read("~/Projects/api/.git/info/exclude");
    let block = exclude
        .split_once("# mindweaver:begin\n")
        .expect("the block")
        .1;
    assert_eq!(
        block,
        format!(
            "# project {id}\n/.agents/skills/review/\n/.claude/skills/review/\n# mindweaver:end\n"
        )
    );
}

#[test]
fn an_installed_skill_reads_as_current_in_every_front_ends_snapshot() {
    let world = library_with_review();
    install(&world, "skill:review", Edits::Kept);

    let snapshot = world.snapshot("~/Projects");

    assert_eq!(
        at(&snapshot, "/projects/0/artifacts"),
        &json!([{
            "agents": ["claude", "codex", "opencode"],
            "issues": [],
            "kind": "skill",
            "name": "review",
            "origin": "workspace",
            "state": "current",
            "summary": REVIEW,
            "targets": [".agents/skills/review", ".claude/skills/review"],
            "unread": null,
        }])
    );
    assert_eq!(
        at(&snapshot, "/boxes"),
        &json!([{
            "closed": [],
            "entry": "api",
            "held": [{
                "kind": "skill", "name": "review", "origin": "workspace",
                "state": "current", "ticked": true,
            }],
        }])
    );
    assert_eq!(
        at(&snapshot, "/library/holders"),
        &json!([{"entry": "api", "kind": "skill", "name": "review"}])
    );
}

#[test]
fn a_rule_installed_is_rendered_into_the_block_of_the_projects_agents_md() {
    let world = World::projects(&["api"]);
    world.add(
        "~/Projects",
        "~/Projects",
        Kind::Rule,
        "style",
        "How code here reads",
    );

    let installed = install(&world, "rule:style", Edits::Kept);

    assert!(
        matches!(installed, Installed::Added { .. }),
        "{installed:?}"
    );
    assert_eq!(
        world.read("~/Projects/api/AGENTS.md"),
        concat!(
            "<!-- mindweaver:begin -->\n",
            "<!-- mindweaver:rule style -->\n",
            "# How code here reads\n",
            "<!-- mindweaver:end -->\n",
        )
    );
    // Made by Mindweaver in a project kept out of git, so kept out with it;
    // no CLAUDE.md is made to import it.
    assert!(world
        .read("~/Projects/api/.git/info/exclude")
        .contains("\n/AGENTS.md\n"));
    assert!(!world.exists("~/Projects/api/CLAUDE.md"));
}

#[test]
fn a_skill_somebody_keeps_where_a_copy_would_go_refuses_the_install_and_is_left_alone() {
    let world = library_with_review();
    let theirs = "---\nname: review\ndescription: My own review\n---\n";
    world.file("~/Projects/api/.claude/skills/review/SKILL.md", theirs);

    let installed = install(&world, "skill:review", Edits::Kept);

    assert_eq!(
        written(&world, &installed),
        ["/home/you/Projects/api/.claude/skills/review"]
    );
    assert!(
        matches!(installed, Installed::Foreign { .. }),
        "{installed:?}"
    );
    assert_eq!(installed.refused().map(|why| why.code()), Some("foreign"));
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/review/SKILL.md"),
        theirs
    );
    assert!(!world.exists("~/Projects/api/.agents/skills/review"));
}

#[test]
fn an_installed_copy_edited_since_is_left_unless_the_install_is_forced() {
    let world = library_with_review();
    install(&world, "skill:review", Edits::Kept);
    let edited = "---\nname: review\ndescription: Read it twice\n---\n";
    world.file("~/Projects/api/.claude/skills/review/SKILL.md", edited);

    let kept = install(&world, "skill:review", Edits::Kept);

    assert!(matches!(kept, Installed::Edited { .. }), "{kept:?}");
    assert_eq!(kept.refused().map(|why| why.code()), Some("edited"));
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/review/SKILL.md"),
        edited
    );

    let forced = install(&world, "skill:review", Edits::Forced);

    assert!(matches!(forced, Installed::Updated { .. }), "{forced:?}");
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/review/SKILL.md"),
        format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n")
    );
}

#[test]
fn a_name_nothing_offers_is_refused_with_what_would_offer_it() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");

    let mut assigned = assign::install(
        &workspace,
        &[world.project("~/Projects/api")],
        &Reference::parse("skill:review"),
        None,
        &world.at("~/Projects"),
        &world.homes(),
        Edits::Kept,
        Confirmed::Yes,
    )
    .expect("open the ledger");

    let refused = assigned.parts.remove(0).done.expect_err("nothing offered");
    assert_eq!(refused.code(), "not_offered");
    assert_eq!(
        world.said(&refused),
        "nothing called `review` can be installed: gather it, load a workspace that has it \
         with `mindweaver load`, or add it to the workspace with `mindweaver add`"
    );
}

#[test]
fn uninstalling_takes_out_the_copies_it_recorded_and_nothing_beside_them() {
    let world = library_with_review();
    install(&world, "skill:review", Edits::Kept);
    world.file(
        "~/Projects/api/.claude/skills/notes/SKILL.md",
        "---\nname: notes\ndescription: My own notes\n---\n",
    );

    let removed = uninstall(&world, "skill:review");

    assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
    assert!(!world.exists("~/Projects/api/.claude/skills/review"));
    assert!(!world.exists("~/Projects/api/.agents/skills/review"));
    assert_eq!(
        world.files("~/Projects/api/.claude/skills"),
        ["notes/SKILL.md"]
    );
    let exclude = world.read("~/Projects/api/.git/info/exclude");
    assert!(!exclude.contains("review"), "{exclude}");
}

#[test]
fn uninstalling_what_somebody_keeps_by_hand_is_refused_and_leaves_it() {
    let world = library_with_review();
    let theirs = "---\nname: review\ndescription: My own review\n---\n";
    world.file("~/Projects/api/.claude/skills/review/SKILL.md", theirs);

    let removed = uninstall(&world, "skill:review");

    assert!(matches!(removed, Removed::Foreign { .. }), "{removed:?}");
    assert_eq!(
        world.read("~/Projects/api/.claude/skills/review/SKILL.md"),
        theirs
    );
}

#[test]
fn a_plan_of_boxes_is_the_installs_and_uninstalls_they_amount_to() {
    let world = library_with_review();
    world
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "commit-style",
            "How every repo here writes commits",
        )
        .install("~/Projects", "~/Projects/api", "skill:commit-style");
    let workspace = world.workspace("~/Projects");

    let planned = install::plan(
        &workspace,
        &[
            Cell::of("api", Kind::Skill, "review", "workspace", true),
            Cell::of("api", Kind::Skill, "commit-style", "workspace", false),
        ],
    )
    .expect("plan the boxes");

    assert_eq!(
        world.json(&planned),
        json!({
            "changes": [
                {"from": "workspace", "kind": "skill", "name": "review", "projects": ["api"], "type": "install"},
                {"kind": "skill", "name": "commit-style", "projects": ["api"], "type": "uninstall"},
            ],
            "conflicts": [],
            "locked": [],
        })
    );
    // A plan is a read: nothing it plans is done.
    assert!(!world.exists("~/Projects/api/.claude/skills/review"));
    assert!(world.exists("~/Projects/api/.claude/skills/commit-style"));
}
