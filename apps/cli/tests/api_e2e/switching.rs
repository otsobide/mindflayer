//! Which workspace the server acts on: the guide's "Switching workspace",
//! only to one this machine knows, as `/workspaces` lists it.

use serde_json::json;

use crate::{Scenario, Served};

/// A machine knowing three workspaces: the default one in the home, made
/// first, and `~/Projects`, linking `api`, and `~/Notes`, each remembered
/// by the default when `flayer init` made it.
fn three_known() -> Scenario {
    let scenario = Scenario::empty();
    scenario
        .given("~", "flayer init")
        .repository("~/Projects/api")
        .given("~/Projects/api", "mind init")
        .given("~/Projects", "flayer init")
        .given("~/Projects", "flayer link api")
        .folder("~/Notes")
        .given("~/Notes", "flayer init");
    scenario
}

/// What `/workspaces` lists of the default workspace, in the home: the
/// machine's, which nothing remembers since it keeps the list.
fn default_one() -> serde_json::Value {
    json!({
        "default": true, "in_use": false, "launched": false, "name": "you",
        "path": "/home/you", "projects": 0, "reason": "explicit", "remembered": false,
        "stale": null, "stale_projects": 0, "switchable": true,
    })
}

/// The workspace the snapshot is of.
fn reading(served: &Served<'_>) -> serde_json::Value {
    served.get("/snapshot").is(200).json()["workspace"].clone()
}

#[test]
fn workspaces_lists_every_one_this_machine_knows_and_which_may_be_switched_to() {
    let scenario = three_known();
    let served = scenario.serve("~/Projects");

    served.get("/workspaces").is(200).answers(json!([
        default_one(),
        {
            "default": false, "in_use": true, "launched": true, "name": "Projects",
            "path": "/home/you/Projects", "projects": 1, "reason": "above", "remembered": true,
            "stale": null, "stale_projects": 0, "switchable": false,
        },
        {
            "default": false, "in_use": false, "launched": false, "name": "Notes",
            "path": "/home/you/Notes", "projects": 0, "reason": "explicit", "remembered": true,
            "stale": null, "stale_projects": 0, "switchable": true,
        },
    ]));
}

#[test]
fn switching_to_a_listed_workspace_reads_and_changes_it_from_then_on() {
    let scenario = three_known();
    let served = scenario.serve("~/Projects");

    served
        .post("/switch", json!({"path": "/home/you/Notes"}))
        .is(200)
        .answers(json!({
            "default": false, "in_use": true, "launched": false, "name": "Notes",
            "path": "/home/you/Notes", "projects": 0, "reason": "explicit", "remembered": true,
            "stale": null, "stale_projects": 0, "switchable": false,
        }));
    assert_eq!(
        reading(&served),
        json!({"made": true, "name": "Notes", "path": "/home/you/Notes", "reason": "explicit"})
    );
    served
        .post(
            "/tasks/preview",
            json!({
                "type": "add", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "journal", "description": "Keep a journal", "template": null,
            }),
        )
        .is(200)
        .answers_at(
            "/lines",
            json!(["flayer -W /home/you/Notes add skill journal 'Keep a journal'"]),
        );
}

#[test]
fn switching_to_a_workspace_nobody_listed_is_refused_and_the_server_stays_where_it_was() {
    let scenario = three_known();
    let served = scenario.serve("~/Projects");

    served
        .post("/switch", json!({"path": "/home/you/Elsewhere"}))
        .refused(
            422,
            "switch_unlisted",
            "`/home/you/Elsewhere` is none of the workspaces this machine knows, nor the one \
             the panel was launched on: only one of those is switched to",
        );
    assert_eq!(
        reading(&served),
        json!({"made": true, "name": "Projects", "path": "/home/you/Projects", "reason": "above"})
    );
}

#[test]
fn a_workspace_is_switched_to_only_by_its_path_as_listed() {
    let scenario = three_known();
    let served = scenario.serve("~/Projects");

    // The same folder, spelled otherwise than the list spells it.
    served
        .post("/switch", json!({"path": "/home/you/Notes/"}))
        .refused(
            422,
            "switch_unlisted",
            "`/home/you/Notes/` is none of the workspaces this machine knows, nor the one \
             the panel was launched on: only one of those is switched to",
        );
    assert_eq!(reading(&served)["name"], "Projects");
}
