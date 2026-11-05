//! Writing skills and rules: the lifecycle changes every front end calls
//! (`lifecycle::add`, `rename`, `remove`, `replace_contents`), each handed
//! its workspace, and what core says of a draft before any is written.

use mindweaver_core::create::Start;
use mindweaver_core::draft;
use mindweaver_core::lifecycle;
use mindweaver_core::{Kind, Reference};
use serde_json::json;

use crate::World;

/// What `review` is said to do.
const REVIEW: &str = "Read a change before it lands";

/// The SHA-256 of the file `add` writes for `review`, as a front end read
/// it before an edit.
const REVIEW_HASH: &str = "785660eaf65b8fce834ca050b26ed265b4d787871877ff87f05b52212d32dbca";

/// `~/Projects` linking `api`, with `review` in its library.
fn library_with_review() -> World {
    let world = World::projects(&["api"]);
    world.add("~/Projects", "~/Projects", Kind::Skill, "review", REVIEW);
    world
}

/// What the library's `review` is, found as a change finds it.
fn review(world: &World) -> lifecycle::Target {
    let workspace = world.workspace("~/Projects");
    lifecycle::find_to_change(
        Some(&workspace),
        &workspace.own(),
        &Reference::parse("skill:review"),
    )
    .expect("find review")
}

#[test]
fn a_skill_added_to_the_library_is_written_with_its_front_matter() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");

    lifecycle::add(
        Some(&workspace),
        &workspace.own(),
        Kind::Skill,
        "review",
        REVIEW,
        Start::Scratch,
    )
    .expect("add review");

    assert_eq!(
        world.read("~/Projects/.mindweaver/library/skills/review/SKILL.md"),
        format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n")
    );
}

#[test]
fn a_rule_added_to_a_project_is_written_where_the_project_keeps_its_rules() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");

    lifecycle::add(
        Some(&workspace),
        &world.project("~/Projects/api"),
        Kind::Rule,
        "style",
        "How code here reads",
        Start::Scratch,
    )
    .expect("add style");

    assert_eq!(
        world.read("~/Projects/api/rules/style.md"),
        "# How code here reads\n"
    );
}

#[test]
fn a_name_no_agent_could_load_is_refused_before_anything_is_written() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");

    let refused = lifecycle::add(
        Some(&workspace),
        &workspace.own(),
        Kind::Skill,
        "Review",
        REVIEW,
        Start::Scratch,
    )
    .expect_err("an unfit name");

    assert_eq!(refused.code(), "add_name_unfit");
    assert_eq!(
        world.said(&refused),
        "`Review` is not usable as a name: it has to be lowercase letters and digits, \
         joined by single hyphens"
    );
    assert!(!world.exists("~/Projects/.mindweaver/library/skills/Review"));
}

#[test]
fn a_draft_is_judged_as_the_add_would_refuse_it_with_nothing_written() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");

    let judged = draft::added(
        Some(&workspace),
        &workspace.own(),
        Kind::Skill,
        "Review",
        REVIEW,
        None,
    );

    assert_eq!(
        world.json(&judged),
        json!({
            "issues": [],
            "refused": {
                "code": "add_name_unfit",
                "message": "`Review` is not usable as a name: it has to be lowercase letters \
                            and digits, joined by single hyphens",
            },
        })
    );
    assert!(!world.exists("~/Projects/.mindweaver/library/skills/Review"));
}

#[test]
fn renaming_a_skill_moves_its_folder_and_rewrites_only_its_name_line() {
    let world = library_with_review();
    let workspace = world.workspace("~/Projects");

    lifecycle::rename(Some(&workspace), &workspace.own(), &review(&world), "ship")
        .expect("rename review");

    assert!(!world.exists("~/Projects/.mindweaver/library/skills/review"));
    assert_eq!(
        world.read("~/Projects/.mindweaver/library/skills/ship/SKILL.md"),
        format!("---\nname: ship\ndescription: {REVIEW}\n---\n\n# review\n")
    );
}

#[test]
fn removing_a_skill_takes_its_folder_away() {
    let world = library_with_review();
    let workspace = world.workspace("~/Projects");

    lifecycle::remove(Some(&workspace), &workspace.own(), &review(&world)).expect("remove review");

    assert!(!world.exists("~/Projects/.mindweaver/library/skills/review"));
}

#[test]
fn contents_given_whole_replace_the_file_read_with_the_hash_named() {
    let world = library_with_review();
    let workspace = world.workspace("~/Projects");
    let text =
        format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n\nRead it twice.\n");

    let edited = lifecycle::replace_contents(
        Some(&workspace),
        &workspace.own(),
        &review(&world),
        text.as_bytes(),
        Some(REVIEW_HASH),
    )
    .expect("replace the contents");

    assert!(edited.changed);
    assert_eq!(
        world.read("~/Projects/.mindweaver/library/skills/review/SKILL.md"),
        text
    );
}

#[test]
fn contents_given_over_a_change_made_since_are_refused_and_the_change_kept() {
    let world = library_with_review();
    // Changed in an editor after the front end read it.
    let changed = format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n\nMine.\n");
    world.file(
        "~/Projects/.mindweaver/library/skills/review/SKILL.md",
        &changed,
    );
    let workspace = world.workspace("~/Projects");

    let refused = lifecycle::replace_contents(
        Some(&workspace),
        &workspace.own(),
        &review(&world),
        b"---\nname: review\ndescription: Theirs\n---\n",
        Some(REVIEW_HASH),
    )
    .expect_err("changed since it was read");

    assert_eq!(refused.code(), "edit_stale");
    assert_eq!(
        world.read("~/Projects/.mindweaver/library/skills/review/SKILL.md"),
        changed
    );
}

#[test]
fn contents_naming_another_skill_are_refused_with_a_pointer_to_rename() {
    let world = library_with_review();
    let workspace = world.workspace("~/Projects");

    let refused = lifecycle::replace_contents(
        Some(&workspace),
        &workspace.own(),
        &review(&world),
        format!("---\nname: ship\ndescription: {REVIEW}\n---\n").as_bytes(),
        None,
    )
    .expect_err("another name");

    assert_eq!(refused.code(), "edit_name_changed");
    assert_eq!(
        world.read("~/Projects/.mindweaver/library/skills/review/SKILL.md"),
        format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n")
    );
}
