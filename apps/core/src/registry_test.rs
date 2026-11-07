//! Unit tests for `registry.rs`: which URL a registry is asked, how one
//! server version is read, and what a listing serializes to. A registry
//! read through a fake transport is `tests/registry_test.rs`'s.

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::http::Answer;
use crate::port::Http;

/// A transport nothing is sent through: these tests only build URLs.
#[derive(Debug)]
struct Nowhere;

impl Http for Nowhere {
    fn send(&self, request: &Request, _: Duration) -> Result<Answer, HttpError> {
        panic!("nothing is sent: {request:?}");
    }
}

fn url(base: &str, segments: &[&str], query: &[(&str, &str)]) -> String {
    let client = Client::new(&Nowhere);
    let registry = Registry::new(&client).at(base);
    let query: Vec<(&str, String)> = query
        .iter()
        .map(|(name, value)| (*name, (*value).to_owned()))
        .collect();
    registry.url(segments, &query).unwrap()
}

#[test]
fn the_official_registry_is_read_unless_another_is_named() {
    let client = Client::new(&Nowhere);

    assert_eq!(Registry::new(&client).base(), OFFICIAL);
    assert_eq!(
        Registry::new(&client).at("https://registry.example").base(),
        "https://registry.example"
    );
}

#[test]
fn the_api_is_asked_below_the_registrys_address_whatever_path_it_has() {
    for base in [
        "https://registry.example",
        "https://registry.example/",
        "https://registry.example/?stale=query#top",
    ] {
        assert_eq!(
            url(base, &["servers"], &[]),
            "https://registry.example/v0.1/servers",
            "{base}"
        );
    }
    assert_eq!(
        url("https://mirror.example/mcp/registry/", &["servers"], &[]),
        "https://mirror.example/mcp/registry/v0.1/servers"
    );
}

/// A server's name holds a `/`, which would read as another segment.
#[test]
fn a_servers_name_is_one_segment_of_the_path() {
    assert_eq!(
        url(
            "https://registry.example",
            &["servers", "io.github.acme/weather", "versions", "1.0.2"],
            &[]
        ),
        "https://registry.example/v0.1/servers/io.github.acme%2Fweather/versions/1.0.2"
    );
}

#[test]
fn a_query_is_escaped_as_the_values_it_holds() {
    let asked = url(
        "https://registry.example",
        &["servers"],
        &[("search", "a&version=1#x"), ("limit", "100")],
    );

    let parsed = reqwest::Url::parse(&asked).unwrap();
    let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [
            (String::from("search"), String::from("a&version=1#x")),
            (String::from("limit"), String::from("100")),
        ]
    );
}

#[test]
fn a_registry_that_is_no_https_url_is_refused_before_anything_is_asked() {
    let client = Client::new(&Nowhere);

    for (base, code) in [
        ("http://registry.example", "http_url_refused"),
        ("not a url", "http_url_invalid"),
        (
            "https://user:pass@registry.example",
            "http_credentials_in_url",
        ),
    ] {
        let failed = Registry::new(&client).at(base).search("x").unwrap_err();
        assert_eq!(failed.error.code(), code, "{base}");
    }
}

fn response(server: serde_json::Value, official: serde_json::Value) -> serde_json::Value {
    json!({
        "server": server,
        "_meta": { "io.modelcontextprotocol.registry/official": official },
    })
}

fn read(value: serde_json::Value) -> Result<ServerVersion, String> {
    ServerVersion::read(value, "https://registry.example", 1_762_507_800)
}

#[test]
fn a_server_version_is_read_with_what_the_registry_says_of_it() {
    let read = read(response(
        json!({
            "name": "io.github.acme/weather",
            "version": "1.0.2",
            "title": "Weather",
            "description": "Forecasts\r\nfor any city",
            "websiteUrl": "https://acme.example",
            "icons": [{ "src": "https://acme.example/icon.png" }],
        }),
        json!({
            "status": "deprecated",
            "publishedAt": "2025-10-01T12:00:00Z",
            "updatedAt": "2025-11-02T08:00:00Z",
            "isLatest": true,
            "serverId": "kept by the registry",
        }),
    ))
    .unwrap();

    assert_eq!(read.name, "io.github.acme/weather");
    assert_eq!(read.version, "1.0.2");
    assert_eq!(read.title.as_deref(), Some("Weather"));
    assert_eq!(read.description, "Forecasts  for any city");
    assert_eq!(read.status, Some(Status::Deprecated));
    assert_eq!(read.published_at.as_deref(), Some("2025-10-01T12:00:00Z"));
    assert_eq!(read.updated_at.as_deref(), Some("2025-11-02T08:00:00Z"));
    assert_eq!(read.is_latest, Some(true));
    assert_eq!(read.registry, "https://registry.example");
    assert_eq!(read.fetched_at, 1_762_507_800);
}

/// A schema before 2025-09-29 put the status in the server's own document,
/// where the registry's metadata says it since: read from there when the
/// metadata says none.
#[test]
fn a_status_the_registry_does_not_say_is_read_from_the_server() {
    let read = read(json!({
        "server": { "name": "io.github.acme/old", "version": "0.1.0", "status": "deprecated" },
    }))
    .unwrap();

    assert_eq!(read.status, Some(Status::Deprecated));
    assert_eq!(read.published_at, None);
}

#[test]
fn a_status_this_build_does_not_know_is_kept_as_the_registry_said_it() {
    for (word, status) in [
        ("active", Status::Active),
        ("deprecated", Status::Deprecated),
        ("deleted", Status::Deleted),
        ("withdrawn", Status::Other(String::from("withdrawn"))),
        ("odd\u{1b}[2J", Status::Other(String::from("odd [2J"))),
    ] {
        let read = read(response(
            json!({ "name": "io.github.acme/a", "version": "1" }),
            json!({ "status": word }),
        ))
        .unwrap();
        assert_eq!(read.status.as_ref(), Some(&status), "{word}");
    }
}

#[test]
fn a_server_with_no_name_or_no_version_is_not_read() {
    for (server, why) in [
        (json!({ "version": "1.0.0" }), "missing field `name`"),
        (
            json!({ "name": "io.github.acme/a" }),
            "missing field `version`",
        ),
        (
            json!({ "name": "io.github.acme/a", "version": "" }),
            "its version is empty",
        ),
        (
            json!({ "name": "io.github.acme/a", "version": 3 }),
            "invalid type: integer `3`",
        ),
        (
            json!({ "name": "io.github.acme/a", "version": "1", "packages": {} }),
            "invalid type: map",
        ),
    ] {
        let reason = read(json!({ "server": server })).unwrap_err();
        assert!(reason.contains(why), "{reason}");
    }
}

/// A list shows a name in printable text, so one that is not that already
/// would be listed as a name the registry is then asked for in vain.
#[test]
fn a_name_or_version_no_list_shows_as_it_is_is_not_read() {
    for server in [
        json!({ "name": "io.github.acme/bell\u{7}", "version": "1" }),
        json!({ "name": " io.github.acme/a", "version": "1" }),
        json!({ "name": "io.github.acme/a", "version": "1\n2" }),
    ] {
        let reason = read(json!({ "server": server })).unwrap_err();
        assert!(
            reason.contains("a control character or a space"),
            "{reason}"
        );
    }
}

/// What a front end prints of a listing is a contract: each name pinned.
#[test]
fn a_server_version_serializes_under_the_names_it_is_given() {
    let read = read(response(
        json!({ "name": "io.github.acme/a", "version": "1", "description": "A" }),
        json!({ "status": "active", "isLatest": false }),
    ))
    .unwrap();

    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({
            "name": "io.github.acme/a",
            "version": "1",
            "title": null,
            "description": "A",
            "status": "active",
            "published_at": null,
            "updated_at": null,
            "is_latest": false,
            "registry": "https://registry.example",
            "fetched_at": 1_762_507_800,
        })
    );
}

#[test]
fn a_listing_serializes_what_it_could_not_read_by_code() {
    let listing = Listing {
        servers: Vec::new(),
        failures: vec![Unreadable {
            at: 2,
            name: Some(String::from("io.github.acme/broken")),
            reason: Redacted::new("missing field `version`"),
        }],
        cut: Some(HttpError::TooManyPages {
            url: Redacted::new("https://registry.example/v0.1/servers"),
            limit: 10,
        }),
        warnings: Vec::new(),
    };

    assert_eq!(
        serde_json::to_value(&listing).unwrap(),
        json!({
            "servers": [],
            "failures": [{
                "code": "registry_server_unreadable",
                "message": "servers[2] (`io.github.acme/broken`): missing field `version`, so it is not listed",
            }],
            "cut": {
                "code": "http_too_many_pages",
                "message": "https://registry.example/v0.1/servers: read no further than 10 pages, the most this listing is let run to",
            },
            "warnings": [],
        })
    );
}

#[test]
fn a_version_found_serializes_what_went_wrong_with_the_cache_by_code() {
    let found = Found {
        server: read(response(
            json!({ "name": "io.github.acme/a", "version": "1", "description": "A" }),
            json!({ "status": "active" }),
        ))
        .unwrap(),
        warnings: vec![HttpError::CacheUnreadable {
            path: std::path::PathBuf::from("cache/http/entry.json"),
            detail: Redacted::new("not JSON"),
        }],
    };

    let json = serde_json::to_value(&found).unwrap();

    assert_eq!(
        json.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["server", "warnings"]
    );
    assert_eq!(json["server"], serde_json::to_value(&found.server).unwrap());
    assert_eq!(
        json["warnings"],
        json!([{
            "code": "http_cache_unreadable",
            "message": "cache/http/entry.json: a kept HTTP answer that cannot be read, so it is fetched again: not JSON",
        }])
    );
}
