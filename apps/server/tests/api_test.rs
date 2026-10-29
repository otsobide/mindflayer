//! The JSON endpoints: what reaches the backend, what comes back, and what a
//! request the API does not know gets.

mod common;

use common::*;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

#[tokio::test]
async fn a_query_reaches_the_backend_and_its_answer_comes_back() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/query",
        Auth::Bearer(server.token()),
        json!({ "echo": "what is linked" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(reply.json(), json!({ "echo": "what is linked" }));
    assert!(runs.lock().unwrap().is_empty(), "a query runs nothing");
}

#[tokio::test]
async fn a_query_the_backend_cannot_read_gets_a_json_error() {
    let (server, _) = launch();

    let reply = post(
        &server,
        "/api/query",
        Auth::Bearer(server.token()),
        json!({ "wrong": 1 }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reply.json()["error"].as_str().unwrap().contains("echo"));

    let reply = send(
        &server,
        Method::POST,
        "/api/query",
        Auth::Bearer(server.token()),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(reply.json()["error"].is_string());
}

/// A query the backend read and will not answer, as its front end's rules
/// say: a 422, the code beside the message for the panel to act on.
#[tokio::test]
async fn a_query_the_backend_refuses_says_why_with_its_code() {
    let (server, _) = launch();

    let reply = post(
        &server,
        "/api/query",
        Auth::Bearer(server.token()),
        json!({ "echo": "/etc/passwd", "refuse": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "/etc/passwd is refused", "code": "fake_refused" })
    );
}

#[tokio::test]
async fn a_preview_shows_a_tasks_lines_and_runs_nothing() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/commands/preview",
        Auth::Bearer(server.token()),
        json!({ "name": "link beta" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json(), json!({ "lines": ["fake run link beta"] }));
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_preview_the_backend_refuses_says_why_with_its_code() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/commands/preview",
        Auth::Bearer(server.token()),
        json!({ "name": "edit", "refuse": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "edit is refused", "code": "fake_refused" })
    );
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_api_path_gets_a_json_404() {
    let (server, _) = launch();

    for path in [
        "/api/no-such-endpoint",
        "/api/query/deeper",
        "/api",
        "/api/",
    ] {
        let reply = get(&server, path, Auth::Bearer(server.token())).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.header("content-type"), Some("application/json"));
        assert_eq!(
            reply.json(),
            json!({ "error": format!("no such endpoint: GET {path}") })
        );
    }
}

#[tokio::test]
async fn an_endpoint_asked_with_the_wrong_method_says_so_in_json() {
    let (server, _) = launch();

    let reply = get(&server, "/api/query", Auth::Bearer(server.token())).await;

    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(reply.header("allow"), Some("POST"));
    assert_eq!(
        reply.json(),
        json!({ "error": "/api/query does not take GET" })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_answers_on_loopback_on_a_port_the_system_chose() {
    let (server, _) = launch();
    let address = server.address();
    let token = server.token().to_owned();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(address.port(), 0);

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async {
        let _ = stopped.await;
    }));
    let response = tokio::task::spawn_blocking(move || {
        let body = r#"{"echo":"over the wire"}"#;
        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "POST /api/query HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(
        response.ends_with(r#"{"echo":"over the wire"}"#),
        "{response}"
    );
}

/// What the panel's dev server needs, whose proxy names the port in its
/// settings. The port was free a moment ago: another process could take it
/// in between, which would fail this test rather than pass it wrongly.
#[test]
fn the_server_listens_on_the_port_it_is_given() {
    let free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let server = Server::new(Fake::default()).port(free).bind().unwrap();

    assert_eq!(
        server.address(),
        SocketAddr::from((Ipv4Addr::LOCALHOST, free))
    );
    assert_eq!(
        server.url(),
        format!("http://127.0.0.1:{free}/?token={}", server.token())
    );
}

/// Another port would be one the client that asked for this one never
/// finds.
#[test]
fn a_port_somebody_holds_is_an_error_rather_than_another_port() {
    let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = held.local_addr().unwrap().port();

    let error = Server::new(Fake::default()).port(port).bind().err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(std::io::ErrorKind::AddrInUse)
    );
}
