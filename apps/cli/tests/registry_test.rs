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

// ---------------------------------------------------------------------------
// A server written into the library
// ---------------------------------------------------------------------------

/// Every value the fixtures give for a secret, a default, an example or a
/// fixed value, which nothing prints and no definition holds.
const SECRET_VALUES: [&str; 4] = [
    "change-me-in-production",
    "your-weather-key-here",
    "acme-key-goes-here",
    "notes-key-that-ships-with-the-docs",
];

/// Every file the workspace keeps, its library and its ledger among them,
/// by its route, with what it holds: all but the answers its HTTP cache
/// keeps, which a read keeps too.
fn kept(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let root = dir.join(WEAVER_DIR);
    let mut kept = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.starts_with(root.join("cache")) {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                let route = path.strip_prefix(&root).unwrap().display().to_string();
                kept.push((route, fs::read(&path).unwrap()));
            }
        }
    }
    kept.sort();
    kept
}

/// What `add` would write said in full, nothing written: the version and
/// its status, what the library would call it, the command an agent would
/// start and its inputs, a secret by the variable it is read from; what
/// the mapping left out a warning that fails nothing.
#[test]
fn add_without_yes_writes_nothing_and_prints_the_resolved_command() {
    let dir = workspace();
    let registry = FixtureRegistry::start();
    let before = kept(dir.path());

    let outcome = mcp(dir.path(), &registry, &["add", "io.github.acme/weather"]).unwrap();

    let file = own_directory_for(dir.path(), Kind::Mcp).join("weather.toml");
    assert_eq!(
        outcome.stdout,
        format!(
            "io.github.acme/weather 1.0.2, active, from {}\n\
             as the MCP server weather: Forecasts and alerts for any city\n\
             \n\
             transport: stdio\n\
             \x20 command: npx\n\
             \x20 args:\n\
             \x20   - -y\n\
             \x20   - @acme/weather-mcp@1.0.2\n\
             \x20   - --units\n\
             \x20   - metric\n\
             \x20   - {{city}}\n\
             \x20 env:\n\
             \x20   WEATHER_API_KEY: {{WEATHER_API_KEY}}\n\
             \x20   WEATHER_ENDPOINT: https://{{region}}.api.acme.example\n\
             \x20   WEATHER_LOG_LEVEL: {{WEATHER_LOG_LEVEL}}\n\
             \n\
             inputs:\n\
             \x20 WEATHER_API_KEY: env, secret, required, read from $WEATHER_API_KEY\n\
             \x20   The key the forecasts are fetched with\n\
             \x20 WEATHER_LOG_LEVEL: env, read from $WEATHER_LOG_LEVEL\n\
             \x20   How much it logs\n\
             \x20 region: env, required, read from $REGION\n\
             \x20   The region closest to you\n\
             \x20 city: arg, required, read from $CITY\n\
             \x20   The city forecasts are for\n\
             \n\
             would create mcp weather at {}\n\
             nothing written: add --yes to go ahead\n",
            registry.url,
            file.display(),
        )
    );
    assert_eq!(
        stderr_codes(&outcome),
        [
            Some(String::from("registry_secret_dropped")),
            Some(String::from("registry_secret_dropped")),
            Some(String::from("registry_argument_left_out")),
        ]
    );
    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert_eq!(kept(dir.path()), before);
    assert!(!file.exists());
    assert_eq!(
        registry.asked(),
        ["/v0.1/servers/io.github.acme%2Fweather/versions/latest"]
    );
}

/// The write is the dry run's, said the same way and ending with where it
/// went, and what it wrote is a definition `validate` finds nothing wrong
/// with, which tells where it came from.
#[test]
fn add_with_yes_writes_a_definition_validate_reads_with_no_issue() {
    let dir = workspace();
    let registry = FixtureRegistry::start();
    let dry = mcp(dir.path(), &registry, &["add", "io.github.acme/weather"]).unwrap();

    let outcome = mcp(
        dir.path(),
        &registry,
        &["add", "--yes", "io.github.acme/weather"],
    )
    .unwrap();

    let file = own_directory_for(dir.path(), Kind::Mcp).join("weather.toml");
    let said = dry
        .stdout
        .strip_suffix("nothing written: add --yes to go ahead\n")
        .unwrap()
        .replace("would create", "created");
    assert_eq!(outcome.stdout, said);
    assert!(outcome.ok, "{:?}", outcome.stderr);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("upstream_name = \"io.github.acme/weather\""),
        "{text}"
    );
    let validated = weaver(dir.path(), &["validate", "mcp:weather"]).unwrap();
    assert!(validated.ok, "{validated:?}");
    assert!(validated.stderr.is_empty(), "{validated:?}");
}

#[test]
fn add_takes_the_version_and_the_name_it_is_given() {
    let dir = workspace();
    let registry = FixtureRegistry::start();

    let outcome = mcp(
        dir.path(),
        &registry,
        &[
            "add",
            "--yes",
            "--version",
            "1.0.2",
            "--name",
            "forecasts",
            "io.github.acme/weather",
        ],
    )
    .unwrap();

    assert!(outcome
        .stdout
        .contains("as the MCP server forecasts: Forecasts and alerts for any city\n"));
    assert!(own_directory_for(dir.path(), Kind::Mcp)
        .join("forecasts.toml")
        .is_file());
    assert_eq!(
        registry.asked(),
        ["/v0.1/servers/io.github.acme%2Fweather/versions/1.0.2"]
    );
}

/// What the library holds under the name is left as it is, by the dry run
/// and by the write alike.
#[test]
fn a_name_the_library_holds_is_refused_and_left_as_it_is() {
    let dir = workspace();
    let registry = FixtureRegistry::start();
    mcp(
        dir.path(),
        &registry,
        &["add", "--yes", "io.github.acme/weather"],
    )
    .unwrap();
    let before = kept(dir.path());

    for args in [
        &["add", "io.github.acme/weather"][..],
        &["add", "--yes", "io.github.acme/weather"][..],
    ] {
        let failure = mcp(dir.path(), &registry, args).unwrap_err();

        assert_eq!(failure.error.code(), "add_exists", "{args:?}");
    }
    assert_eq!(kept(dir.path()), before);
}

/// A name typed for it that `add` refuses is refused as `add` refuses it,
/// by the dry run and by the write alike, rather than as a version nothing
/// maps, and nothing is written.
#[test]
fn a_name_add_refuses_is_refused_as_add_refuses_it() {
    let dir = workspace();
    let registry = FixtureRegistry::start();
    let before = kept(dir.path());

    for args in [
        &["add", "--name", "Bad Name", "io.github.acme/weather"][..],
        &[
            "add",
            "--yes",
            "--name",
            "Bad Name",
            "io.github.acme/weather",
        ][..],
    ] {
        let failure = mcp(dir.path(), &registry, args).unwrap_err();

        assert_eq!(failure.error.code(), "add_name_unfit", "{args:?}");
        assert!(failure.warnings.is_empty(), "{args:?}");
    }
    assert_eq!(kept(dir.path()), before);
}

/// A bundle, which nothing here starts, is refused, each step to take by
/// hand said with its code, and nothing is written.
#[test]
fn a_server_with_no_runnable_mapping_is_refused_with_its_manual_steps() {
    let dir = workspace();
    let registry = FixtureRegistry::start();
    let before = kept(dir.path());

    let failure = mcp(
        dir.path(),
        &registry,
        &["add", "--yes", "io.github.acme/notes-bundle"],
    )
    .unwrap_err();

    assert_eq!(failure.error.code(), "registry_import_unmapped");
    assert_eq!(
        failure
            .warnings
            .iter()
            .map(|warning| warning.code)
            .collect::<Vec<_>>(),
        [Some("registry_package_mcpb")]
    );
    assert_eq!(kept(dir.path()), before);
}

/// Nothing the registry gives for a secret is printed, nor written: a
/// default, an example, a value, each read from a variable instead.
#[test]
fn a_secrets_value_is_never_printed_nor_written() {
    let dir = workspace();
    let registry = FixtureRegistry::start();

    for server in ["io.github.acme/weather", "io.github.acme/notes-cloud"] {
        for args in [&["add", server][..], &["add", "--yes", server][..]] {
            let outcome = mcp(dir.path(), &registry, args).unwrap();

            let said = format!("{}{:?}", outcome.stdout, outcome.stderr);
            for secret in SECRET_VALUES {
                assert!(!said.contains(secret), "{args:?}: {said}");
            }
        }
    }
    for (_, written) in kept(dir.path()) {
        let written = String::from_utf8_lossy(&written);
        for secret in SECRET_VALUES {
            assert!(!written.contains(secret), "{written}");
        }
    }
}

/// Neither the dry run nor the write starts anything the definition names:
/// each program an agent would start stands on the `PATH` the binary is
/// given, and none of them is run.
#[cfg(unix)]
#[test]
fn nothing_a_definition_starts_is_run_by_the_dry_run_or_the_write() {
    use std::os::unix::fs::PermissionsExt;

    let dir = workspace();
    let registry = FixtureRegistry::start();
    let bin = TempDir::new().unwrap();
    let started = bin.path().join("started");
    for program in ["npx", "uvx", "docker", "dnx"] {
        let path = bin.path().join(program);
        fs::write(
            &path,
            format!("#!/bin/sh\necho {program} >> '{}'\n", started.display()),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = bin.path().to_string_lossy().into_owned();

    for server in [
        "io.github.acme/weather",
        "io.github.acme/pyweather",
        "io.github.acme/weather-docker",
        "io.github.acme/dotnet-weather",
    ] {
        for yes in [&[][..], &["--yes"][..]] {
            let mut args = vec!["gather", "mcp", "add", "--registry", &registry.url];
            args.extend_from_slice(yes);
            args.push(server);

            let ran = spawn("mindweaver", dir.path(), &args, &[("PATH", &path)]);

            assert!(ran.ok, "{server}: {}", ran.stderr);
        }
    }
    assert!(
        !started.exists(),
        "{}",
        fs::read_to_string(&started).unwrap_or_default()
    );
    assert_eq!(
        fs::read_dir(own_directory_for(dir.path(), Kind::Mcp))
            .unwrap()
            .count(),
        4
    );
}
