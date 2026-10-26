//! The one thread every change runs on.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc};
use std::thread;

use tokio::sync::oneshot;

use crate::transcript::{Entry, Transcript};
use crate::Backend;

/// A task waiting its turn, and where to say how it went.
struct Write<T> {
    task: T,
    reply: oneshot::Sender<Result<Entry, String>>,
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
    /// Start the thread, recording every run in `transcript`. It lives as
    /// long as somebody can still queue a task, and ends when the last router
    /// holding this writer is dropped.
    pub(crate) fn start<B: Backend<Task = T>>(
        backend: Arc<B>,
        transcript: Transcript,
    ) -> io::Result<Self> {
        let (queue, tasks) = mpsc::channel::<Write<T>>();
        thread::Builder::new()
            .name(String::from("mindflayer-writes"))
            .spawn(move || {
                for Write { task, reply } in tasks {
                    // A backend that panics has a bug, but the thread
                    // outlives it so the tasks queued behind can still run.
                    let done = panic::catch_unwind(AssertUnwindSafe(|| perform(&*backend, task)))
                        .unwrap_or_else(|_| {
                            Err(String::from("the backend panicked while running the task"))
                        });
                    if let Ok(entry) = &done {
                        transcript.record(entry.clone());
                    }
                    // Nobody may be waiting any more: a client that gives up
                    // does not take its task back, which has already run.
                    let _ = reply.send(done);
                }
            })?;
        Ok(Self { queue })
    }

    /// Queue a task behind every one that arrived before it, and wait for it
    /// to have run.
    pub(crate) async fn run(&self, task: T) -> Result<Entry, String> {
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
fn perform<B: Backend>(backend: &B, task: B::Task) -> Result<Entry, String> {
    let lines = backend.preview(&task);
    let outcome = backend.run(task);
    let outcome = serde_json::to_value(outcome)
        .map_err(|error| format!("the backend's outcome is not JSON: {error}"))?;
    Ok(Entry { lines, outcome })
}
