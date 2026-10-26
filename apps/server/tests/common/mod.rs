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
pub use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// The fake backend
// ---------------------------------------------------------------------------

/// The fake's read: say back what was sent, so a test can tell it arrived.
#[derive(Deserialize)]
pub struct FakeQuery {
    pub echo: String,
}

/// The fake's task: a name to run under.
#[derive(Deserialize)]
pub struct FakeTask {
    pub name: String,
}

/// What running a fake task says.
#[derive(Serialize)]
pub struct FakeOutcome {
    pub ran: String,
}

/// A backend that knows nothing of mindflayer, which is the point: the server
/// must carry types it has never heard of.
#[derive(Default)]
pub struct Fake {
    /// The tasks it has run, in the order it ran them.
    pub runs: Arc<Mutex<Vec<String>>>,
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
        self.runs.lock().unwrap().push(task.name.clone());
        FakeOutcome { ran: task.name }
    }

    fn watched_paths(&self) -> Vec<PathBuf> {
        Vec::new()
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
