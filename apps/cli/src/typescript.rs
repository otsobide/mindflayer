//! What the panel sends the CLI's backend and is answered with, in
//! TypeScript (#70): the `ts` feature's, as core's types are core's
//! (`mindflayer_core::typescript`).
//!
//! Derived, every type serde reads or writes: a query, a task and what a
//! run produced go through serde's derive, which ts-rs reads, so none needs
//! words of its own, and what they name of core's, a kind, an agent, what
//! went wrong, is written as core writes it. Only `Code`, which serde never
//! sees, is written by hand.

use std::path::PathBuf;

use mindflayer_core::typescript::{docs, lines, CoreCode};
use ts_rs::{Config, TypeVisitor, TS};

use crate::backend::{Confirm, Outcome, Planned, Query, Scanned, Switch};
use crate::render::Envelope;
use crate::task::Task;
use crate::CODES;

/// Every type the CLI hands the panel whole, or takes from it, each with
/// every type it names: a query, a task and a switch, which the panel
/// sends; what a scan, a plan, a preview and a run are answered with, the
/// last two what to confirm and what it produced; the envelope a reading
/// command's `--json` prints; and every code either crate gives.
pub fn roots(visitor: &mut impl TypeVisitor) {
    visitor.visit::<Query>();
    visitor.visit::<Task>();
    visitor.visit::<Scanned>();
    visitor.visit::<Planned>();
    visitor.visit::<Confirm>();
    visitor.visit::<Outcome>();
    visitor.visit::<Switch>();
    visitor.visit::<Envelope<'static, ()>>();
    visitor.visit::<Code>();
}

/// Every code what a front end hands on can hold, as the union of them:
/// core's ([`CoreCode`]), and the CLI's own ([`CODES`]), which reach the
/// panel beside core's in what a run says and in a refusal.
///
/// Not the type of a warning's `code`, which stays a `string`, as a
/// `Said`'s does: a union is a promise a test of the codes themselves
/// keeps, and no test holds every line a command says to a code listed.
pub struct Code;

impl TS for Code {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn docs() -> Option<String> {
        Some(docs(&[
            " Every code a front end is handed for what went wrong: core's, and the",
            " CLI's own beside them. Unique across both, and never renamed once given.",
        ]))
    }

    fn name(_: &Config) -> String {
        "Code".to_owned()
    }

    fn decl(config: &Config) -> String {
        format!("type Code ={};", Self::inline(config))
    }

    fn decl_concrete(config: &Config) -> String {
        Self::decl(config)
    }

    fn inline(config: &Config) -> String {
        lines(
            std::iter::once(CoreCode::name(config)).chain(
                CODES
                    .iter()
                    .flat_map(|(_, codes)| codes.iter())
                    .map(|code| format!("{code:?}")),
            ),
        )
    }

    fn visit_dependencies(visitor: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        visitor.visit::<CoreCode>();
    }

    fn output_path() -> Option<PathBuf> {
        Some("Code.ts".into())
    }
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
