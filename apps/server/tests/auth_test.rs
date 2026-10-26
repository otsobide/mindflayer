//! Who the API answers: whoever holds the launch token, as a cookie named for
//! the port or as a Bearer token.

mod common;

use common::*;

#[tokio::test]
async fn a_request_without_the_token_is_refused() {
    let (server, runs) = launch();
    let router = server.router();

    for (method, path, body) in [
        (Method::POST, "/api/query", Some(json!({ "echo": "hi" }))),
        (Method::GET, "/api/no-such-endpoint", None),
        (Method::GET, "/api", None),
    ] {
        let reply = send(&router, method.clone(), path, Auth::Nothing, body).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(reply.header("www-authenticate"), Some("Bearer"));
        assert!(reply.json()["error"].as_str().unwrap().contains("token"));
    }
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (server, _) = launch();
    let router = server.router();
    let (other, _) = launch();

    for auth in [
        Auth::Bearer(other.token()),
        Auth::Bearer(""),
        Auth::Cookie(&format!("{}={}", server.cookie(), other.token())),
    ] {
        let reply = post(&router, "/api/query", auth, json!({ "echo": "hi" })).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn a_bearer_token_is_accepted() {
    let (server, _) = launch();

    let reply = post(
        &server.router(),
        "/api/query",
        Auth::Bearer(server.token()),
        json!({ "echo": "hi" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json(), json!({ "echo": "hi" }));
}

#[tokio::test]
async fn the_cookie_named_for_the_port_is_accepted_among_others() {
    let (server, _) = launch();
    let cookies = format!(
        "theme=dark; {}={}; other=1",
        server.cookie(),
        server.token()
    );

    let reply = post(
        &server.router(),
        "/api/query",
        Auth::Cookie(&cookies),
        json!({ "echo": "hi" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn the_token_under_another_ports_cookie_is_refused() {
    let (server, _) = launch();
    let port = server.address().port();
    let elsewhere = format!("mf_{}={}", port.wrapping_add(1), server.token());

    let reply = post(
        &server.router(),
        "/api/query",
        Auth::Cookie(&elsewhere),
        json!({ "echo": "hi" }),
    )
    .await;

    assert_eq!(server.cookie(), format!("mf_{port}"));
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}
