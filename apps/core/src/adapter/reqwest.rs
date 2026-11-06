//! The [`Http`] a build sends with: reqwest's blocking client over rustls,
//! the reqwest gix's transport already builds, so a catalog adds no crate
//! and no second TLS.
//!
//! It only carries bytes, as the port asks: it follows no redirect, reads no
//! body, and keeps nothing of a request once it is sent. Everything reqwest
//! says is turned into core's words here, at the edge ([`HttpError`],
//! [`Answer`]), so no reqwest type reaches the port or anything behind it.

use std::error::Error as _;
use std::io::{self, Read};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::redirect::Policy;

use crate::http::{Answer, Headers, HttpError, Request, USER_AGENT};
use crate::port::Http;
use crate::value::Redacted;

/// How long making a connection may take: a host that never answers is told
/// apart sooner than the whole exchange's limit would tell it, and ten
/// seconds is what a TLS handshake to a host far away takes, many times
/// over.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// reqwest, as the [`Http`] a catalog is fetched through.
///
/// Built once and handed to every [`crate::http::Client`] that needs it,
/// since a client keeps its connections and its TLS configuration, and
/// building one spawns the thread its runtime runs on.
#[derive(Debug, Clone)]
pub struct Reqwest {
    client: Client,
}

impl Reqwest {
    /// A transport that gives up connecting after [`CONNECT_TIMEOUT`].
    pub fn new() -> Result<Self, HttpError> {
        Self::connecting_within(CONNECT_TIMEOUT)
    }

    /// A transport that gives up connecting after `timeout`.
    ///
    /// It says it is [`USER_AGENT`], and sets no header else of its own: a
    /// request's own go with that request alone, so no credential is held
    /// here to be sent with the next. It follows no redirect, which the
    /// client follows by its rules, and reads no proxy from the environment,
    /// which core never reads, so a proxy is for a front end to hand over.
    pub fn connecting_within(timeout: Duration) -> Result<Self, HttpError> {
        Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(timeout)
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map(|client| Self { client })
            .map_err(|error| HttpError::Unavailable {
                detail: Redacted::new(&words(&error)),
            })
    }
}

impl Http for Reqwest {
    /// The whole exchange is held to `timeout` by reqwest's own timeout for
    /// the request, which runs from connecting to the body's last byte: the
    /// client's timeout would hold each read to it on its own, and a body
    /// read a byte at a time would never end.
    fn send(&self, request: &Request, timeout: Duration) -> Result<Answer, HttpError> {
        let url = request.url();
        let mut sending = self.client.get(url).timeout(timeout);
        for header in request.headers() {
            // The name and no more: the value may be a credential.
            let invalid = || HttpError::Header {
                url: Redacted::new(url),
                name: header.name().to_owned(),
            };
            let name = HeaderName::from_bytes(header.name().as_bytes()).map_err(|_| invalid())?;
            let mut value = HeaderValue::from_str(header.value()).map_err(|_| invalid())?;
            // So reqwest and hyper leave it out of anything they print.
            value.set_sensitive(header.is_credential());
            sending = sending.header(name, value);
        }
        let response = sending.send().map_err(|error| said(url, timeout, error))?;

        let mut headers = Headers::new();
        for (name, value) in response.headers() {
            // A value that is not text is none a rule of core's reads.
            if let Ok(value) = value.to_str() {
                headers.push(name.as_str(), value);
            }
        }
        Ok(Answer {
            status: response.status().as_u16(),
            headers,
            body: Box::new(Body(response)),
        })
    }
}

/// A response's body, read as it arrives, its running out of time said as
/// the timeout it is.
struct Body(Response);

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|error| {
            let timed_out = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
                .is_some_and(reqwest::Error::is_timeout);
            if timed_out {
                io::Error::new(io::ErrorKind::TimedOut, error)
            } else {
                error
            }
        })
    }
}

/// What reqwest said of `url`, in core's words.
fn said(url: &str, timeout: Duration, error: reqwest::Error) -> HttpError {
    let shown = Redacted::new(url);
    if error.is_timeout() {
        return HttpError::Timeout {
            url: shown,
            after: timeout,
        };
    }
    let (connect, builder) = (error.is_connect(), error.is_builder());
    // Without the URL reqwest puts in its own words, which the error names
    // already.
    let detail = Redacted::new(&words(&error.without_url()));
    if connect {
        HttpError::Connect { url: shown, detail }
    } else if builder {
        HttpError::Url {
            url: shown,
            detail: detail.into_string(),
        }
    } else {
        HttpError::Transfer { url: shown, detail }
    }
}

/// `error` and every error under it, each in its own words: reqwest's top
/// one says only that sending failed, and what failed is further down (a
/// refused connection, a certificate not trusted).
fn words(error: &reqwest::Error) -> String {
    let mut said = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        said.push_str(": ");
        said.push_str(&cause.to_string());
        source = cause.source();
    }
    said
}

#[cfg(test)]
#[path = "reqwest_test.rs"]
mod tests;
