//! What the session has run.

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

/// What one write did: the lines it was, and what running them produced.
#[derive(Clone, Serialize)]
pub(crate) struct Entry {
    pub(crate) lines: Vec<String>,
    pub(crate) outcome: serde_json::Value,
}

/// Every task the session ran, in the order it ran them: what the TUI prints
/// when it closes, kept here so the panel can show it at any time and a
/// person can replay it as a script.
///
/// Only runs: a query or a preview changes nothing, so it has nothing to
/// replay. And only in memory, for as long as the server runs, as the TUI's
/// lasts as long as its screen.
#[derive(Clone, Default)]
pub(crate) struct Transcript(Arc<Mutex<Vec<Entry>>>);

impl Transcript {
    /// Add what a run did. Called on the writer thread, after the run and
    /// before anyone is told it finished, so the order here is the order the
    /// runs happened in and a reply never gets ahead of the transcript.
    pub(crate) fn record(&self, entry: Entry) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }

    pub(crate) fn entries(&self) -> Vec<Entry> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
