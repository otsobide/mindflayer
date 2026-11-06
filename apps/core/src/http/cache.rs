//! Answers kept on disk, in a workspace's `.mindweaver/cache/http`, so a
//! catalog read again is asked whether it changed rather than sent whole,
//! and one read recently is not asked at all.
//!
//! An entry a file, named for its URL's SHA-256: a URL with a query holds
//! `?`, `&`, `/` and `..` in any order and at any length, none of which
//! may reach a file name, and a digest is one plain name of one length
//! whatever the URL was. The file says nothing of the URL, which may carry
//! a key in its query no rule of text can tell from a page number, and
//! nothing of the request, whose headers are the ones a credential travels
//! in: only what an answer said of itself, and only the headers a later
//! read needs.
//!
//! Read leniently, as a cache can be: an entry that cannot be read, or that
//! another build wrote in a format of its own, is a miss and said
//! ([`HttpError::CacheUnreadable`]), never a reason to fail the fetch, and
//! the answer fetched in its place writes over it. Written through a
//! temporary file and a rename, so a reader finds an entry whole or not at
//! all, and two writers of one entry leave the last one whole: no lock is
//! taken, as none is needed for that.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Fetched, Headers, HttpError, Request};
use crate::adapter::SystemClock;
use crate::port::Clock;
use crate::workspace::{replace_bytes_unresolved, WeaverWorkspace};

/// The format an entry is written in, the first thing it says: one this
/// build does not know is a miss, as a corrupt one is.
const FORMAT: u32 = 1;

/// The headers of an answer an entry keeps: what tells whether it changed
/// (`ETag`, `Last-Modified`), what it is (`Content-Type`), and where a
/// listing's next page is (`Link`). No other, `Set-Cookie` least of all,
/// which is a credential a server hands out.
const KEPT: [&str; 4] = ["content-type", "etag", "last-modified", "link"];

/// Where answers are kept, and for how long one is served without asking.
#[derive(Debug, Clone)]
pub struct Cache {
    folder: PathBuf,
    fresh_for: Duration,
    clock: Arc<dyn Clock>,
}

impl Cache {
    /// The cache `workspace` keeps ([`WeaverWorkspace::http_cache_dir`]).
    pub fn of(workspace: &WeaverWorkspace) -> Self {
        Self::at(workspace.http_cache_dir())
    }

    /// A cache in `folder`, made when the first answer is kept. Every entry
    /// is asked about before it is served, until [`fresh_for`](Self::fresh_for)
    /// says otherwise: how long a catalog stays the same is its caller's to
    /// know.
    pub fn at(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            fresh_for: Duration::ZERO,
            clock: Arc::new(SystemClock),
        }
    }

    /// Serve an entry kept less than `fresh_for` ago without asking.
    pub fn fresh_for(mut self, fresh_for: Duration) -> Self {
        self.fresh_for = fresh_for;
        self
    }

    /// Tell when an entry was kept by `clock`, the system's unless a test
    /// hands another.
    pub fn stamped_by(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The file the answer to `url` is kept in.
    pub fn path_of(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(url.as_bytes());
        let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        self.folder.join(name)
    }

    /// The entry kept for `url`, `None` when there is none, and a failure,
    /// to be said and taken as a miss, when there is one that cannot be read.
    pub fn read(&self, url: &str) -> Result<Option<Entry>, HttpError> {
        let path = self.path_of(url);
        let unreadable = |detail: String| HttpError::CacheUnreadable {
            path: path.clone(),
            detail,
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(unreadable(error.to_string())),
        };
        let Some(end) = bytes.iter().position(|byte| *byte == b'\n') else {
            return Err(unreadable(String::from("it says nothing of itself")));
        };
        let said: Said = serde_json::from_slice(&bytes[..end])
            .map_err(|error| unreadable(format!("what it says of itself: {error}")))?;
        if said.format != FORMAT {
            return Err(unreadable(format!(
                "it is in format {}, and this build reads format {FORMAT}",
                said.format
            )));
        }
        let mut headers = Headers::new();
        for (name, value) in said.headers {
            headers.push(name, value);
        }
        Ok(Some(Entry {
            stored: said.stored,
            headers,
            body: bytes[end + 1..].to_vec(),
        }))
    }

    /// Keep `entry` as the answer to `url`, in place of any kept before.
    pub fn write(&self, url: &str, entry: &Entry) -> Result<(), HttpError> {
        let path = self.path_of(url);
        let said = Said {
            format: FORMAT,
            stored: entry.stored,
            headers: kept(&entry.headers)
                .iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        };
        let unwritable = |path: &Path, source| HttpError::CacheUnwritable {
            path: path.to_path_buf(),
            source,
        };
        let mut bytes = serde_json::to_vec(&said)
            .map_err(|error| unwritable(&path, io::Error::other(error)))?;
        bytes.push(b'\n');
        bytes.extend_from_slice(&entry.body);
        fs::create_dir_all(&self.folder).map_err(|source| unwritable(&self.folder, source))?;
        replace_bytes_unresolved(&path, &bytes).map_err(|(path, source)| unwritable(&path, source))
    }

    /// The time an entry kept now is stamped with.
    pub(super) fn now(&self) -> i64 {
        self.clock.now()
    }

    /// Whether `entry` was kept recently enough to be served unasked.
    pub(super) fn is_fresh(&self, entry: &Entry) -> bool {
        // A time ahead of the clock, which a clock set back gives, is not
        // fresh: it says nothing of how long ago the entry was kept.
        let age = self.now().saturating_sub(entry.stored);
        age >= 0 && (age as u64) < self.fresh_for.as_secs()
    }
}

/// One kept answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it was kept, or last said by the server to be unchanged, in
    /// seconds since the epoch.
    pub stored: i64,
    /// The headers the answer came with that an entry keeps.
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Entry {
    /// What `fetched` keeps, at `stored`.
    pub(super) fn of(fetched: &Fetched, stored: i64) -> Self {
        Self {
            stored,
            headers: kept(&fetched.headers),
            body: fetched.body.clone(),
        }
    }

    /// `request`, asking whether the answer is still this one, when the
    /// answer said how to ask: by its `ETag`, or its `Last-Modified`.
    pub(super) fn conditional(&self, request: &Request) -> Option<Request> {
        let etag = self.headers.get("etag");
        let modified = self.headers.get("last-modified");
        if etag.is_none() && modified.is_none() {
            return None;
        }
        let mut asking = request.clone();
        if let Some(etag) = etag {
            asking = asking.header("If-None-Match", etag);
        }
        if let Some(modified) = modified {
            asking = asking.header("If-Modified-Since", modified);
        }
        Some(asking)
    }
}

/// What an entry says of itself, on its first line.
#[derive(Debug, Serialize, Deserialize)]
struct Said {
    format: u32,
    stored: i64,
    headers: Vec<(String, String)>,
}

/// `headers`, those an entry keeps alone ([`KEPT`]).
fn kept(headers: &Headers) -> Headers {
    let mut kept = Headers::new();
    for (name, value) in headers.iter() {
        if KEPT.iter().any(|each| name.eq_ignore_ascii_case(each)) {
            kept.push(name, value);
        }
    }
    kept
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
