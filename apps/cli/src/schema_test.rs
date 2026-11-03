//! Unit tests for `schema.rs`: the export of the API's schemas the CLI
//! gives, and what they say held to what serde writes.

use std::path::Path;

use mindflayer_core::schema::{self as core, Schemas, Together};
use mindflayer_server::Backend;
use schemars::generate::SchemaSettings;

use super::*;
use crate::backend::PanelBackend;
use crate::commands::ui::handed;

/// Where the API's schemas are written, as core's export writes them:
/// `docs/api/schemas` in the repository, whichever folder cargo was
/// started in.
const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/schemas");

/// The export of the CLI's schemas, which `make api/schemas` runs with the
/// feature beside core's: every root the CLI lists, the server's own
/// among them, each into a file of its own.
#[test]
fn export_schemas() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    schemas.write(Path::new(FOLDER)).unwrap();
}

/// Every root of core's, the CLI's and the server's, as one description
/// of the API names them.
fn every_root() -> Schemas {
    let mut schemas = Schemas::default();
    core::roots(&mut schemas);
    roots(&mut schemas);
    schemas
}

/// Across the crates, since all three write into one folder: a root of the
/// CLI's and one of core's of one name would be written one over the other.
#[test]
fn no_two_roots_of_any_crate_are_written_to_one_file() {
    assert_eq!(every_root().shared(), Vec::<String>::new());
}

/// Every type every root names at once, as a description of the whole API
/// holds them, where schemars would give a second type of a name taken a
/// number of its own without a word.
#[test]
fn no_two_types_of_any_crate_go_by_one_name() {
    let mut together = Together::default();
    core::roots(&mut together);
    roots(&mut together);

    assert_eq!(together.renamed(), Vec::<String>::new());
}

/// What a route is sent and answers with is named by its type's name,
/// `[]` after it for a list of them: each a root, so a description of the
/// API finds the schema of every route's body and answer by it.
#[test]
fn every_type_a_route_is_sent_or_answers_with_is_a_root() {
    let schemas = every_root();
    for route in PanelBackend::routes() {
        for named in route.body.into_iter().chain([route.answers]) {
            let name = named.strip_suffix("[]").unwrap_or(named);
            assert!(
                schemas.get(name).is_some(),
                "{} {}: {named} is no root",
                route.method,
                route.path
            );
        }
    }
}

/// Whether `value` is one `schema` describes, and what keeps it from being
/// one when it is not, as a JSON Schema 2020-12 validator says it, with no
/// loader at all: every schema holds what it names in its own `$defs`.
pub(super) fn holds(schema: &Schema, value: &Value) -> Result<(), String> {
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

/// `T`'s schema as its file holds it, the way it is given.
fn given<T: JsonSchema + ?Sized>() -> Schema {
    SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

#[test]
fn every_code_a_front_end_is_handed_holds_to_its_schema_and_nothing_else_does() {
    let schema = given::<Code>();
    let every = mindflayer_core::code::ALL
        .iter()
        .chain(CODES)
        .flat_map(|(_, codes)| codes.iter())
        .chain(Condition::CODES);

    for code in every {
        let code = Value::from(*code);
        if let Err(error) = holds(&schema, &code) {
            panic!("{code}: {error}");
        }
    }
    assert!(holds(&schema, &Value::from("nope")).is_err());
}

/// What `--token-file` writes, the bytes a script reads, as its schema
/// says them; and a file without the token, which it never writes, not.
#[test]
fn the_token_file_holds_to_its_schema() {
    let schema = given::<Handed<'static>>();
    let written: Value =
        serde_json::from_slice(&handed("http://127.0.0.1:4000/api/v1", "abc")).unwrap();

    assert_eq!(schema.get("title"), Some(&Value::from("TokenFile")));
    holds(&schema, &written).unwrap();
    let mut tokenless = written;
    tokenless.as_object_mut().unwrap().remove("token");
    assert!(holds(&schema, &tokenless).is_err());
}

/// Each root of the panel's types has a schema of the name its file has,
/// so what the panel is handed is described for every other client too.
/// With the `ts` feature beside `schema`, as `make api/schemas` builds.
#[cfg(feature = "ts")]
#[test]
fn every_root_the_panel_s_types_export_has_a_schema_of_its_name() {
    use std::collections::BTreeSet;

    struct Exported(BTreeSet<String>);

    impl ts_rs::TypeVisitor for Exported {
        fn visit<T: ts_rs::TS + 'static + ?Sized>(&mut self) {
            let file = T::output_path().expect("a root is written to a file");
            let name = file.file_stem().unwrap().to_string_lossy().into_owned();
            self.0.insert(name);
        }
    }

    let mut exported = Exported(BTreeSet::new());
    mindflayer_core::typescript::roots(&mut exported);
    crate::typescript::roots(&mut exported);
    let schemas = every_root();
    let described: BTreeSet<String> = schemas.names().map(str::to_owned).collect();

    assert_eq!(
        exported.0.difference(&described).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}
