//! Unit tests for `typescript.rs`: the export of the panel's types the CLI
//! gives, and what it says held to what serde writes.

use std::env;

use mindflayer_core::typescript::{self as core, Export, Files};
use ts_rs::{Config, TS};

use super::*;
use crate::task::{AgentsLine, Holder};

/// The export of the CLI's types, which `make web/types` runs with the
/// feature after core's: every root and what it names, core's types among
/// them, written where `.cargo/config.toml` says, and refused when nothing
/// says where, as core's is.
#[test]
fn export_bindings() {
    assert!(
        env::var_os("TS_RS_EXPORT_DIR").is_some(),
        "TS_RS_EXPORT_DIR is not set: run cargo from inside the repository, \
         whose .cargo/config.toml points it at the panel"
    );
    let mut export = Export::from_env();
    roots(&mut export);
    export.finish().unwrap();
}

/// Across both crates, since both write into one folder: a type of the
/// CLI's and one of core's of one name would be written one over the other.
#[test]
fn no_two_types_are_written_to_one_file_in_either_crate() {
    let mut files = Files::default();
    core::roots(&mut files);
    roots(&mut files);

    assert_eq!(files.shared(), Vec::<String>::new());
}

/// Every `"type": "..."` a union in TypeScript tells its members apart by,
/// in the order it lists them.
fn tags_in(union: &str) -> Vec<String> {
    union
        .split(r#""type": ""#)
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().to_owned())
        .collect()
}

/// Every name serde reads a `T` by, in the order it lists them: what it
/// says it expected when handed one it does not know (`one of` them, or
/// two joined by `or`), the way the task tests read every name a task goes
/// by, which a list kept here could miss one of.
fn tags_serde_reads<T: serde::de::DeserializeOwned + std::fmt::Debug>() -> Vec<String> {
    let error = serde_json::from_str::<T>(r#"{"type": "?"}"#)
        .unwrap_err()
        .to_string();
    let (_, expected) = error
        .split_once("expected ")
        .unwrap_or_else(|| panic!("{error}"));
    expected
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// What the panel sends is read by serde's derive, so its TypeScript names
/// each variant by the name serde reads it by, and no other.
#[test]
fn every_union_the_panel_sends_names_its_members_as_serde_reads_them() {
    let config = Config::new();

    assert_eq!(
        tags_in(&Query::inline(&config)),
        tags_serde_reads::<Query>()
    );
    assert_eq!(tags_in(&Task::inline(&config)), tags_serde_reads::<Task>());
    assert_eq!(
        tags_in(&Holder::inline(&config)),
        tags_serde_reads::<Holder>()
    );
    assert_eq!(
        tags_in(&AgentsLine::inline(&config)),
        tags_serde_reads::<AgentsLine>()
    );
}

/// A holder and an agents line keep what they hold beside their tag, under
/// a name of its own, as serde writes them, and a holder of the workspace
/// holds nothing there.
#[test]
fn a_holder_and_an_agents_line_say_where_they_keep_what_they_hold() {
    let config = Config::new();

    assert_eq!(
        Holder::inline(&config),
        r#"{ "type": "workspace" } | { "type": "project", "entry": string } | { "type": "elsewhere", "entry": string }"#
    );
    assert_eq!(
        serde_json::to_value(Holder::Workspace).unwrap(),
        serde_json::json!({"type": "workspace"})
    );
    assert_eq!(
        serde_json::to_value(Holder::Project("alpha".to_owned())).unwrap(),
        serde_json::json!({"type": "project", "entry": "alpha"})
    );
    assert_eq!(
        AgentsLine::inline(&config),
        r#"{ "type": "named", "agents": Array<Agent> } | { "type": "removed", "agents": Array<Agent> }"#
    );
}

#[test]
fn every_code_the_cli_gives_is_in_its_union_once_beside_core_s() {
    let union = Code::inline(&Config::new());
    let members: Vec<&str> = union
        .split('\n')
        .filter_map(|line| line.trim().strip_prefix("| "))
        .collect();
    let given: Vec<String> = CODES
        .iter()
        .flat_map(|(_, codes)| codes.iter())
        .map(|code| format!("{code:?}"))
        .collect();

    assert_eq!(members.first(), Some(&"CoreCode"));
    assert_eq!(members[1..], given);
}
