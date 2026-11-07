//! Unit tests for `import.rs`: what a server version the registry gives
//! maps to, each written here as the JSON the API answers with. A server
//! read from the fixtures through a fake transport is
//! `tests/registry_test.rs`'s.

use serde_json::{json, Value};

use super::*;
use crate::mother::McpMother;

/// The server `server` is, as the registry answers with it, active.
fn version(server: Value) -> ServerVersion {
    let response = json!({
        "server": server,
        "_meta": { "io.modelcontextprotocol.registry/official": { "status": "active" } },
    });
    ServerVersion::read(response, "https://registry.example", 1_762_507_800).unwrap()
}

/// A server named `name` started from `packages`.
fn packaged(name: &str, packages: Value) -> ServerVersion {
    version(json!({
        "name": name,
        "description": "A server",
        "version": "1.0.0",
        "packages": packages,
    }))
}

fn npm(identifier: &str) -> Value {
    json!({ "registryType": "npm", "identifier": identifier, "transport": { "type": "stdio" } })
}

fn args(imported: &Imported) -> Vec<String> {
    match &imported
        .definition
        .as_ref()
        .expect("a definition")
        .transport
    {
        Transport::Stdio(stdio) => stdio.args.clone(),
        other => panic!("{other:?}"),
    }
}

fn manual(imported: &Imported) -> Vec<&'static str> {
    imported.manual.iter().map(Manual::code).collect()
}

#[test]
fn a_definition_is_named_after_the_last_part_of_what_the_registry_names_it() {
    for (upstream, suggested) in [
        ("io.github.acme/weather", "weather"),
        ("com.example/my.server.v2", "my-server-v2"),
        ("io.github.acme/Weather_Tools", "Weather_Tools"),
        ("io.github.acme/--odd..name--", "odd-name"),
        ("plain", "plain"),
    ] {
        assert_eq!(super::suggested(upstream), suggested, "{upstream}");
    }
    let long = format!("io.github.acme/{}", "a".repeat(80));
    assert_eq!(super::suggested(&long).len(), 64);
}

#[test]
fn a_name_given_is_the_one_the_definition_takes() {
    let server = packaged("io.github.acme/weather", json!([npm("@acme/weather")]));

    let imported = server.import(Some("acme-weather"));

    assert_eq!(imported.definition.unwrap().name, "acme-weather");
}

/// A name an agent keeps for itself is no server's, whether suggested or
/// given: said, for whoever imports it to give another.
#[test]
fn a_name_no_server_may_be_given_is_a_manual_step() {
    let reserved = packaged("io.github.acme/workspace", json!([npm("@acme/workspace")]));
    let given = packaged("io.github.acme/weather", json!([npm("@acme/weather")]));

    for imported in [reserved.import(None), given.import(Some("my server"))] {
        assert_eq!(imported.definition, None);
        assert_eq!(manual(&imported), ["registry_name_unfit"]);
    }
    let said = reserved.import(None).manual[0].to_string();
    assert!(
        said.starts_with("`workspace` is no name a server can be given: "),
        "{said}"
    );
    assert!(said.ends_with("; give it another"), "{said}");
}

#[test]
fn a_server_with_nothing_to_start_or_reach_is_a_manual_step() {
    let imported = packaged("io.github.acme/empty", json!([])).import(None);

    assert_eq!(imported.definition, None);
    assert_eq!(manual(&imported), ["registry_server_unrunnable"]);
}

/// The first package that maps is the definition, and what came before it
/// is no step anybody has to take.
#[test]
fn the_first_package_that_maps_is_taken_and_those_before_it_said_nowhere() {
    let server = packaged(
        "io.github.acme/both",
        json!([
            { "registryType": "mcpb", "identifier": "https://downloads.acme.example/a.mcpb" },
            npm("@acme/both"),
        ]),
    );

    let imported = server.import(None);

    assert_eq!(args(&imported), ["-y", "@acme/both"]);
    assert_eq!(imported.manual, []);
}

/// What a package maps to that `validate` would call invalid is a step,
/// and the next way of starting the server is tried.
#[test]
fn a_package_that_maps_to_no_valid_definition_is_passed_for_the_next() {
    let server = version(json!({
        "name": "io.github.acme/next",
        "description": "A server",
        "version": "1",
        "packages": [{
            "registryType": "npm",
            "identifier": "@acme/next",
            "environmentVariables": [{ "name": "{X}", "value": "{X}", "variables": { "X": {} } }],
            "packageArguments": [{ "type": "positional", "value": "{X}", "variables": { "X": {} } }],
        }],
        "remotes": [{ "type": "streamable-http", "url": "https://mcp.acme.example" }],
    }));

    let imported = server.import(None);

    let definition = imported.definition.expect("the remote is taken");
    assert_eq!(definition.transport.slug(), "http");
    assert_eq!(imported.manual, []);
}

/// A program is every agent's to start, where an address over sse is not.
#[test]
fn a_package_is_taken_before_a_remote() {
    let server = version(json!({
        "name": "io.github.acme/both",
        "description": "A server",
        "version": "1",
        "remotes": [{ "type": "sse", "url": "https://mcp.acme.example/sse" }],
        "packages": [npm("@acme/both")],
    }));

    let imported = server.import(None);

    assert_eq!(args(&imported), ["-y", "@acme/both"]);
    assert_eq!(imported.warnings, []);
}

/// What a command line would read as an option or split is not written
/// into one: an identifier such as `--privileged` would be `docker`'s.
#[test]
fn an_identifier_a_version_or_a_name_a_command_line_would_misread_is_a_manual_step() {
    for (package, what) in [
        (
            json!({ "registryType": "oci", "identifier": "--privileged" }),
            "identifier",
        ),
        (
            json!({ "registryType": "npm", "identifier": "a b" }),
            "identifier",
        ),
        (
            json!({ "registryType": "npm", "identifier": "" }),
            "identifier",
        ),
        (
            json!({ "registryType": "pypi", "identifier": "a", "version": "-1" }),
            "version",
        ),
        (
            json!({ "registryType": "npm", "identifier": "a", "version": "1\n2" }),
            "version",
        ),
        (
            json!({
                "registryType": "oci",
                "identifier": "acme/a:1",
                "environmentVariables": [{ "name": "A=1" }],
            }),
            "environment variable's name",
        ),
    ] {
        let imported = packaged("io.github.acme/odd", json!([package])).import(None);

        assert_eq!(imported.definition, None, "{package}");
        assert_eq!(manual(&imported), ["registry_package_unfit"], "{package}");
        let Manual::Unfit { what: said, .. } = &imported.manual[0] else {
            unreachable!()
        };
        assert_eq!(*said, what, "{package}");
    }
}

/// `npx` reads packages from npm's registry whatever the registry says, so
/// one it says is elsewhere would be another package of the same name.
#[test]
fn a_package_from_another_registry_than_its_program_reads_is_a_manual_step() {
    let elsewhere = packaged(
        "io.github.acme/private",
        json!([{
            "registryType": "npm",
            "registryBaseUrl": "https://npm.acme.example",
            "identifier": "@acme/private",
        }]),
    );
    let there = packaged(
        "io.github.acme/public",
        json!([{
            "registryType": "npm",
            "registryBaseUrl": "https://registry.npmjs.org/",
            "identifier": "@acme/public",
        }]),
    );

    let imported = elsewhere.import(None);

    assert_eq!(manual(&imported), ["registry_package_elsewhere"]);
    assert_eq!(
        imported.manual[0].to_string(),
        "packages[0]: the npm package is from https://npm.acme.example, where `npx` does not \
         read packages, so it would start one of the same name from elsewhere: install it from \
         there by hand"
    );
    assert_eq!(args(&there.import(None)), ["-y", "@acme/public"]);
}

#[test]
fn a_package_that_serves_at_an_address_once_started_is_a_manual_step() {
    let served = packaged(
        "io.github.acme/served",
        json!([{
            "registryType": "npm",
            "identifier": "@acme/served",
            "transport": { "type": "streamable-http", "url": "http://localhost:{port}/mcp" },
        }]),
    );

    assert_eq!(manual(&served.import(None)), ["registry_package_not_stdio"]);
}

#[test]
fn a_remote_over_a_transport_this_build_does_not_know_is_a_manual_step() {
    let server = version(json!({
        "name": "io.github.acme/socket",
        "description": "A server",
        "version": "1",
        "remotes": [{ "type": "websocket", "url": "wss://mcp.acme.example" }],
    }));

    assert_eq!(
        manual(&server.import(None)),
        ["registry_remote_type_unknown"]
    );
}

#[test]
fn an_images_reference_takes_its_registry_and_tag_only_where_it_names_none() {
    for (identifier, version, base, image) in [
        ("ghcr.io/acme/a:2.0.0", None, None, "ghcr.io/acme/a:2.0.0"),
        (
            "ghcr.io/acme/a:2.0.0",
            Some("9"),
            Some("https://docker.io"),
            "ghcr.io/acme/a:2.0.0",
        ),
        (
            "acme/a",
            Some("1.0.2"),
            Some("https://docker.io"),
            "docker.io/acme/a:1.0.2",
        ),
        (
            "acme/a",
            Some("1.0.2"),
            Some("https://ghcr.io"),
            "ghcr.io/acme/a:1.0.2",
        ),
        ("acme/a", None, None, "acme/a"),
        (
            "localhost:5000/a",
            Some("1"),
            Some("https://docker.io"),
            "localhost:5000/a:1",
        ),
        ("acme/a@sha256:abc", Some("1"), None, "acme/a@sha256:abc"),
        (
            "a",
            Some("1"),
            Some("https://registry.acme.example:5000"),
            "registry.acme.example:5000/a:1",
        ),
    ] {
        assert_eq!(
            super::image(identifier, version, base),
            image,
            "{identifier}"
        );
    }
}

/// Positional and named arguments, an input where the registry gives no
/// value and the argument is required, and left out where it is not.
#[test]
fn arguments_are_written_in_order_and_one_with_no_value_is_an_input_or_left_out() {
    let server = packaged(
        "io.github.acme/args",
        json!([{
            "registryType": "pypi",
            "identifier": "acme-args",
            "runtimeArguments": [{ "type": "named", "name": "--python", "value": "3.12" }],
            "packageArguments": [
                { "type": "positional", "value": "serve" },
                { "type": "named", "name": "--root", "isRequired": true, "description": "Where" },
                { "type": "positional", "valueHint": "port number", "isRequired": true },
                { "type": "named", "name": "--debug" },
                { "type": "flag", "value": "x" },
                { "type": "named", "value": "nameless", "isRequired": true },
            ],
        }]),
    );

    let imported = server.import(None);

    assert_eq!(
        args(&imported),
        [
            "--python",
            "3.12",
            "acme-args",
            "serve",
            "--root",
            "{root}",
            "{port_number}"
        ]
    );
    let definition = imported.definition.as_ref().unwrap();
    let inputs: Vec<(&str, InputKind, bool, Option<&str>)> = definition
        .inputs
        .iter()
        .map(|input| {
            (
                input.name.as_str(),
                input.kind,
                input.required,
                input.env_var.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        inputs,
        [
            ("root", InputKind::Arg, true, Some("ROOT")),
            ("port_number", InputKind::Arg, true, Some("PORT_NUMBER")),
        ]
    );
    let left_out: Vec<String> = imported.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(
        left_out,
        [
            "packages[0].packageArguments[3]: an optional argument with no value, left out: add \
             it to `args` by hand if the server is wanted with it",
            "packages[0].packageArguments[4]: an argument of a type this build does not know, \
             left out: add it to `args` by hand if the server is wanted with it",
            "packages[0].packageArguments[5]: a named argument with no name, left out: add it to \
             `args` by hand if the server is wanted with it",
        ]
    );
}

/// A secret the registry gives the whole value of is a value no file may
/// hold: left out, said, and read from a variable of its name.
#[test]
fn a_fixed_value_for_a_secret_is_left_out_and_read_from_the_environment() {
    let server = packaged(
        "io.github.acme/fixed",
        json!([{
            "registryType": "npm",
            "identifier": "@acme/fixed",
            "environmentVariables": [
                { "name": "FIXED_SECRET", "value": "baked-into-the-registry", "isSecret": true },
                { "name": "FIXED_PLAIN", "value": "info" },
            ],
        }]),
    );

    let imported = server.import(None);

    let definition = imported.definition.as_ref().unwrap();
    let Transport::Stdio(stdio) = &definition.transport else {
        unreachable!()
    };
    assert_eq!(stdio.env["FIXED_SECRET"], "{FIXED_SECRET}");
    assert_eq!(stdio.env["FIXED_PLAIN"], "info");
    assert!(definition.inputs[0].secret);
    assert_eq!(
        imported.warnings,
        [Warning::SecretDropped {
            at: String::from("packages[0].environmentVariables[0]"),
            what: "value",
        }]
    );
    assert!(!definition.text().contains("baked-into-the-registry"));
}

/// A variable of a secret is a secret, and so is its fixed value.
#[test]
fn a_variable_of_a_secret_is_a_secret_and_its_fixed_value_is_left_out() {
    let server = version(json!({
        "name": "io.github.acme/bearer",
        "description": "A server",
        "version": "1",
        "remotes": [{
            "type": "streamable-http",
            "url": "https://mcp.acme.example",
            "headers": [{
                "name": "Authorization",
                "value": "Bearer {token}",
                "isSecret": true,
                "variables": { "token": { "value": "baked-into-the-registry" } },
            }],
        }],
    }));

    let imported = server.import(None);

    let definition = imported.definition.as_ref().unwrap();
    assert_eq!(definition.inputs[0].name, "token");
    assert!(definition.inputs[0].secret);
    assert_eq!(definition.inputs[0].env_var.as_deref(), Some("TOKEN"));
    assert_eq!(imported.warnings.len(), 1);
    assert!(!definition.text().contains("baked-into-the-registry"));
}

#[test]
fn a_variable_is_named_as_every_agent_refers_to_one() {
    for (name, variable) in [
        ("api_key", "API_KEY"),
        ("X-Acme-Region", "X_ACME_REGION"),
        ("9lives", "_9LIVES"),
        ("a.b c", "A_B_C"),
    ] {
        let read = variable_for(name);
        assert_eq!(read, variable, "{name}");
        assert!(read.is_portable(), "{name}");
    }
}

/// What a front end prints of an import is a contract: each name pinned,
/// the definition as its file's text and what is said by code.
#[test]
fn what_is_imported_serializes_under_the_names_it_is_given() {
    let mapped = packaged("io.github.acme/a", json!([npm("@acme/a")])).import(None);
    let unmapped = packaged("io.github.acme/b", json!([])).import(None);

    let json = serde_json::to_value(&mapped).unwrap();
    assert_eq!(
        json["definition"],
        Value::String(mapped.definition.as_ref().unwrap().text())
    );
    assert_eq!(json["manual"], json!([]));
    assert_eq!(json["warnings"], json!([]));
    assert_eq!(
        json.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["definition", "manual", "warnings"]
    );
    assert_eq!(
        serde_json::to_value(&unmapped).unwrap(),
        json!({
            "definition": null,
            "manual": [{
                "code": "registry_server_unrunnable",
                "message": "it names no package and no remote server, so there is nothing to start or reach",
            }],
            "warnings": [],
        })
    );
}

/// Whatever a step quotes of the registry is printable, a URL without a
/// credential.
#[test]
fn what_a_step_says_of_the_registry_is_printable_and_holds_no_credential() {
    // Marked, so it is found nowhere else in what is said by chance.
    let password = format!("password-{}", McpMother::name());
    let server = packaged(
        "io.github.acme/odd",
        json!([{
            "registryType": "mcpb",
            "identifier": format!("https://user:{password}@downloads.acme.example/\r\x1b[2Ja.mcpb"),
        }]),
    );

    let said = server.import(None).manual[0].to_string();

    assert!(!said.contains(&password), "{said}");
    assert!(!said.contains(['\r', '\x1b']), "{said}");
}
