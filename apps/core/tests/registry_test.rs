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
use mindweaver_core::mcp::{Input, InputKind, Transport, Upstream};
use mindweaver_core::mother::McpMother;
use mindweaver_core::registry::{Imported, Registry, ServerVersion, Status, Warning, OFFICIAL};
use mindweaver_core::value::{InputName, Redacted, VariableName};
use mindweaver_core::{Clock, Http, McpDefinition};
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

// ---------------------------------------------------------------------------
// A server version, mapped to a definition
// ---------------------------------------------------------------------------

/// Every server the 2025-09-29 search lists, over its two pages.
fn listed_2025_09_29() -> Vec<ServerVersion> {
    let second = format!(
        "{}&cursor=io.github.acme%2Fdotnet-weather%3A2.1.0",
        searched("acme")
    );
    let fake = Fake::new()
        .answering(&searched("acme"), &fixture("2025-09-29", "search-1.json"))
        .answering(&second, &fixture("2025-09-29", "search-2.json"));
    let client = Client::new(&fake);
    Registry::new(&client)
        .at(REGISTRY)
        .stamped_by(clock())
        .search("acme")
        .unwrap()
        .servers
}

/// Every server the 2025-12-11 search lists.
fn listed_2025_12_11() -> Vec<ServerVersion> {
    let fake = Fake::new().answering(&searched("acme"), &fixture("2025-12-11", "search.json"));
    let client = Client::new(&fake);
    Registry::new(&client)
        .at(REGISTRY)
        .stamped_by(clock())
        .search("acme")
        .unwrap()
        .servers
}

fn named(servers: &[ServerVersion], name: &str) -> ServerVersion {
    servers
        .iter()
        .find(|server| server.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name}"))
}

/// The definition `imported` holds, held to what every definition the
/// mapping makes is: its text reads back as it, and `validate` finds
/// nothing wrong with it.
fn held(imported: &Imported) -> McpDefinition {
    let definition = imported
        .definition
        .clone()
        .unwrap_or_else(|| panic!("no definition: {:?}", imported.manual));
    assert_eq!(imported.manual, []);
    let text = definition.text();
    let read = McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    assert_eq!(read, definition, "{text}");
    assert_eq!(read.issues(), [], "{text}");
    definition
}

fn program(definition: &McpDefinition) -> (&str, Vec<&str>, Vec<(&str, &str)>) {
    let Transport::Stdio(stdio) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    (
        stdio.command.as_str(),
        stdio.args.iter().map(String::as_str).collect(),
        stdio
            .env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect(),
    )
}

fn address(definition: &McpDefinition) -> (&str, Vec<(&str, &str)>) {
    let (Transport::Http(endpoint) | Transport::Sse(endpoint)) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    (
        endpoint.url.as_str(),
        endpoint
            .headers
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect(),
    )
}

fn input(name: &str, kind: InputKind, variable: &str) -> Input {
    Input {
        name: InputName::from(name),
        kind,
        description: None,
        required: false,
        secret: false,
        env_var: Some(VariableName::from(variable)),
    }
}

fn codes<'a>(warnings: impl IntoIterator<Item = &'a Warning>) -> Vec<(&'static str, String)> {
    warnings
        .into_iter()
        .map(|warning| (warning.code(), warning.to_string()))
        .collect()
}

/// npm, at the version the registry gives: `npx -y id@version`, its
/// arguments after it, every environment variable an input read from a
/// variable of its name, and what is left out said.
#[test]
fn an_npm_package_is_started_with_npx_at_its_version() {
    let weather = named(&listed_2025_09_29(), "io.github.acme/weather");

    let imported = weather.import(None);

    let definition = held(&imported);
    assert_eq!(definition.name, "weather");
    assert_eq!(definition.description, "Forecasts and alerts for any city");
    assert_eq!(
        program(&definition),
        (
            "npx",
            vec![
                "-y",
                "@acme/weather-mcp@1.0.2",
                "--units",
                "metric",
                "{city}"
            ],
            vec![
                ("WEATHER_API_KEY", "{WEATHER_API_KEY}"),
                ("WEATHER_ENDPOINT", "https://{region}.api.acme.example"),
                ("WEATHER_LOG_LEVEL", "{WEATHER_LOG_LEVEL}"),
            ]
        )
    );
    assert_eq!(
        definition.inputs,
        [
            Input {
                description: Some(String::from("The key the forecasts are fetched with")),
                required: true,
                secret: true,
                ..input("WEATHER_API_KEY", InputKind::Env, "WEATHER_API_KEY")
            },
            Input {
                description: Some(String::from("How much it logs")),
                ..input("WEATHER_LOG_LEVEL", InputKind::Env, "WEATHER_LOG_LEVEL")
            },
            Input {
                description: Some(String::from("The region closest to you")),
                required: true,
                ..input("region", InputKind::Env, "REGION")
            },
            Input {
                description: Some(String::from("The city forecasts are for")),
                required: true,
                ..input("city", InputKind::Arg, "CITY")
            },
        ]
    );
    assert_eq!(
        codes(&imported.warnings),
        [
            (
                "registry_secret_dropped",
                String::from(
                    "packages[0].environmentVariables[0]: the registry gives a default for a \
                     secret, which is left out: its value is read from the environment variable \
                     its input names"
                )
            ),
            (
                "registry_secret_dropped",
                String::from(
                    "packages[0].environmentVariables[0]: the registry gives a placeholder for a \
                     secret, which is left out: its value is read from the environment variable \
                     its input names"
                )
            ),
            (
                "registry_argument_left_out",
                String::from(
                    "packages[0].packageArguments[2]: an optional argument with no value, left \
                     out: add it to `args` by hand if the server is wanted with it"
                )
            ),
        ]
    );
    assert_eq!(
        definition.provenance,
        Some(Upstream {
            registry: Some(Redacted::new(REGISTRY)),
            upstream_name: Some(String::from("io.github.acme/weather")),
            version: Some(String::from("1.0.2")),
            fetched_at: Some(String::from("2026-11-07T09:30:00Z")),
            status: Some(String::from("active")),
        })
    );
}

#[test]
fn an_npm_package_with_no_version_is_started_with_npx_as_it_is_named() {
    let notes = named(&listed_2025_12_11(), "io.github.acme/notes");

    let definition = held(&notes.import(None));

    assert_eq!(definition.name, "notes");
    assert_eq!(
        program(&definition),
        ("npx", vec!["-y", "@acme/notes-mcp"], vec![])
    );
    assert_eq!(definition.inputs, []);
}

#[test]
fn a_pypi_package_is_started_with_uvx_at_its_version_or_as_it_is_named() {
    let pinned = held(&named(&listed_2025_09_29(), "io.github.acme/pyweather").import(None));
    let unpinned = held(&named(&listed_2025_12_11(), "io.github.acme/pynotes").import(None));

    assert_eq!(
        program(&pinned),
        (
            "uvx",
            vec!["acme-weather-mcp==0.4.1"],
            vec![("WEATHER_API_KEY", "{WEATHER_API_KEY}")]
        )
    );
    assert!(pinned.inputs[0].secret);
    assert_eq!(program(&unpinned), ("uvx", vec!["acme-notes"], vec![]));
}

#[test]
fn a_nuget_package_is_started_with_dnx_at_its_version_or_as_it_is_named() {
    let pinned = held(&named(&listed_2025_09_29(), "io.github.acme/dotnet-weather").import(None));
    let unpinned = held(&named(&listed_2025_12_11(), "io.github.acme/notes-dotnet").import(None));

    assert_eq!(
        program(&pinned),
        ("dnx", vec!["Acme.Weather.Mcp@2.1.0"], vec![])
    );
    assert_eq!(program(&unpinned), ("dnx", vec!["Acme.Notes.Mcp"], vec![]));
}

/// The 2025-12-11 schema names an image whole, its registry and its tag;
/// the 2025-09-29 one named its registry and its tag apart, put together
/// here, since one left out would pull an image of the same name from
/// Docker Hub. A container is passed each variable by its name alone.
#[test]
fn an_oci_image_is_run_with_docker_as_its_reference_names_it() {
    let whole = held(&named(&listed_2025_12_11(), "io.github.acme/notes-image").import(None));
    let apart = held(&named(&listed_2025_09_29(), "io.github.acme/weather-docker").import(None));

    assert_eq!(
        program(&whole),
        (
            "docker",
            vec![
                "run",
                "-i",
                "--rm",
                "-e",
                "NOTES_TOKEN",
                "--mount",
                "type=bind,src={notes_dir},dst=/notes",
                "ghcr.io/acme/notes-mcp:2.0.0",
            ],
            vec![("NOTES_TOKEN", "{NOTES_TOKEN}")]
        )
    );
    assert_eq!(
        whole.inputs,
        [
            Input {
                description: Some(String::from("The token your notes are read with")),
                required: true,
                secret: true,
                ..input("NOTES_TOKEN", InputKind::Env, "NOTES_TOKEN")
            },
            Input {
                description: Some(String::from("The folder your notes are in")),
                required: true,
                ..input("notes_dir", InputKind::Arg, "NOTES_DIR")
            },
        ]
    );
    assert_eq!(
        program(&apart),
        (
            "docker",
            vec![
                "run",
                "-i",
                "--rm",
                "-e",
                "WEATHER_API_KEY",
                "docker.io/acme/weather-mcp:1.0.2",
            ],
            vec![("WEATHER_API_KEY", "{WEATHER_API_KEY}")]
        )
    );
}

/// A header the registry writes a variable into is that variable an input
/// where it stands; one it gives no value for is an input whole; each a
/// secret when the registry says so.
#[test]
fn a_remotes_headers_are_inputs_and_its_secrets_stay_secrets() {
    let remote = named(&listed_2025_09_29(), "io.github.acme/weather-remote");

    let imported = remote.import(None);

    let definition = held(&imported);
    assert_eq!(definition.transport.slug(), "http");
    assert_eq!(
        address(&definition),
        (
            "https://mcp.acme.example/weather",
            vec![
                ("Authorization", "Bearer {api_key}"),
                ("X-Acme-Region", "{X-Acme-Region}"),
            ]
        )
    );
    assert_eq!(
        definition.inputs,
        [
            Input {
                description: Some(String::from("Your Acme key")),
                required: true,
                secret: true,
                ..input("api_key", InputKind::Header, "API_KEY")
            },
            Input {
                description: Some(String::from("The region closest to you")),
                ..input("X-Acme-Region", InputKind::Header, "X_ACME_REGION")
            },
        ]
    );
    assert_eq!(
        codes(&imported.warnings),
        [(
            "registry_secret_dropped",
            String::from(
                "remotes[0].headers[0].variables.api_key: the registry gives a placeholder for a \
                 secret, which is left out: its value is read from the environment variable its \
                 input names"
            )
        )]
    );
}

/// A remote's URL variables (2025-12-11) are inputs of the URL, one with a
/// fixed value written as it, and a secret header's default is left out.
#[test]
fn a_remotes_url_variables_are_inputs_of_its_url() {
    let cloud = named(&listed_2025_12_11(), "io.github.acme/notes-cloud");

    let imported = cloud.import(None);

    let definition = held(&imported);
    assert_eq!(
        address(&definition),
        (
            "https://{tenant}.notes.acme.example/mcp/v2",
            vec![("X-Notes-Key", "{X-Notes-Key}")]
        )
    );
    assert_eq!(
        definition.inputs,
        [
            Input {
                description: Some(String::from("Your workspace at Acme")),
                required: true,
                ..input("tenant", InputKind::UrlVar, "TENANT")
            },
            Input {
                description: Some(String::from("Your notes key")),
                required: true,
                secret: true,
                ..input("X-Notes-Key", InputKind::Header, "X_NOTES_KEY")
            },
        ]
    );
    assert_eq!(
        imported.warnings,
        [Warning::SecretDropped {
            at: String::from("remotes[0].headers[0]"),
            what: "default",
        }]
    );
}

#[test]
fn a_remote_over_server_sent_events_is_mapped_and_warned_of() {
    let events = named(&listed_2025_09_29(), "io.github.acme/weather-events");

    let imported = events.import(None);

    let definition = held(&imported);
    assert_eq!(definition.transport.slug(), "sse");
    assert_eq!(
        address(&definition),
        ("https://mcp.acme.example/weather/sse", vec![])
    );
    assert_eq!(
        codes(&imported.warnings),
        [(
            "registry_remote_sse",
            String::from(
                "remotes[0]: reached over server-sent events, which the MCP specification has \
                 replaced with streamable HTTP, and which Codex and OpenCode are not given"
            )
        )]
    );
}

/// What a server maps to when it maps to no definition: none, and each
/// step by its code and what it says.
type Stepped = (Option<McpDefinition>, Vec<(&'static str, String)>);

/// What nothing here starts is a manual step, coded, and no definition.
#[test]
fn an_mcp_bundle_a_crate_and_an_unknown_type_are_manual_steps() {
    let servers = listed_2025_12_11();

    let steps: Vec<Stepped> = [
        "io.github.acme/notes-bundle",
        "io.github.acme/notes-crate",
        "io.github.acme/notes-brew",
    ]
    .iter()
    .map(|name| {
        let imported = named(&servers, name).import(None);
        let said = imported
            .manual
            .iter()
            .map(|step| (step.code(), step.to_string()))
            .collect();
        (imported.definition, said)
    })
    .collect();

    assert_eq!(
        steps,
        [
            (
                None,
                vec![(
                    "registry_package_mcpb",
                    String::from(
                        "packages[0]: an MCP bundle at \
                         https://downloads.acme.example/notes-2.0.0.mcpb, which no agent is given \
                         as a command: install it by hand into an agent that takes bundles"
                    )
                )]
            ),
            (
                None,
                vec![(
                    "registry_package_cargo",
                    String::from(
                        "packages[0]: the Rust crate `acme-notes-mcp`, which nothing here \
                         installs: install it with cargo, then add the server it starts with \
                         `add mcp --command`"
                    )
                )]
            ),
            (
                None,
                vec![(
                    "registry_package_type_unknown",
                    String::from(
                        "packages[0]: a `brew` package (`acme/tap/notes-mcp`), which this build \
                         does not know how to start: start it by hand"
                    )
                )]
            ),
        ]
    );
}

/// A refresh lists what the registry deprecated and deleted, and a
/// definition imported from one carries what it said.
#[test]
fn what_a_refresh_says_was_deprecated_or_deleted_is_carried_into_the_definition() {
    let since = format!("{REGISTRY}/v0.1/servers?updated_since=2025-11-01T00%3A00%3A00Z&limit=100");
    let fake = Fake::new().answering(&since, &fixture("2025-09-29", "updated-since.json"));
    let client = Client::new(&fake);
    let listing = Registry::new(&client)
        .at(REGISTRY)
        .stamped_by(clock())
        .updated_since(1_761_955_200)
        .unwrap();

    let carried: Vec<(String, Option<String>, Option<String>)> = listing
        .servers
        .iter()
        .map(|server| {
            let upstream = held(&server.import(None)).provenance.unwrap();
            (server.name.clone(), upstream.version, upstream.status)
        })
        .collect();

    assert_eq!(
        carried,
        [
            (
                String::from("io.github.acme/weather"),
                Some(String::from("1.0.1")),
                Some(String::from("deprecated"))
            ),
            (
                String::from("io.github.acme/forecast-old"),
                Some(String::from("0.3.0")),
                Some(String::from("deleted"))
            ),
        ]
    );
}

/// Every value the fixtures give for a secret, a default, an example or a
/// fixed value, which no definition may hold.
const SECRET_VALUES: [&str; 4] = [
    "change-me-in-production",
    "your-weather-key-here",
    "acme-key-goes-here",
    "notes-key-that-ships-with-the-docs",
];

/// Every server of every fixture that maps to a definition maps to one
/// that reads back as itself and that `validate` finds nothing wrong
/// with, and none holds a value the registry gave for a secret.
#[test]
fn no_definition_holds_a_value_given_for_a_secret() {
    let servers: Vec<ServerVersion> = listed_2025_09_29()
        .into_iter()
        .chain(listed_2025_12_11())
        .collect();

    let mut mapped = 0;
    for server in &servers {
        let imported = server.import(None);
        let said = serde_json::to_string(&imported).unwrap();
        for secret in SECRET_VALUES {
            assert!(!said.contains(secret), "{}: {said}", server.name);
        }
        if imported.definition.is_some() {
            held(&imported);
            mapped += 1;
        }
    }
    assert_eq!(mapped, servers.len() - 3, "all but the three manual steps");
}

/// A token is put together at run time ([`McpMother::tokens`]), so no file
/// holds one: written as a secret's value, it is left out and said; written
/// as a value that is no secret, what it maps to is no definition, said by
/// where the token is and what it starts with, never whole.
#[test]
fn a_token_the_registry_gives_is_never_written_nor_said_whole() {
    for (prefix, token) in McpMother::tokens() {
        let page = format!(
            r#"{{"servers":[
                {{"server":{{"name":"io.github.acme/secret","description":"Given a secret","version":"1",
                  "remotes":[{{"type":"streamable-http","url":"https://mcp.acme.example",
                    "headers":[{{"name":"X-Key","value":"{token}","isSecret":true}}]}}]}}}},
                {{"server":{{"name":"io.github.acme/plain","description":"Given a token as text","version":"1",
                  "remotes":[{{"type":"streamable-http","url":"https://mcp.acme.example",
                    "headers":[{{"name":"X-Key","value":"{token}"}}]}}]}}}}
            ]}}"#
        );
        let fake = Fake::new().answering(&searched("acme"), &page);
        let client = Client::new(&fake);
        let servers = Registry::new(&client)
            .at(REGISTRY)
            .search("acme")
            .unwrap()
            .servers;

        let secret = servers[0].import(None);
        let plain = servers[1].import(None);

        let definition = held(&secret);
        assert_eq!(address(&definition).1, [("X-Key", "{X-Key}")], "{prefix}");
        assert_eq!(
            secret.warnings,
            [Warning::SecretDropped {
                at: String::from("remotes[0].headers[0]"),
                what: "value",
            }]
        );
        assert_eq!(plain.definition, None);
        assert_eq!(plain.manual.len(), 1);
        assert_eq!(plain.manual[0].code(), "registry_definition_invalid");
        let said = plain.manual[0].to_string();
        assert!(said.contains("transport.headers.X-Key"), "{said}");
        assert!(said.contains(prefix), "{said}");
        for imported in [&secret, &plain] {
            let json = serde_json::to_string(imported).unwrap();
            assert!(!json.contains(&token), "{json}");
        }
    }
}
