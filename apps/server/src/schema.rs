//! What the server answers with of its own, as JSON Schema (#217): what the
//! `schema` feature adds, and nothing else builds.
//!
//! The API is described by JSON Schema written from the Rust types (D-24):
//! a front end's for what its backend is sent and answers, and the
//! server's own for what it answers whatever the backend, a preview, a run,
//! the events and a refusal. Derived beside each type's serde derive, as a
//! front end's are, so what a schema says is what serde writes. What the
//! backend hands over as JSON the server carries unread, so each place for
//! it, an item of a preview's `confirm`, a run's `outcome`, a change's
//! scope, is any JSON here: which of the backend's types fills it is the
//! front end's to say, as a description of its routes names its own types.
//!
//! The front end lists these beside its own roots ([`roots`]), since it is
//! its API they are part of; the server depends on no front end's crate, so
//! it names them through a [`Visitor`] of its own.

use schemars::JsonSchema;

use crate::{Event, Preview, Ran, Refused};

/// What [`roots`] is handed: each type the server answers with of its own,
/// for whatever is made of its schema. Only what is given, as serde writes
/// it, since everything the server is sent is the backend's.
pub trait Visitor {
    /// A type the server answers with, described as serde writes it.
    fn given<T: JsonSchema + ?Sized>(&mut self);
}

/// Every type the server answers with of its own, whatever the backend:
/// what a task's preview is ([`Preview`]), what a run did, which the
/// transcript lists too ([`Ran`]), each event the stream tells ([`Event`]),
/// and what every answer under `/api` that is no success holds
/// ([`Refused`]).
pub fn roots(visitor: &mut impl Visitor) {
    visitor.given::<Preview>();
    visitor.given::<Ran>();
    visitor.given::<Event>();
    visitor.given::<Refused>();
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
