//! Gathering skills from git onto the workspace's shelf (`gather::gather`),
//! through the one port a gather fetches by, and what it refuses to keep.

use mindflayer_core::gather::{self, Request};
use serde_json::json;

use crate::World;

/// Where the team keeps its skills.
const SOURCE: &str = "~/src/acme-skills";

/// A skill's file in a source.
fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}

/// `~/Projects`, linking `api`, and beside it a source of two skills.
fn with_a_source() -> World {
    let world = World::projects(&["api"]);
    world.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How every repo here writes commits"),
            ),
            (
                "skills/review/SKILL.md",
                &skill("review", "Read a change before it lands"),
            ),
        ],
    );
    world
}

/// A gather of the source at `at` by its path, as `flayer gather git`
/// asks for one.
fn gather_from(world: &World, at: &str) -> Result<gather::Report, gather::GatherError> {
    let workspace = world.workspace("~/Projects");
    let (ledger, _) = workspace.ledger_to_write().expect("open the ledger");
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(world.at(at).to_string_lossy().into_owned()),
    )
}

/// What a report says of each skill it gathered: its name, what it says
/// it does and the folder it is in on the shelf, each held to lie in the
/// shelf's folder for the source that the report names.
fn harvested(report: &serde_json::Value, list: &str) -> Vec<(String, String)> {
    let shelf = format!(
        "/home/you/Projects/.mindflayer/skills/{}",
        report["directory"].as_str().unwrap()
    );
    report[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| {
            let name = skill["name"].as_str().unwrap();
            assert_eq!(skill["directory"], name, "{skill}");
            assert_eq!(skill["path"], format!("{shelf}/{name}"), "{skill}");
            (
                name.to_owned(),
                skill["summary"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// One of what [`harvested`] gives: a skill's name and what it says it does.
fn skill_said(name: &str, summary: &str) -> (String, String) {
    (name.to_owned(), summary.to_owned())
}

#[test]
fn gathering_a_source_puts_each_skill_it_holds_on_the_shelf() {
    let world = with_a_source();

    let report = gather_from(&world, SOURCE).expect("gather");

    assert!(report.failures.is_empty() && report.warnings.is_empty());
    let report = world.json(&report);
    assert_eq!(report["revision"], world.head(SOURCE));
    // The shelf's folder for the source is named from its address, which
    // here is the world's temporary folder spelled out: the CLI's suite
    // gathers by a path from the workspace to hold it word for word, which
    // a test sharing its process's folder with the others cannot.
    assert!(
        report["directory"]
            .as_str()
            .unwrap()
            .ends_with("-you-src-acme-skills"),
        "{report}"
    );
    assert_eq!(
        harvested(&report, "added"),
        [
            skill_said("commit-style", "How every repo here writes commits"),
            skill_said("review", "Read a change before it lands"),
        ]
    );
    assert_eq!(harvested(&report, "updated"), []);
    assert_eq!(harvested(&report, "unchanged"), []);
    assert_eq!(
        world
            .files("~/Projects/.mindflayer/skills")
            .iter()
            .filter(|file| file.ends_with("/SKILL.md"))
            .count(),
        2
    );
}

#[test]
fn gathering_a_source_again_says_which_skills_it_changed() {
    let world = with_a_source();
    gather_from(&world, SOURCE).expect("gather");
    world.source(
        SOURCE,
        &[(
            "skills/review/SKILL.md",
            &skill("review", "Read every change before it lands"),
        )],
    );

    let report = world.json(&gather_from(&world, SOURCE).expect("gather again"));

    assert_eq!(harvested(&report, "added"), []);
    assert_eq!(
        harvested(&report, "updated"),
        [skill_said("review", "Read every change before it lands")]
    );
    assert_eq!(
        harvested(&report, "unchanged"),
        [skill_said(
            "commit-style",
            "How every repo here writes commits"
        )]
    );
}

#[test]
fn a_gathered_skill_is_offered_to_every_project_from_its_source() {
    let world = with_a_source();
    gather_from(&world, SOURCE).expect("gather");

    let snapshot = world.snapshot("~/Projects");

    let offers: Vec<_> = snapshot["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|offer| (offer["name"].clone(), offer["origin"].clone()))
        .collect();
    let origin = json!("/home/you/src/acme-skills");
    assert_eq!(
        offers,
        [
            (json!("commit-style"), origin.clone()),
            (json!("review"), origin),
        ]
    );
}

#[test]
fn a_url_carrying_a_credential_is_refused_before_anything_is_kept() {
    let world = World::projects(&["api"]);
    let workspace = world.workspace("~/Projects");
    let (ledger, _) = workspace.ledger_to_write().expect("open the ledger");

    let refused = gather::gather(
        &workspace,
        &ledger,
        &Request::git("https://you:ghp_secret@example.com/acme/skills.git"),
    )
    .expect_err("a credential in the URL");

    assert_eq!(refused.code(), "credentials_in_url");
    let said = world.said(&refused);
    assert!(!said.contains("ghp_secret"), "{said}");
    assert_eq!(world.snapshot("~/Projects")["sources"], json!([]));
}
