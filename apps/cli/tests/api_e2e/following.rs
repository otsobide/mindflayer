//! What the event stream tells: the guide's "Following what happens", a
//! stream opened as a script opens one and read an event at a time, while
//! a test runs a task, a command in a terminal or a switch.
//!
//! What a change on disk is told as comes from the watcher, which takes in
//! what happens together before it tells it: a test waits for the event it
//! is about, and holds that one, not how many came before it.

use serde_json::{json, Value};

use crate::{guide_workspace, linking, Scenario};

/// Whether `event` is of the type `kind`.
fn is(kind: &'static str) -> impl Fn(&Value) -> bool {
    move |event| event["type"] == kind
}

#[test]
fn every_stream_starts_by_saying_to_read_everything_and_whether_it_watches() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let mut stream = served.events();

    assert_eq!(
        [stream.told(), stream.told()],
        [
            json!({"type": "resync", "switches": 0}),
            json!({"type": "watching"}),
        ]
    );
}

#[test]
fn a_run_is_told_as_it_starts_and_once_it_finishes_whoever_sent_it() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    served
        .post(
            "/tasks",
            json!({"type": "link", "path": "/home/you/Projects/web"}),
        )
        .is(200);

    let told: Vec<Value> = stream
        .until(is("command_finished"))
        .into_iter()
        .filter(|event| event["type"] != "changed")
        .collect();
    let lines = json!(["flayer -W /home/you/Projects link /home/you/Projects/web"]);
    assert_eq!(
        told,
        [
            json!({"type": "command_started", "lines": lines}),
            json!({"type": "command_finished", "lines": lines}),
        ]
    );
}

#[test]
fn a_change_made_in_a_terminal_is_told_by_the_project_it_reached() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    scenario.given(
        "~/Projects/api",
        "mind add skill deploy 'Ship the service to staging'",
    );

    let told = stream.until(is("changed"));
    assert_eq!(
        told.last(),
        Some(&json!({"type": "changed", "scopes": [{"type": "project", "entry": "api"}]}))
    );
}

#[test]
fn a_change_to_the_library_made_in_an_editor_is_told_as_the_workspaces() {
    let scenario = linking(&["api"]);
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    scenario.file(
        "~/Projects/.mindflayer/library/skills/review/SKILL.md",
        &crate::added("review", "Read every change before it lands"),
    );

    let told = stream.until(is("changed"));
    assert_eq!(
        told.last(),
        Some(&json!({"type": "changed", "scopes": [{"type": "workspace"}]}))
    );
}

#[test]
fn a_switch_is_told_to_every_stream_with_how_many_have_been_taken() {
    let scenario = Scenario::empty();
    scenario
        .given("~", "flayer init")
        .folder("~/Projects")
        .given("~/Projects", "flayer init")
        .folder("~/Notes")
        .given("~/Notes", "flayer init");
    let served = scenario.serve("~/Projects");
    let mut first = served.events();
    first.opened();
    let mut second = served.events();
    second.opened();

    served
        .post("/switch", json!({"path": "/home/you/Notes"}))
        .is(200);

    for stream in [&mut first, &mut second] {
        assert_eq!(
            stream.until(is("switched")).last(),
            Some(&json!({"type": "switched", "switches": 1}))
        );
    }
}
