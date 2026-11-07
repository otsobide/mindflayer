//! A run under way: how far it has got, as its backend says it, and how a
//! cancel reaches it.

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

use crate::code::Condition;

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

/// What a cancel taken is answered with: the lines of the run it asked to
/// stop, `{"lines": [...]}`.
///
/// Asked, not done: a run stops where its backend next looks, which a cancel
/// that comes just as it stops looking misses, and the run's own outcome is
/// what says whether it stopped. The lines say which run that is, since the
/// cancel names none: it is the run under way's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Cancelling {
    /// The command lines of the run asked to stop, each as somebody would
    /// type it.
    pub lines: Vec<String>,
}

/// A cancel a run takes: what its backend has it call
/// ([`Running::cancelled_by`]).
type Cancel = Arc<dyn Fn() + Send + Sync>;

/// A run under way, as the backend running it is handed it
/// ([`Backend::run`](crate::Backend::run)): where it says how far it has
/// got, and how a cancel reaches it.
///
/// One for each run, made by the writer thread as the run starts, its
/// progress told to every subscriber as `progress`, with the run's lines,
/// and a cancel sent to `POST /api/v1/commands/cancel` handed to it while it
/// runs. A front end that runs a task without a server, or a test, makes
/// its own ([`Running::new`]) and hears what is said where it likes, or
/// nowhere ([`Running::default`]), and cancels it itself
/// ([`Running::cancel`]). Cloned, it is the same run.
#[derive(Clone)]
pub struct Running {
    inner: Arc<Inner>,
}

struct Inner {
    /// Where each step said goes.
    told: Box<dyn Fn(&Progress) + Send + Sync>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The latest step said, none before the first.
    latest: Option<Progress>,
    /// What a cancel calls, once the backend has said one reaches the run.
    cancel: Option<Cancel>,
}

impl Running {
    /// A run whose progress goes to `told`, each step as it is said.
    pub fn new(told: impl Fn(&Progress) + Send + Sync + 'static) -> Self {
        Running {
            inner: Arc::new(Inner {
                told: Box::new(told),
                state: Mutex::default(),
            }),
        }
    }

    /// Say how far the run has got: kept as the latest, then told.
    ///
    /// From any thread the backend likes, as a gather's progress comes from
    /// one of its own while it fetches; told outside the lock the latest is
    /// kept under, so what hears it may ask anything of the run, a cancel
    /// included.
    pub fn report(&self, progress: Progress) {
        self.state().latest = Some(progress.clone());
        (self.inner.told)(&progress);
    }

    /// The latest step said, if any has been.
    pub fn progress(&self) -> Option<Progress> {
        self.state().latest.clone()
    }

    /// Have a cancel call `cancel` from now on: how a backend says one
    /// reaches the run, and how. A run whose backend never calls this takes
    /// no cancel, and a cancel sent while it runs is refused
    /// ([`Condition::RunUncancellable`]): most runs are the local disk, over
    /// before anybody could ask, and none of them looks for one.
    ///
    /// A function rather than a flag of the server's: what stops the run is
    /// the front end's own flag, a gather's watch, which the server cannot
    /// make, and setting it is all `cancel` should do. It is called where
    /// the cancel is answered, on the async runtime, so it must not block.
    pub fn cancelled_by(&self, cancel: impl Fn() + Send + Sync + 'static) {
        self.state().cancel = Some(Arc::new(cancel));
    }

    /// Ask the run to stop, as a cancel sent to the server does: call what
    /// its backend handed over, once it handed something over, while the
    /// latest step said still takes one. Asked again, it is asked again.
    ///
    /// Refused otherwise, with the condition the server answers it with and
    /// its words, and nothing called: a run that takes no cancel
    /// ([`Condition::RunUncancellable`]), or one past the point where a
    /// cancel stops it, by what its backend said last
    /// ([`Condition::CancelTooLate`]). Whether a cancel taken stopped the
    /// run, its outcome says: one that comes as the run stops looking for
    /// one is missed, and it goes on to its end.
    pub fn cancel(&self) -> Result<(), (Condition, &'static str)> {
        let cancel = {
            let state = self.state();
            let cancel = state.cancel.clone().ok_or((
                Condition::RunUncancellable,
                "the run under way takes no cancel, and goes on to its end",
            ))?;
            if state
                .latest
                .as_ref()
                .is_some_and(|progress| !progress.cancellable)
            {
                return Err((
                    Condition::CancelTooLate,
                    "the run under way is past the point a cancel stops it, and goes on to \
                     its end",
                ));
            }
            cancel
        };
        // Outside the lock, so what it calls may report a step of its own.
        cancel();
        Ok(())
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner
            .state
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

/// By hand, for what a cancel calls, which is a function: whether there is
/// one.
impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state();
        f.debug_struct("Running")
            .field("latest", &state.latest)
            .field("cancellable", &state.cancel.is_some())
            .finish()
    }
}

#[cfg(test)]
#[path = "running_test.rs"]
mod tests;
