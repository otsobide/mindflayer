//! The request log: a line for every request, and never the launch token.

mod common;

use common::*;

#[tokio::test]
async fn every_request_is_logged_once_answered() {
    let (server, lines) = launch_logged();
    let token = Auth::Bearer(server.token());

    post(&server, "/api/query", token, json!({ "echo": "x" })).await;
    post(&server, "/api/commands", token, json!({ "name": "a" })).await;
    get(&server, "/api/transcript?view=all", Auth::Nothing).await;
    get(&server, "/api/nowhere", token).await;

    assert_eq!(
        *lines.lock().unwrap(),
        [
            "POST /api/query 200",
            "POST /api/commands 200",
            "GET /api/transcript?view=all 401",
            "GET /api/nowhere 404",
        ]
    );
}

#[tokio::test]
async fn the_token_never_appears_in_a_logged_line() {
    let (server, lines) = launch_logged();
    let secret = server.token().to_owned();
    let cookie = format!("{}={secret}", server.cookie());

    // Everywhere a token can travel, and some places it should not.
    get(&server, &format!("/?token={secret}"), Auth::Nothing).await;
    get(
        &server,
        &format!("/?view=1&%74oken={secret}"),
        Auth::Nothing,
    )
    .await;
    get(&server, &format!("/?token={secret}x"), Auth::Nothing).await;
    get(&server, "/api/transcript", Auth::Cookie(&cookie)).await;
    get(&server, "/api/transcript", Auth::Bearer(&secret)).await;
    get(
        &server,
        &format!("/api/transcript?token={secret}"),
        Auth::Nothing,
    )
    .await;
    get(&server, &format!("/api/{secret}"), Auth::Bearer(&secret)).await;
    let shouted = secret.to_ascii_uppercase();
    get(
        &server,
        &format!("/api/transcript?t={shouted}"),
        Auth::Nothing,
    )
    .await;

    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 8, "{lines:#?}");
    for line in lines.iter() {
        assert!(
            !line.to_ascii_lowercase().contains(&secret),
            "the token is in {line:?}"
        );
    }
    assert_eq!(lines[0], "GET /?token=<redacted> 303");
    assert_eq!(lines[1], "GET /?view=1&%74oken=<redacted> 303");
    assert_eq!(lines[2], "GET /?token=<redacted> 401");
    assert_eq!(lines[6], "GET /api/<redacted> 404");
}

#[tokio::test]
async fn a_server_given_nowhere_to_log_answers_all_the_same() {
    let (server, _) = launch();

    let reply = get(&server, "/api/transcript", Auth::Bearer(server.token())).await;

    assert_eq!(reply.status, StatusCode::OK);
}
