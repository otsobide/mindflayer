//! Starting the server and stopping it: the guide's "Starting the server
//! for a script" and "Stopping it", what `flayer ui` prints, the token file
//! it writes for a script, and the session it hands back on Ctrl-C.

use std::os::unix::fs::PermissionsExt;

use serde_json::json;

use crate::{guide_workspace, Scenario};

/// Where the guide has `ui` write the token file.
const TOKEN_FILE: &str = "~/.local/state/mindflayer.json";

#[test]
fn the_link_is_alone_on_stdout_and_stderr_says_what_is_served_until_when() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");
    let address = served.address;
    let token = served.token.clone();
    assert_eq!(served.link, format!("http://{address}/?token={token}"));
    assert_eq!(address.ip().to_string(), "127.0.0.1");
    assert_eq!(token.len(), 64, "a token of 32 bytes, in hex: {token}");
    assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));

    let stopped = served.stop();

    assert_eq!(stopped.status, Some(0));
    assert_eq!(stopped.stdout, "");
    assert_eq!(
        stopped.stderr,
        format!("note: serving the workspace at /home/you/Projects on {address} until Ctrl-C\n")
    );
}

#[test]
fn ctrl_c_prints_each_line_that_changed_something_so_the_session_runs_again() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    served
        .post(
            "/tasks/preview",
            json!({"type": "link", "path": "/home/you/Projects/web"}),
        )
        .is(200);
    served
        .given(json!({"type": "link", "path": "/home/you/Projects/web"}))
        .given(json!({
            "type": "install", "projects": ["web"], "kind": "skill", "name": "review", "from": null,
        }));
    served.get("/snapshot").is(200);

    let stopped = served.stop();

    assert_eq!(stopped.status, Some(0));
    assert_eq!(
        stopped.stdout,
        concat!(
            "$ flayer -W /home/you/Projects link /home/you/Projects/web\n",
            "linked web as web\n",
            "$ flayer -W /home/you/Projects install -p /home/you/Projects/web skill:review\n",
            "installed review into web, from workspace\n",
        )
    );
}

#[test]
fn the_token_file_says_where_the_api_is_and_its_token_for_this_account_alone() {
    let scenario = Scenario::projects(&["api"]);
    scenario.folder("~/.local/state");

    let served = scenario.serve_line(
        "~/Projects",
        &format!("flayer ui --no-open --token-file {TOKEN_FILE}"),
    );

    assert_eq!(
        scenario.read(TOKEN_FILE),
        format!(
            "{{\n  \"version\": 1,\n  \"api\": \"http://{}/api/v1\",\n  \"token\": \"{}\"\n}}\n",
            served.address, served.token
        )
    );
    let mode = std::fs::metadata(scenario.at(TOKEN_FILE))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_token_file_is_taken_away_once_the_server_stops() {
    let scenario = Scenario::projects(&["api"]);
    scenario.folder("~/.local/state");
    let served = scenario.serve_line(
        "~/Projects",
        &format!("flayer ui --no-open --token-file {TOKEN_FILE}"),
    );

    let stopped = served.stop();

    assert_eq!(stopped.status, Some(0));
    assert!(!scenario.exists(TOKEN_FILE));
}

#[test]
fn a_token_file_where_a_file_already_is_stops_the_launch_with_nothing_served() {
    let scenario = Scenario::projects(&["api"]);
    scenario.file(TOKEN_FILE, "kept\n");

    scenario
        .run(
            "~/Projects",
            &format!("flayer ui --no-open --token-file {TOKEN_FILE}"),
        )
        .exits(1)
        .prints("")
        .says(
            "error: /home/you/.local/state/mindflayer.json: already exists, and is left \
             alone, so the panel was not served: --token-file writes a file of its own, \
             never over one\n",
        );
    assert_eq!(scenario.read(TOKEN_FILE), "kept\n");
}

#[test]
fn a_token_file_in_a_folder_that_is_not_there_stops_the_launch() {
    let scenario = Scenario::projects(&["api"]);

    scenario
        .run(
            "~/Projects",
            &format!("flayer ui --no-open --token-file {TOKEN_FILE}"),
        )
        .exits(1)
        .prints("")
        .says(
            "error: /home/you/.local/state/mindflayer.json: its folder does not exist, so \
             the panel was not served: --token-file makes no folder\n",
        );
    assert!(!scenario.exists("~/.local"));
}

#[test]
fn serving_where_no_workspace_is_reads_the_default_one_without_making_it() {
    let scenario = Scenario::empty();
    let served = scenario.serve("~");

    served.get("/snapshot").is(200).answers_at(
        "/workspace",
        json!({"made": false, "name": "you", "path": "/home/you", "reason": "home"}),
    );
    served.get("/projects").is(200).answers(json!([]));
    served.stop();

    assert!(!scenario.exists("~/.mindflayer"));
}
