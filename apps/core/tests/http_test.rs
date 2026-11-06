//! The `Http` port's users: what a `Client` makes of the answers a transport
//! hands it, run against a transport kept in memory, so the rules are held
//! of whatever brings the bytes. Nothing reaches the network: the fake
//! answers each URL it is told to from what it holds, and a 404 otherwise.
//! The default transport is tested against a server of its own, in
//! `adapter/reqwest_test.rs`.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mindweaver_core::http::{Answer, Client, Headers, HttpError, Limits, Request};
use mindweaver_core::value::Redacted;
use mindweaver_core::Http;

// ---------------------------------------------------------------------------
// The fake
// ---------------------------------------------------------------------------

/// What the fake answers one request with.
#[derive(Debug, Clone)]
enum Reply {
    /// A status, headers and a body, handed over at once.
    Whole {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// A 200 whose body comes `chunk` bytes at a time, `pause` apart.
    Slow {
        body: Vec<u8>,
        chunk: usize,
        pause: Duration,
    },
    /// A 200 whose body never ends, with these headers.
    Endless { headers: Vec<(String, String)> },
    /// A 200 whose body fails partway with an error quoting `quoting`.
    Broken { quoting: String },
    /// No answer: what a transport says when its time ran out.
    TimedOut,
}

impl Reply {
    fn ok(body: &str) -> Self {
        Self::status(200, body)
    }

    fn status(status: u16, body: &str) -> Self {
        Self::Whole {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn with(self, name: &str, value: &str) -> Self {
        match self {
            Self::Whole {
                status,
                mut headers,
                body,
            } => {
                headers.push((name.to_owned(), value.to_owned()));
                Self::Whole {
                    status,
                    headers,
                    body,
                }
            }
            Self::Endless { mut headers } => {
                headers.push((name.to_owned(), value.to_owned()));
                Self::Endless { headers }
            }
            other => other,
        }
    }
}

/// A request as the fake received it, credentials and all, for a test to
/// ask what was sent where.
#[derive(Debug, Clone)]
struct Sent {
    url: String,
    headers: Vec<(String, String)>,
}

impl Sent {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A transport kept in memory: each URL answered with the replies it was
/// given, in order, the last one again once they run out.
#[derive(Debug, Default)]
struct Fake {
    replies: Mutex<BTreeMap<String, VecDeque<Reply>>>,
    sent: Mutex<Vec<Sent>>,
    /// Set once a body is read, for a test that it never was.
    read: Arc<AtomicBool>,
}

impl Fake {
    fn new() -> Self {
        Self::default()
    }

    fn answering(self, url: &str, reply: Reply) -> Self {
        self.replies
            .lock()
            .unwrap()
            .entry(url.to_owned())
            .or_default()
            .push_back(reply);
        self
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }
}

impl Http for Fake {
    fn send(&self, request: &Request, timeout: Duration) -> Result<Answer, HttpError> {
        assert!(!timeout.is_zero(), "a send is given time left");
        self.sent.lock().unwrap().push(Sent {
            url: request.url().to_owned(),
            headers: request
                .headers()
                .iter()
                .map(|header| (header.name().to_owned(), header.value().to_owned()))
                .collect(),
        });
        let reply = {
            let mut replies = self.replies.lock().unwrap();
            match replies.get_mut(request.url()) {
                Some(queue) if queue.len() > 1 => queue.pop_front(),
                Some(queue) => queue.front().cloned(),
                None => None,
            }
        }
        .unwrap_or_else(|| Reply::status(404, "not here"));

        let read = Arc::clone(&self.read);
        let (status, headers, body): (u16, Vec<(String, String)>, Box<dyn Read + Send>) =
            match reply {
                Reply::Whole {
                    status,
                    headers,
                    body,
                } => (
                    status,
                    headers,
                    Box::new(Watched(io::Cursor::new(body), read)),
                ),
                Reply::Slow { body, chunk, pause } => (
                    200,
                    Vec::new(),
                    Box::new(Watched(
                        Dribble {
                            body,
                            at: 0,
                            chunk,
                            pause,
                        },
                        read,
                    )),
                ),
                Reply::Endless { headers } => (200, headers, Box::new(Watched(Endless, read))),
                Reply::Broken { quoting } => (
                    200,
                    Vec::new(),
                    Box::new(Watched(
                        Breaking {
                            quoting,
                            gave: false,
                        },
                        read,
                    )),
                ),
                Reply::TimedOut => {
                    return Err(HttpError::Timeout {
                        url: Redacted::new(request.url()),
                        after: timeout,
                    })
                }
            };
        let mut answered = Headers::new();
        for (name, value) in headers {
            answered.push(name, value);
        }
        Ok(Answer {
            status,
            headers: answered,
            body,
        })
    }
}

/// A body that says once it has been read.
struct Watched<R>(R, Arc<AtomicBool>);

impl<R: Read> Read for Watched<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.1.store(true, Ordering::SeqCst);
        self.0.read(buf)
    }
}

/// A body handed over a few bytes at a time, with a wait before each.
struct Dribble {
    body: Vec<u8>,
    at: usize,
    chunk: usize,
    pause: Duration,
}

impl Read for Dribble {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.body.len() {
            return Ok(0);
        }
        thread::sleep(self.pause);
        let end = (self.at + self.chunk).min(self.body.len());
        let count = (end - self.at).min(buf.len());
        buf[..count].copy_from_slice(&self.body[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

/// A body with no end.
struct Endless;

impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        buf.fill(b'x');
        Ok(buf.len())
    }
}

/// A body that hands over a little, then fails with words quoting a URL.
struct Breaking {
    quoting: String,
    gave: bool,
}

impl Read for Breaking {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.gave {
            return Err(io::Error::other(format!(
                "connection reset while reading {}",
                self.quoting
            )));
        }
        self.gave = true;
        buf[0] = b'{';
        Ok(1)
    }
}

const CATALOG: &str = "https://catalog.example/v0/servers";

fn limits(timeout: Duration, max_bytes: u64) -> Limits {
    Limits {
        timeout,
        max_bytes,
        ..Limits::default()
    }
}

// ---------------------------------------------------------------------------
// An answer, held to the limits
// ---------------------------------------------------------------------------

#[test]
fn a_whole_answer_comes_back_as_it_was_sent() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::ok(r#"{"servers":[]}"#).with("Content-Type", "application/json"),
    );

    let fetched = Client::new(&fake).get(&Request::get(CATALOG)).unwrap();

    assert_eq!(fetched.status, 200);
    assert_eq!(fetched.url, CATALOG);
    assert_eq!(fetched.body, br#"{"servers":[]}"#);
    assert_eq!(
        fetched.headers.get("content-type"),
        Some("application/json")
    );
}

#[test]
fn a_requests_headers_are_sent_with_it() {
    let fake = Fake::new().answering(CATALOG, Reply::ok("[]"));

    Client::new(&fake)
        .get(
            &Request::get(CATALOG)
                .header("Accept", "application/json")
                .credential("X-Api-Key", "k3y"),
        )
        .unwrap();

    let sent = fake.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].url, CATALOG);
    assert_eq!(sent[0].header("accept"), Some("application/json"));
    assert_eq!(sent[0].header("x-api-key"), Some("k3y"));
}

/// The cap is held to what arrives, so a body that never ends stops at it
/// rather than filling the memory.
#[test]
fn a_body_larger_than_the_cap_is_refused_while_it_is_read() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Endless {
            headers: Vec::new(),
        },
    );

    let error = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(30), 64 * 1024))
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert_eq!(error.code(), "http_too_large");
    assert_eq!(
        error.to_string(),
        format!("{CATALOG}: the answer is larger than 65536 bytes, the most one fetch takes")
    );
}

#[test]
fn a_length_declared_over_the_cap_is_refused_before_a_byte_is_read() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Endless {
            headers: Vec::new(),
        }
        .with("Content-Length", "1000000"),
    );

    let error = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(30), 1000))
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert_eq!(error.code(), "http_too_large");
    assert!(!fake.read.load(Ordering::SeqCst), "the body was read");
}

/// A length is only what the server says.
#[test]
fn a_length_declared_under_the_cap_does_not_lift_it() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Endless {
            headers: Vec::new(),
        }
        .with("Content-Length", "10"),
    );

    let error = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(30), 64 * 1024))
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert_eq!(error.code(), "http_too_large");
}

/// A length is no promise, so no room is made for it beyond what a body
/// may bring at once: under a cap raised as far as it goes, a length no
/// machine could hold is still read as what arrives.
#[test]
fn a_length_no_body_could_bring_is_not_made_room_for() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::ok("[]").with("Content-Length", &u64::MAX.to_string()),
    );

    let fetched = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(30), u64::MAX))
        .get(&Request::get(CATALOG))
        .unwrap();

    assert_eq!(fetched.body, b"[]");
}

#[test]
fn a_body_exactly_at_the_cap_is_taken() {
    let body = "x".repeat(1000);
    let fake = Fake::new().answering(CATALOG, Reply::ok(&body));

    let fetched = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(30), 1000))
        .get(&Request::get(CATALOG))
        .unwrap();

    assert_eq!(fetched.body.len(), 1000);
}

/// A server that keeps sending, slowly, never trips a timeout per read: the
/// limit is on the whole exchange.
#[test]
fn a_slow_body_is_cut_at_the_time_limit() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Slow {
            body: vec![b'x'; 1000],
            chunk: 10,
            pause: Duration::from_millis(20),
        },
    );
    let started = Instant::now();

    let error = Client::new(&fake)
        .with_limits(limits(Duration::from_millis(200), 1024 * 1024))
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert_eq!(error.code(), "http_timed_out");
    assert_eq!(
        error.to_string(),
        format!("{CATALOG}: no whole answer within 200ms")
    );
    // A hundred reads of 20ms would be two seconds: one read past the limit
    // is all it may overrun by.
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_slow_body_within_the_limit_is_taken() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Slow {
            body: vec![b'x'; 30],
            chunk: 10,
            pause: Duration::from_millis(5),
        },
    );

    let fetched = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(10), 1024))
        .get(&Request::get(CATALOG))
        .unwrap();

    assert_eq!(fetched.body, vec![b'x'; 30]);
}

/// A transport is given what is left of the time, so the timeout it says is
/// the fetch's, named for the limit somebody set.
#[test]
fn a_transport_that_gives_up_is_said_as_the_fetchs_timeout() {
    let fake = Fake::new().answering(CATALOG, Reply::TimedOut);

    let error = Client::new(&fake)
        .with_limits(limits(Duration::from_secs(7), 1024))
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert!(
        matches!(&error, HttpError::Timeout { after, .. } if *after == Duration::from_secs(7)),
        "{error:?}"
    );
}

#[test]
fn a_status_that_is_not_a_success_is_a_failure() {
    for status in [404, 500, 503, 301] {
        let fake = Fake::new().answering(CATALOG, Reply::status(status, "<html>no</html>"));

        let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

        assert_eq!(error.code(), "http_status_unsuccessful", "{status}");
        assert_eq!(
            error.to_string(),
            format!("{CATALOG}: the server answered HTTP {status}")
        );
    }
}

#[test]
fn a_url_refused_is_never_sent() {
    let fake = Fake::new();

    for url in [
        "http://catalog.example/v0/servers",
        "https://me:s3cret@catalog.example/v0/servers",
        "ftp://127.0.0.1/catalog.json",
    ] {
        assert!(Client::new(&fake).get(&Request::get(url)).is_err(), "{url}");
    }

    assert!(fake.sent().is_empty(), "{:?}", fake.sent());
}

/// What a transport says of a failure can quote a URL, one a redirect led
/// to among them, with a credential in it.
#[test]
fn a_failure_while_reading_is_said_without_the_credentials_it_quotes() {
    let fake = Fake::new().answering(
        CATALOG,
        Reply::Broken {
            quoting: String::from("https://me:s3cret@mirror.example/v0/servers"),
        },
    );

    let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

    assert_eq!(error.code(), "http_transfer_failed");
    assert_eq!(
        error.to_string(),
        format!("{CATALOG}: connection reset while reading https://mirror.example/v0/servers")
    );
}

// ---------------------------------------------------------------------------
// Redirects
// ---------------------------------------------------------------------------

fn redirect(status: u16, location: &str) -> Reply {
    Reply::status(status, "").with("Location", location)
}

#[test]
fn a_redirect_is_followed_to_where_it_leads() {
    let fake = Fake::new()
        .answering(CATALOG, redirect(301, "/v1/servers?limit=10"))
        .answering(
            "https://catalog.example/v1/servers?limit=10",
            Reply::ok("[]"),
        );

    let fetched = Client::new(&fake).get(&Request::get(CATALOG)).unwrap();

    assert_eq!(fetched.url, "https://catalog.example/v1/servers?limit=10");
    assert_eq!(fetched.body, b"[]");
    let sent: Vec<String> = fake.sent().into_iter().map(|sent| sent.url).collect();
    assert_eq!(
        sent,
        [CATALOG, "https://catalog.example/v1/servers?limit=10"]
    );
}

#[test]
fn every_redirect_status_is_followed() {
    for status in [301, 302, 303, 307, 308] {
        let fake = Fake::new()
            .answering(CATALOG, redirect(status, "https://mirror.example/servers"))
            .answering("https://mirror.example/servers", Reply::ok("[]"));

        let fetched = Client::new(&fake).get(&Request::get(CATALOG)).unwrap();

        assert_eq!(fetched.url, "https://mirror.example/servers", "{status}");
    }
}

/// A loop never ends on its own, so the bound is what ends it.
#[test]
fn redirects_are_bounded() {
    let other = "https://catalog.example/elsewhere";
    let fake = Fake::new()
        .answering(CATALOG, redirect(302, other))
        .answering(other, redirect(302, CATALOG));

    let error = Client::new(&fake)
        .with_limits(Limits {
            max_redirects: 3,
            ..Limits::default()
        })
        .get(&Request::get(CATALOG))
        .unwrap_err();

    assert_eq!(error.code(), "http_too_many_redirects");
    assert_eq!(
        error.to_string(),
        format!("{CATALOG}: redirected more than 3 times")
    );
    assert_eq!(
        fake.sent().len(),
        4,
        "the first request and three redirects"
    );
}

#[test]
fn a_redirect_to_plain_http_elsewhere_is_refused_and_never_followed() {
    let fake = Fake::new().answering(CATALOG, redirect(302, "http://mirror.example/servers"));

    let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

    assert_eq!(error.code(), "http_redirect_refused");
    assert_eq!(
        error.to_string(),
        format!(
            "{CATALOG}: redirected to http://mirror.example/servers, which is not followed: \
             only https is, or http on this machine, and never a URL with credentials in it"
        )
    );
    assert_eq!(fake.sent().len(), 1);
}

#[test]
fn a_redirect_to_another_scheme_is_refused() {
    for location in [
        "ftp://mirror.example/servers",
        "file:///etc/passwd",
        "ftp://127.0.0.1/servers",
    ] {
        let fake = Fake::new().answering(CATALOG, redirect(302, location));

        let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

        assert_eq!(error.code(), "http_redirect_refused", "{location}");
        assert_eq!(fake.sent().len(), 1, "{location}");
    }
}

#[test]
fn a_redirect_to_http_on_this_machine_is_followed() {
    let local = "http://127.0.0.1:8080/servers";
    let fake = Fake::new()
        .answering(CATALOG, redirect(307, local))
        .answering(local, Reply::ok("[]"));

    let fetched = Client::new(&fake).get(&Request::get(CATALOG)).unwrap();

    assert_eq!(fetched.url, local);
}

#[test]
fn a_redirect_carrying_credentials_is_refused_without_repeating_them() {
    let fake = Fake::new().answering(
        CATALOG,
        redirect(302, "https://me:s3cret@mirror.example/servers"),
    );

    let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

    assert_eq!(error.code(), "http_redirect_refused");
    assert!(!error.to_string().contains("s3cret"), "{error}");
    assert_eq!(fake.sent().len(), 1);
}

/// A credential is given for the server a request names, not for the one it
/// is sent on to, whatever the header carrying it is called; what is not a
/// credential goes along.
#[test]
fn credentials_stay_with_the_origin_they_were_given_for() {
    let mirror = "https://mirror.example/servers";
    let fake = Fake::new()
        .answering(CATALOG, redirect(302, mirror))
        .answering(mirror, Reply::ok("[]"));

    Client::new(&fake)
        .get(
            &Request::get(CATALOG)
                .header("Accept", "application/json")
                .header("Authorization", "Bearer ghp_token")
                .credential("X-Api-Key", "k3y"),
        )
        .unwrap();

    let sent = fake.sent();
    assert_eq!(sent[0].header("authorization"), Some("Bearer ghp_token"));
    assert_eq!(sent[0].header("x-api-key"), Some("k3y"));
    assert_eq!(sent[1].url, mirror);
    assert_eq!(sent[1].header("accept"), Some("application/json"));
    assert_eq!(sent[1].header("authorization"), None);
    assert_eq!(sent[1].header("x-api-key"), None);
}

/// Another path on the same server is where the credential was meant for.
#[test]
fn credentials_go_along_a_redirect_within_the_origin() {
    let moved = "https://catalog.example/v1/servers";
    let fake = Fake::new()
        .answering(CATALOG, redirect(308, moved))
        .answering(moved, Reply::ok("[]"));

    Client::new(&fake)
        .get(&Request::get(CATALOG).credential("X-Api-Key", "k3y"))
        .unwrap();

    assert_eq!(fake.sent()[1].header("x-api-key"), Some("k3y"));
}

#[test]
fn a_redirect_status_with_nowhere_to_go_is_a_failure() {
    let fake = Fake::new().answering(CATALOG, Reply::status(302, ""));

    let error = Client::new(&fake).get(&Request::get(CATALOG)).unwrap_err();

    assert_eq!(error.code(), "http_status_unsuccessful");
}
