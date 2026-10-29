//! What the panel sends the CLI's backend and is answered with, in
//! TypeScript (#70): the `ts` feature's, as core's types are core's
//! (`mindflayer_core::typescript`).
//!
//! Derived, every type serde reads or writes: a query, a task and what a
//! run produced go through serde's derive, which ts-rs reads, so none needs
//! words of its own, and what they name of core's, a kind, an agent, what
//! went wrong, is written as core writes it.

use ts_rs::TypeVisitor;

use crate::backend::{Outcome, Query};
use crate::task::Task;

/// Every type the CLI hands the panel whole, or takes from it, each with
/// every type it names: a query and a task, which the panel sends, and what
/// a run produced, which it is answered with.
pub fn roots(visitor: &mut impl TypeVisitor) {
    visitor.visit::<Query>();
    visitor.visit::<Task>();
    visitor.visit::<Outcome>();
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
