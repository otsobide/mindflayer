//! A run under way: how far it has got, as its backend says it.

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

/// How far a run under way has got, as its backend says it
/// ([`Running::report`]): what a `progress` event carries
/// ([`Event::Progress`](crate::Event::Progress)).
///
/// The server's own shape, with the backend's words in it: a step, how many
/// of it are done and of how many, and whether a cancel still stops the
/// run. What the steps are, and from which one a cancel comes too late,
/// are the front end's rules, a gather's, so the backend says both, and
/// the server carries them unread, as it carries a scope. JSON of the
/// backend's own in its place was the other way, and is rejected: a panel
/// draws a count the same whatever counts it, and the description of the
/// API then says what an event holds, for a script to read by it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Progress {
    /// What the run is doing, as one word of the backend's own, in
    /// snake_case: `receiving`.
    pub step: String,
    /// The same, for a person, with its counts: `receiving objects 12/40`.
    pub message: String,
    /// How many of the step are done.
    pub done: u64,
    /// Of how many, once that is known.
    pub total: Option<u64>,
    /// Whether a cancel sent now still stops the run.
    pub cancellable: bool,
}

/// A run under way, as the backend running it is handed it
/// ([`Backend::run`](crate::Backend::run)): where it says how far it has
/// got.
///
/// One for each run, made by the writer thread as the run starts, its
/// progress told to every subscriber as `progress`, with the run's lines.
/// A front end that runs a task without a server, or a test, makes its own
/// ([`Running::new`]) and hears what is said where it likes, or nowhere
/// ([`Running::default`]). Cloned, it is the same run.
#[derive(Clone)]
pub struct Running {
    inner: Arc<Inner>,
}

struct Inner {
    /// Where each step said goes.
    told: Box<dyn Fn(&Progress) + Send + Sync>,
    /// The latest step said, none before the first.
    latest: Mutex<Option<Progress>>,
}

impl Running {
    /// A run whose progress goes to `told`, each step as it is said.
    pub fn new(told: impl Fn(&Progress) + Send + Sync + 'static) -> Self {
        Running {
            inner: Arc::new(Inner {
                told: Box::new(told),
                latest: Mutex::new(None),
            }),
        }
    }

    /// Say how far the run has got: kept as the latest, then told.
    ///
    /// From any thread the backend likes, as a gather's progress comes from
    /// one of its own while it fetches; told outside the lock the latest is
    /// kept under, so what hears it may ask anything of the run.
    pub fn report(&self, progress: Progress) {
        *self.latest() = Some(progress.clone());
        (self.inner.told)(&progress);
    }

    /// The latest step said, if any has been.
    pub fn progress(&self) -> Option<Progress> {
        self.latest().clone()
    }

    fn latest(&self) -> std::sync::MutexGuard<'_, Option<Progress>> {
        self.inner
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// A run nobody follows: what it says goes nowhere.
impl Default for Running {
    fn default() -> Self {
        Running::new(|_| {})
    }
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("latest", &*self.latest())
            .finish()
    }
}

#[cfg(test)]
#[path = "running_test.rs"]
mod tests;
