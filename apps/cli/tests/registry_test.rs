//! The MCP Registry at the command line (#107): `gather mcp search`, read
//! from a registry this process serves on 127.0.0.1 with the fixtures
//! core's suite reads ([`FixtureRegistry`]), named with `--registry`, so
//! nothing reaches the network and no test names the official registry.

mod common;

use common::*;

/// A workspace of its own, made, with nothing in its library.
fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    dir
}

/// `mindweaver gather mcp <args>` in `dir`, reading `registry`.
fn mcp(dir: &Path, registry: &FixtureRegistry, args: &[&str]) -> Result<Outcome, Failure> {
    let mut line = vec!["gather", "mcp"];
    line.extend_from_slice(args);
    line.extend(["--registry", registry.url.as_str()]);
    weaver(dir, &line)
}

/// What every `--json` of a search prints as its data, read back field by
/// field: a name renamed or added fails here before a script reading it.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    servers: Vec<Server>,
    failures: Vec<Said>,
    cut: Option<Said>,
    warnings: Vec<Said>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Server {
    name: String,
    version: String,
    title: Option<String>,
    description: String,
    status: Option<String>,
    published_at: Option<String>,
    updated_at: Option<String>,
    is_latest: Option<bool>,
    registry: String,
    fetched_at: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    code: String,
    message: String,
}

/// Every server the search found, a row each in the registry's order, by
/// the name `gather mcp add` takes, its version, its status and what it is
/// for; one that could not be read a warning with its code, which fails
/// the search as a failure collected does.
#[test]
fn a_search_lists_each_server_by_its_name_version_status_and_description() {
    let dir = workspace();
    let registry = FixtureRegistry::start();

    let outcome = mcp(dir.path(), &registry, &["search", "acme"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "io.github.acme/weather         1.0.2  active  Forecasts and alerts for any city\n\
         io.github.acme/pyweather       0.4.1  active  Forecasts and alerts, in Python\n\
         io.github.acme/dotnet-weather  2.1.0  active  Forecasts and alerts, in .NET\n\
         io.github.acme/weather-docker  1.0.2  active  Forecasts and alerts, in a container\n\
         io.github.acme/weather-remote  3.0.0  active  Forecasts and alerts, hosted by Acme\n\
         io.github.acme/weather-events  0.9.0  active  Forecasts and alerts, over server-sent events\n"
    );
    assert_eq!(
        outcome.stderr,
        ["warning: servers[2] (`io.github.acme/broken`): missing field `version`, so it is not listed"]
    );
    assert_eq!(
        stderr_codes(&outcome),
        [Some(String::from("registry_server_unreadable"))]
    );
    assert!(!outcome.ok);
    assert_eq!(
        registry.asked(),
        [
            "/v0.1/servers?search=acme&version=latest&limit=100",
            "/v0.1/servers?search=acme&version=latest&limit=100\
             &cursor=io.github.acme%2Fdotnet-weather%3A2.1.0",
        ]
    );
}

/// Its JSON is core's listing in the envelope, each name a contract, what
/// it could not read said beside the rest as the text says it.
#[test]
fn a_search_as_json_is_the_listing_in_the_envelope() {
    let dir = workspace();
    let registry = FixtureRegistry::start();

    let outcome = mcp(dir.path(), &registry, &["search", "notes", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&outcome);
    let names: Vec<&str> = listing
        .servers
        .iter()
        .map(|server| server.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "io.github.acme/notes",
            "io.github.acme/pynotes",
            "io.github.acme/notes-image",
            "io.github.acme/notes-dotnet",
            "io.github.acme/notes-cloud",
            "io.github.acme/notes-bundle",
            "io.github.acme/notes-crate",
            "io.github.acme/notes-brew",
        ]
    );
    let notes = &listing.servers[0];
    assert_eq!(notes.version, "2.0.0");
    assert_eq!(notes.title.as_deref(), Some("Notes"));
    assert_eq!(notes.description, "Your notes, searched and written");
    assert_eq!(notes.status.as_deref(), Some("active"));
    assert_eq!(notes.published_at.as_deref(), Some("2025-12-15T09:00:00Z"));
    assert_eq!(notes.updated_at.as_deref(), Some("2025-12-15T09:00:00Z"));
    assert_eq!(notes.is_latest, Some(true));
    assert_eq!(notes.registry, registry.url);
    assert!(notes.fetched_at > 0);
    assert_eq!(listing.failures.len(), 1);
    assert_eq!(listing.failures[0].code, "registry_server_unreadable");
    assert!(listing.cut.is_none());
    assert!(listing.warnings.is_empty());
    assert_eq!(warnings, [listing.failures[0].message.clone()]);
    assert!(!outcome.ok);
}

/// What the registry answers is kept in the workspace's cache, to be asked
/// whether it changed next time.
#[test]
fn a_search_keeps_what_it_was_answered_in_the_workspaces_cache() {
    let dir = workspace();
    let registry = FixtureRegistry::start();

    mcp(dir.path(), &registry, &["search", "notes"]).unwrap();

    let cache = dir.path().join(WEAVER_DIR).join("cache/http");
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 1);
}

#[test]
fn a_search_that_finds_nothing_says_so() {
    let dir = workspace();
    let registry = FixtureRegistry::serving(
        [(
            String::from("/v0.1/servers?search=nothing&version=latest&limit=100"),
            String::from(r#"{"servers": [], "metadata": {"count": 0}}"#),
        )]
        .into(),
    );

    let outcome = mcp(dir.path(), &registry, &["search", "nothing"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "the registry lists no server for `nothing`\n"
    );
    assert!(outcome.ok);
}

/// A registry is read over https, or http on this machine, and never with
/// a credential in its address, which is said without it: nothing is sent.
#[test]
fn a_registry_the_client_would_not_fetch_is_refused_before_anything_is_sent() {
    let dir = workspace();
    for (registry, code) in [
        ("http://registry.example", "http_url_refused"),
        (
            "https://user:hunter2@registry.example",
            "http_credentials_in_url",
        ),
    ] {
        let failure = weaver(
            dir.path(),
            &["gather", "mcp", "search", "acme", "--registry", registry],
        )
        .unwrap_err();

        assert_eq!(failure.error.code(), code, "{registry}");
        let said = failure.error.to_string();
        assert!(!said.contains("hunter2"), "{said}");
    }
}
