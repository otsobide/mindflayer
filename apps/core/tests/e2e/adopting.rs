//! Bringing a skill somebody keeps in a project into the library
//! (`adopt::plan`, `adopt::apply`), its copies recorded as they are, and
//! taking one of the library's own out of every project it reached before
//! the library (`everywhere::apply`), or what nothing offers any more
//! (`assign::uninstall_orphaned`).

use mindflayer_core::install::Edits;
use mindflayer_core::{adopt, assign, everywhere, Kind, Reference};

use crate::{at, World};

/// The skill `collapse` keeps by hand where Claude reads.
const NOTES: &str = "---\nname: notes\ndescription: My own notes\n---\n";

/// `~/Projects` linking `collapse`, which keeps `notes` by hand.
fn collapse_keeping_notes() -> World {
    let world = World::projects(&["collapse"]);
    world.file("~/Projects/collapse/.claude/skills/notes/SKILL.md", NOTES);
    world
}

/// `~/Projects` linking `collapse` and `tanukeys`, `commit-style` from the
/// library installed into both, and `tanukeys`'s copy changed since.
fn installed_twice_one_edited() -> World {
    let world = World::projects(&["collapse", "tanukeys"]);
    world
        .add(
            "~/Projects",
            "~/Projects",
            Kind::Skill,
            "commit-style",
            "How every repo here writes commits",
        )
        .install("~/Projects", "~/Projects/collapse", "skill:commit-style")
        .install("~/Projects", "~/Projects/tanukeys", "skill:commit-style")
        .file(
            "~/Projects/tanukeys/.agents/skills/commit-style/SKILL.md",
            "---\nname: commit-style\ndescription: How tanukeys writes commits\n---\n",
        );
    world
}

#[test]
fn adopting_plans_first_and_the_plan_writes_nothing() {
    let world = collapse_keeping_notes();
    let workspace = world.workspace("~/Projects");

    adopt::plan(
        &workspace,
        &world.project("~/Projects/collapse"),
        &Reference::parse("skill:notes"),
        &world.homes(),
    )
    .expect("plan the adoption");

    assert!(!world.exists("~/Projects/.mindflayer/library/skills/notes"));
    assert_eq!(
        at(
            &world.snapshot("~/Projects"),
            "/projects/0/artifacts/0/state"
        ),
        "foreign"
    );
}

#[test]
fn an_adopted_skill_is_the_librarys_and_its_copy_reads_as_installed_from_there() {
    let world = collapse_keeping_notes();
    let workspace = world.workspace("~/Projects");

    adopt::apply(
        &workspace,
        &world.project("~/Projects/collapse"),
        &Reference::parse("skill:notes"),
        &world.homes(),
    )
    .expect("adopt notes");

    assert_eq!(
        world.read("~/Projects/.mindflayer/library/skills/notes/SKILL.md"),
        NOTES
    );
    assert_eq!(
        world.read("~/Projects/collapse/.claude/skills/notes/SKILL.md"),
        NOTES
    );
    let snapshot = world.snapshot("~/Projects");
    // Recorded where it is, and missing where codex reads until an update
    // puts it there: an adoption writes nothing into the project.
    assert_eq!(at(&snapshot, "/projects/0/artifacts/0/origin"), "workspace");
    assert_eq!(at(&snapshot, "/projects/0/artifacts/0/state"), "missing");
    assert!(!world.exists("~/Projects/collapse/.agents/skills/notes"));
}

#[test]
fn a_name_the_library_holds_already_is_not_adopted() {
    let world = collapse_keeping_notes();
    world.add(
        "~/Projects",
        "~/Projects",
        Kind::Skill,
        "notes",
        "The team's notes",
    );
    let workspace = world.workspace("~/Projects");

    let refused = adopt::apply(
        &workspace,
        &world.project("~/Projects/collapse"),
        &Reference::parse("skill:notes"),
        &world.homes(),
    )
    .expect_err("the library holds notes");

    assert_eq!(refused.code(), "adopt_in_library");
    assert_eq!(
        world.said(&refused),
        "the library holds `notes` already, at \
         /home/you/Projects/.mindflayer/library/skills/notes, so \
         /home/you/Projects/collapse/.claude/skills/notes is not adopted: rename one of them first"
    );
}

#[test]
fn removing_one_everywhere_takes_out_every_untouched_copy_then_the_librarys() {
    let world = installed_twice_one_edited();
    let workspace = world.workspace("~/Projects");

    everywhere::apply(
        &workspace,
        &Reference::parse("skill:commit-style"),
        &world.homes(),
    )
    .expect("remove it everywhere");

    assert!(!world.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(!world.exists("~/Projects/collapse/.agents/skills/commit-style"));
    assert!(world.exists("~/Projects/tanukeys/.agents/skills/commit-style/SKILL.md"));
    assert!(!world.exists("~/Projects/.mindflayer/library/skills/commit-style"));
    let snapshot = world.snapshot("~/Projects");
    assert_eq!(
        at(&snapshot, "/projects/0/artifacts"),
        &serde_json::json!([])
    );
    assert_eq!(at(&snapshot, "/projects/1/artifacts/0/state"), "edited");
}

#[test]
fn what_nothing_offers_any_more_is_taken_out_unless_it_was_edited() {
    let world = installed_twice_one_edited();
    let workspace = world.workspace("~/Projects");
    let own = workspace.own();
    let target = mindflayer_core::lifecycle::find_to_change(
        Some(&workspace),
        &own,
        &Reference::parse("skill:commit-style"),
    )
    .expect("find it");
    mindflayer_core::lifecycle::remove(Some(&workspace), &own, &target)
        .expect("remove it from the library alone");
    assert_eq!(
        at(
            &world.snapshot("~/Projects"),
            "/projects/0/artifacts/0/state"
        ),
        "orphaned"
    );

    assign::uninstall_orphaned(
        &workspace,
        &workspace.projects().0,
        &world.homes(),
        Edits::Kept,
    )
    .expect("take out what is orphaned");

    assert!(!world.exists("~/Projects/collapse/.claude/skills/commit-style"));
    assert!(world.exists("~/Projects/tanukeys/.agents/skills/commit-style/SKILL.md"));
}
