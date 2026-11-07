//! The skill directories on 127.0.0.1, each a server of its own in the
//! test's own process, answering with the fixtures core's suite reads
//! (`apps/core/tests/fixtures/finders`), so a search given their addresses
//! with `--finder-url` reaches nothing else: no test names GitHub's API,
//! skills.sh or api.claude-plugins.dev, and nothing reaches the network.
//! A port each, so each is an origin of its own, as the three services
//! are, and what was sent to which can be told apart.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// What a finder answers one path and query with: a status, headers and a
/// body.
pub type Answered = (u16, Vec<(String, String)>, String);

/// A finder served on a port of its own: each path and query it was given
/// answered as it was told, and any other with a 404.
pub struct FixtureFinder {
    /// Where it is, `http://127.0.0.1:<port>`.
    pub url: String,
    /// Each request's head, its first line and its headers, in order.
    heads: Arc<Mutex<Vec<String>>>,
}

impl FixtureFinder {
    /// A finder answering `target`, a path and its query, with a `200` and
    /// `body`.
    pub fn answering(target: &str, body: &str) -> Self {
        Self::serving(BTreeMap::from([(
            target.to_owned(),
            (200, Vec::new(), body.to_owned()),
        )]))
    }

    /// A finder answering each of `answers`.
    pub fn serving(answers: BTreeMap<String, Answered>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let heads = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&heads);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let head = head(&mut stream);
                let target = head.split_whitespace().nth(1).unwrap_or_default();
                let (status, headers, body) = answers
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| (404, Vec::new(), String::from("{}")));
                kept.lock().unwrap().push(head);
                let mut said = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (name, value) in headers {
                    said.push_str(&format!("{name}: {value}\r\n"));
                }
                let _ = write!(stream, "{said}\r\n{body}");
            }
        });
        Self { url, heads }
    }

    /// Each path and query it was asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        self.heads()
            .iter()
            .map(|head| {
                head.split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    /// Each request's head as it came, its headers among it.
    pub fn heads(&self) -> Vec<String> {
        self.heads.lock().unwrap().clone()
    }
}

/// Where each finder is asked for `pdf`, below its address.
pub const GITHUB_PDF: &str = "/search/code?q=pdf+filename%3ASKILL.md&per_page=20";
pub const SKILLS_SH_PDF: &str = "/api/search?q=pdf&limit=20";
pub const CLAUDE_PLUGINS_PDF: &str = "/api/skills/search?q=pdf&limit=20";

/// The three finders, each answering a search for `pdf` with its fixture
/// in core's suite.
pub struct Finders {
    pub github: FixtureFinder,
    pub skills_sh: FixtureFinder,
    pub claude_plugins: FixtureFinder,
}

impl Finders {
    pub fn start() -> Self {
        Self {
            github: FixtureFinder::answering(GITHUB_PDF, &finder_fixture("github.json")),
            skills_sh: FixtureFinder::answering(SKILLS_SH_PDF, &finder_fixture("skills-sh.json")),
            claude_plugins: FixtureFinder::answering(
                CLAUDE_PLUGINS_PDF,
                &finder_fixture("claude-plugins.json"),
            ),
        }
    }

    /// `--finder-url` for each, as a line takes it.
    pub fn options(&self) -> Vec<String> {
        [
            ("github", &self.github),
            ("skills.sh", &self.skills_sh),
            ("claude-plugins.dev", &self.claude_plugins),
        ]
        .into_iter()
        .flat_map(|(name, finder)| {
            [
                String::from("--finder-url"),
                format!("{name}={}", finder.url),
            ]
        })
        .collect()
    }

    /// `FINDER=URL` for each, as a task carries them.
    pub fn at(&self) -> Vec<String> {
        self.options()
            .into_iter()
            .filter(|word| word != "--finder-url")
            .collect()
    }
}

/// What the fixture `name` in core's suite holds.
pub fn finder_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/finders")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A request's head: its first line and its headers, read up to the empty
/// line that ends them.
fn head(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut byte = [0; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&head).into_owned()
}
