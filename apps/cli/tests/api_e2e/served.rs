//! `flayer ui` started in a scenario, and what a script asks of it.
//!
//! The server is the binary as a person starts it, `flayer ui --no-open`
//! typed in a folder of the scenario, with the environment every command
//! of the scenario gets: `PATH` and the home, nothing else. The link it
//! prints is read for where it listens and its token, and every request is
//! sent over loopback as curl sends it, a whole request with
//! `Connection: close`, so what a test holds is what a script is answered.
//!
//! A path a test spells in a request is spelled as the person would,
//! under `/home`, and every path an answer holds is read back the same
//! way, so an answer reads as the API's guide shows it (`docs/api.md`).
//! Each answer to a route the API describes is held to its description
//! too (`contract::answered`), so a use case that meets an answer outside
//! the document fails.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, ChildStdout, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::contract;
use crate::Scenario;

/// What every route is under.
const API: &str = "/api/v1";

/// How long a request, a stop or an event is waited for: long past
/// anything a run takes, for a machine under load, and still bounded.
const PATIENCE: Duration = Duration::from_secs(30);

impl Scenario {
    /// `flayer ui --no-open` typed in `at`: the panel and its API served
    /// on a port the system picks, with no browser opened.
    pub fn serve(&self, at: &str) -> Served<'_> {
        self.serve_line(at, "flayer ui --no-open")
    }

    /// `line`, a launch of `flayer ui` as it is typed, from `at`, held to
    /// print its link before anything is asked of it.
    pub fn serve_line(&self, at: &str, line: &str) -> Served<'_> {
        let mut child = self
            .command(at, line, &[])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the server");
        let mut stdout = BufReader::new(child.stdout.take().expect("its stdout"));
        let mut link = String::new();
        stdout.read_line(&mut link).expect("read its link");
        let Some(link) = link.strip_suffix('\n').map(str::to_owned) else {
            let _ = child.wait();
            let mut said = String::new();
            let _ = child
                .stderr
                .take()
                .expect("its stderr")
                .read_to_string(&mut said);
            panic!(
                "`{line}` printed no link; stderr:\n{}",
                self.seen(said.as_bytes())
            );
        };
        let (address, token) = link
            .strip_prefix("http://")
            .and_then(|link| link.split_once("/?token="))
            .unwrap_or_else(|| panic!("`{line}` printed no link but {link:?}"));
        Served {
            scenario: self,
            line: line.to_owned(),
            address: address.parse().expect("the link names an address"),
            token: token.to_owned(),
            link,
            child,
            stdout,
        }
    }
}

/// A server running in a scenario, killed when the test lets go of it
/// unless the test stopped it.
pub struct Served<'a> {
    scenario: &'a Scenario,
    line: String,
    pub address: SocketAddr,
    pub token: String,
    /// The link, as it was printed.
    pub link: String,
    child: Child,
    /// What it prints after the link.
    stdout: BufReader<ChildStdout>,
}

impl Served<'_> {
    /// `GET <path>` under `/api/v1`, with the token as a script sends it.
    pub fn get(&self, route: &str) -> Answer {
        self.send(Ask::get(route))
    }

    /// `POST <path>` under `/api/v1`, with `body` as JSON and the token as
    /// a script sends it.
    pub fn post(&self, route: &str, body: Value) -> Answer {
        self.send(Ask::post(route, body))
    }

    /// Run `task` as a step of building the world through the API, held
    /// to what every such step must do: answered 200, with every line it
    /// ran gone well and nothing said on stderr.
    pub fn given(&self, task: Value) -> &Self {
        let answer = self.post("/tasks", task);
        let ran = answer.json();
        let clean = answer.status == 200
            && ran["outcome"]["ok"] == true
            && ran["outcome"]["runs"].as_array().is_some_and(|runs| {
                runs.iter()
                    .all(|run| run["stderr"] == serde_json::json!([]))
            });
        if !clean {
            panic!("a task building the scenario did not go cleanly:\n{answer}");
        }
        self
    }

    /// What `ask` says, sent: its paths spelled under `/home` read as the
    /// scenario's, and what came back with every path read as `/home`.
    pub fn send(&self, ask: Ask) -> Answer {
        let port = self.address.port().to_string();
        let target = self.target(&ask.target);
        let body = ask.body.map(|body| match body {
            Body::Json(value) => self.sent(value).to_string(),
            Body::Text(text) => text,
        });
        let mut headers = vec![format!("Host: {}", self.address)];
        if ask.bearer {
            headers.push(format!("Authorization: Bearer {}", self.token));
        }
        if ask.page {
            headers.extend([
                format!("Cookie: mf_{port}={}", self.token),
                format!("Origin: http://{}", self.address),
                String::from("Sec-Fetch-Site: same-origin"),
                String::from("X-Mindflayer: 1"),
            ]);
        }
        if body.is_some() && ask.json {
            headers.push(String::from("Content-Type: application/json"));
        }
        // One a test names in place of the one sent otherwise, and one it
        // leaves out left out.
        for header in &ask.headers {
            let header = header.replace("{port}", &port);
            headers.retain(|sent| !same_header(sent, &header));
            headers.push(header);
        }
        headers.retain(|sent| !ask.without.iter().any(|name| same_header(sent, name)));
        let body = body.unwrap_or_default();
        let mut request = format!(
            "{} {target} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n",
            ask.method,
            body.len()
        );
        for header in &headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        request.push_str(&body);

        let mut stream = TcpStream::connect(self.address).expect("reach the server");
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        stream
            .write_all(request.as_bytes())
            .expect("send the request");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .expect("read the answer to the end");
        let response = String::from_utf8(response).expect("an answer in UTF-8");
        let (head, raw) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("no answer to {} {target}: {response:?}", ask.method));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("no status in {head:?}"));
        let asked = format!("{} {}", ask.method, ask.target);
        if let Ok(json) = serde_json::from_str::<Value>(raw) {
            if described(ask.method, &ask.target) {
                contract::answered(ask.method, &ask.target, status, &json);
            }
        }
        Answer {
            asked,
            status,
            head: self.scenario.seen(head.as_bytes()),
            body: self.scenario.seen(raw.as_bytes()),
        }
    }

    /// The event stream, opened as a script opens it.
    pub fn events(&self) -> Stream<'_> {
        let mut stream = TcpStream::connect(self.address).expect("reach the server");
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let request = format!(
            "GET {API}/events HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\n",
            self.address, self.token
        );
        stream
            .write_all(request.as_bytes())
            .expect("ask for the stream");
        Stream {
            reader: BufReader::new(stream),
            pending: String::new(),
            scenario: self.scenario,
        }
    }

    /// Ctrl-C, as a terminal sends it, and what the server did once it
    /// was told: how it exited, what it printed after its link, and what
    /// it said on stderr, from its first line.
    pub fn stop(mut self) -> Stopped {
        let pid = self.child.id().to_string();
        let sent = std::process::Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .expect("run kill");
        assert!(sent.success(), "kill -INT {pid} failed");
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("wait for the server") {
                break status;
            }
            assert!(
                started.elapsed() < PATIENCE,
                "`{}` still running {PATIENCE:?} after Ctrl-C",
                self.line
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut stdout = String::new();
        self.stdout.read_to_string(&mut stdout).unwrap();
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("its stderr")
            .read_to_string(&mut stderr)
            .unwrap();
        Stopped {
            status: status.code(),
            stdout: self.scenario.seen(stdout.as_bytes()),
            stderr: self.scenario.seen(stderr.as_bytes()),
        }
    }

    /// `target` as it is sent: each value of its query spelled under
    /// `/home` read as the scenario's, and encoded as a browser's form
    /// encodes it.
    fn target(&self, target: &str) -> String {
        let Some((path, query)) = target.split_once('?') else {
            return target.to_owned();
        };
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((name, value)) if under_home(value) => format!(
                    "{name}={}",
                    encoded(&self.scenario.at(value).to_string_lossy())
                ),
                _ => pair.to_owned(),
            })
            .collect();
        format!("{path}?{}", pairs.join("&"))
    }

    /// `value`, a body as a test spells it, as it is sent: every string in
    /// it spelled under `/home` read as the scenario's.
    fn sent(&self, value: Value) -> Value {
        match value {
            Value::String(text) if under_home(&text) => {
                Value::String(self.scenario.at(&text).to_string_lossy().into_owned())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.sent(v)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(name, v)| (name, self.sent(v)))
                    .collect(),
            ),
            other => other,
        }
    }
}

impl Drop for Served<'_> {
    fn drop(&mut self) {
        // A test that failed, or never stopped it: nothing of it may
        // outlive the test, its scenario's folder least of all.
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Whether `sent` is the header `named`, which may be a header whole.
fn same_header(sent: &str, named: &str) -> bool {
    let name = |header: &str| {
        header
            .split(':')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    name(sent).eq_ignore_ascii_case(&name(named))
}

/// Whether `value` is a path spelled under the scenario's `/home`.
fn under_home(value: &str) -> bool {
    value == "/home" || value.starts_with("/home/")
}

/// Whether the API's document describes `method` at `target`: every route
/// under `/api/v1` but those a test sends to be refused as none.
fn described(method: &str, target: &str) -> bool {
    let path = target.split('?').next().unwrap_or(target);
    let Some(route) = path.strip_prefix(API) else {
        return false;
    };
    contract::contract(|contract| {
        contract.document()["paths"][route][method.to_ascii_lowercase()].is_object()
    })
}

/// `value` as a query string carries it: every byte but a letter, a digit
/// and `-._~` written as `%XX`.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// A request, as a test says it: a route under `/api/v1` (or a path of
/// its own), and what it carries besides.
pub struct Ask {
    method: &'static str,
    target: String,
    body: Option<Body>,
    /// The token as `Authorization: Bearer`, as a script sends it.
    bearer: bool,
    /// The cookie the link sets, and what the panel's page sends beside it.
    page: bool,
    /// `Content-Type: application/json` with a body.
    json: bool,
    /// Sent in place of the header of the same name, `{port}` the
    /// server's port.
    headers: Vec<String>,
    /// Headers left out.
    without: Vec<String>,
}

enum Body {
    /// Its paths under `/home` read as the scenario's.
    Json(Value),
    /// As it is.
    Text(String),
}

impl Ask {
    /// `GET <route>` under `/api/v1`.
    pub fn get(route: &str) -> Self {
        Self::new("GET", format!("{API}{route}"), None)
    }

    /// `POST <route>` under `/api/v1`, with `body` as JSON.
    pub fn post(route: &str, body: Value) -> Self {
        Self::new("POST", format!("{API}{route}"), Some(Body::Json(body)))
    }

    /// `POST <route>` under `/api/v1`, with `body` as it is, said to be
    /// JSON whatever it holds.
    pub fn post_text(route: &str, body: &str) -> Self {
        Self::new(
            "POST",
            format!("{API}{route}"),
            Some(Body::Text(body.to_owned())),
        )
    }

    /// `method <path>` with a path of its own, under `/api` or not.
    pub fn at(method: &'static str, path: &str) -> Self {
        Self::new(method, path.to_owned(), None)
    }

    fn new(method: &'static str, target: String, body: Option<Body>) -> Self {
        Self {
            method,
            target,
            body,
            bearer: true,
            page: false,
            json: true,
            headers: Vec::new(),
            without: Vec::new(),
        }
    }

    /// Sent with no token at all.
    pub fn without_token(mut self) -> Self {
        self.bearer = false;
        self
    }

    /// Sent as the panel's page sends it: the cookie its link set, from
    /// its own origin, with the headers it adds, and no Bearer token.
    pub fn as_the_page(mut self) -> Self {
        self.bearer = false;
        self.page = true;
        self
    }

    /// Its body sent without saying it is JSON, as curl's `-d` alone does.
    pub fn untyped(mut self) -> Self {
        self.json = false;
        self
    }

    /// `header` sent in place of the one of its name, `{port}` the
    /// server's port: `Host: evil.example:{port}`.
    pub fn header(mut self, header: &str) -> Self {
        self.headers.push(header.to_owned());
        self
    }

    /// The header `name` left out.
    pub fn without(mut self, name: &str) -> Self {
        self.without.push(name.to_owned());
        self
    }
}

/// What the server answered one request, every path read as `/home`.
pub struct Answer {
    asked: String,
    pub status: u16,
    pub head: String,
    pub body: String,
}

impl Answer {
    /// It was answered with `status`.
    pub fn is(&self, status: u16) -> &Self {
        if self.status != status {
            panic!("expected {} to be answered {status}:\n{self}", self.asked);
        }
        self
    }

    /// Its body is JSON, and exactly `expected`.
    pub fn answers(&self, expected: Value) -> &Self {
        let got = self.json();
        if got != expected {
            panic!(
                "{} was answered something else.\n--- expected\n{}\n--- got\n{}\n---\n{self}",
                self.asked,
                pretty(&expected),
                pretty(&got)
            );
        }
        self
    }

    /// What its JSON holds at `pointer` is exactly `expected`: the part of
    /// the answer a script reads, as `jq` picks it.
    pub fn answers_at(&self, pointer: &str, expected: Value) -> &Self {
        let whole = self.json();
        let got = whole
            .pointer(pointer)
            .unwrap_or_else(|| panic!("{} answered nothing at {pointer}:\n{self}", self.asked));
        if *got != expected {
            panic!(
                "{} was answered something else at {pointer}.\n--- expected\n{}\n--- got\n{}\n---\n{self}",
                self.asked,
                pretty(&expected),
                pretty(got)
            );
        }
        self
    }

    /// It was refused with `status`, and the answer every refusal under
    /// `/api` is: what went wrong in words, and its code.
    pub fn refused(&self, status: u16, code: &str, error: &str) -> &Self {
        self.is(status)
            .answers(serde_json::json!({"code": code, "error": error}))
    }

    /// Its body, read as JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|error| panic!("{} answered no JSON ({error}):\n{self}", self.asked))
    }

    /// The value of its header `name`, if it carries one.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|line| {
            let (header, value) = line.split_once(':')?;
            header.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\n{}\n\n{}", self.asked, self.head, self.body)
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap()
}

/// The event stream, as a script reads it.
pub struct Stream<'a> {
    reader: BufReader<TcpStream>,
    /// What a read cut off, kept for the next.
    pending: String,
    scenario: &'a Scenario,
}

impl Stream<'_> {
    /// The next event told, held to what the API's document says an event
    /// holds, every path in it read as `/home`; none once a short wait
    /// passes without one.
    pub fn next(&mut self, wait: Duration) -> Option<Value> {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match self.reader.read_line(&mut self.pending) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(error) => panic!("read the stream: {error}"),
            }
            if !self.pending.ends_with('\n') {
                continue;
            }
            let line = std::mem::take(&mut self.pending);
            // The head, a chunk's size, a comment that keeps the
            // connection open, the end of a message: all but an event.
            let Some(data) = line.trim_end().strip_prefix("data:") else {
                continue;
            };
            let event: Value = serde_json::from_str(data.trim()).expect("an event is JSON");
            contract::told(&format!("{API}/events"), &event);
            return Some(
                serde_json::from_str(&self.scenario.seen(event.to_string().as_bytes())).unwrap(),
            );
        }
        None
    }

    /// The two events every stream starts with: `resync`, no switch taken
    /// yet, then that the server watches every file it reads.
    pub fn opened(&mut self) -> &mut Self {
        let first = [self.told(), self.told()];
        assert_eq!(
            first,
            [
                serde_json::json!({"type": "resync", "switches": 0}),
                serde_json::json!({"type": "watching"}),
            ],
            "the stream opened otherwise"
        );
        self
    }

    /// The next event told, waited for as long as anything is.
    pub fn told(&mut self) -> Value {
        self.next(PATIENCE)
            .unwrap_or_else(|| panic!("nothing told within {PATIENCE:?}"))
    }

    /// Every event told until one `until` picks, that one included: what
    /// the stream said of something a test set off.
    pub fn until(&mut self, until: impl Fn(&Value) -> bool) -> Vec<Value> {
        let mut told = Vec::new();
        loop {
            let event = self.told();
            let done = until(&event);
            told.push(event);
            if done {
                return told;
            }
        }
    }
}

/// How a server ended once it was told to stop, every path read as
/// `/home`.
pub struct Stopped {
    pub status: Option<i32>,
    /// What it printed after its link.
    pub stdout: String,
    /// What it said on stderr, from its first line.
    pub stderr: String,
}
