//! Unit tests for `file.rs`: a definition written out reads back as itself.

use std::collections::BTreeMap;

use super::super::{Endpoint, Input, InputKind, McpDefinition, Stdio, Transport, Upstream};
use crate::mother::{random, McpMother};
use crate::value::{ArtifactName, InputName, Redacted, VariableName};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// Every shape the mother writes, each transport, inputs of every kind,
/// provenance and extras among them, written out and read again.
#[test]
fn a_definition_written_out_reads_back_as_itself() {
    for _ in 0..32 {
        let (_, text) = McpMother::random();
        let definition = parsed(&text);

        let written = definition.text();

        assert_eq!(parsed(&written), definition, "{written}");
    }
}

#[test]
fn every_value_is_quoted_so_that_it_reads_back_as_it_was() {
    let definition = McpDefinition {
        name: ArtifactName::from(McpMother::name()),
        description: String::from("Says \"hello\"\nover two lines, \\ and all"),
        transport: Transport::Stdio(Stdio {
            command: String::from("uvx"),
            args: vec![
                String::from("--from=a b"),
                String::from(r#"{"debug":true}"#),
                String::from("{key}"),
            ],
            env: BTreeMap::from([
                (String::from("A.B"), String::from("dotted")),
                (String::from("KEY"), String::from("{key}")),
            ]),
            cwd: Some(String::from("some folder")),
        }),
        inputs: vec![Input {
            name: InputName::from("key"),
            kind: InputKind::Arg,
            description: Some(String::from("What it says \"key\" for")),
            required: true,
            secret: false,
            env_var: Some(VariableName::from("THE_KEY")),
        }],
        provenance: None,
        extras: BTreeMap::new(),
    };

    let written = definition.text();

    let read = parsed(&written);
    assert_eq!(read.description, definition.description);
    assert_eq!(read.transport, definition.transport);
    assert_eq!(read.inputs, definition.inputs);
}

#[test]
fn a_time_it_was_fetched_is_written_as_the_toml_date_and_time_it_reads_as_its_status_after_it() {
    let mut definition = parsed(&McpMother::http(&random::kebab()));
    definition.provenance = Some(Upstream {
        registry: Some(Redacted::new("https://registry.example")),
        upstream_name: Some(String::from("io.github.acme/weather")),
        version: Some(String::from("1.0.2")),
        fetched_at: Some(String::from("2026-11-07T09:30:00Z")),
        status: Some(String::from("deprecated")),
    });

    let written = definition.text();

    assert!(
        written.contains("\nfetched_at = 2026-11-07T09:30:00Z\nstatus = \"deprecated\"\n"),
        "{written}"
    );
    assert_eq!(parsed(&written), definition);
}

#[test]
fn a_time_written_as_text_stays_text() {
    let mut definition = parsed(&McpMother::sse(&random::kebab()));
    definition.provenance = Some(Upstream {
        fetched_at: Some(String::from("last Tuesday")),
        ..Upstream::default()
    });

    let written = definition.text();

    assert!(
        written.contains("\nfetched_at = \"last Tuesday\"\n"),
        "{written}"
    );
    assert_eq!(parsed(&written), definition);
}

/// The address is written as the definition keeps it: a credential typed
/// into its userinfo went when it was read, and is written nowhere.
#[test]
fn an_address_is_written_as_it_is_kept() {
    // Marked, so it is found nowhere else in the text by chance.
    let password = format!("password-{}", random::kebab());
    let mut definition = parsed(&McpMother::http(&random::kebab()));
    definition.transport = Transport::Http(Endpoint {
        url: Redacted::new(&format!("https://user:{password}@example.com/mcp")),
        headers: BTreeMap::new(),
        withheld: Vec::new(),
        tokens: Vec::new(),
    });
    definition.inputs.clear();

    let written = definition.text();

    assert!(!written.contains(&password), "{written}");
    assert_eq!(parsed(&written).transport, definition.transport);
}
