//! The API's OpenAPI document as it is committed and served, and an answer
//! held to it: to the schema the document gives the route it came from,
//! for the status it came with, so a route that answers outside its
//! document fails the suite that met it.
//!
//! Validated with boon, as the schemas are, its loader taken away: the
//! document holds every definition it names, and boon carries JSON Schema
//! 2020-12's own meta-schema, so nothing is fetched.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

/// The document, as the binaries embed it and serve it at
/// `/api/v1/openapi.json`.
pub const DOCUMENT: &str = include_str!("../../../../docs/api/openapi.json");

/// What the document is called to the validator: a name of its own, never
/// fetched, since the document is added to it whole.
const AT: &str = "file:///openapi.json";

/// The document read, and each schema in it compiled once asked for.
pub struct Contract {
    document: Value,
    compiler: boon::Compiler,
    schemas: boon::Schemas,
    compiled: HashMap<String, boon::SchemaIndex>,
}

thread_local! {
    /// One for each test's thread, as tests run side by side.
    static CONTRACT: RefCell<Contract> = RefCell::new(Contract::new());
}

impl Contract {
    fn new() -> Self {
        let document: Value = serde_json::from_str(DOCUMENT).expect("the document is JSON");
        let mut compiler = boon::Compiler::new();
        compiler.use_loader(Box::new(boon::SchemeUrlLoader::new()));
        compiler.set_default_draft(boon::Draft::V2020_12);
        compiler
            .add_resource(AT, document.clone())
            .expect("the document is added whole");
        Contract {
            document,
            compiler,
            schemas: boon::Schemas::new(),
            compiled: HashMap::new(),
        }
    }

    /// The document, read.
    pub fn document(&self) -> &Value {
        &self.document
    }

    /// The schema at `pointer` in the document, compiled, every reference
    /// in it resolved within the document; or why it cannot be.
    pub fn compile(&mut self, pointer: &str) -> Result<boon::SchemaIndex, String> {
        if let Some(index) = self.compiled.get(pointer) {
            return Ok(*index);
        }
        let index = self
            .compiler
            .compile(&format!("{AT}#{pointer}"), &mut self.schemas)
            .map_err(|error| format!("{pointer} does not compile: {error:#}"))?;
        self.compiled.insert(pointer.to_owned(), index);
        Ok(index)
    }

    /// Whether `value` holds to the schema at `pointer` in the document,
    /// and what keeps it from it when it does not.
    pub fn holds(&mut self, pointer: &str, value: &Value) -> Result<(), String> {
        let index = self.compile(pointer)?;
        self.schemas
            .validate(value, index)
            .map_err(|error| format!("{error:#}"))
    }

    /// The pointer to the schema of what `method` at `path` answers with
    /// `status` as JSON, its query string left out and the API's server,
    /// `/api/v1`, before it.
    pub fn answer(&self, method: &str, path: &str, status: u16) -> String {
        let route = self.route(path);
        let method = method.to_ascii_lowercase();
        let operation = &self.document["paths"][&route][&method];
        assert!(
            operation.is_object(),
            "{method} {route} is no route of the document"
        );
        let status = status.to_string();
        assert!(
            operation["responses"].get(&status).is_some(),
            "{method} {route} answered {status}, which its document does not list"
        );
        format!(
            "/paths/{}/{method}/responses/{status}/content/application~1json/schema",
            escaped(&route)
        )
    }

    /// The pointer to what each event the stream at `path` tells holds.
    pub fn event(&self, path: &str) -> String {
        format!(
            "/paths/{}/get/responses/200/content/text~1event-stream/x-itemSchema",
            escaped(&self.route(path))
        )
    }

    /// `path` as the document's paths have it, under its server.
    fn route(&self, path: &str) -> String {
        let server = self.document["servers"][0]["url"]
            .as_str()
            .expect("the document names its server");
        let path = path.split('?').next().unwrap_or(path);
        path.strip_prefix(server)
            .unwrap_or_else(|| panic!("{path} is not under {server}"))
            .to_owned()
    }
}

/// `path` as a JSON pointer spells a key that holds a `/`.
fn escaped(path: &str) -> String {
    path.replace('~', "~0").replace('/', "~1")
}

/// Run `with` with this thread's contract.
pub fn contract<T>(with: impl FnOnce(&mut Contract) -> T) -> T {
    CONTRACT.with(|contract| with(&mut contract.borrow_mut()))
}

/// That `body`, which `method` at `path` answered with `status`, holds to
/// what the document says that route answers with that status: a route
/// the document does not hold, a status it does not list for it, and a
/// body its schema refuses each fail.
pub fn answered(method: &str, path: &str, status: u16, body: &Value) {
    contract(|contract| {
        let pointer = contract.answer(method, path, status);
        if let Err(error) = contract.holds(&pointer, body) {
            panic!("{method} {path} answered {status} outside its document: {error}\n{body}");
        }
    });
}

/// That `event`, which the stream at `path` told, holds to what the
/// document says each of its events holds.
pub fn told(path: &str, event: &Value) {
    contract(|contract| {
        let pointer = contract.event(path);
        if let Err(error) = contract.holds(&pointer, event) {
            panic!("{path} told an event outside its document: {error}\n{event}");
        }
    });
}
