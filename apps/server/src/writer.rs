//! The one thread every change runs on, and every switch is made on.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc};
use std::thread;

use serde_json::json;
use tokio::sync::oneshot;

use crate::events::{Event, Events};
use crate::transcript::{Entry, Transcript};
use crate::watcher::Watcher;
use crate::{Backend, Refusal};

/// What came of a task: what it ran, or why the backend ran none of it;
/// and, around that, why the thread could not say, when it could not.
type Done = Result<Result<Entry, Refusal>, String>;

/// What came of a switch: what the backend acts on now, as it says it, or
/// why it switched to nothing; and, around that, why the thread could not
/// say, when it could not.
type Switched = Result<Result<serde_json::Value, Refusal>, String>;

/// What waits its turn on the thread, and where to say how it went: a
/// task to run, or a switch of what the backend acts on, which takes its
/// turn as a task does, so it never lands while one runs.
enum Job<B: Backend> {
    Run {
        task: B::Task,
        reply: oneshot::Sender<Done>,
    },
    Switch {
        to: B::Switch,
        reply: oneshot::Sender<Switched>,
    },
}

/// The queue in front of the thread that runs every task.
///
/// One thread fed by a channel, not tokio's blocking pool behind a lock. A
/// channel is first in, first out, so tasks run in the order they arrived; a
/// lock promises no order to whoever waits on it, and the panel shows a queue
/// of lines that must run as shown. One thread also means a write never runs
/// beside another, which is what the commands underneath were written for:
/// they come from a CLI, where one command at a time is all there ever is.
/// And a long write holds that thread, not one of the pool's, so queries go
/// on being answered while it runs.
pub(crate) struct Writer<B: Backend> {
    queue: mpsc::Sender<Job<B>>,
}

impl<B: Backend> Writer<B> {
    /// Start the thread, recording every run in `transcript`, telling
    /// `events` as each starts and finishes and as a switch is taken, and
    /// having `watcher` watch what the backend names after a switch. It
    /// lives as long as somebody can still queue a task, and ends when the
    /// last router holding this writer is dropped.
    pub(crate) fn start(
        backend: Arc<B>,
        transcript: Transcript,
        events: Events,
        watcher: Arc<Watcher>,
    ) -> io::Result<Self> {
        let (queue, jobs) = mpsc::channel::<Job<B>>();
        thread::Builder::new()
            .name(String::from("mindflayer-writes"))
            .spawn(move || {
                // Nobody may be waiting any more: a client that gives up
                // does not take its task back, which has already run.
                for job in jobs {
                    match job {
                        Job::Run { task, reply } => {
                            let _ = reply.send(perform(&*backend, task, &transcript, &events));
                        }
                        Job::Switch { to, reply } => {
                            let _ = reply.send(switch(&*backend, to, &watcher, &events));
                        }
                    }
                }
            })?;
        Ok(Self { queue })
    }

    /// Queue a task behind every one that arrived before it, and wait for it
    /// to have run, or to have been refused.
    pub(crate) async fn run(&self, task: B::Task) -> Done {
        let (reply, done) = oneshot::channel();
        self.queue
            .send(Job::Run { task, reply })
            .map_err(|_| stopped())?;
        done.await.map_err(|_| stopped())?
    }

    /// Queue a switch behind every task that arrived before it, and wait
    /// for it to have been taken, or refused.
    pub(crate) async fn switch(&self, to: B::Switch) -> Switched {
        let (reply, done) = oneshot::channel();
        self.queue
            .send(Job::Switch { to, reply })
            .map_err(|_| stopped())?;
        done.await.map_err(|_| stopped())?
    }
}

/// Why the thread could not say how a job went.
fn stopped() -> String {
    String::from("the thread that runs tasks has stopped")
}

/// Run one task: its lines first, worked out on this thread, so they are the
/// lines of the disk the run itself meets, after every write before it.
/// A task whose lines the backend refuses is never handed to it to run, and
/// nobody is told it started, since nothing did.
///
/// One that runs is told to every subscriber as it starts and as it
/// finishes, however it finishes, a refusal or a panic included, so a panel
/// showing it running never waits for an end that is not coming. Finished
/// is told once the run is in the transcript, as the reply is sent, so
/// whoever asks for the transcript on hearing it finds the run there.
///
/// A backend that panics has a bug, but the thread outlives it so the
/// tasks queued behind can still run. An outcome that will not serialize is
/// still a run that happened, so it is recorded as an error beside the
/// lines that ran rather than raised: raised, it would leave the transcript
/// a run short, and the client a bare 500 for lines that may well have
/// changed the disk.
fn perform<B: Backend>(
    backend: &B,
    task: B::Task,
    transcript: &Transcript,
    events: &Events,
) -> Done {
    let panicked = || String::from("the backend panicked while running the task");
    let lines = match panic::catch_unwind(AssertUnwindSafe(|| backend.preview(&task))) {
        Ok(Ok(preview)) => preview.lines,
        // A refused task ran nothing, so it is no run to replay.
        Ok(Err(refusal)) => return Ok(Err(refusal)),
        Err(_) => return Err(panicked()),
    };
    events.publish(Event::CommandStarted {
        lines: lines.clone(),
    });
    let done = match panic::catch_unwind(AssertUnwindSafe(move || backend.run(task))) {
        Ok(Ok(outcome)) => {
            let outcome = serde_json::to_value(outcome).unwrap_or_else(
                |error| json!({ "error": format!("the backend's outcome is not JSON: {error}") }),
            );
            let entry = Entry {
                lines: lines.clone(),
                outcome,
            };
            transcript.record(entry.clone());
            Ok(Ok(entry))
        }
        Ok(Err(refusal)) => Ok(Err(refusal)),
        Err(_) => Err(panicked()),
    };
    events.publish(Event::CommandFinished { lines });
    done
}

/// Switch what the backend acts on, and, once it is taken, have the watcher
/// watch what it names now, then tell every subscriber, in that order: so
/// whatever the panel reads on hearing it, every change to it is watched
/// for.
///
/// Told for a backend that panics too, which may have switched before it
/// did: reading everything again is how the panel finds out, and costs
/// nothing when nothing moved. Not for a refusal, which switched nothing.
fn switch<B: Backend>(backend: &B, to: B::Switch, watcher: &Watcher, events: &Events) -> Switched {
    let switched = panic::catch_unwind(AssertUnwindSafe(move || backend.switch(to)));
    if !matches!(switched, Ok(Err(_))) {
        watcher.again();
        events.publish(Event::Switched);
    }
    switched.map_err(|_| String::from("the backend panicked while switching"))
}
