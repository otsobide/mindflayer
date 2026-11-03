//! The API's OpenAPI document as it is committed, embedded and served
//! (`docs/api/openapi.json`): read as OpenAPI 3.1 by a reader of its own,
//! every schema in it one of JSON Schema 2020-12, as the document says its
//! schemas are, that compiles with every reference resolved within it, and
//! every route of both tables in it, and no other. Offline: the reader is
//! handed the text, and the validator carries 2020-12's meta-schema.

// The routes are the panel's backend's, which a build without `ui` lacks.
#![cfg(feature = "ui")]

mod common;

use std::collections::BTreeSet;

use common::contract::{contract, DOCUMENT};
use mindflayer_cli::backend::PanelBackend;
use mindflayer_server::{Backend, Own};
use serde_json::Value;

#[test]
fn the_document_is_read_as_openapi_3_1_with_every_reference_resolved() {
    let spec = oas3::from_json(DOCUMENT).unwrap_or_else(|error| panic!("no OpenAPI: {error}"));
    let version = spec
        .validate_version()
        .unwrap_or_else(|error| panic!("{error}"));

    assert_eq!((version.major, version.minor), (3, 1));
    assert_eq!(spec.info.title, "Mindflayer");
    let mut read = 0;
    for (path, method, operation) in spec.operations() {
        let at = format!("{method} {path}");
        let parameters = operation
            .parameters(&spec)
            .unwrap_or_else(|error| panic!("{at}: {error}"));
        for parameter in parameters {
            assert!(parameter.schema.is_some(), "{at}: {}", parameter.name);
        }
        operation
            .request_body(&spec)
            .unwrap_or_else(|error| panic!("{at}: {error}"));
        let responses = operation.responses(&spec);
        assert!(responses.contains_key("200"), "{at}");
        for (status, response) in &responses {
            assert!(response.description.is_some(), "{at} {status}");
            for (media, content) in &response.content {
                let schema = content
                    .schema(&spec)
                    .unwrap_or_else(|error| panic!("{at} {status} {media}: {error}"));
                assert!(schema.is_some(), "{at} {status} {media}");
            }
        }
        read += 1;
    }
    assert!(read > 10, "{read} operations");
    let components = spec.components.as_ref().expect("components");
    assert!(!components.schemas.is_empty());
    for (name, schema) in &components.schemas {
        schema
            .resolve(&spec)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    for (name, scheme) in &components.security_schemes {
        scheme
            .resolve(&spec)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

/// The method and path of every operation the document describes.
fn described(document: &Value) -> BTreeSet<(String, String)> {
    document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .unwrap()
                .keys()
                .map(move |method| (method.to_ascii_uppercase(), path.clone()))
        })
        .collect()
}

/// The table the backend hands the server and the server's own, as it
/// serves them, against the document: each route of either described, and
/// nothing described that neither serves.
#[test]
fn every_route_served_is_in_the_document_and_every_route_in_it_is_served() {
    let served: BTreeSet<(String, String)> = PanelBackend::routes()
        .iter()
        .map(|route| (route.method.to_string(), route.path.to_owned()))
        .chain(
            Own::ALL
                .iter()
                .map(|own| (own.method().to_string(), own.path().to_owned())),
        )
        .collect();
    let described = contract(|contract| described(contract.document()));

    assert_eq!(
        served.difference(&described).collect::<Vec<_>>(),
        Vec::<&(String, String)>::new(),
        "served and not described"
    );
    assert_eq!(
        described.difference(&served).collect::<Vec<_>>(),
        Vec::<&(String, String)>::new(),
        "described and not served"
    );
}

/// Where every Schema Object of the document is, as a JSON pointer: each
/// definition, and every parameter's, body's, answer's, header's and
/// event's schema.
fn schemas(document: &Value) -> Vec<String> {
    let escape = |key: &str| key.replace('~', "~0").replace('/', "~1");
    let mut found: Vec<String> = document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .map(|name| format!("/components/schemas/{}", escape(name)))
        .collect();
    for (path, item) in document["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            let at = format!("/paths/{}/{method}", escape(path));
            let parameters = operation["parameters"].as_array().into_iter().flatten();
            for (index, _) in parameters.enumerate() {
                found.push(format!("{at}/parameters/{index}/schema"));
            }
            let body = operation["requestBody"]["content"].as_object();
            for media in body.into_iter().flat_map(|content| content.keys()) {
                found.push(format!("{at}/requestBody/content/{}/schema", escape(media)));
            }
            for (status, response) in operation["responses"].as_object().unwrap() {
                let at = format!("{at}/responses/{status}");
                for (media, content) in response["content"].as_object().unwrap() {
                    let at = format!("{at}/content/{}", escape(media));
                    found.push(format!("{at}/schema"));
                    if content.get("x-itemSchema").is_some() {
                        found.push(format!("{at}/x-itemSchema"));
                    }
                }
                let headers = response["headers"].as_object();
                for header in headers.into_iter().flat_map(|headers| headers.keys()) {
                    found.push(format!("{at}/headers/{}/schema", escape(header)));
                }
            }
        }
    }
    found
}

/// Each a JSON Schema 2020-12 schema, as the document's
/// `jsonSchemaDialect` says, by the dialect's own meta-schema, and each
/// compiled from the document, so every reference in it names a schema
/// the document holds.
#[test]
fn every_schema_in_the_document_is_one_of_json_schema_2020_12_and_compiles() {
    let mut meta = boon::Compiler::new();
    meta.use_loader(Box::new(boon::SchemeUrlLoader::new()));
    let mut compiled = boon::Schemas::new();
    let dialect = contract(|contract| contract.document()["jsonSchemaDialect"].clone());
    let dialect = meta
        .compile(dialect.as_str().unwrap(), &mut compiled)
        .unwrap_or_else(|error| panic!("{error:#}"));

    let pointers = contract(|contract| schemas(contract.document()));
    assert!(pointers.len() > 100, "{} schemas", pointers.len());
    for pointer in pointers {
        let schema = contract(|contract| contract.document().pointer(&pointer).cloned())
            .unwrap_or_else(|| panic!("nothing at {pointer}"));
        if let Err(error) = compiled.validate(&schema, dialect) {
            panic!("{pointer} is no JSON Schema 2020-12: {error:#}");
        }
        if let Err(error) = contract(|contract| contract.compile(&pointer)) {
            panic!("{error}");
        }
    }
}
