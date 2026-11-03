//! Unit tests for `schema.rs`: what the server answers with of its own,
//! held to what serde writes of it.

use schemars::generate::SchemaSettings;
use schemars::Schema;
use serde::Serialize;
use serde_json::{json, Value};

use super::*;
use crate::Condition;

/// Each root's schema as a file of it would hold it, by its name.
#[derive(Default)]
struct Given(Vec<(String, Schema)>);

impl Visitor for Given {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        let schema = SchemaSettings::draft2020_12()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<T>();
        self.0.push((T::schema_name().into_owned(), schema));
    }
}

/// The schema of the root named `name`.
fn schema(name: &str) -> Schema {
    let mut given = Given::default();
    roots(&mut given);
    given
        .0
        .into_iter()
        .find(|(named, _)| named == name)
        .unwrap_or_else(|| panic!("no root {name}"))
        .1
}

/// Whether `value` is one `schema` describes, and what keeps it from being
/// one when it is not, as a JSON Schema 2020-12 validator says it, with no
/// loader at all: every schema holds what it names in its own `$defs`.
fn holds(schema: &Schema, value: &Value) -> Result<(), String> {
    let mut compiler = boon::Compiler::new();
    compiler.use_loader(Box::new(boon::SchemeUrlLoader::new()));
    compiler
        .add_resource("file:///schema.json", schema.as_value().clone())
        .unwrap();
    let mut compiled = boon::Schemas::new();
    let index = compiler
        .compile("file:///schema.json", &mut compiled)
        .unwrap_or_else(|error| panic!("{error:#}"));
    compiled
        .validate(value, index)
        .map_err(|error| format!("{error:#}"))
}

/// That what serde writes of `value` holds to the root named `name`.
fn holds_to(name: &str, value: &impl Serialize) {
    let json = serde_json::to_value(value).unwrap();
    if let Err(error) = holds(&schema(name), &json) {
        panic!("{json} is no {name}: {error}");
    }
}

/// That `value` does not hold to the root named `name`.
fn refused_by(name: &str, value: &Value) {
    assert!(
        holds(&schema(name), value).is_err(),
        "{value} holds to {name}"
    );
}

/// One event of each kind, matched with no wildcard so a kind added is
/// added here before it compiles.
fn every_event() -> Vec<Event> {
    fn _every(event: Event) {
        match event {
            Event::Changed { .. }
            | Event::CommandStarted { .. }
            | Event::CommandFinished { .. }
            | Event::Switched { .. }
            | Event::Resync { .. }
            | Event::Watching
            | Event::Unwatched { .. } => {}
        }
    }
    let lines = vec!["flayer link /home/you/blog".to_owned()];
    vec![
        Event::Changed {
            scopes: vec![json!({"type": "workspace"}), json!("anything")],
        },
        Event::CommandStarted {
            lines: lines.clone(),
        },
        Event::CommandFinished { lines },
        Event::Switched { switches: 2 },
        Event::Resync { switches: 0 },
        Event::Watching,
        Event::Unwatched {
            reason: "the system's limit on watches was reached".to_owned(),
        },
    ]
}

#[test]
fn the_server_s_roots_are_a_preview_a_run_an_event_and_a_refusal() {
    let mut given = Given::default();
    roots(&mut given);
    let names: Vec<&str> = given.0.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(names, ["Preview", "Ran", "Event", "Refused"]);
}

#[test]
fn every_event_serde_writes_holds_to_its_schema() {
    for event in every_event() {
        holds_to("Event", &event);
    }
}

/// An event serde never writes: a type that is none, one without what its
/// type carries, and a count that is no count.
#[test]
fn an_event_serde_never_writes_is_refused() {
    refused_by("Event", &json!({"type": "nope"}));
    refused_by("Event", &json!({"type": "changed"}));
    refused_by("Event", &json!({"type": "switched", "switches": -1}));
    refused_by("Event", &json!({"scopes": []}));
}

/// The backend's JSON, which the server carries unread, is any JSON in
/// the server's own schema: what fills it is the front end's to say.
#[test]
fn a_preview_and_a_run_hold_to_their_schemas_whatever_the_backend_hands_over() {
    holds_to(
        "Preview",
        &Preview {
            lines: vec!["flayer unlink gone".to_owned()],
            confirm: vec![json!({"line": 0, "caution": "unlinks"}), json!(1)],
        },
    );
    holds_to(
        "Preview",
        &Preview {
            lines: Vec::new(),
            confirm: Vec::new(),
        },
    );
    holds_to(
        "Ran",
        &Ran {
            lines: vec!["flayer unlink gone".to_owned()],
            outcome: json!({"ok": true, "runs": []}),
        },
    );
    refused_by("Preview", &json!({"lines": ["flayer unlink gone"]}));
    refused_by(
        "Ran",
        &json!({"lines": "flayer unlink gone", "outcome": null}),
    );
}

#[test]
fn a_refusal_of_every_condition_holds_to_its_schema() {
    for condition in Condition::ALL {
        holds_to(
            "Refused",
            &Refused {
                code: condition.code(),
                error: format!("{condition:?}"),
            },
        );
    }
    refused_by("Refused", &json!({"error": "no code"}));
    refused_by("Refused", &json!({"code": 404, "error": "a number"}));
}
