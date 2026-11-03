//! The API's OpenAPI document as it is committed and served, read, and
//! each schema in it compiled to hold a value to.
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
}

/// Run `with` with this thread's contract.
pub fn contract<T>(with: impl FnOnce(&mut Contract) -> T) -> T {
    CONTRACT.with(|contract| with(&mut contract.borrow_mut()))
}
