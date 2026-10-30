//! What every server suite shares: a fake backend with types of its own, and
//! a way to send it requests without a socket.
//!
//! Everything is re-exported so a suite says `use common::*;` and nothing
//! else; not every suite uses every helper.

#![allow(dead_code, unused_imports)]

pub use axum::body::{Body, Bytes};
pub use axum::http::{header, HeaderMap, Method, Request, StatusCode};
pub use mindflayer_server::{Backend, Listening, Preview, Refusal, Server, Watched};
pub use serde_json::{json, Value};
pub use std::sync::{Arc, Condvar, Mutex};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize, Serializer};
use std::time::Duration;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// The fake backend
// ---------------------------------------------------------------------------

/// The fake's read: say back what was sent, so a test can tell it arrived,
/// or refuse to.
#[derive(Deserialize)]
pub struct FakeQuery {
    pub echo: String,
    #[serde(default)]
    pub refuse: bool,
}

/// The fake's task: a name to run under, whether to wait at the fake's hold
/// before finishing, whether to finish with an outcome JSON cannot carry,
/// whether to refuse it, when its lines are asked for (`refuse`) or only
/// once it is handed over to run (`refuse_run`), whether its run panics, as
/// a backend with a bug would, and what its preview says to confirm first,
/// in words of the fake's own (`confirm`).
///
/// A run takes no notice of `refuse`: one whose lines were refused and is
/// handed over all the same runs, and the runs it leaves say so.
#[derive(Deserialize)]
pub struct FakeTask {
    pub name: String,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub unwritable: bool,
    #[serde(default)]
    pub refuse: bool,
    #[serde(default)]
    pub refuse_run: bool,
    #[serde(default)]
    pub panic: bool,
    #[serde(default)]
    pub confirm: Vec<String>,
}

/// What the fake refuses with: a code of its own, which a real backend's
/// would be one of its front end's.
pub fn refusal(what: &str) -> Refusal {
    Refusal {
        code: "fake_refused",
        message: format!("{what} is refused"),
    }
}

/// What running a fake task says.
#[derive(Serialize)]
pub struct FakeOutcome {
    pub ran: String,
    /// The thread it ran on.
    pub thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unwritable: Option<Unwritable>,
}

/// A part of an outcome that refuses to serialize, as a path that is not
/// UTF-8 or a map keyed by something other than strings does.
pub struct Unwritable;

impl Serialize for Unwritable {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("this part cannot be written"))
    }
}

/// A backend that knows nothing of mindflayer, which is the point: the server
/// must carry types it has never heard of.
#[derive(Default)]
pub struct Fake {
    /// `start <name>` and `end <name>` for every task it ran, in order, so
    /// two runs at once would show as one starting before the other ended.
    pub runs: Arc<Mutex<Vec<String>>>,
    /// Where a task that asks to be held waits.
    pub hold: Arc<Hold>,
    /// What it asks the server to watch, which a test changes as it goes.
    pub watched: Arc<Mutex<Vec<Watched>>>,
}

impl Backend for Fake {
    type Query = FakeQuery;
    type Task = FakeTask;
    type Outcome = FakeOutcome;

    fn query(&self, query: FakeQuery) -> Result<Value, Refusal> {
        if query.refuse {
            return Err(refusal(&query.echo));
        }
        Ok(json!({ "echo": query.echo }))
    }

    fn preview(&self, task: &FakeTask) -> Result<Preview, Refusal> {
        if task.refuse {
            return Err(refusal(&task.name));
        }
        Ok(Preview {
            lines: vec![format!("fake run {}", task.name)],
            confirm: task
                .confirm
                .iter()
                .map(|what| json!({ "fake_takes": what }))
                .collect(),
        })
    }

    fn run(&self, task: FakeTask) -> Result<FakeOutcome, Refusal> {
        if task.refuse_run {
            return Err(refusal(&task.name));
        }
        if task.panic {
            panic!("{} panics, as asked", task.name);
        }
        self.runs
            .lock()
            .unwrap()
            .push(format!("start {}", task.name));
        if task.hold {
            self.hold.wait();
        }
        self.runs.lock().unwrap().push(format!("end {}", task.name));
        Ok(FakeOutcome {
            ran: task.name,
            thread: std::thread::current().name().map(str::to_owned),
            unwritable: task.unwritable.then_some(Unwritable),
        })
    }

    fn watched_paths(&self) -> Vec<Watched> {
        self.watched.lock().unwrap().clone()
    }
}

/// A point a run waits at until the test lets it go, so the test can do
/// things while a write is known to be under way.
#[derive(Default)]
pub struct Hold {
    /// Whether a run has reached the hold, and whether it has been let go.
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl Hold {
    /// A run arrives, says so, and waits to be let go.
    fn wait(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            state = self.changed.wait(state).unwrap();
        }
    }

    /// Block until a run is waiting here, or fail the test after long enough
    /// that none is coming.
    pub fn reached(&self) {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(10), |state| !state.0)
            .unwrap();
        assert!(state.0, "no run reached the hold");
    }

    /// Let the waiting run go on.
    pub fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

/// Releases the hold when dropped, so an assertion that fails while a run is
/// held cannot leave it waiting, and the test hanging on it, for ever.
pub struct Release(pub Arc<Hold>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// A server over a fake, bound and ready, and the fake's record of its runs.
pub fn launch() -> (Listening, Arc<Mutex<Vec<String>>>) {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    (Server::new(fake).bind().unwrap(), runs)
}

/// A server over a fake that logs, and the lines it has logged so far.
pub fn launch_logged() -> (Listening, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let server = Server::new(Fake::default())
        .log(move |line| sink.lock().unwrap().push(line.to_owned()))
        .bind()
        .unwrap();
    (server, lines)
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// What a request carries to prove it may be answered.
#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Nothing,
    Bearer(&'a str),
    /// A whole `name=value` pair, sent as the panel's page sends it: with the
    /// headers the browser adds to every request the page makes, and the one
    /// the panel adds itself.
    Cookie(&'a str),
}

/// What came back.
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("not JSON ({error}): {:?}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }
}

/// The request [`send`] sends, for a test that changes it before it goes.
///
/// It names the server as the launch link does, `Host: 127.0.0.1:<port>`:
/// over the socket every request names one, a browser's and a script's alike.
pub fn request(
    server: &Listening,
    method: Method,
    path: &str,
    auth: Auth<'_>,
    body: Option<Value>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, server.address().to_string());
    match auth {
        Auth::Nothing => {}
        Auth::Bearer(token) => {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"))
        }
        Auth::Cookie(pair) => {
            request = request
                .header(header::COOKIE, pair)
                .header(header::ORIGIN, format!("http://{}", server.address()))
                .header("sec-fetch-site", "same-origin")
                .header("x-mindflayer", "1")
        }
    }
    match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap()
}

/// Send a request through the server's router, in process.
pub async fn deliver(server: &Listening, request: Request<Body>) -> Reply {
    let response = server.router().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body,
    }
}

/// Send one request to the server, in process.
pub async fn send(
    server: &Listening,
    method: Method,
    path: &str,
    auth: Auth<'_>,
    body: Option<Value>,
) -> Reply {
    deliver(server, request(server, method, path, auth, body)).await
}

/// `GET path`.
pub async fn get(server: &Listening, path: &str, auth: Auth<'_>) -> Reply {
    send(server, Method::GET, path, auth, None).await
}

/// `POST path` with a JSON body.
pub async fn post(server: &Listening, path: &str, auth: Auth<'_>, body: Value) -> Reply {
    send(server, Method::POST, path, auth, Some(body)).await
}

// ---------------------------------------------------------------------------
// The event stream
// ---------------------------------------------------------------------------

/// How long a test waits for an event it expects before it fails: long
/// past the watcher's debounce, for a machine under load, and still bounded.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// `GET /api/events` answered, read an event at a time as a browser's
/// `EventSource` reads it.
pub struct Stream {
    pub status: StatusCode,
    pub headers: HeaderMap,
    body: axum::body::BodyDataStream,
    buffer: String,
}

impl Stream {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }

    /// The next event's JSON, or a failed test once [`PATIENCE`] has
    /// passed without one.
    pub async fn next(&mut self) -> Value {
        self.within(PATIENCE)
            .await
            .unwrap_or_else(|| panic!("no event in {PATIENCE:?}"))
    }

    /// The next event's JSON, or none when `wait` passes without one or
    /// the stream ends: comments, which keep the connection alive, are
    /// passed over.
    pub async fn within(&mut self, wait: Duration) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let frame: String = self.buffer.drain(..end + 2).collect();
                let data: Vec<&str> = frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(|data| data.strip_prefix(' ').unwrap_or(data))
                    .collect();
                if data.is_empty() {
                    continue;
                }
                return Some(serde_json::from_str(&data.join("\n")).unwrap());
            }
            let chunk = tokio::time::timeout_at(deadline, self.body.next())
                .await
                .ok()??;
            self.buffer
                .push_str(std::str::from_utf8(&chunk.unwrap()).unwrap());
        }
    }

    /// Every event up to the first of `type`, which is the last returned,
    /// or a failed test once [`PATIENCE`] has passed without one.
    pub async fn until(&mut self, kind: &str) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let event = self.next().await;
            let found = event["type"] == kind;
            seen.push(event);
            if found {
                return seen;
            }
        }
    }
}

/// Open the event stream, in process.
pub async fn subscribe(server: &Listening, auth: Auth<'_>) -> Stream {
    let response = server
        .router()
        .oneshot(request(server, Method::GET, "/api/events", auth, None))
        .await
        .unwrap();
    Stream {
        status: response.status(),
        headers: response.headers().clone(),
        body: response.into_body().into_data_stream(),
        buffer: String::new(),
    }
}

/// The event stream opened, past what it says first: to ask for
/// everything, and that everything named is watched.
pub async fn opened(server: &Listening, auth: Auth<'_>) -> Stream {
    let mut stream = subscribe(server, auth).await;
    assert_eq!(stream.next().await, json!({"type": "resync"}));
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    stream
}

/// A server over a fake that names `watched` to watch, bound and ready, and
/// the list, which a test changes for the next time the server asks.
pub fn launch_watching(watched: Vec<Watched>) -> (Listening, Arc<Mutex<Vec<Watched>>>) {
    let fake = Fake::default();
    *fake.watched.lock().unwrap() = watched;
    let list = Arc::clone(&fake.watched);
    (Server::new(fake).bind().unwrap(), list)
}
