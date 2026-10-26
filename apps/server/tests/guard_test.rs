//! What keeps other pages out: every request the panel's page would not send
//! is refused, whether or not it carries the cookie, and every one it would
//! send gets through.

mod common;

use common::*;

/// A header to set to a value, or, for `None`, to take out.
type Change = (&'static str, Option<String>);

fn set(name: &'static str, value: impl Into<String>) -> Change {
    (name, Some(value.into()))
}

fn unset(name: &'static str) -> Change {
    (name, None)
}

/// A change to the disk as the panel's page asks for one: the cookie its
/// launch link set, and everything else a browser sends with it.
fn from_the_panel(server: &Listening, changes: &[Change]) -> Request<Body> {
    let cookie = format!("{}={}", server.cookie(), server.token());
    let mut request = request(
        server,
        Method::POST,
        "/api/commands",
        Auth::Cookie(&cookie),
        Some(json!({ "name": "change" })),
    );
    let headers = request.headers_mut();
    for (name, value) in changes {
        match value {
            Some(value) => {
                headers.insert(*name, value.parse().unwrap());
            }
            None => {
                headers.remove(*name);
            }
        }
    }
    request
}

#[tokio::test]
async fn a_request_the_panel_would_not_send_is_refused() {
    let (server, runs) = launch();
    let port = server.address().port();

    let cases: Vec<(&str, Vec<Change>, StatusCode)> = vec![
        (
            "a name rebound to 127.0.0.1",
            vec![set("host", format!("attacker.example:{port}"))],
            StatusCode::FORBIDDEN,
        ),
        (
            "another port",
            vec![set("host", format!("127.0.0.1:{}", port.wrapping_add(1)))],
            StatusCode::FORBIDDEN,
        ),
        ("no Host", vec![unset("host")], StatusCode::FORBIDDEN),
        (
            "a rebound name, forwarded as this one",
            vec![
                set("host", format!("attacker.example:{port}")),
                set("x-forwarded-host", format!("127.0.0.1:{port}")),
            ],
            StatusCode::FORBIDDEN,
        ),
        (
            "a page on another site",
            vec![set("origin", "https://attacker.example")],
            StatusCode::FORBIDDEN,
        ),
        (
            "a page on the same site, from another port",
            vec![set(
                "origin",
                format!("http://127.0.0.1:{}", port.wrapping_add(1)),
            )],
            StatusCode::FORBIDDEN,
        ),
        (
            "a page under another of the server's names",
            vec![set("origin", format!("http://localhost:{port}"))],
            StatusCode::FORBIDDEN,
        ),
        (
            "a sandboxed frame or a file",
            vec![set("origin", "null")],
            StatusCode::FORBIDDEN,
        ),
        (
            "the same site from another port, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "same-site")],
            StatusCode::FORBIDDEN,
        ),
        (
            "another site, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "cross-site")],
            StatusCode::FORBIDDEN,
        ),
        ("no token", vec![unset("cookie")], StatusCode::UNAUTHORIZED),
    ];
    for (case, changes, status) in cases {
        let reply = deliver(&server, from_the_panel(&server, &changes)).await;
        assert_eq!(reply.status, status, "{case}");
        assert!(reply.json()["error"].is_string(), "{case}");
    }
    let runs = runs.lock().unwrap();
    assert!(runs.is_empty(), "{runs:?}");
}

#[tokio::test]
async fn a_request_the_panel_would_send_gets_through() {
    let (server, runs) = launch();
    let port = server.address().port();

    let cases: Vec<(&str, Vec<Change>)> = vec![
        ("as the launch link names it", vec![]),
        (
            "with its own origin",
            vec![set("origin", format!("http://127.0.0.1:{port}"))],
        ),
        (
            "as localhost, from a page loaded as localhost",
            vec![
                set("host", format!("localhost:{port}")),
                set("origin", format!("http://localhost:{port}")),
            ],
        ),
        (
            "as [::1], from a page loaded as [::1]",
            vec![
                set("host", format!("[::1]:{port}")),
                set("origin", format!("http://[::1]:{port}")),
            ],
        ),
        (
            "forwarded from anywhere, which is not looked at",
            vec![set("x-forwarded-host", "attacker.example")],
        ),
    ];
    for (case, changes) in &cases {
        let reply = deliver(&server, from_the_panel(&server, changes)).await;
        assert_eq!(reply.status, StatusCode::OK, "{case}: {}", reply.text());
    }
    assert_eq!(runs.lock().unwrap().len(), 2 * cases.len());
}

#[tokio::test]
async fn a_page_on_a_rebound_name_gets_neither_the_panel_nor_a_cookie() {
    let (server, _) = launch();
    let rebound = format!("attacker.example:{}", server.address().port());

    for path in ["/".to_owned(), format!("/?token={}", server.token())] {
        let mut request = request(&server, Method::GET, &path, Auth::Nothing, None);
        request
            .headers_mut()
            .insert("host", rebound.parse().unwrap());
        let reply = deliver(&server, request).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            reply.header("content-type"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(reply.header("set-cookie"), None);
        assert!(reply.text().contains("answers only as 127.0.0.1:"));
    }
}

#[tokio::test]
async fn a_change_by_cookie_without_fetch_metadata_is_refused_and_taken_by_bearer() {
    let (server, runs) = launch();

    let reply = deliver(&server, from_the_panel(&server, &[unset("sec-fetch-site")])).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(reply.json()["error"]
        .as_str()
        .unwrap()
        .contains("Sec-Fetch-Site"));
    assert!(runs.lock().unwrap().is_empty());

    let bearer = set("authorization", format!("Bearer {}", server.token()));
    let reply = deliver(
        &server,
        from_the_panel(&server, &[unset("sec-fetch-site"), bearer]),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(*runs.lock().unwrap(), ["start change", "end change"]);
}

#[tokio::test]
async fn a_read_by_cookie_needs_no_fetch_metadata() {
    let (server, _) = launch();
    let cookie = format!("{}={}", server.cookie(), server.token());
    let mut request = request(
        &server,
        Method::GET,
        "/api/transcript",
        Auth::Cookie(&cookie),
        None,
    );
    request.headers_mut().remove("sec-fetch-site");

    let reply = deliver(&server, request).await;

    assert_eq!(reply.status, StatusCode::OK);
}
