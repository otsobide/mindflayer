//! What keeps other pages out.
//!
//! The token keeps out other processes. These checks keep out other pages in
//! the browser that holds it: one whose DNS name has been rebound to
//! 127.0.0.1, and one on another port of 127.0.0.1, which the browser counts
//! as the same site and sends the cookie from.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{CONTENT_TYPE, HOST};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api;

/// The names a browser may reach the server by, each with the server's port.
///
/// Loopback's three spellings and nothing else. A page whose name an attacker
/// rebinds to 127.0.0.1 reaches this port with its own name in the Host
/// header, because the browser still believes it is talking to the attacker's
/// server; refusing every other name is what stops it. A list of names the
/// server is never reached by rather than one it is would have to guess every
/// name an attacker could buy.
pub(crate) struct Hosts {
    names: [String; 3],
}

impl Hosts {
    pub(crate) fn new(port: u16) -> Self {
        Self {
            names: [
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ],
        }
    }

    /// The name a request reached the server by, if it is one of the
    /// server's.
    ///
    /// The raw Host header, never `X-Forwarded-Host`: nothing stands between
    /// the browser and this server to forward anything, so that header is
    /// only ever what the sender chose to write. And exactly one Host, as
    /// HTTP/1.1 requires; with two, which one a check believed would decide
    /// whether it passed. Over HTTP/2 the name would come as `:authority`
    /// instead, which this would refuse for want of a Host; the server speaks
    /// only HTTP/1.1, and a server that grew HTTP/2 would fail closed.
    pub(crate) fn named(&self, headers: &HeaderMap) -> Option<&str> {
        let mut hosts = headers.get_all(HOST).iter();
        let (Some(host), None) = (hosts.next(), hosts.next()) else {
            return None;
        };
        let host = host.to_str().ok()?;
        // A DNS name is the same name in any case.
        self.names
            .iter()
            .find(|name| name.eq_ignore_ascii_case(host))
            .map(String::as_str)
    }

    /// The names, for a refusal to list.
    fn listed(&self) -> String {
        let [first, second, third] = &self.names;
        format!("{first}, {second} or {third}")
    }
}

/// Turn away a request that does not name this server.
///
/// Every request, not only those under `/api`: the launch link is answered
/// with the cookie, and a rebound page has no business with the panel either.
pub(crate) async fn same_origin(
    State(hosts): State<Arc<Hosts>>,
    request: Request,
    next: Next,
) -> Response {
    if hosts.named(request.headers()).is_none() {
        return forbidden(
            request.uri().path(),
            &format!("this server answers only as {}", hosts.listed()),
        );
    }
    next.run(request).await
}

/// A 403, in JSON under `/api` like every other refusal there, and as text
/// anywhere else, where it is a person at a browser who reads it.
fn forbidden(path: &str, message: &str) -> Response {
    if api::is_api(path) {
        return api::error(StatusCode::FORBIDDEN, message);
    }
    (
        StatusCode::FORBIDDEN,
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{message}\n"),
    )
        .into_response()
}

#[cfg(test)]
#[path = "guard_test.rs"]
mod tests;
