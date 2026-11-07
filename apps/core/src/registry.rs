//! The official MCP Registry, `registry.modelcontextprotocol.io`, read
//! through its `/v0.1` API (#106): its servers searched, a version of one
//! fetched, and what changed since a time listed, deleted servers among it.
//!
//! The first catalog core reads over HTTP (D-14), through the
//! [`Client`] its caller hands it, so a front end chooses the transport and
//! whether answers are kept ([`crate::http::Cache`]), and a test a fake:
//! every page and every version fetched goes through the client's limits
//! and its cache. Where the registry is, is a parameter
//! ([`Registry::at`]), the official one unless a caller says otherwise, so
//! a test, or a registry of one's own that speaks the same API, is read the
//! same way.
//!
//! What the registry answers is JSON somebody else wrote, so nothing in it
//! is trusted. A listing is read a server at a time: one that cannot be
//! read is said on its own ([`Unreadable`]) and the rest are listed all the
//! same; a key it does not know is passed over; and every string of it a
//! front end is handed, or a definition is written with, is made printable
//! first ([`printable`]), so a carriage return in a description cannot
//! overwrite what a terminal showed above it. A name or a version that is
//! not printable as it is, which a front end would show as another, is no
//! server's at all.
//!
//! Nothing is written here: a server is read, and writing what it maps to
//! into a library is a command's (#107).

use std::fmt;
use std::sync::Arc;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::adapter::SystemClock;
use crate::code::{codes, Said};
use crate::http::{fetchable, Client, Failed, HttpError, Paging, Request};
use crate::port::Clock;
use crate::text::{printable, Utc};
use crate::value::Redacted;

mod server;

/// Where the official registry is: what a [`Registry`] reads unless it is
/// told another.
pub const OFFICIAL: &str = "https://registry.modelcontextprotocol.io";

/// The version of the API read, the first path segment below the
/// registry's address: `v0.1`, the one the registry keeps stable while the
/// rest of it is in preview.
pub const API: &str = "v0.1";

/// How many servers a page is asked for: the most the registry gives one.
pub const PAGE_SIZE: usize = 100;

/// How many pages a listing is read to unless a caller says otherwise: a
/// thousand servers, more than a search answers and enough for what a
/// day's changes are, while a cursor that never runs out still ends.
pub const MAX_PAGES: usize = 10;

/// A registry, read through a client.
#[derive(Debug)]
pub struct Registry<'a> {
    client: &'a Client<'a>,
    base: String,
    max_pages: usize,
    clock: Arc<dyn Clock>,
}

impl<'a> Registry<'a> {
    /// The official registry ([`OFFICIAL`]), read through `client`.
    pub fn new(client: &'a Client<'a>) -> Self {
        Self {
            client,
            base: String::from(OFFICIAL),
            max_pages: MAX_PAGES,
            clock: Arc::new(SystemClock),
        }
    }

    /// The registry at `base` rather than the official one: an address its
    /// API's path is put below, `https://registry.example` for
    /// `https://registry.example/v0.1/servers`.
    pub fn at(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    /// Read a listing to no more than `max_pages` pages.
    pub fn max_pages(mut self, max_pages: usize) -> Self {
        self.max_pages = max_pages;
        self
    }

    /// Tell when a server was fetched by `clock`, the system's unless a test
    /// hands another.
    pub fn stamped_by(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Where the registry is.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The latest version of every server `query` finds, a page at a time
    /// as far as the bound: what the registry matches it against is its own
    /// to say (a server's name, as it is today). An empty query asks for
    /// every server.
    pub fn search(&self, query: &str) -> Result<Listing, Failed> {
        let mut pairs = Vec::new();
        if !query.is_empty() {
            pairs.push(("search", query.to_owned()));
        }
        pairs.push(("version", String::from("latest")));
        self.listing(&pairs)
    }

    /// Every server version the registry changed at `since`, seconds since
    /// the epoch, or after: what a refresh asks for rather than the whole
    /// registry again. Every version, not only the latest, and with the
    /// deleted among them, each said `deleted` ([`Status::Deleted`]), since
    /// a refresh that passed over them would keep what the registry took
    /// away.
    pub fn updated_since(&self, since: i64) -> Result<Listing, Failed> {
        self.listing(&[("updated_since", Utc::of(since).to_string())])
    }

    /// The version `version` of the server `name`, or its latest when
    /// `version` is `latest`, as the registry names both.
    ///
    /// One server asked for by name, so one that does not read is the
    /// answer failing, as one that is not JSON does
    /// ([`HttpError::Json`]), rather than a server passed over.
    pub fn version(&self, name: &str, version: &str) -> Result<Found, Failed> {
        let failed = |error| Failed {
            error,
            warnings: Vec::new(),
        };
        let url = self
            .url(&["servers", name, "versions", version], &[])
            .map_err(failed)?;
        let fetched = self.client.get(&request(&url))?;
        let fetched_at = self.clock.now();
        let read = fetched.json::<serde_json::Value>().and_then(|value| {
            ServerVersion::read(value, &self.base, fetched_at).map_err(|reason| HttpError::Json {
                url: Redacted::new(&fetched.url),
                detail: Redacted::new(&reason),
            })
        });
        match read {
            Ok(server) => Ok(Found {
                server,
                warnings: fetched.warnings,
            }),
            Err(error) => Err(Failed {
                error,
                warnings: fetched.warnings,
            }),
        }
    }

    /// The listing `/servers` answers with `query`, a page at a time by the
    /// cursor each page carries, each server read on its own.
    fn listing(&self, query: &[(&str, String)]) -> Result<Listing, Failed> {
        let mut pairs: Vec<(&str, String)> = query.to_vec();
        pairs.push(("limit", PAGE_SIZE.to_string()));
        let url = self.url(&["servers"], &pairs).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })?;
        let paged = self.client.pages::<server::ServerList>(
            &request(&url),
            &Paging::Cursor {
                param: String::from("cursor"),
            },
            self.max_pages,
        )?;
        let fetched_at = self.clock.now();
        let mut servers = Vec::new();
        let mut failures = Vec::new();
        let listed = paged.pages.into_iter().flat_map(|page| page.servers);
        for (at, value) in listed.enumerate() {
            let name = value
                .get("server")
                .and_then(|server| server.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(printable);
            match ServerVersion::read(value, &self.base, fetched_at) {
                Ok(server) => servers.push(server),
                Err(reason) => failures.push(Unreadable { at, name, reason }),
            }
        }
        Ok(Listing {
            servers,
            failures,
            cut: paged.cut,
            warnings: paged.warnings,
        })
    }

    /// The URL of `segments` below the API's version, each escaped as one
    /// segment (a server's name holds a `/`), with `query`, each value
    /// escaped as the value it is.
    fn url(&self, segments: &[&str], query: &[(&str, String)]) -> Result<String, HttpError> {
        let mut url = fetchable(&self.base)?;
        url.set_query(None);
        url.set_fragment(None);
        url.path_segments_mut()
            .map_err(|()| HttpError::Url {
                url: Redacted::new(&self.base),
                detail: String::from("no path can be put below it"),
            })?
            .pop_if_empty()
            .push(API)
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(name, value)| (*name, value.as_str())));
        }
        Ok(url.to_string())
    }
}

/// A request for JSON at `url`.
fn request(url: &str) -> Request {
    Request::get(url).header("Accept", "application/json")
}

/// The servers a listing read, and what it could not.
#[derive(Debug)]
pub struct Listing {
    /// Each server version read, in the order the registry listed them.
    pub servers: Vec<ServerVersion>,
    /// Each server that could not be read, said beside the rest.
    pub failures: Vec<Unreadable>,
    /// [`HttpError::TooManyPages`] when the listing went on past the
    /// bound: `servers` is all of it before that.
    pub cut: Option<HttpError>,
    /// What went wrong with the cache on the way, which did not stop it.
    pub warnings: Vec<HttpError>,
}

/// By hand, since what it could not read and what went wrong are errors,
/// which go out as their code and their message ([`Said`]), each under the
/// name its field has.
impl Serialize for Listing {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let said = |errors: &[HttpError]| -> Vec<Said> {
            errors
                .iter()
                .map(|error| Said::of(error.code(), error))
                .collect()
        };
        let mut listing = serializer.serialize_struct("Listing", 4)?;
        listing.serialize_field("servers", &self.servers)?;
        listing.serialize_field(
            "failures",
            &self
                .failures
                .iter()
                .map(|failure| Said::of(failure.code(), failure))
                .collect::<Vec<_>>(),
        )?;
        listing.serialize_field(
            "cut",
            &self.cut.as_ref().map(|error| Said::of(error.code(), error)),
        )?;
        listing.serialize_field("warnings", &said(&self.warnings))?;
        listing.end()
    }
}

/// One server version fetched by name, and what went wrong with the cache
/// on the way.
#[derive(Debug)]
pub struct Found {
    pub server: ServerVersion,
    pub warnings: Vec<HttpError>,
}

/// One version of one server, as the registry listed it.
///
/// What a listing shows of it, each string printable, and beside it, kept
/// for the import and never serialized, the `server.json` it was read from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerVersion {
    /// What the registry names it, `io.github.owner/server`: a name no agent
    /// keys a server by, and what the registry is asked for it by.
    pub name: String,
    /// The version, as its publisher wrote it.
    pub version: String,
    /// What a person calls it, when its publisher said.
    pub title: Option<String>,
    /// What it is for.
    pub description: String,
    /// What the registry says of it, when it says.
    pub status: Option<Status>,
    /// When the registry took it, as the registry wrote the time.
    pub published_at: Option<String>,
    /// When the registry last changed what it says of it.
    pub updated_at: Option<String>,
    /// Whether it is the server's latest version, when the registry says.
    pub is_latest: Option<bool>,
    /// The registry it was read from.
    pub registry: Redacted,
    /// When it was read, seconds since the epoch.
    pub fetched_at: i64,
    #[serde(skip)]
    pub(crate) server: server::Server,
}

impl ServerVersion {
    /// One server version, as the API answers with it, read from `base` at
    /// `fetched_at`: what is wrong with it otherwise.
    fn read(value: serde_json::Value, base: &str, fetched_at: i64) -> Result<Self, String> {
        let response: server::Response =
            serde_json::from_value(value).map_err(|error| printable(&error.to_string()))?;
        let server = response.server;
        // A name or a version shown as other than it is would be one a
        // front end names, and the registry is asked for, in vain.
        for (key, text) in [("name", &server.name), ("version", &server.version)] {
            if text.is_empty() {
                return Err(format!("its {key} is empty"));
            }
            if printable(text) != *text {
                return Err(format!(
                    "its {key} has a control character or a space at either end, which no \
                     list shows as it is"
                ));
            }
        }
        let official: server::Official = response
            .meta
            .as_ref()
            .and_then(|meta| meta.get(server::OFFICIAL))
            .map(|official| serde_json::from_value(official.clone()))
            .transpose()
            .map_err(|error| {
                format!(
                    "what the registry says of it: {}",
                    printable(&error.to_string())
                )
            })?
            .unwrap_or_default();
        let shown = |text: &Option<String>| text.as_deref().map(printable);
        Ok(ServerVersion {
            name: server.name.clone(),
            version: server.version.clone(),
            title: shown(&server.title),
            description: shown(&server.description).unwrap_or_default(),
            status: official
                .status
                .as_deref()
                .or(server.status.as_deref())
                .map(Status::read),
            published_at: shown(&official.published_at),
            updated_at: shown(&official.updated_at),
            is_latest: official.is_latest,
            registry: Redacted::new(base),
            fetched_at,
            server,
        })
    }
}

/// What the registry says of a server version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Listed, and nothing said against it.
    Active,
    /// Still listed, and not to be used for anything new.
    Deprecated,
    /// Taken away: listed only to a refresh, so what kept it knows.
    Deleted,
    /// A word this build does not know, as the registry said it.
    Other(String),
}

impl Status {
    fn read(word: &str) -> Self {
        match word {
            "active" => Status::Active,
            "deprecated" => Status::Deprecated,
            "deleted" => Status::Deleted,
            other => Status::Other(printable(other)),
        }
    }

    /// The word the registry says it as.
    pub fn slug(&self) -> &str {
        match self {
            Status::Active => "active",
            Status::Deprecated => "deprecated",
            Status::Deleted => "deleted",
            Status::Other(word) => word,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// By the word the registry says it as, rather than derived: a derive
/// spells the variant, and what a front end shows is the registry's word.
impl Serialize for Status {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

/// A server of a listing that could not be read, said beside the rest.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("servers[{at}]{}: {reason}, so it is not listed", .name.as_deref().map(|name| format!(" (`{name}`)")).unwrap_or_default())]
pub struct Unreadable {
    /// Where it is in the listing, from 0, across its pages.
    pub at: usize,
    /// Its name, when it has one to say, printable.
    pub name: Option<String>,
    /// Why, printable.
    pub reason: String,
}

codes!(Unreadable => "registry_server_unreadable");

#[cfg(test)]
#[path = "registry_test.rs"]
mod tests;
