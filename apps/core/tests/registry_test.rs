//! The MCP Registry, read through a transport kept in memory: nothing
//! reaches the network, and no test names the real registry but to hold
//! that it is the one asked by default, which the fake answers itself.
//!
//! Every answer is a fixture under `tests/fixtures/registry/`, by the date
//! of the `server.json` schema it is shaped on, 2025-09-29 and 2025-12-11.
//! They are written by hand to those schemas and to the `/v0.1` API's
//! wrapper (`{"servers": [{"server": ..., "_meta": ...}], "metadata":
//! ...}`), not recorded from the registry: what a real answer holds beside
//! them is the owner's to record later. Each holds keys nothing here reads
//! (`websiteUrl`, `repository`, `icons`, a publisher's `_meta`), so that
//! reading past them is held too.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mindweaver_core::http::{Answer, Cache, Client, Headers, HttpError, Request};
use mindweaver_core::registry::{Registry, ServerVersion, Status, OFFICIAL};
use mindweaver_core::{Clock, Http};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// The fake, and the fixtures it answers with
// ---------------------------------------------------------------------------

/// What the fake answers one URL with: a status, headers and a body.
type Reply = (u16, Vec<(String, String)>, String);

/// A transport kept in memory: each URL answered with what it was given,
/// and a 404 for any other.
#[derive(Debug, Default)]
struct Fake {
    answers: BTreeMap<String, Reply>,
    sent: Mutex<Vec<String>>,
}

impl Fake {
    fn new() -> Self {
        Self::default()
    }

    fn answering(self, url: &str, body: &str) -> Self {
        self.answering_with(url, 200, &[], body)
    }

    fn answering_with(
        mut self,
        url: &str,
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Self {
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.answers
            .insert(url.to_owned(), (status, headers, body.to_owned()));
        self
    }

    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
}

impl Http for Fake {
    fn send(&self, request: &Request, _: Duration) -> Result<Answer, HttpError> {
        self.sent.lock().unwrap().push(request.url().to_owned());
        let (status, headers, body) = self
            .answers
            .get(request.url())
            .cloned()
            .unwrap_or_else(|| (404, Vec::new(), String::from("not here")));
        let mut answered = Headers::new();
        for (name, value) in headers {
            answered.push(name, value);
        }
        Ok(Answer {
            status,
            headers: answered,
            body: Box::new(io::Cursor::new(body.into_bytes())),
        })
    }
}

/// A clock that says what it was set to.
#[derive(Debug)]
struct Set(AtomicI64);

impl Clock for Set {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// When every fixture is fetched: 2026-11-07T09:30:00Z.
const NOW: i64 = 1_794_043_800;

fn clock() -> Arc<Set> {
    Arc::new(Set(AtomicI64::new(NOW)))
}

/// What the fixture `schema/name` holds.
fn fixture(schema: &str, name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/registry")
        .join(schema)
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

const REGISTRY: &str = "https://registry.example";

/// The first page a search for `query` asks for.
fn searched(query: &str) -> String {
    format!("{REGISTRY}/v0.1/servers?search={query}&version=latest&limit=100")
}

fn names(servers: &[ServerVersion]) -> Vec<&str> {
    servers.iter().map(|server| server.name.as_str()).collect()
}

// ---------------------------------------------------------------------------
// Searching
// ---------------------------------------------------------------------------

/// The registry pages by a cursor in each page's body, and the listing reads
/// it to the page that carries none; a server that does not read is said
/// beside the rest, by where it was listed.
#[test]
fn a_search_reads_every_page_and_says_a_server_it_cannot_read_beside_the_rest() {
    let second = format!(
        "{}&cursor=io.github.acme%2Fdotnet-weather%3A2.1.0",
        searched("acme")
    );
    let fake = Fake::new()
        .answering(&searched("acme"), &fixture("2025-09-29", "search-1.json"))
        .answering(&second, &fixture("2025-09-29", "search-2.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client)
        .at(REGISTRY)
        .stamped_by(clock())
        .search("acme")
        .unwrap();

    assert_eq!(
        names(&listing.servers),
        [
            "io.github.acme/weather",
            "io.github.acme/pyweather",
            "io.github.acme/dotnet-weather",
            "io.github.acme/weather-docker",
            "io.github.acme/weather-remote",
            "io.github.acme/weather-events",
        ]
    );
    assert_eq!(listing.failures.len(), 1);
    assert_eq!(listing.failures[0].code(), "registry_server_unreadable");
    assert_eq!(
        listing.failures[0].to_string(),
        "servers[2] (`io.github.acme/broken`): missing field `version`, so it is not listed"
    );
    assert!(listing.cut.is_none());
    assert_eq!(fake.sent(), [searched("acme"), second]);

    let weather = &listing.servers[0];
    assert_eq!(weather.version, "1.0.2");
    assert_eq!(weather.title.as_deref(), Some("Weather"));
    assert_eq!(weather.description, "Forecasts and alerts for any city");
    assert_eq!(weather.status, Some(Status::Active));
    assert_eq!(
        weather.published_at.as_deref(),
        Some("2025-10-01T12:00:00Z")
    );
    assert_eq!(weather.is_latest, Some(true));
    assert_eq!(weather.registry, REGISTRY);
    assert_eq!(weather.fetched_at, NOW);
}

#[test]
fn the_official_registry_is_asked_unless_another_is_named() {
    let official = format!("{OFFICIAL}/v0.1/servers?search=acme&version=latest&limit=100");
    let fake = Fake::new().answering(&official, &fixture("2025-12-11", "search.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client).search("acme").unwrap();

    assert_eq!(fake.sent(), [official]);
    assert_eq!(listing.servers[0].registry, OFFICIAL);
}

/// A schema's later keys (`icons`, a `variables` table on a remote) are
/// read past by what does not need them, and a name that would rewrite the
/// line above it on a terminal is no server's.
#[test]
fn a_later_schemas_servers_are_read_as_the_earlier_ones_are() {
    let fake = Fake::new().answering(&searched("notes"), &fixture("2025-12-11", "search.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client).at(REGISTRY).search("notes").unwrap();

    assert_eq!(listing.servers.len(), 8);
    assert_eq!(listing.failures.len(), 1);
    assert_eq!(listing.failures[0].at, 8);
    let said = listing.failures[0].to_string();
    assert!(!said.contains(['\r', '\n']), "{said}");
    assert!(
        said.contains("its name has a control character or a space at either end"),
        "{said}"
    );
}

#[test]
fn an_empty_query_asks_for_every_servers_latest_version() {
    let every = format!("{REGISTRY}/v0.1/servers?version=latest&limit=100");
    let fake = Fake::new().answering(&every, &fixture("2025-12-11", "search.json"));
    let client = Client::new(&fake);

    Registry::new(&client).at(REGISTRY).search("").unwrap();

    assert_eq!(fake.sent(), [every]);
}

/// What was read before the bound is kept, and the cut said beside it.
#[test]
fn a_search_is_read_no_further_than_its_bound() {
    let fake = Fake::new().answering(&searched("acme"), &fixture("2025-09-29", "search-1.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client)
        .at(REGISTRY)
        .max_pages(1)
        .search("acme")
        .unwrap();

    assert_eq!(listing.servers.len(), 3);
    assert_eq!(listing.cut.unwrap().code(), "http_too_many_pages");
    assert_eq!(fake.sent().len(), 1);
}

/// A cursor that never runs out, as one naming the page it is on does,
/// ends at the bound rather than never.
#[test]
fn a_cursor_that_never_runs_out_ends_at_the_bound() {
    let again = format!("{}&cursor=again", searched("loop"));
    let page = r#"{"servers":[{"server":{"name":"io.github.acme/loop","version":"1"}}],"metadata":{"nextCursor":"again"}}"#;
    let fake = Fake::new()
        .answering(&searched("loop"), page)
        .answering(&again, page);
    let client = Client::new(&fake);

    let listing = Registry::new(&client)
        .at(REGISTRY)
        .max_pages(4)
        .search("loop")
        .unwrap();

    assert_eq!(listing.servers.len(), 4);
    assert_eq!(listing.cut.unwrap().code(), "http_too_many_pages");
    assert_eq!(fake.sent().len(), 4);
}

/// A page that cannot be fetched fails the listing: what came before it is
/// a listing cut short where nobody chose.
#[test]
fn a_page_that_fails_fails_the_search() {
    let fake = Fake::new().answering(&searched("acme"), &fixture("2025-09-29", "search-1.json"));
    let client = Client::new(&fake);

    let failed = Registry::new(&client)
        .at(REGISTRY)
        .search("acme")
        .unwrap_err();

    assert_eq!(failed.error.code(), "http_status_unsuccessful");
}

/// Every page goes through the client, so one given the workspace's cache
/// keeps each answer and serves it again while it is fresh.
#[test]
fn a_search_is_kept_in_the_cache_the_client_is_given() {
    let folder = TempDir::new().unwrap();
    let fake = Fake::new().answering_with(
        &searched("notes"),
        200,
        &[("ETag", "\"notes-1\"")],
        &fixture("2025-12-11", "search.json"),
    );
    let cache = Cache::at(folder.path().join("http"))
        .fresh_for(Duration::from_secs(600))
        .stamped_by(clock());
    let client = Client::new(&fake).with_cache(cache);
    let registry = Registry::new(&client).at(REGISTRY);

    let first = registry.search("notes").unwrap();
    let again = registry.search("notes").unwrap();

    assert_eq!(
        fake.sent().len(),
        1,
        "the second search is served from the cache"
    );
    assert_eq!(names(&again.servers), names(&first.servers));
    assert!(again.warnings.is_empty());
}

// ---------------------------------------------------------------------------
// One version
// ---------------------------------------------------------------------------

#[test]
fn a_version_is_fetched_by_the_servers_name_as_one_segment() {
    let url = format!("{REGISTRY}/v0.1/servers/io.github.acme%2Fweather/versions/1.0.2");
    let fake = Fake::new().answering(&url, &fixture("2025-09-29", "version.json"));
    let client = Client::new(&fake);

    let found = Registry::new(&client)
        .at(REGISTRY)
        .stamped_by(clock())
        .version("io.github.acme/weather", "1.0.2")
        .unwrap();

    assert_eq!(fake.sent(), [url]);
    assert_eq!(found.server.name, "io.github.acme/weather");
    assert_eq!(found.server.version, "1.0.2");
    assert_eq!(found.server.fetched_at, NOW);
    assert!(found.warnings.is_empty());
}

#[test]
fn the_latest_version_is_asked_for_as_the_registry_names_it() {
    let url = format!("{REGISTRY}/v0.1/servers/io.github.acme%2Fweather/versions/latest");
    let fake = Fake::new().answering(&url, &fixture("2025-09-29", "version.json"));
    let client = Client::new(&fake);

    let found = Registry::new(&client)
        .at(REGISTRY)
        .version("io.github.acme/weather", "latest")
        .unwrap();

    assert_eq!(found.server.version, "1.0.2");
}

/// One server asked for by name that does not read is the answer failing,
/// said without the credential a string of it quotes.
#[test]
fn a_version_that_does_not_read_is_a_failure() {
    let url = format!("{REGISTRY}/v0.1/servers/io.github.acme%2Fbroken/versions/1");
    let fake = Fake::new().answering(
        &url,
        r#"{"server":{"name":"io.github.acme/broken","version":{"url":"https://u:p4ss@acme.example"}}}"#,
    );
    let client = Client::new(&fake);

    let failed = Registry::new(&client)
        .at(REGISTRY)
        .version("io.github.acme/broken", "1")
        .unwrap_err();

    assert_eq!(failed.error.code(), "http_json_invalid");
    assert!(
        !failed.error.to_string().contains("p4ss"),
        "{}",
        failed.error
    );
}

// ---------------------------------------------------------------------------
// What changed since
// ---------------------------------------------------------------------------

/// A refresh asks for what changed since a time, every version of it, and
/// is answered with what the registry deprecated and deleted among it.
#[test]
fn a_refresh_asks_for_what_changed_since_and_lists_what_was_deprecated_and_deleted() {
    let since = format!("{REGISTRY}/v0.1/servers?updated_since=2025-11-01T00%3A00%3A00Z&limit=100");
    let fake = Fake::new().answering(&since, &fixture("2025-09-29", "updated-since.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client)
        .at(REGISTRY)
        .updated_since(1_761_955_200)
        .unwrap();

    assert_eq!(fake.sent(), [since]);
    let said: Vec<(&str, &str, Option<&Status>)> = listing
        .servers
        .iter()
        .map(|server| {
            (
                server.name.as_str(),
                server.version.as_str(),
                server.status.as_ref(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("io.github.acme/weather", "1.0.1", Some(&Status::Deprecated)),
            (
                "io.github.acme/forecast-old",
                "0.3.0",
                Some(&Status::Deleted)
            ),
        ]
    );
    assert_eq!(listing.servers[0].is_latest, Some(false));
}

/// A refresh pages as a search does, and an empty cursor is its end.
#[test]
fn a_refresh_is_read_page_by_page_to_an_empty_cursor() {
    let since = format!("{REGISTRY}/v0.1/servers?updated_since=2026-01-01T00%3A00%3A00Z&limit=100");
    let second = format!("{since}&cursor=io.github.acme%2Fnotes%3A1.4.0");
    let fake = Fake::new()
        .answering(&since, &fixture("2025-12-11", "updated-since-1.json"))
        .answering(&second, &fixture("2025-12-11", "updated-since-2.json"));
    let client = Client::new(&fake);

    let listing = Registry::new(&client)
        .at(REGISTRY)
        .updated_since(1_767_225_600)
        .unwrap();

    assert_eq!(fake.sent(), [since, second]);
    let statuses: Vec<Option<&Status>> = listing
        .servers
        .iter()
        .map(|server| server.status.as_ref())
        .collect();
    assert_eq!(
        statuses,
        [Some(&Status::Deprecated), Some(&Status::Deleted)]
    );
}
