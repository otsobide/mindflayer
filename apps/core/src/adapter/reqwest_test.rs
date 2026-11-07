//! Unit tests for `reqwest.rs`: the default transport, through the client
//! every catalog fetches with, against a server each test starts on this
//! machine, on a port the system picks. Nothing else is reached: every URL
//! names 127.0.0.1, and one a redirect names elsewhere is refused before it
//! could be sent.

use super::*;

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::http::{Client, Limits};

/// What a server does with one connection once it has read the request.
type Answering = Box<dyn FnOnce(&mut TcpStream) + Send>;

/// A server on 127.0.0.1, answering the connections it accepts each with
/// the next of its answers, in order, then accepting no more.
struct Server {
    url: String,
    /// The head of each request it read, as it was sent.
    heads: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(answers: Vec<Answering>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let heads = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&heads);
        thread::spawn(move || {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                kept.lock().unwrap().push(head(&mut stream));
                answer(&mut stream);
            }
        });
        Self { url, heads }
    }

    fn at(&self, path: &str) -> String {
        format!("{}{path}", self.url)
    }

    fn heads(&self) -> Vec<String> {
        self.heads.lock().unwrap().clone()
    }
}

/// The request's head, up to the empty line that ends it.
fn head(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut read = Vec::new();
    let mut byte = [0; 1];
    while !read.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => read.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&read).into_owned()
}

/// Answer with `status`, these headers and `body`, and close the connection.
fn answer(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Answering {
    let mut written = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for (name, value) in headers {
        written.push_str(&format!("{name}: {value}\r\n"));
    }
    written.push_str("\r\n");
    let mut written = written.into_bytes();
    written.extend_from_slice(body);
    Box::new(move |stream: &mut TcpStream| {
        let _ = stream.write_all(&written);
    })
}

fn ok(body: &str) -> Answering {
    let length = body.len().to_string();
    answer(
        "200 OK",
        &[
            ("Content-Length", &length),
            ("Content-Type", "application/json"),
        ],
        body.as_bytes(),
    )
}

/// Write `first`, then hold the connection open, sending nothing, for
/// longer than any limit a test sets.
fn stalling(first: &'static [u8]) -> Answering {
    Box::new(move |stream: &mut TcpStream| {
        let _ = stream.write_all(first);
        thread::sleep(Duration::from_secs(3));
    })
}

fn limits(timeout: Duration, max_bytes: u64) -> Limits {
    Limits {
        timeout,
        max_bytes,
        ..Limits::default()
    }
}

#[test]
fn an_answer_comes_back_through_the_client() {
    let server = Server::start(vec![ok(r#"{"servers":[]}"#)]);
    let http = Reqwest::new().unwrap();

    let fetched = Client::new(&http)
        .get(&Request::get(server.at("/v0/servers")))
        .unwrap();

    assert_eq!(fetched.status, 200);
    assert_eq!(fetched.body, br#"{"servers":[]}"#);
    assert_eq!(
        fetched.headers.get("content-type"),
        Some("application/json")
    );
}

/// What a service reads of who is asking, and the headers a request was
/// given, and no other header of the transport's own choosing.
#[test]
fn the_user_agent_and_a_requests_headers_are_sent() {
    let server = Server::start(vec![ok("[]")]);
    let http = Reqwest::new().unwrap();

    Client::new(&http)
        .get(
            &Request::get(server.at("/search?q=skills"))
                .header("Accept", "application/json")
                .credential("X-Api-Key", "k3y"),
        )
        .unwrap();

    let heads = server.heads();
    assert_eq!(heads.len(), 1);
    let head = heads[0].to_ascii_lowercase();
    assert!(
        head.starts_with("get /search?q=skills http/1.1\r\n"),
        "{head}"
    );
    assert!(
        head.contains(&format!("\r\nuser-agent: {USER_AGENT}\r\n")),
        "{head}"
    );
    assert!(head.contains("\r\naccept: application/json\r\n"), "{head}");
    assert!(head.contains("\r\nx-api-key: k3y\r\n"), "{head}");
    assert!(!head.contains("proxy"), "{head}");
}

/// A body that says no length is read until the cap, and no further.
#[test]
fn a_body_past_the_cap_is_refused_while_it_is_read() {
    let server = Server::start(vec![answer("200 OK", &[], &vec![b'x'; 1024 * 1024])]);
    let http = Reqwest::new().unwrap();

    let error = Client::new(&http)
        .with_limits(limits(Duration::from_secs(10), 64 * 1024))
        .get(&Request::get(server.at("/big")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_too_large");
}

#[test]
fn a_length_declared_past_the_cap_is_refused() {
    let server = Server::start(vec![stalling(
        b"HTTP/1.1 200 OK\r\nContent-Length: 1000000000\r\n\r\nxx",
    )]);
    let http = Reqwest::new().unwrap();
    let started = Instant::now();

    let error = Client::new(&http)
        .with_limits(limits(Duration::from_secs(10), 1024))
        .get(&Request::get(server.at("/big")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_too_large");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

/// A server that sends the start of a body and then nothing more is held to
/// the whole exchange's limit, not one per read.
#[test]
fn a_body_that_stops_coming_is_cut_at_the_time_limit() {
    let server = Server::start(vec![stalling(
        b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n0123456789",
    )]);
    let http = Reqwest::new().unwrap();
    let started = Instant::now();

    let error = Client::new(&http)
        .with_limits(limits(Duration::from_millis(300), 1024 * 1024))
        .get(&Request::get(server.at("/slow")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_timed_out");
    assert!(
        matches!(&error, HttpError::Timeout { after, .. } if *after == Duration::from_millis(300)),
        "{error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_server_that_never_answers_is_cut_at_the_time_limit() {
    let server = Server::start(vec![stalling(b"")]);
    let http = Reqwest::new().unwrap();
    let started = Instant::now();

    let error = Client::new(&http)
        .with_limits(limits(Duration::from_millis(300), 1024))
        .get(&Request::get(server.at("/silent")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_timed_out");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

/// A port nothing listens on any more, on this machine.
#[test]
fn a_refused_connection_is_said_as_one() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v0/servers", listener.local_addr().unwrap());
    drop(listener);
    let http = Reqwest::new().unwrap();

    let error = Client::new(&http)
        .get(&Request::get(&url))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_connect_failed");
    assert!(
        error
            .to_string()
            .starts_with(&format!("{url}: cannot connect: ")),
        "{error}"
    );
}

#[test]
fn a_status_that_is_not_a_success_is_a_failure() {
    let server = Server::start(vec![answer(
        "404 Not Found",
        &[("Content-Length", "2")],
        b"no",
    )]);
    let http = Reqwest::new().unwrap();

    let error = Client::new(&http)
        .get(&Request::get(server.at("/gone")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_status_unsuccessful");
}

/// reqwest follows none: the client does, by its rules, so the second
/// request is one the server sees.
#[test]
fn a_redirect_is_handed_back_and_followed_by_the_client() {
    let server = Server::start(vec![
        answer(
            "302 Found",
            &[("Location", "/moved"), ("Content-Length", "0")],
            b"",
        ),
        ok("[]"),
    ]);
    let http = Reqwest::new().unwrap();

    let fetched = Client::new(&http)
        .get(&Request::get(server.at("/first")))
        .unwrap();

    assert_eq!(fetched.url, server.at("/moved"));
    let heads = server.heads();
    assert_eq!(heads.len(), 2);
    assert!(
        heads[1].starts_with("GET /moved HTTP/1.1\r\n"),
        "{}",
        heads[1]
    );
}

#[test]
fn redirects_past_the_bound_are_not_followed() {
    let again = || {
        answer(
            "302 Found",
            &[("Location", "/again"), ("Content-Length", "0")],
            b"",
        )
    };
    let server = Server::start(vec![again(), again(), again(), again()]);
    let http = Reqwest::new().unwrap();

    let error = Client::new(&http)
        .with_limits(Limits {
            max_redirects: 2,
            ..Limits::default()
        })
        .get(&Request::get(server.at("/again")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_too_many_redirects");
    assert_eq!(server.heads().len(), 3);
}

/// To another scheme on this very machine, so that a rule that let it
/// through would reach nothing but the test's own port.
#[test]
fn a_redirect_to_another_scheme_is_refused_and_never_followed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let elsewhere = format!("ftp://{}/servers", listener.local_addr().unwrap());
    let server = Server::start(vec![answer(
        "301 Moved Permanently",
        &[("Location", &elsewhere), ("Content-Length", "0")],
        b"",
    )]);
    let http = Reqwest::new().unwrap();

    let error = Client::new(&http)
        .get(&Request::get(server.at("/first")))
        .unwrap_err()
        .error;

    assert_eq!(error.code(), "http_redirect_refused");
    assert_eq!(server.heads().len(), 1);
}

/// Refused before it is sent, and said by its name alone: the value may be
/// a credential.
#[test]
fn a_header_http_cannot_carry_is_refused_without_its_value() {
    let server = Server::start(vec![]);
    let http = Reqwest::new().unwrap();

    for request in [
        Request::get(server.at("/")).header("Bad Name", "x"),
        Request::get(server.at("/")).credential("X-Api-Key", "k3y\r\nInjected: yes"),
    ] {
        let error = Client::new(&http).get(&request).unwrap_err().error;

        assert_eq!(error.code(), "http_header_invalid");
        assert!(!error.to_string().contains("k3y"), "{error}");
    }
}
