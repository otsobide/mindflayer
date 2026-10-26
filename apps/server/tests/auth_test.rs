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
        (
            Method::POST,
            "/api/commands/preview",
            Some(json!({ "name": "a" })),
        ),
        (Method::POST, "/api/commands", Some(json!({ "name": "a" }))),
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

// ---------------------------------------------------------------------------
// The launch link
// ---------------------------------------------------------------------------

/// The `name=value` a reply's `Set-Cookie` sets, without its attributes.
fn set_pair(reply: &Reply) -> String {
    let set = reply.header("set-cookie").expect("a cookie");
    set.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn the_launch_link_trades_its_token_for_a_cookie() {
    let (server, _) = launch();
    let router = server.router();
    let port = server.address().port();
    assert_eq!(
        server.url(),
        format!("http://127.0.0.1:{port}/?token={}", server.token())
    );
    let path = server
        .url()
        .replace(&format!("http://127.0.0.1:{port}"), "");

    let reply = get(&router, &path, Auth::Nothing).await;

    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.header("location"), Some("/"));
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(
        reply.header("set-cookie").unwrap(),
        format!(
            "mf_{port}={}; Path=/; HttpOnly; SameSite=Strict",
            server.token()
        )
    );

    let cookie = set_pair(&reply);
    let reply = post(
        &router,
        "/api/query",
        Auth::Cookie(&cookie),
        json!({ "echo": "in" }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn a_launch_link_with_another_token_sets_no_cookie() {
    let (server, _) = launch();
    let (other, _) = launch();

    let reply = get(
        &server.router(),
        &format!("/?token={}", other.token()),
        Auth::Nothing,
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.header("set-cookie"), None);
    assert!(reply.text().contains("not this server's"));
}

#[tokio::test]
async fn the_token_is_found_however_the_link_encodes_its_name() {
    let (server, _) = launch();

    let reply = get(
        &server.router(),
        &format!("/?panel=1&%74oken={}", server.token()),
        Auth::Nothing,
    )
    .await;

    assert_eq!(reply.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn two_servers_on_different_ports_set_differently_named_cookies() {
    let (first, _) = launch();
    let (second, _) = launch();
    assert_ne!(first.address().port(), second.address().port());

    let mut pairs = Vec::new();
    for server in [&first, &second] {
        let reply = get(
            &server.router(),
            &format!("/?token={}", server.token()),
            Auth::Nothing,
        )
        .await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER);
        let pair = set_pair(&reply);
        assert_eq!(
            pair,
            format!("mf_{}={}", server.address().port(), server.token())
        );
        pairs.push(pair);
    }
    let name = |pair: &str| pair.split('=').next().unwrap().to_owned();
    assert_ne!(name(&pairs[0]), name(&pairs[1]));

    // A browser sends both cookies to both ports, and each server takes only
    // its own.
    let both = format!("{}; {}", pairs[0], pairs[1]);
    for server in [&first, &second] {
        let reply = post(
            &server.router(),
            "/api/query",
            Auth::Cookie(&both),
            json!({ "echo": "x" }),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK);
    }
    let reply = post(
        &second.router(),
        "/api/query",
        Auth::Cookie(&pairs[0]),
        json!({ "echo": "x" }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}
