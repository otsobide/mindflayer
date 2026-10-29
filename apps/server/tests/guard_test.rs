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

/// Each case names a part of the refusal it must get, so it shows which check
/// turned it away and not only that something did: the endpoint's JSON
/// extractor would answer a text body with a 415 of its own, and a case that
/// asserted only the status would pass with the guard's check gone.
#[tokio::test]
async fn a_request_the_panel_would_not_send_is_refused() {
    let (server, runs) = launch();
    let port = server.address().port();
    let host = "answers only as";
    let origin = "only from its own pages";
    let fetch = "Sec-Fetch-Site: same-origin";
    let json = "must be JSON";
    let custom = "X-Mindflayer: 1";

    let cases: Vec<(&str, Vec<Change>, StatusCode, &str)> = vec![
        (
            "a name rebound to 127.0.0.1",
            vec![set("host", format!("attacker.example:{port}"))],
            StatusCode::FORBIDDEN,
            host,
        ),
        (
            "another port",
            vec![set("host", format!("127.0.0.1:{}", port.wrapping_add(1)))],
            StatusCode::FORBIDDEN,
            host,
        ),
        ("no Host", vec![unset("host")], StatusCode::FORBIDDEN, host),
        (
            "a rebound name, forwarded as this one",
            vec![
                set("host", format!("attacker.example:{port}")),
                set("x-forwarded-host", format!("127.0.0.1:{port}")),
            ],
            StatusCode::FORBIDDEN,
            host,
        ),
        (
            "a page on another site",
            vec![set("origin", "https://attacker.example")],
            StatusCode::FORBIDDEN,
            origin,
        ),
        (
            "a page on the same site, from another port",
            vec![set(
                "origin",
                format!("http://127.0.0.1:{}", port.wrapping_add(1)),
            )],
            StatusCode::FORBIDDEN,
            origin,
        ),
        (
            "a page under another of the server's names",
            vec![set("origin", format!("http://localhost:{port}"))],
            StatusCode::FORBIDDEN,
            origin,
        ),
        (
            "a sandboxed frame or a file",
            vec![set("origin", "null")],
            StatusCode::FORBIDDEN,
            origin,
        ),
        (
            "the same site from another port, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "same-site")],
            StatusCode::FORBIDDEN,
            fetch,
        ),
        (
            "another site, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "cross-site")],
            StatusCode::FORBIDDEN,
            fetch,
        ),
        (
            "no token",
            vec![unset("cookie")],
            StatusCode::UNAUTHORIZED,
            "launch token",
        ),
        (
            "a text body, which a page may send anywhere unasked",
            vec![set("content-type", "text/plain")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            json,
        ),
        (
            "a form",
            vec![set("content-type", "application/x-www-form-urlencoded")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            json,
        ),
        (
            "no content type",
            vec![unset("content-type")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            json,
        ),
        (
            "no X-Mindflayer",
            vec![unset("x-mindflayer")],
            StatusCode::FORBIDDEN,
            custom,
        ),
        (
            "X-Mindflayer other than 1",
            vec![set("x-mindflayer", "yes")],
            StatusCode::FORBIDDEN,
            custom,
        ),
    ];
    for (case, changes, status, refusal) in cases {
        let reply = deliver(&server, from_the_panel(&server, &changes)).await;
        assert_eq!(reply.status, status, "{case}");
        let error = reply.json()["error"].clone();
        assert!(
            error.as_str().is_some_and(|error| error.contains(refusal)),
            "{case}: {error}"
        );
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
            "JSON with its charset",
            vec![set("content-type", "application/json; charset=utf-8")],
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

#[tokio::test]
async fn every_answer_carries_the_content_security_policy_and_nosniff() {
    let (server, _) = launch();
    let token = server.token();
    let mut rebound = request(&server, Method::GET, "/", Auth::Nothing, None);
    rebound.headers_mut().insert(
        "host",
        format!("attacker.example:{}", server.address().port())
            .parse()
            .unwrap(),
    );

    let replies = [
        ("the panel", get(&server, "/", Auth::Nothing).await),
        (
            "the launch link",
            get(&server, &format!("/?token={token}"), Auth::Nothing).await,
        ),
        (
            "the API",
            get(&server, "/api/transcript", Auth::Bearer(token)).await,
        ),
        (
            "the gate's refusal",
            get(&server, "/api/transcript", Auth::Nothing).await,
        ),
        ("the guard's refusal", deliver(&server, rebound).await),
    ];
    for (case, reply) in replies {
        assert_eq!(
            reply.header("content-security-policy"),
            Some("default-src 'self'; frame-ancestors 'none'"),
            "{case}"
        );
        assert_eq!(
            reply.header("x-content-type-options"),
            Some("nosniff"),
            "{case}"
        );
    }
}

#[tokio::test]
async fn no_answer_grants_another_origin_anything() {
    let (server, _) = launch();
    let port = server.address().port();

    for origin in [
        format!("http://127.0.0.1:{}", port.wrapping_add(1)),
        format!("http://127.0.0.1:{port}"),
    ] {
        // A preflight: what a browser asks before it lets a page send a
        // header of its own, or JSON, to another origin.
        let mut preflight = request(
            &server,
            Method::OPTIONS,
            "/api/commands",
            Auth::Nothing,
            None,
        );
        let headers = preflight.headers_mut();
        headers.insert("origin", origin.parse().unwrap());
        headers.insert("access-control-request-method", "POST".parse().unwrap());
        headers.insert(
            "access-control-request-headers",
            "content-type, x-mindflayer".parse().unwrap(),
        );

        let reply = deliver(&server, preflight).await;

        assert!(!reply.status.is_success(), "{origin}: {}", reply.status);
        let granted: Vec<_> = reply
            .headers
            .keys()
            .filter(|name| name.as_str().starts_with("access-control-"))
            .collect();
        assert!(granted.is_empty(), "{origin}: {granted:?}");
    }
}

/// Where `make web/dev` serves the panel from while it is worked on.
const DEV_SERVER: &str = "http://127.0.0.1:5173";

/// What the dev server's proxy forwards: the panel's change, with the
/// browser's Origin, the dev server's, and Host naming this server, as
/// `changeOrigin` makes it.
#[tokio::test]
async fn through_its_dev_server_the_panel_changes_things_and_nothing_else_does() {
    let server = Server::new(Fake::default())
        .dev_server(DEV_SERVER)
        .bind()
        .unwrap();
    let port = server.address().port();

    let proxied = from_the_panel(&server, &[set("origin", DEV_SERVER)]);
    let reply = deliver(&server, proxied).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());

    // Its own pages still: the panel built into the binary.
    let own = from_the_panel(
        &server,
        &[set("origin", format!("http://127.0.0.1:{port}"))],
    );
    assert_eq!(deliver(&server, own).await.status, StatusCode::OK);

    for (case, changes) in [
        ("another port", vec![set("origin", "http://127.0.0.1:5174")]),
        ("another name", vec![set("origin", "http://localhost:5173")]),
        (
            "naming the dev server in Host",
            vec![set("host", "127.0.0.1:5173"), set("origin", DEV_SERVER)],
        ),
    ] {
        let reply = deliver(&server, from_the_panel(&server, &changes)).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{case}");
    }

    assert_eq!(
        server.url(),
        format!("{DEV_SERVER}/?token={}", server.token())
    );
}

/// Only when the front end names it: otherwise the dev server is one more
/// page on another port of loopback.
#[tokio::test]
async fn without_being_told_of_it_the_server_refuses_the_dev_servers_pages() {
    let (server, runs) = launch();

    let reply = deliver(
        &server,
        from_the_panel(&server, &[set("origin", DEV_SERVER)]),
    )
    .await;

    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(
        reply.json()["error"],
        format!(
            "this server takes requests only from its own pages, at http://{}",
            server.address()
        )
    );
    assert!(runs.lock().unwrap().is_empty());
}
