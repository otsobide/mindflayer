//! An MCP Registry on 127.0.0.1, in the test's own process, answering with
//! the fixtures core's suite reads (`apps/core/tests/fixtures/registry`),
//! so a command given its address with `--registry` reaches nothing else:
//! no test names the official registry, and nothing reaches the network.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A registry served on a port of its own: each path and query it was
/// given answered with its body, and any other with a 404.
pub struct FixtureRegistry {
    /// Where it is, `http://127.0.0.1:<port>`, what `--registry` is given.
    pub url: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl FixtureRegistry {
    /// The fixtures as the API answers with them: a search for `acme`
    /// over the 2025-09-29 schema's two pages, one for `notes` in the
    /// 2025-12-11 schema's one, and each server either lists that has a
    /// name and a version, by its name and `latest` or its version,
    /// `io.github.acme/weather` answered with the fixture of one version.
    pub fn start() -> Self {
        let mut answers = BTreeMap::new();
        let search = |query: &str| format!("/v0.1/servers?search={query}&version=latest&limit=100");
        let pages = [
            (search("acme"), fixture("2025-09-29", "search-1.json")),
            (
                format!(
                    "{}&cursor=io.github.acme%2Fdotnet-weather%3A2.1.0",
                    search("acme")
                ),
                fixture("2025-09-29", "search-2.json"),
            ),
            (search("notes"), fixture("2025-12-11", "search.json")),
        ];
        for (_, page) in &pages {
            let page: serde_json::Value = serde_json::from_str(page).unwrap();
            for listed in page["servers"].as_array().unwrap() {
                let server = &listed["server"];
                let (Some(name), Some(version)) =
                    (server["name"].as_str(), server["version"].as_str())
                else {
                    continue;
                };
                if name.contains(['\r', '\n']) {
                    continue;
                }
                let body = listed.to_string();
                for asked in ["latest", version] {
                    answers.insert(versions(name, asked), body.clone());
                }
            }
        }
        answers.extend(pages);
        for asked in ["latest", "1.0.2"] {
            answers.insert(
                versions("io.github.acme/weather", asked),
                fixture("2025-09-29", "version.json"),
            );
        }
        Self::serving(answers)
    }

    /// A registry answering each of `answers`, a path and its query, with
    /// its body.
    pub fn serving(answers: BTreeMap<String, String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&asked);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let target = target(&mut stream);
                let (status, body) = match answers.get(&target) {
                    Some(body) => ("200 OK", body.as_str()),
                    None => ("404 Not Found", "{}"),
                };
                kept.lock().unwrap().push(target);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { url, asked }
    }

    /// Each path and query it was asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// The path of the version `version` of the server `name`, its name one
/// segment though it holds a `/`.
fn versions(name: &str, version: &str) -> String {
    format!(
        "/v0.1/servers/{}/versions/{version}",
        name.replace('/', "%2F")
    )
}

/// What the fixture `schema/name` holds.
pub fn fixture(schema: &str, name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/registry")
        .join(schema)
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The path and query a request asked for: the second word of its first
/// line, its head read up to the empty line that ends it.
fn target(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut byte = [0; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&head)
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned()
}
