//! What keeps other pages out.
//!
//! The token keeps out other processes. These checks keep out other pages in
//! the browser that holds it: one whose DNS name has been rebound to
//! 127.0.0.1, and one on another port of 127.0.0.1, which the browser counts
//! as the same site and sends the cookie from.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{AsHeaderName, CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{HeaderMap, Method, StatusCode};
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
    /// HTTP/1.1 requires. Over HTTP/2 the name would come as `:authority`
    /// instead, which this would refuse for want of a Host; the server speaks
    /// only HTTP/1.1, and a server that grew HTTP/2 would fail closed.
    pub(crate) fn named(&self, headers: &HeaderMap) -> Option<&str> {
        let host = single(headers, HOST)?;
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

/// Whether a request's Origin, if it sends one, is the origin it was sent
/// to: `http://` and the name it reached the server by.
///
/// The browser writes Origin, and no page can change it, so when it is there
/// it says which page asked. It is not always there: a browser leaves it out
/// of a GET that is not cross-origin, and a script out of everything, so its
/// absence proves nothing. `null`, sent by a sandboxed frame or a file, is no
/// origin of the server's.
///
/// One origin, not any of the three: a page loaded as `localhost` that asks
/// `127.0.0.1` is another origin to the browser, which keeps the two names'
/// cookies apart, and is treated as one here too.
pub(crate) fn from_itself(name: &str, headers: &HeaderMap) -> bool {
    let mut origins = headers.get_all(ORIGIN).iter();
    match (origins.next(), origins.next()) {
        (None, _) => true,
        (Some(origin), None) => origin.to_str().is_ok_and(|origin| {
            // Neither a scheme nor a DNS name depends on its case.
            let (scheme, host) = origin.split_at_checked(7).unwrap_or_default();
            scheme.eq_ignore_ascii_case("http://") && host.eq_ignore_ascii_case(name)
        }),
        (Some(_), Some(_)) => false,
    }
}

/// Turn away a request that does not name this server, or that a page of
/// another origin sent.
///
/// Every request, not only those under `/api`: the launch link is answered
/// with the cookie, and a rebound page has no business with the panel either.
pub(crate) async fn same_origin(
    State(hosts): State<Arc<Hosts>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let Some(name) = hosts.named(request.headers()) else {
        return forbidden(
            path,
            &format!("this server answers only as {}", hosts.listed()),
        );
    };
    if !from_itself(name, request.headers()) {
        return forbidden(
            path,
            &format!("this server takes requests only from its own pages, at http://{name}"),
        );
    }
    next.run(request).await
}

/// Hold a request that carries the cookie to what only the panel's own page
/// sends, if it would change something.
///
/// The cookie is `SameSite=Strict`, but every port of 127.0.0.1 is one site,
/// so a page served by anything else on loopback (a dev server, a notebook,
/// a tool's sign-in callback) has its requests here sent with the cookie. It
/// cannot read the answers, since nothing here grants it CORS, but a change
/// does its harm whether or not anybody reads the reply.
///
/// A change is any method but GET and HEAD. By method rather than by
/// endpoint, so an endpoint added later is covered without being listed; a
/// query is a POST, since it has a body, and is held to the same, which costs
/// the panel nothing, as it sends every POST alike.
///
/// A refusal is its status and what to say, for the caller to answer in the
/// API's JSON.
pub(crate) fn from_the_panel(
    method: &Method,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, &'static str)> {
    if matches!(*method, Method::GET | Method::HEAD) {
        return Ok(());
    }
    // Fetch Metadata: a browser says on every request to loopback which page
    // sent it, relative to where it is going, and no page can say otherwise.
    // Without the header the sender is no browser of today's, so it is a
    // script, and a script that should be here holds the token to send.
    if single(headers, "sec-fetch-site") != Some("same-origin") {
        return Err((
            StatusCode::FORBIDDEN,
            "a change sent with the cookie must come from the panel's own page \
             (Sec-Fetch-Site: same-origin); a script sends the token as Bearer instead",
        ));
    }
    // A header of the panel's own and a JSON body. A page may send another
    // origin text/plain, a form or multipart without asking first; anything
    // else, a header of its own included, makes the browser ask with a
    // preflight, which no CORS here ever answers, so the request never
    // leaves. Fetch Metadata already says where the request came from, and
    // these say it again by other means, in case a browser gets one wrong.
    if single(headers, "x-mindflayer") != Some("1") {
        return Err((
            StatusCode::FORBIDDEN,
            "a change sent with the cookie must carry X-Mindflayer: 1, as the panel's do",
        ));
    }
    if !single(headers, CONTENT_TYPE).is_some_and(is_json) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "a change sent with the cookie must be JSON, sent as application/json",
        ));
    }
    Ok(())
}

/// Whether a Content-Type is `application/json`, whatever parameters follow
/// it, as `; charset=utf-8` may.
fn is_json(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or_default();
    essence.trim().eq_ignore_ascii_case("application/json")
}

/// A header's value when a request sends it exactly once. Twice is as good
/// as never: which of the two a check believed would decide whether it
/// passed.
fn single<K: AsHeaderName>(headers: &HeaderMap, name: K) -> Option<&str> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => value.to_str().ok(),
        _ => None,
    }
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
