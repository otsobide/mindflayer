//! What every server suite shares: a fake backend with types of its own, and
//! a way to send it requests without a socket.
//!
//! Everything is re-exported so a suite says `use common::*;` and nothing
//! else; not every suite uses every helper.

#![allow(dead_code, unused_imports)]

pub use axum::body::{Body, Bytes};
pub use axum::http::{header, HeaderMap, Method, Request, StatusCode};
pub use axum::Router;
pub use mindflayer_server::{Backend, Listening, Server};
pub use serde_json::{json, Value};
pub use std::sync::{Arc, Condvar, Mutex};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// The fake backend
// ---------------------------------------------------------------------------

/// The fake's read: say back what was sent, so a test can tell it arrived.
#[derive(Deserialize)]
pub struct FakeQuery {
    pub echo: String,
}

/// The fake's task: a name to run under, and whether to wait at the fake's
/// hold before finishing.
#[derive(Deserialize)]
pub struct FakeTask {
    pub name: String,
    #[serde(default)]
    pub hold: bool,
}

/// What running a fake task says.
#[derive(Serialize)]
pub struct FakeOutcome {
    pub ran: String,
    /// The thread it ran on.
    pub thread: Option<String>,
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
}

impl Backend for Fake {
    type Query = FakeQuery;
    type Task = FakeTask;
    type Outcome = FakeOutcome;

    fn query(&self, query: FakeQuery) -> Value {
        json!({ "echo": query.echo })
    }

    fn preview(&self, task: &FakeTask) -> Vec<String> {
        vec![format!("fake run {}", task.name)]
    }

    fn run(&self, task: FakeTask) -> FakeOutcome {
        self.runs
            .lock()
            .unwrap()
            .push(format!("start {}", task.name));
        if task.hold {
            self.hold.wait();
        }
        self.runs.lock().unwrap().push(format!("end {}", task.name));
        FakeOutcome {
            ran: task.name,
            thread: std::thread::current().name().map(str::to_owned),
        }
    }

    fn watched_paths(&self) -> Vec<PathBuf> {
        Vec::new()
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

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// What a request carries to prove it may be answered.
#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Nothing,
    Bearer(&'a str),
    /// A whole `name=value` pair, as a browser sends it.
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

/// Send one request through the router, in process.
pub async fn send(
    router: &Router,
    method: Method,
    path: &str,
    auth: Auth<'_>,
    body: Option<Value>,
) -> Reply {
    let mut request = Request::builder().method(method).uri(path);
    match auth {
        Auth::Nothing => {}
        Auth::Bearer(token) => {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"))
        }
        Auth::Cookie(pair) => request = request.header(header::COOKIE, pair),
    }
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
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

/// `GET path`.
pub async fn get(router: &Router, path: &str, auth: Auth<'_>) -> Reply {
    send(router, Method::GET, path, auth, None).await
}

/// `POST path` with a JSON body.
pub async fn post(router: &Router, path: &str, auth: Auth<'_>, body: Value) -> Reply {
    send(router, Method::POST, path, auth, Some(body)).await
}
