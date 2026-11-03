//! A panel the real binary serves, and the client the suites that call it
//! over loopback share: `ui_test`, which sends the panel's requests as a
//! script sends them, and `api_guide_test`, which replays the API's guide.
//!
//! A client of plain `TcpStream`s rather than an HTTP crate, which would
//! be one more dependency to say the few lines a request is: a request is
//! written whole with `Connection: close` and its answer read to the end,
//! and the event stream is read a line at a time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::ChildStdout;
use std::time::Duration;

use serde_json::Value;

use super::contract;
use super::{start, Started};

/// A panel being served, with what its link said.
pub struct Panel {
    pub started: Started,
    pub address: SocketAddr,
    pub token: String,
    /// What it prints after the link.
    pub stdout: BufReader<ChildStdout>,
}

/// Every agent home empty, so nothing the panel runs reads a real one.
pub const NO_AGENT_HOMES: [&str; 4] = [
    "--claude-config-dir=",
    "--codex-home=",
    "--opencode-config-dir=",
    "--agents-dir=",
];

/// `binary ui --no-open --port 0` started from `dir`, with `home` as the
/// default workspace's home, nothing named and every agent home empty, and
/// the link it printed read back into an address and a token.
pub fn serve(binary: &str, dir: &Path, home: &Path) -> Panel {
    serve_with(binary, dir, home, &[])
}

/// [`serve`], with `options` besides.
pub fn serve_with(binary: &str, dir: &Path, home: &Path, options: &[&str]) -> Panel {
    let (started, stdout, link) = launch(binary, dir, home, options);
    let (address, token) = link
        .strip_prefix("http://")
        .and_then(|link| link.split_once("/?token="))
        .unwrap_or_else(|| panic!("not a link: {link:?}"));
    let address: SocketAddr = address.parse().unwrap();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(address.port(), 0);
    Panel {
        address,
        token: token.to_owned(),
        started,
        stdout,
    }
}

/// `binary ui --no-open --port 0`, and `options`, started as [`serve`]
/// starts it, and the one line it printed, the link.
pub fn launch(
    binary: &str,
    dir: &Path,
    home: &Path,
    options: &[&str],
) -> (Started, BufReader<ChildStdout>, String) {
    let home = format!("--home={}", home.display());
    let mut args = vec!["ui", "--no-open", "--port", "0", &home, "--workspace="];
    args.extend(NO_AGENT_HOMES);
    args.extend(options);
    let mut started = start(binary, dir, &args, &[]);
    let mut stdout = BufReader::new(started.child.stdout.take().unwrap());
    let mut link = String::new();
    stdout.read_line(&mut link).unwrap();
    let Some(link) = link.strip_suffix('\n').map(str::to_owned) else {
        let mut said = String::new();
        let _ = started
            .child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut said);
        panic!("no link printed: {link:?}; stderr: {said}");
    };
    (started, stdout, link)
}

/// What a request carries to be let in.
#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Nothing,
    /// The token, as a script sends it.
    Bearer,
    /// A token as a script sends it, read from wherever it was handed.
    BearerOf(&'a str),
    /// The cookie the link sets, with what the panel's page sends beside it,
    /// from a page at this origin.
    Cookie(&'a str),
}

impl Panel {
    /// One request over loopback: Host naming the server, what `auth` says,
    /// and a JSON body; and the status and the JSON that came back, held to
    /// what the API's document says that route answers with that status
    /// (`contract::answered`), so a route answering outside its document
    /// fails whichever test met it.
    pub fn send(
        &self,
        method: &str,
        path: &str,
        auth: Auth<'_>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let (status, _, body) = self.exchange(method, path, auth, body);
        let body = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"));
        contract::answered(method, path, status, &body);
        (status, body)
    }

    /// [`send`](Self::send), and the status, the head and the body as
    /// they came back, read as text.
    pub fn exchange(
        &self,
        method: &str,
        path: &str,
        auth: Auth<'_>,
        body: Option<Value>,
    ) -> (u16, String, String) {
        let body = body.map(|body| body.to_string()).unwrap_or_default();
        let mut headers = vec![String::from("Content-Type: application/json")];
        match auth {
            Auth::Nothing => {}
            Auth::Bearer => headers.push(format!("Authorization: Bearer {}", self.token)),
            Auth::BearerOf(token) => headers.push(format!("Authorization: Bearer {token}")),
            Auth::Cookie(origin) => headers.extend([
                format!("Cookie: mf_{}={}", self.address.port(), self.token),
                format!("Origin: {origin}"),
                String::from("Sec-Fetch-Site: same-origin"),
                String::from("X-Mindflayer: 1"),
            ]),
        }
        self.request(method, path, &headers, &body)
    }

    /// One request over loopback with `headers` and `body` as they are,
    /// beside Host naming the server, `Connection: close` and the body's
    /// length; and the status, the head and the body that came back, as
    /// text. For a client that says its own headers, as curl does.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[String],
        body: &str,
    ) -> (u16, String, String) {
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\
             Content-Length: {}\r\n",
            self.address,
            body.len()
        );
        for header in headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        request.push_str(body);

        let mut stream = TcpStream::connect(self.address).unwrap();
        // Long enough for a run on a slow runner; a server that never
        // answers fails the test rather than hanging it.
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, head.to_owned(), body.to_owned())
    }

    pub fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.send("POST", path, Auth::Bearer, Some(body))
    }

    pub fn get(&self, path: &str) -> (u16, Value) {
        self.send("GET", path, Auth::Bearer, None)
    }

    pub fn snapshot(&self) -> Value {
        let (status, snapshot) = self.get("/api/v1/snapshot");
        assert_eq!(status, 200, "{snapshot}");
        snapshot
    }
}

/// `value` as a query string carries it: every byte but a letter, a digit
/// and `-._~` written as `%XX`, as a browser's form encodes it.
pub fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// How long a test waits for an event it expects: long past the watcher's
/// debounce, for a machine under load, and still bounded.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// Every event the stream tells over loopback, each as its `data:` line
/// holds it, until one `until` picks is told or [`PATIENCE`] has passed;
/// `run` once the first is told, so what it does is told after it.
pub fn events(panel: &Panel, run: impl FnOnce(), until: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut stream = TcpStream::connect(panel.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let request = format!(
        "GET /api/v1/events HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\n",
        panel.address, panel.token
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let mut told: Vec<Value> = Vec::new();
    let mut run = Some(run);
    let mut line = String::new();
    let deadline = std::time::Instant::now() + PATIENCE;
    while std::time::Instant::now() < deadline && !told.iter().any(&until) {
        // What a timeout cut off stays in `line`, for the read after it.
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(error) => panic!("{error}"),
        }
        if !line.ends_with('\n') {
            continue;
        }
        let whole = std::mem::take(&mut line);
        // A chunk's size, a comment that keeps the connection open, or the
        // end of a message: everything but the event itself.
        let Some(data) = whole.trim_end().strip_prefix("data:") else {
            continue;
        };
        told.push(serde_json::from_str(data.trim()).unwrap());
        if let Some(run) = run.take() {
            run();
        }
    }
    told
}

/// Ctrl-C, as a terminal sends it: SIGINT, by `kill`. And how the panel
/// exited, what it printed after the link, and all it said on stderr.
#[cfg(unix)]
pub fn interrupt(panel: &mut Panel) -> (std::process::ExitStatus, String, String) {
    signal(panel, "-INT")
}

/// [`interrupt`], with the signal `kill` is given (`-INT`, `-TERM`).
#[cfg(unix)]
pub fn signal(panel: &mut Panel, name: &str) -> (std::process::ExitStatus, String, String) {
    let pid = panel.started.child.id().to_string();
    let sent = std::process::Command::new("kill")
        .args([name, &pid])
        .status()
        .expect("run kill");
    assert!(sent.success());
    let exited = exit(&mut panel.started, Duration::from_secs(60));
    let mut rest = String::new();
    panel.stdout.read_to_string(&mut rest).unwrap();
    let mut said = String::new();
    let mut stderr = panel.started.child.stderr.take().unwrap();
    stderr.read_to_string(&mut said).unwrap();
    (exited, rest, said)
}

/// How a started binary exited, waited for until `limit` has passed.
pub fn exit(started: &mut Started, limit: Duration) -> std::process::ExitStatus {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = started.child.try_wait().unwrap() {
            return status;
        }
        assert!(start.elapsed() < limit, "still running after {limit:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
