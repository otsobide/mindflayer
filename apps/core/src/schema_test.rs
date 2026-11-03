//! Unit tests for `schema.rs`: the export the API's schemas come from.

use std::path::Path;

use super::*;

/// Where the API's schemas are written: `docs/api/schemas` in the
/// repository, whichever folder cargo was started in, beside the docs that
/// describe the API rather than inside the panel, which reads none of them.
const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/schemas");

/// The export the API's schemas come from, which `make api/schemas` runs
/// with the feature: every root core gives, each into a file of its own.
#[test]
fn export_schemas() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    schemas.write(Path::new(FOLDER)).unwrap();
}

#[test]
fn no_two_roots_are_written_to_one_file() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    assert_eq!(schemas.shared(), Vec::<String>::new());
}

/// Across every root at once, as a description of the whole API holds
/// them: the CLI's test holds core's, its own and the server's together.
#[test]
fn no_two_types_core_gives_go_by_one_name() {
    let mut together = Together::default();
    roots(&mut together);

    assert_eq!(together.renamed(), Vec::<String>::new());
}

/// Two types of one name, from two modules.
mod one {
    #[derive(schemars::JsonSchema)]
    pub struct Twin;
}

mod two {
    #[derive(schemars::JsonSchema)]
    pub struct Twin;
}

/// What finds them, which a test that found none would pass unseen:
/// schemars names the second with a number, and says nothing.
#[test]
fn two_types_going_by_one_name_are_found() {
    let mut together = Together::default();
    together.given::<one::Twin>();
    together.given::<two::Twin>();

    assert_eq!(together.renamed(), vec!["Twin2".to_owned()]);
}

/// A root by the name a route gives its type, its file's schema whole,
/// every type it names in its `$defs`.
#[test]
fn a_root_is_found_by_the_name_its_type_goes_by() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    let snapshot = schemas.get("Snapshot").unwrap();
    assert_eq!(snapshot.get("title"), Some(&Value::from("Snapshot")));
    assert_eq!(
        snapshot.get("$schema"),
        Some(&Value::from("https://json-schema.org/draft/2020-12/schema"))
    );
    assert!(snapshot.pointer("/$defs/Project").is_some());
    assert!(schemas.get("Project").is_some());
    assert!(schemas.get("Nothing").is_none());
}
