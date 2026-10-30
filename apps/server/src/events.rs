//! What the panel is told as it happens.
//!
//! A bus every subscriber hears all of: a tokio broadcast channel, which
//! hands each event to every receiver and keeps the latest few for one that
//! has not read them. Free of axum, as the watcher is, so a desktop shell
//! answering the panel without HTTP (D-17) takes its events from the same
//! bus; the stream at `/api/events` is only one way of carrying them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tokio::sync::broadcast::{self, error::RecvError};

/// How many events wait for a subscriber that has not read them yet before
/// it is told it fell behind ([`Event::Resync`]).
///
/// Enough for the bursts there are, a command's start and end and a change
/// or two, many times over; a subscriber further behind than this has
/// stopped reading, and what it needs then is one word to ask for
/// everything again, not the backlog.
pub const CAPACITY: usize = 64;

/// One thing the panel is told.
///
/// Tagged by its name in snake_case beside its fields, as a task and a
/// query are: `{"type": "changed", "scopes": [...]}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Something a backend watches changed on disk, so what the panel shows
    /// of each scope may be out of date: one event for every change the
    /// watcher took in together, each scope once, in the backend's words.
    Changed { scopes: Vec<serde_json::Value> },
    /// A task's lines started to run on the writer thread.
    CommandStarted { lines: Vec<String> },
    /// They stopped, however that went: the reply to the run says how.
    CommandFinished { lines: Vec<String> },
    /// The subscriber may have missed something: it has just connected, or
    /// it fell behind. Everything is to be asked for again.
    Resync,
    /// Every path the backend named is watched, so a change to one arrives
    /// as [`Changed`](Event::Changed).
    Watching,
    /// Not every one is, so a change there arrives as nothing, and what the
    /// panel shows is refreshed by hand: the system's limit on watches was
    /// reached, or a watch could not be set up at all.
    Unwatched { reason: String },
}

impl Event {
    /// Whether it says how things are watched, which a subscriber is told
    /// on its own as soon as it subscribes.
    fn is_watch(&self) -> bool {
        matches!(self, Event::Watching | Event::Unwatched { .. })
    }
}

/// What goes through the channel: an event, or the word that it is closed.
#[derive(Clone)]
enum Message {
    Event(Event),
    Closed,
}

/// The bus: publish to it, subscribe to it. Cloned, it is the same bus.
#[derive(Clone)]
pub struct Events {
    inner: Arc<Inner>,
}

struct Inner {
    sender: broadcast::Sender<Message>,
    /// Behind one lock with the send, so a subscriber is never told how
    /// things are watched twice over or not at all.
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The latest [`Event::Watching`] or [`Event::Unwatched`]; none until
    /// something watches.
    watch: Option<Event>,
    closed: bool,
}

impl Default for Events {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Events {
            inner: Arc::new(Inner {
                sender,
                state: Mutex::default(),
            }),
        }
    }
}

impl Events {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tell every subscriber, if there is one; nothing once the bus is
    /// closed. How things are watched is kept besides, for whoever
    /// subscribes next.
    pub fn publish(&self, event: Event) {
        let mut state = self.lock();
        if state.closed {
            return;
        }
        if event.is_watch() {
            state.watch = Some(event.clone());
        }
        // An error only says nobody is listening, which is no failure.
        let _ = self.inner.sender.send(Message::Event(event));
    }

    /// Hear every event from now on, after [`Event::Resync`], since
    /// whatever happened before was missed, and how things are watched,
    /// once something has said so.
    pub fn subscribe(&self) -> Subscription {
        let state = self.lock();
        let receiver = self.inner.sender.subscribe();
        let mut pending = VecDeque::new();
        if !state.closed {
            pending.push_back(Event::Resync);
            pending.extend(state.watch.clone());
        }
        Subscription {
            receiver,
            pending,
            events: self.clone(),
            done: state.closed,
        }
    }

    /// End every subscription, and every one taken from now on: for a
    /// server that is stopping, whose streams would otherwise hold it open
    /// for as long as a browser keeps the page.
    pub fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        let _ = self.inner.sender.send(Message::Closed);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// How things are watched now, if anything has said.
    fn watch(&self) -> Option<Event> {
        self.lock().watch.clone()
    }
}

/// One subscriber's hearing of the bus.
pub struct Subscription {
    receiver: broadcast::Receiver<Message>,
    /// What it is told before the channel's next event.
    pending: VecDeque<Event>,
    events: Events,
    done: bool,
}

impl Subscription {
    /// The next event, waiting for one; none once the bus is closed.
    ///
    /// One that fell behind by more than [`CAPACITY`] is told
    /// [`Event::Resync`] in place of what it missed, and how things are
    /// watched again, in case that was among it; then the events the
    /// channel still holds.
    pub async fn next(&mut self) -> Option<Event> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        if self.done {
            return None;
        }
        match self.receiver.recv().await {
            Ok(Message::Event(event)) => Some(event),
            Ok(Message::Closed) | Err(RecvError::Closed) => {
                self.done = true;
                None
            }
            Err(RecvError::Lagged(_)) => {
                self.pending.extend(self.events.watch());
                Some(Event::Resync)
            }
        }
    }
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
