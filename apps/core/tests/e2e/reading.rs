//! Reading what is where, as every front end reads it: the snapshot
//! (`view::Snapshot`), what a project holds (`status::read`), an
//! artifact's files (`contents`), the folders inside one (`browse`) and
//! what could be linked (`scan`), each answered as the JSON a front end
//! is handed.

use mindweaver_core::browse::Folders;
use mindweaver_core::contents::{Contents, Files};
use mindweaver_core::scan::{self, Request};
use mindweaver_core::Kind;
use mindweaver_core::ProjectStatus;
use serde_json::json;

use crate::{at, World};

/// What `review` is said to do.
const REVIEW: &str = "Read a change before it lands";

/// `~/Projects` linking `api`, which holds one of each state a project
/// shows most: `deploy`, its own; `review`, installed from the library;
/// and `notes`, kept by hand where Claude and OpenCode read.
fn api_holding_three() -> World {
    let world = World::projects(&["api"]);
    world
        .add("~/Projects", "~/Projects", Kind::Skill, "review", REVIEW)
        .install("~/Projects", "~/Projects/api", "skill:review")
        .add(
            "~/Projects",
            "~/Projects/api",
            Kind::Skill,
            "deploy",
            "Ship the service to staging",
        )
        .file(
            "~/Projects/api/.claude/skills/notes/SKILL.md",
            "---\nname: notes\ndescription: My own notes\n---\n",
        );
    world
}

/// The counts of each state, in the snapshot's order, from `current` to
/// `authored`.
fn states(counts: [u64; 7]) -> serde_json::Value {
    let names = [
        "current", "outdated", "edited", "missing", "orphaned", "foreign", "authored",
    ];
    names
        .iter()
        .zip(counts)
        .map(|(state, count)| json!({"count": count, "state": state}))
        .collect()
}

#[test]
fn the_snapshot_says_which_workspace_it_is_of_and_why_it_was_chosen() {
    let world = World::projects(&["api"]);

    let snapshot = world.snapshot("~/Projects/api");

    assert_eq!(
        at(&snapshot, "/workspace"),
        &json!({"made": true, "name": "Projects", "path": "/home/you/Projects", "reason": "above"})
    );
    assert_eq!(at(&snapshot, "/format"), &json!(1));
}

#[test]
fn the_snapshot_counts_what_the_projects_hold_by_kind_and_by_state() {
    let world = api_holding_three();

    let snapshot = world.snapshot("~/Projects");

    assert_eq!(at(&snapshot, "/summary/projects"), &json!(1));
    assert_eq!(
        at(&snapshot, "/summary/held"),
        &json!({
            "kinds": [
                {"count": 3, "kind": "skill"},
                {"count": 0, "kind": "rule"},
                {"count": 0, "kind": "mcp"},
            ],
            "states": states([1, 0, 0, 0, 0, 1, 1]),
            "total": 3,
        })
    );
}

#[test]
fn what_a_project_holds_is_said_with_how_each_stands_and_who_reads_it() {
    let world = api_holding_three();
    let workspace = world.workspace("~/Projects");

    let (status, _) = ProjectStatus::read(
        &world.project("~/Projects/api"),
        &Kind::ALL,
        Some(&workspace),
    );

    let status = world.json(&status);
    let artifacts: Vec<_> = status["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|held| {
            json!({
                "name": held["name"], "state": held["state"],
                "agents": held["agents"], "origin": held["origin"],
            })
        })
        .collect();
    assert_eq!(
        artifacts,
        [
            json!({"name": "deploy", "state": "authored", "agents": [], "origin": null}),
            json!({"name": "notes", "state": "foreign", "agents": ["claude", "opencode"], "origin": null}),
            json!({"name": "review", "state": "current", "agents": ["claude", "codex", "opencode"], "origin": "workspace"}),
        ]
    );
}

#[test]
fn an_artifacts_files_are_listed_and_one_read_as_text_with_its_hash() {
    let world = World::projects(&["api"]);
    world.add("~/Projects", "~/Projects", Kind::Skill, "review", REVIEW);
    let workspace = world.workspace("~/Projects");

    let files = Files::of_library(&workspace, "skill:review").expect("list its files");
    let contents =
        Contents::of_library(&workspace, "skill:review", "SKILL.md").expect("read its file");

    assert_eq!(
        world.json(&files),
        json!({
            "entries": [{"folder": false, "path": "SKILL.md", "refused": null, "size": 74}],
            "kind": "skill",
            "manifest": "SKILL.md",
            "name": "review",
            "truncated": false,
        })
    );
    assert_eq!(
        world.json(&contents),
        json!({
            "file": "SKILL.md",
            "hash": "785660eaf65b8fce834ca050b26ed265b4d787871877ff87f05b52212d32dbca",
            "kind": "skill",
            "manifest": {
                "body": "\n# review\n",
                "front_matter": [
                    {"key": "name", "value": "review"},
                    {"key": "description", "value": REVIEW},
                ],
            },
            "name": "review",
            "size": 74,
            "text": format!("---\nname: review\ndescription: {REVIEW}\n---\n\n# review\n"),
            "withheld": null,
        })
    );
}

#[test]
fn a_file_leading_out_of_its_artifact_through_a_link_is_never_read() {
    let world = World::projects(&["api"]);
    world
        .add("~/Projects", "~/Projects", Kind::Skill, "review", REVIEW)
        .file("~/.ssh/id_ed25519", "a private key\n");
    std::os::unix::fs::symlink(
        world.at("~/.ssh/id_ed25519"),
        world.at("~/Projects/.mindweaver/library/skills/review/key"),
    )
    .expect("link out of the skill");
    let workspace = world.workspace("~/Projects");

    let refused =
        Contents::of_library(&workspace, "skill:review", "key").expect_err("a link leading out");

    let said = world.said(&refused);
    assert!(!said.contains("a private key"), "{said}");
    assert_eq!(refused.code(), "contents_outside");
}

#[test]
fn the_folders_inside_one_are_each_said_to_be_a_project_a_repository_or_a_folder() {
    let world = World::projects(&["api"]);
    world
        .folder("~/Projects/notes")
        .repository("~/Projects/site")
        .repository("~/Projects/web")
        .init_project("~/Projects/web");
    let workspace = world.workspace("~/Projects");

    let folders = Folders::read(
        &workspace,
        &world.at("~/Projects"),
        false,
        Some(&world.home()),
    )
    .expect("list the folders");

    assert_eq!(
        at(&world.json(&folders), "/folders"),
        &json!([
            {"is": "project", "linked": true, "name": "api", "path": "/home/you/Projects/api", "symlink": false},
            {"is": "folder", "linked": false, "name": "notes", "path": "/home/you/Projects/notes", "symlink": false},
            {"is": "repository", "linked": false, "name": "site", "path": "/home/you/Projects/site", "symlink": false},
            {"is": "project", "linked": false, "name": "web", "path": "/home/you/Projects/web", "symlink": false},
        ])
    );
}

#[test]
fn a_scan_finds_a_project_not_linked_yet_beside_the_one_linked() {
    let world = World::projects(&["api"]);
    world
        .repository("~/Projects/web")
        .init_project("~/Projects/web");
    let web = world.project_id("~/Projects/web");
    let workspace = world.workspace("~/Projects");

    let scanned = scan::scan(
        &workspace,
        &Request {
            roots: vec![world.at("~/Projects")],
            depth: 1,
            ..Request::default()
        },
    );

    let found: Vec<_> = world.json(&scanned)["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|found| {
            json!({
                "route": found["route"], "linked": found["linked"],
                "mind": found["mind"], "id": found["id"],
            })
        })
        .collect();
    let api = world.project_id("~/Projects/api");
    assert_eq!(
        found,
        [
            json!({"route": "api", "linked": true, "mind": true, "id": api}),
            json!({"route": "web", "linked": false, "mind": true, "id": web}),
        ]
    );
}
