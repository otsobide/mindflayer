//! One line per request, with the launch token taken out.

use std::sync::Arc;

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

    /// `GET /api/transcript 200`: the method, what was asked for and the
    /// status. Headers never, since the cookie and the Bearer token are in
    /// them, and no body, since a body is the user's business.
    fn line(&self, method: &str, uri: &Uri, status: u16) -> String {
        let mut target = uri.path().to_owned();
        if let Some(query) = uri.query() {
            target.push('?');
            target.push_str(&redact(query));
        }
        scrub(&format!("{method} {target} {status}"), self.token.as_str())
    }
}

/// Write one line for a request once it has been answered.
pub(crate) async fn record(State(log): State<Arc<Log>>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let response = next.run(request).await;
    (log.sink)(&log.line(method.as_str(), &uri, response.status().as_u16()));
    response
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
