//! Fetching over HTTP, the one place core does, through the [`Http`] port
//! (D-14).
//!
//! Catalogs (the MCP Registry, the skill directories) are HTTP APIs, and
//! what is done with them is rules: which URLs a fetch may name, which
//! redirects it follows and what goes along, how long an answer may take and
//! how large it may be. Rules live in core, so the CLI,
//! the TUI and the server cannot disagree about them. What carries the bytes
//! is not a rule, and is behind the port: [`crate::adapter::reqwest`] by
//! default, and a fake in every test, so none goes online. Front ends
//! fetching the bytes and handing them over was the other way, rejected
//! because it spreads these rules across every front end; calling a client
//! with no seam would take every test of a catalog online.
//!
//! A [`Client`] is what the rest of core fetches through. It holds every
//! answer to its [`Limits`] whichever [`Http`] brought it, reading the body
//! itself rather than trusting a transport to stop, and names a URL in an
//! error only as a [`Redacted`] one. Everything here blocks: core is
//! synchronous, and the server already runs what a backend does off tokio's
//! threads, where blocking is allowed.

use std::fmt;
use std::io::{self, Read};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use reqwest::Url;
use thiserror::Error;

use crate::code::codes;
use crate::port::Http;
use crate::url::carries_credentials;
use crate::value::Redacted;

/// Who Mindweaver says it is on every request: GitHub's API refuses one that
/// does not say, and a service that wants to tell its callers apart reads it.
pub const USER_AGENT: &str = concat!("mindweaver/", env!("CARGO_PKG_VERSION"));

/// Headers whose value is a credential whoever adds them, held as one
/// however they were added ([`Request::header`]).
const CREDENTIAL_HEADERS: [&str; 3] = ["authorization", "proxy-authorization", "cookie"];

/// The statuses that send a request on to another URL, the one their
/// `Location` names.
const REDIRECTS: [u16; 5] = [301, 302, 303, 307, 308];

/// What an answer is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// From sending the request to the last byte of the body.
    ///
    /// One span for the whole exchange rather than a timeout for each read:
    /// what somebody waiting on a command feels is how long it takes, and a
    /// server sending one byte just before each read would time out never
    /// trips a timeout per read at all. Connecting has a bound of its own,
    /// the transport's.
    pub timeout: Duration,
    /// The most bytes a body may have.
    pub max_bytes: u64,
    /// The most redirects one fetch follows.
    pub max_redirects: usize,
}

impl Default for Limits {
    /// Thirty seconds, reqwest's own default, and 16 MiB: far more than a
    /// page of a catalog needs, and little enough that a server sending bytes
    /// without end fills neither the memory nor the disk. Ten redirects, as
    /// reqwest would follow, where a service moving its API takes one.
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_bytes: 16 * 1024 * 1024,
            max_redirects: 10,
        }
    }
}

/// One header of a [`Request`].
#[derive(Clone, PartialEq, Eq)]
pub struct Header {
    name: String,
    value: String,
    credential: bool,
}

impl Header {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether the value is a credential: never shown, never kept, and sent
    /// to no server but the one the request named.
    pub fn is_credential(&self) -> bool {
        self.credential
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if self.credential {
            "<credential>"
        } else {
            &self.value
        };
        write!(f, "{}: {value}", self.name)
    }
}

/// A GET: a URL, and the headers that go with it.
///
/// Headers belong to the request and to nothing that outlives it. A
/// credential a front end supplies (D-16) lasts one run, so it is handed in
/// with each request that needs it, rather than kept on a client or a
/// transport that would send it with a request that does not.
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    url: String,
    headers: Vec<Header>,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: Vec::new(),
        }
    }

    /// Send `name: value` with it, in place of any value `name` had.
    ///
    /// A header named like one that carries a credential (`Authorization`,
    /// `Proxy-Authorization`, `Cookie`) is held as one however it was added:
    /// shown in the clear, it would be in every log a request is debugged
    /// into.
    pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into();
        let credential = CREDENTIAL_HEADERS
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known));
        self.with(name, value.into(), credential)
    }

    /// Send `name: value` with it as a credential, such as a key in a header
    /// of a service's own naming.
    pub fn credential(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.with(name.into(), value.into(), true)
    }

    fn with(mut self, name: String, value: String, credential: bool) -> Self {
        self.headers
            .retain(|header| !header.name.eq_ignore_ascii_case(&name));
        self.headers.push(Header {
            name,
            value,
            credential,
        });
        self
    }

    /// The URL as it was given, which a [`Client`] refuses when it carries
    /// credentials.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// The value sent as `name`, if any.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(Header::value)
    }

    /// Whether any header carries a credential.
    pub fn carries_credential(&self) -> bool {
        self.headers.iter().any(Header::is_credential)
    }

    /// The same request sent to `url`, its credentials with it only when
    /// `credentials` says they may go there.
    fn to(&self, url: &Url, credentials: bool) -> Self {
        Self {
            url: url.to_string(),
            headers: self
                .headers
                .iter()
                .filter(|header| credentials || !header.credential)
                .cloned()
                .collect(),
        }
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("url", &Redacted::new(&self.url))
            .field("headers", &self.headers)
            .finish()
    }
}

/// The headers of an answer, looked up without regard to case, as HTTP
/// compares their names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    /// These headers and `name: value`, for an answer built by hand.
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.push(name, value);
        self
    }

    pub fn push(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.push((name.into(), value.into()));
    }

    /// The first value given for `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

/// An answer as an [`Http`] hands it over: its status and headers read, its
/// body not yet, so that the [`Client`] reading it stops at its limits.
pub struct Answer {
    pub status: u16,
    pub headers: Headers,
    pub body: Box<dyn Read + Send>,
}

impl fmt::Debug for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Answer")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

/// An answer that came back whole, within its limits, with a status that
/// says it succeeded.
#[derive(Debug)]
pub struct Fetched {
    /// The URL the answer came from: the one asked of, or the one the last
    /// redirect led to. [`fetchable`] let it through, so it carries no
    /// credential.
    pub url: String,
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

/// What core fetches through: an [`Http`], and the limits every answer it
/// brings back is held to.
#[derive(Debug)]
pub struct Client<'a> {
    http: &'a dyn Http,
    limits: Limits,
}

impl<'a> Client<'a> {
    /// A client sending through `http`, which the caller hands over rather
    /// than one core picks itself: a front end chooses the default adapter,
    /// a test a fake, and nothing is read from a global or the environment.
    pub fn new(http: &'a dyn Http) -> Self {
        Self {
            http,
            limits: Limits::default(),
        }
    }

    /// Hold answers to `limits` rather than to the default ones.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Fetch `request` and read its answer whole, following its redirects.
    ///
    /// Refused before anything is sent: a URL [`fetchable`] refuses. A
    /// failure: an answer that takes longer than the limit, every redirect
    /// included, one that is larger, a redirect `followed` refuses or one
    /// past [`Limits::max_redirects`], and an answer whose status is not a
    /// success, since what a 404 or a 503 has in its body is not what was
    /// asked for.
    ///
    /// Redirects are followed here rather than by the transport, so a fake
    /// is held to the same rules as the network. A request's credentials go
    /// only to the origin it named: they were given for that server, not for
    /// whichever one it sends the request on to. reqwest drops
    /// `Authorization` and `Cookie` on such a hop by itself, and only those,
    /// so a key in a header of a service's own naming would have gone along.
    pub fn get(&self, request: &Request) -> Result<Fetched, HttpError> {
        let url = request.url();
        let first = fetchable(url)?;
        let started = Instant::now();
        let mut hop = request.clone();
        let mut at = first.clone();
        let mut redirects = 0;
        let answer = loop {
            let left = self.left(url, started)?;
            let answer = self
                .http
                .send(&hop, left)
                .map_err(|error| self.overall(url, error))?;
            self.in_time(url, started)?;
            let Some(location) = redirect(&answer) else {
                break answer;
            };
            redirects += 1;
            if redirects > self.limits.max_redirects {
                return Err(HttpError::TooManyRedirects {
                    url: Redacted::new(url),
                    limit: self.limits.max_redirects,
                });
            }
            let next = followed(url, &at, location)?;
            hop = request.to(&next, next.origin() == first.origin());
            at = next;
        };

        if !(200..300).contains(&answer.status) {
            return Err(HttpError::Status {
                url: Redacted::new(url),
                status: answer.status,
            });
        }
        let declared = answer
            .headers
            .get("content-length")
            .and_then(|length| length.trim().parse::<u64>().ok());
        let body = self.read(url, answer.body, declared, started)?;
        Ok(Fetched {
            url: if redirects == 0 {
                url.to_owned()
            } else {
                at.to_string()
            },
            status: answer.status,
            headers: answer.headers,
            body,
        })
    }

    /// Read `body` whole, as long as it stays within the limits.
    ///
    /// A length declared over the cap is refused before a byte is read; past
    /// that, the cap is held to what arrives, since a length is only what
    /// the server says, and a body with none says nothing. The time is looked
    /// at after every read, which is all a blocking read allows: a transport
    /// that ignores the time it was given overruns it by one read and no
    /// more.
    fn read(
        &self,
        url: &str,
        mut body: Box<dyn Read + Send>,
        declared: Option<u64>,
        started: Instant,
    ) -> Result<Vec<u8>, HttpError> {
        let limit = self.limits.max_bytes;
        let too_large = || HttpError::TooLarge {
            url: Redacted::new(url),
            limit,
        };
        if declared.is_some_and(|length| length > limit) {
            return Err(too_large());
        }

        // Room for what is declared, up to a mebibyte and no further: a
        // length is not a promise, and a cap a caller raised past what this
        // machine can hold would have the first allocation fail on a length
        // no body ever brings.
        let room = declared.unwrap_or(0).min(1024 * 1024) as usize;
        let mut kept = Vec::with_capacity(room);
        let mut chunk = [0; 16 * 1024];
        loop {
            let read = match body.read(&mut chunk) {
                Ok(0) => return Ok(kept),
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::TimedOut
                        || started.elapsed() >= self.limits.timeout =>
                {
                    return Err(self.timed_out(url));
                }
                Err(error) => {
                    return Err(HttpError::Transfer {
                        url: Redacted::new(url),
                        detail: Redacted::new(&error.to_string()),
                    })
                }
            };
            if kept.len() as u64 + read as u64 > limit {
                return Err(too_large());
            }
            kept.extend_from_slice(&chunk[..read]);
            self.in_time(url, started)?;
        }
    }

    /// What is left of the time a fetch started at `started` may take.
    fn left(&self, url: &str, started: Instant) -> Result<Duration, HttpError> {
        self.limits
            .timeout
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| self.timed_out(url))
    }

    fn in_time(&self, url: &str, started: Instant) -> Result<(), HttpError> {
        if started.elapsed() > self.limits.timeout {
            Err(self.timed_out(url))
        } else {
            Ok(())
        }
    }

    fn timed_out(&self, url: &str) -> HttpError {
        HttpError::Timeout {
            url: Redacted::new(url),
            after: self.limits.timeout,
        }
    }

    /// What a transport said of one exchange, said of the whole fetch: a
    /// timeout is the fetch's, whatever was left of it when the transport
    /// gave up.
    fn overall(&self, url: &str, error: HttpError) -> HttpError {
        match error {
            HttpError::Timeout { .. } => self.timed_out(url),
            error => error,
        }
    }
}

/// `url` parsed, if it is one a fetch may name: https, or http to this
/// machine.
///
/// Plain http elsewhere is refused: what a catalog lists is what gets
/// gathered, and soon commands an agent will run, and over plain http
/// anybody on the way can change it. This machine is the exception, as a
/// catalog served by a test or a tool of one's own is reached over it, and
/// the way there is nobody else's. Taken as this machine: a loopback
/// address, or `localhost`, which RFC 6761 keeps for it.
///
/// A URL with credentials in it is refused rather than sent once and
/// dropped from what is shown, as a gather's is, for the same reason: it is
/// already where the shell history and `ps` show it, and the URL is what
/// errors name. A credential goes in a header instead
/// ([`Request::credential`]), which is never shown. The parsed URL is asked
/// too, so a form [`carries_credentials`] reads differently from the parser
/// cannot slip one through.
pub fn fetchable(url: &str) -> Result<Url, HttpError> {
    let refused = || HttpError::CredentialsInUrl {
        url: Redacted::new(url),
    };
    if carries_credentials(url) {
        return Err(refused());
    }
    let parsed = Url::parse(url).map_err(|error| HttpError::Url {
        url: Redacted::new(url),
        detail: error.to_string(),
    })?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(refused());
    }
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if is_this_machine(&parsed) => Ok(parsed),
        _ => Err(HttpError::Insecure {
            url: Redacted::new(url),
        }),
    }
}

/// Where `answer` sends the request on to, if it is a redirect.
///
/// A redirect status without a `Location` is no redirect: there is nowhere
/// to follow it to, and it is a failure for the status it is.
fn redirect(answer: &Answer) -> Option<&str> {
    REDIRECTS
        .contains(&answer.status)
        .then(|| answer.headers.get("location"))
        .flatten()
}

/// Where a redirect from `from` to `location` leads, when it may be
/// followed: to a URL [`fetchable`] lets through, and nowhere else.
///
/// So a redirect may not leave https for plain http elsewhere, which would
/// hand what is fetched to anybody on the way, nor lead to another scheme,
/// nor carry credentials, which would then be shown wherever its URL is.
/// Leaving the origin is allowed, since that is where a service keeps its
/// downloads, but the credentials stay behind ([`Client::get`]). `url` is
/// the URL the fetch was asked of, which the refusal names.
fn followed(url: &str, from: &Url, location: &str) -> Result<Url, HttpError> {
    let refused = |to: &str| HttpError::Redirect {
        url: Redacted::new(url),
        to: Redacted::new(to),
    };
    let next = from.join(location).map_err(|_| refused(location))?;
    fetchable(next.as_str()).map_err(|_| refused(next.as_str()))
}

/// Whether `url` names this machine: a loopback address, or `localhost`.
///
/// Read off the host as the parser writes it, an IPv6 address in brackets,
/// so that `127.1` and `[0:0::1]` are the addresses they are.
fn is_this_machine(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let address = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || address
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Why a fetch brought nothing back.
///
/// Every URL in one is [`Redacted`], and so is every word a transport said,
/// which can quote the URL it failed on: this is what a front end shows and
/// a log keeps. A header's value is never in one, since it may be a
/// credential.
#[derive(Debug, Error)]
pub enum HttpError {
    /// Not a URL at all.
    #[error("{url}: not a URL that can be fetched: {detail}")]
    Url { url: Redacted, detail: String },
    /// A URL [`fetchable`] refuses for its scheme or its host.
    #[error("{url}: only an https URL is fetched, or an http one on this machine")]
    Insecure { url: Redacted },
    /// A URL with a user name, a password or a token in it.
    #[error(
        "{url}: a password, a token or a user name in the URL is refused, because a fetch's URL \
         is shown and kept: fetch it without one"
    )]
    CredentialsInUrl { url: Redacted },
    /// No connection was made: refused, a name that does not resolve, a
    /// certificate not trusted.
    #[error("{url}: cannot connect: {detail}")]
    Connect { url: Redacted, detail: Redacted },
    /// The connection was made, and failed before the answer was whole.
    #[error("{url}: {detail}")]
    Transfer { url: Redacted, detail: Redacted },
    /// No whole answer within [`Limits::timeout`].
    #[error("{url}: no whole answer within {after:?}")]
    Timeout { url: Redacted, after: Duration },
    /// A body larger than [`Limits::max_bytes`].
    #[error("{url}: the answer is larger than {limit} bytes, the most one fetch takes")]
    TooLarge { url: Redacted, limit: u64 },
    /// An answer whose status is not a success.
    #[error("{url}: the server answered HTTP {status}")]
    Status { url: Redacted, status: u16 },
    /// A redirect `followed` refuses: to plain http elsewhere than this
    /// machine, to another scheme, to a URL with credentials in it, or to
    /// what is not a URL.
    #[error(
        "{url}: redirected to {to}, which is not followed: only https is, or http on this \
         machine, and never a URL with credentials in it"
    )]
    Redirect { url: Redacted, to: Redacted },
    /// More redirects than [`Limits::max_redirects`], a loop among them.
    #[error("{url}: redirected more than {limit} times")]
    TooManyRedirects { url: Redacted, limit: usize },
}

codes!(HttpError {
    Self::Url { .. } => "http_url_invalid",
    Self::Insecure { .. } => "http_url_refused",
    Self::CredentialsInUrl { .. } => "http_credentials_in_url",
    Self::Connect { .. } => "http_connect_failed",
    Self::Transfer { .. } => "http_transfer_failed",
    Self::Timeout { .. } => "http_timed_out",
    Self::TooLarge { .. } => "http_too_large",
    Self::Status { .. } => "http_status_unsuccessful",
    Self::Redirect { .. } => "http_redirect_refused",
    Self::TooManyRedirects { .. } => "http_too_many_redirects",
});

#[cfg(test)]
#[path = "http_test.rs"]
mod tests;
