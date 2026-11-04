//! What keeps other pages out.
//!
//! The token keeps out a process that connects without it. These checks keep
//! out other pages in the browser that holds it: one whose DNS name has been
//! rebound to 127.0.0.1, and one on another port of 127.0.0.1, which the
//! browser counts as the same site and sends the cookie from. Not the server
//! behind that page, which the browser sends the cookie to as well, and which
//! can then bring the token as Bearer; docs/architecture.md leaves that open.

use std::sync::{Arc, LazyLock};

use axum::extract::{Request, State};
use axum::http::header::{
    AsHeaderName, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api;
use crate::code::Condition;

/// The names the server answers to, each with the server's port.
///
/// Loopback's three spellings and nothing else. A page whose name an attacker
/// rebinds to 127.0.0.1 reaches this port with its own name in the Host
/// header, because the browser still believes it is talking to the attacker's
/// server; refusing every other name is what stops it. A list of names the
/// server is never reached by rather than one it is would have to guess every
/// name an attacker could buy.
///
/// No browser sends `[::1]` yet: the listener binds only 127.0.0.1, so a
/// connection to ::1 is refused before any Host is sent. It is accepted
/// because that costs nothing, and a server that also bound ::1 would then
/// need no change here.
pub(crate) struct Hosts {
    names: [String; 3],
    /// The origin of the panel's dev server, when the front end named one:
    /// the one page outside the server whose changes it takes.
    dev_server: Option<String>,
}

impl Hosts {
    pub(crate) fn new(port: u16) -> Self {
        Self {
            names: [
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ],
            dev_server: None,
        }
    }

    /// Take requests from the pages of the panel's dev server at `origin`
    /// too, as [`Server::dev_server`](crate::Server::dev_server) asks.
    pub(crate) fn dev_server(mut self, origin: Option<String>) -> Self {
        self.dev_server = origin;
        self
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

    /// Whether a request that reached the server as `name` comes from one of
    /// its pages, if it says where it comes from: its own ([`from_itself`]),
    /// or the dev server's, when it has one.
    ///
    /// That origin exactly, in any case as a scheme and a name are, and
    /// never a pattern: one more page is what working on the panel needs,
    /// and every port of loopback that a pattern would let in is a page the
    /// checks here are for.
    pub(crate) fn sent_from(&self, name: &str, headers: &HeaderMap) -> bool {
        from_itself(name, headers)
            || self.dev_server.as_deref().is_some_and(|dev_server| {
                single(headers, ORIGIN)
                    .is_some_and(|origin| origin.eq_ignore_ascii_case(dev_server))
            })
    }

    /// Where a page must be for the server to take its requests, for a
    /// refusal to say.
    fn pages(&self, name: &str) -> String {
        match &self.dev_server {
            Some(dev_server) => format!("http://{name} or {dev_server}"),
            None => format!("http://{name}"),
        }
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
            Condition::UnknownHost,
            &format!("this server answers only as {}", hosts.listed()),
        );
    };
    if !hosts.sent_from(name, request.headers()) {
        return forbidden(
            path,
            Condition::ForeignOrigin,
            &format!(
                "this server takes requests only from its own pages, at {}",
                hosts.pages(name)
            ),
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
/// endpoint, so an endpoint added later is covered without being listed. A
/// read is a GET, and needs nothing here; one that takes a body is a POST
/// ([`route::Method::Post`]) that changes nothing, and is held to the same
/// all the same, which costs the panel nothing, as it sends every POST
/// alike.
///
/// [`route::Method::Post`]: crate::route::Method::Post
///
/// A refusal is its condition and what to say, for the caller to answer in
/// the API's JSON.
/// The header of the panel's own that a change by cookie carries, as `1`:
/// one no form and no plain link can add ([`from_the_panel`]). Spelled here
/// alone, what the refusal says taken from it, since the panel sends it by
/// this name and a later one changes in one place. Looked up whatever its
/// case, as every header name is.
pub(crate) const PANEL_HEADER: &str = "X-Mindflayer";

/// What a change by cookie without [`PANEL_HEADER`] is told.
static PANEL_HEADER_MISSING: LazyLock<String> = LazyLock::new(|| {
    format!("a change sent with the cookie must carry {PANEL_HEADER}: 1, as the panel's do")
});

pub(crate) fn from_the_panel(
    method: &Method,
    headers: &HeaderMap,
) -> Result<(), (Condition, &'static str)> {
    if matches!(*method, Method::GET | Method::HEAD) {
        return Ok(());
    }
    // Fetch Metadata: a browser says on every request to loopback which page
    // sent it, relative to where it is going, and no page can say otherwise.
    // Without the header the sender is no browser of today's, so it is a
    // script, and a script that should be here holds the token to send.
    if single(headers, "sec-fetch-site") != Some("same-origin") {
        return Err((
            Condition::SameOriginRequired,
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
    if single(headers, PANEL_HEADER) != Some("1") {
        return Err((
            Condition::PanelHeaderRequired,
            PANEL_HEADER_MISSING.as_str(),
        ));
    }
    if !single(headers, CONTENT_TYPE).is_some_and(is_json) {
        return Err((
            Condition::JsonRequired,
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

/// What a page of the server's may load, and who may show it: its own files,
/// and nobody.
///
/// `default-src 'self'` stands for every kind of resource, scripts among
/// them, so no inline script runs: whatever markup ever reached the page, a
/// gathered skill's included, could not bring a script with it. Nor does
/// anything from another origin. `frame-ancestors 'none'` keeps another page
/// from showing the panel in a frame under its own and steering a click onto
/// one of its buttons.
const POLICY: &str = "default-src 'self'; frame-ancestors 'none'";

/// Send the policy with every answer, and `nosniff`, so the browser takes an
/// answer for what its Content-Type says it is and never guesses: a page on
/// another port that loads `/api/v1/transcript` as a script has it refused,
/// not run, and JSON is never mistaken for a page.
///
/// And `no-store` with every answer under `/api`, refusals included, so the
/// browser keeps none of them in the profile's cache on disk: a read is a
/// GET, whose answer it is otherwise free to store there, the whole text of
/// an artifact's file among them. Nothing is lost by it, since the panel
/// keeps what it read in a cache of its own while the page is open, and
/// asks again whenever the disk changes.
///
/// No CORS layer goes with them, on purpose: without one, no answer grants
/// another origin the right to read it, and a preflight is never passed.
pub(crate) async fn restrict(request: Request, next: Next) -> Response {
    let of_the_api = api::is_api(request.uri().path());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(POLICY));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if of_the_api {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
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

/// A 403, in JSON with its code under `/api` like every other refusal
/// there, and as text anywhere else, where it is a person at a browser who
/// reads it, and the panel's files and the launch link have no client that
/// acts on a code.
fn forbidden(path: &str, condition: Condition, message: &str) -> Response {
    if api::is_api(path) {
        return api::error(condition, message);
    }
    (
        condition.status(),
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{message}\n"),
    )
        .into_response()
}

#[cfg(test)]
#[path = "guard_test.rs"]
mod tests;
