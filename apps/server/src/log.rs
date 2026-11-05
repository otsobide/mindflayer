//! One line per request, and per job the writer thread takes, with the
//! launch token taken out.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::Uri;
use axum::middleware::Next;
use axum::response::Response;

use crate::auth;
use crate::token::Token;

/// What stands in for the token wherever it would have been.
const REDACTED: &str = "<redacted>";

/// Where the lines go: whatever the front end hands the server. A function
/// rather than a logging framework, because the server prints nothing of its
/// own accord, as a CLI command builds its output and leaves printing it to
/// the caller; the front end decides whether a line reaches the terminal.
pub(crate) type Sink = Arc<dyn Fn(&str) + Send + Sync>;

pub(crate) struct Log {
    sink: Sink,
    token: Token,
}

impl Log {
    pub(crate) fn new(sink: Sink, token: Token) -> Self {
        Self { sink, token }
    }

    /// `GET /api/v1/transcript 200 3ms`: the method, what was asked for,
    /// the status and how long it took to answer. Headers never, since the
    /// cookie and the Bearer token are in them, and no body, since a body is
    /// the user's business.
    ///
    /// How long, because what a log is read for is most often a request
    /// that took too long, and a client's own clock cannot tell the
    /// server's part of that from its own (#240).
    fn line(&self, method: &str, uri: &Uri, status: u16, took: Duration) -> String {
        let mut target = uri.path().to_owned();
        if let Some(query) = uri.query() {
            target.push('?');
            target.push_str(&redact(query));
        }
        scrub(
            &format!("{method} {target} {status} {}", millis(took)),
            self.token.as_str(),
        )
    }
}

/// How the writer thread's work on a task went before any of it ran, for
/// the line that says so ([`Log::planned`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Planned {
    /// Its lines were worked out, this many, and it runs.
    Lines(usize),
    /// The backend refused its lines, so none of it runs.
    Refused,
    /// The backend panicked working them out.
    Panicked,
}

impl Log {
    /// Say `line`, with the token taken out wherever it turns up, as every
    /// line the server logs is, whoever built it.
    fn say(&self, line: &str) {
        (self.sink)(&scrub(line, self.token.as_str()));
    }

    /// A task taken off the writer thread's queue, `queued` after it was
    /// sent, and its lines worked out in `working`, as `planned` says:
    /// `run: 0ms waiting its turn, 1ms working out its 1 line`. Said before
    /// any line runs, so a run that never ends is still said to have
    /// started, and when.
    pub(crate) fn planned(&self, queued: Duration, working: Duration, planned: Planned) {
        let waited = format!(
            "run: {} waiting its turn, {}",
            millis(queued),
            millis(working)
        );
        self.say(&match planned {
            Planned::Lines(1) => format!("{waited} working out its 1 line"),
            Planned::Lines(lines) => format!("{waited} working out its {lines} lines"),
            Planned::Refused => format!("{waited} working out its lines, which were refused"),
            Planned::Panicked => {
                format!("{waited} working out its lines, which panicked")
            }
        });
    }

    /// A task's `lines` run, in `took`, however that went: `run: 5402ms
    /// running its 1 line`.
    pub(crate) fn ran(&self, took: Duration, lines: usize) {
        let lines = match lines {
            1 => String::from("1 line"),
            lines => format!("{lines} lines"),
        };
        self.say(&format!("run: {} running its {lines}", millis(took)));
    }

    /// A switch taken off the writer thread's queue, `queued` after it was
    /// sent, and made, or refused, in `took`: `switch: 0ms waiting its
    /// turn, 3ms switching`.
    pub(crate) fn switched(&self, queued: Duration, took: Duration) {
        self.say(&format!(
            "switch: {} waiting its turn, {} switching",
            millis(queued),
            millis(took)
        ));
    }
}

/// Write one line for a request once it has been answered, with how long
/// that took: from the request reaching the server to its answer's head,
/// so an event stream's is how long it took to open, not how long it
/// stayed open.
pub(crate) async fn record(State(log): State<Arc<Log>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let response = next.run(request).await;
    (log.sink)(&log.line(
        method.as_str(),
        &uri,
        response.status().as_u16(),
        started.elapsed(),
    ));
    response
}

/// `took` in whole milliseconds, as every line says a time: `3ms`, `0ms`
/// for less than one. Coarser would hide the difference between a read
/// and a write, and finer is no help in reading why one took seconds.
fn millis(took: Duration) -> String {
    format!("{}ms", took.as_millis())
}

/// A query string with the value of every `token` parameter replaced,
/// whichever way its name was encoded: decoded as the exchange decodes it, so
/// whatever the server takes for the token is what the log leaves out.
pub(crate) fn redact(query: &str) -> String {
    query
        .split('&')
        .map(|pair| {
            let name = pair.split_once('=').map_or(pair, |(name, _)| name);
            let decoded = form_urlencoded::parse(name.as_bytes()).next();
            if decoded.is_some_and(|(decoded, _)| decoded == auth::PARAMETER) {
                format!("{name}={REDACTED}")
            } else {
                pair.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// `line` with the token taken out wherever else it turns up: pasted into a
/// path, under another parameter's name, upper-cased. Not where it belongs,
/// so the server would not take it there, but still the secret.
pub(crate) fn scrub(line: &str, token: &str) -> String {
    // ASCII lower-casing keeps every byte where it was, so an offset found in
    // the copy is the same offset in the line.
    let lower = line.to_ascii_lowercase();
    let mut scrubbed = String::with_capacity(line.len());
    let mut rest = 0;
    for (at, _) in lower.match_indices(token) {
        scrubbed.push_str(&line[rest..at]);
        scrubbed.push_str(REDACTED);
        rest = at + token.len();
    }
    scrubbed.push_str(&line[rest..]);
    scrubbed
}

#[cfg(test)]
#[path = "log_test.rs"]
mod tests;
