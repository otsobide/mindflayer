//! The one thread every change runs on.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc};
use std::thread;

use serde_json::json;
use tokio::sync::oneshot;

use crate::events::{Event, Events};
use crate::transcript::{Entry, Transcript};
use crate::{Backend, Refusal};

/// What came of a task: what it ran, or why the backend ran none of it;
/// and, around that, why the thread could not say, when it could not.
type Done = Result<Result<Entry, Refusal>, String>;

/// A task waiting its turn, and where to say how it went.
struct Write<T> {
    task: T,
    reply: oneshot::Sender<Done>,
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
pub(crate) struct Writer<T> {
    queue: mpsc::Sender<Write<T>>,
}

impl<T: Send + 'static> Writer<T> {
    /// Start the thread, recording every run in `transcript` and telling
    /// `events` as each starts and finishes. It lives as long as somebody
    /// can still queue a task, and ends when the last router holding this
    /// writer is dropped.
    pub(crate) fn start<B: Backend<Task = T>>(
        backend: Arc<B>,
        transcript: Transcript,
        events: Events,
    ) -> io::Result<Self> {
        let (queue, tasks) = mpsc::channel::<Write<T>>();
        thread::Builder::new()
            .name(String::from("mindflayer-writes"))
            .spawn(move || {
                for Write { task, reply } in tasks {
                    let done = perform(&*backend, task, &transcript, &events);
                    // Nobody may be waiting any more: a client that gives up
                    // does not take its task back, which has already run.
                    let _ = reply.send(done);
                }
            })?;
        Ok(Self { queue })
    }

    /// Queue a task behind every one that arrived before it, and wait for it
    /// to have run, or to have been refused.
    pub(crate) async fn run(&self, task: T) -> Done {
        let stopped = || String::from("the thread that runs tasks has stopped");
        let (reply, done) = oneshot::channel();
        self.queue
            .send(Write { task, reply })
            .map_err(|_| stopped())?;
        done.await.map_err(|_| stopped())?
    }
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
        Ok(Ok(lines)) => lines,
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
