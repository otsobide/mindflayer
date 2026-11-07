//! The one thread every change runs on, and every switch is made on.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use serde_json::json;
use tokio::sync::oneshot;

use crate::code::Condition;
use crate::events::{Event, Events};
use crate::log::{Log, Planned};
use crate::running::{Cancelling, Running};
use crate::transcript::{Ran, Transcript};
use crate::watcher::Watcher;
use crate::{Backend, Refusal};

/// What came of a task: what it ran, or why the backend ran none of it;
/// and, around that, why the thread could not say, when it could not.
type Done = Result<Result<Ran, Refusal>, Failed>;

/// What came of a switch: what the backend acts on now, as it says it, or
/// why it switched to nothing; and, around that, why the thread could not
/// say, when it could not.
type Switched = Result<Result<serde_json::Value, Refusal>, Failed>;

/// Why the thread could not say how a job went: the condition, a 500 of the
/// server's own, and the words for it, which are fixed, since what failed
/// left nothing to say of itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Failed {
    pub(crate) condition: Condition,
    pub(crate) message: &'static str,
}

/// What waits its turn on the thread, when it was queued, and where to say
/// how it went: a task to run, or a switch of what the backend acts on,
/// which takes its turn as a task does, so it never lands while one runs.
enum Job<B: Backend> {
    Run {
        task: B::Task,
        sent: Instant,
        reply: oneshot::Sender<Done>,
    },
    Switch {
        to: B::Switch,
        sent: Instant,
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
    /// The run the thread is on, which a cancel reaches beside the queue,
    /// and how many jobs wait their turn behind it.
    underway: Underway,
    /// Where a job sent while a run is under way is told to wait.
    events: Events,
}

/// The run under way on the writer thread, if one is, and how many jobs
/// wait their turn: what a cancel reaches the run through, while the
/// thread is the run's, and what a `busy` event says.
#[derive(Clone, Default)]
struct Underway(Arc<Mutex<Turns>>);

#[derive(Default)]
struct Turns {
    current: Option<Current>,
    /// Jobs queued and not yet taken by the thread.
    waiting: usize,
}

/// A run under way: its lines, and what its backend was handed of it.
#[derive(Clone)]
struct Current {
    lines: Vec<String>,
    running: Running,
}

impl Underway {
    /// A job joins the queue: counted, and, when a run is under way, told
    /// to every subscriber as waiting behind it ([`Event::Busy`]).
    ///
    /// Told under the lock the count is kept under, so two jobs sent at
    /// once are told as one and then two, never two twice.
    fn queued(&self, events: &Events) {
        let mut turns = self.lock();
        turns.waiting += 1;
        if let Some(current) = &turns.current {
            events.publish(Event::Busy {
                lines: current.lines.clone(),
                waiting: turns.waiting,
            });
        }
    }

    /// A job counted never reached the queue, the thread having stopped.
    fn unqueued(&self) {
        let mut turns = self.lock();
        turns.waiting = turns.waiting.saturating_sub(1);
    }

    /// The thread takes the next job from the queue.
    fn taken(&self) {
        self.unqueued();
    }

    /// The run of `lines` is under way, from its start, once its lines
    /// passed, to its end: told to every subscriber as started only once
    /// it is, so a cancel sent on hearing that reaches it, as one sent on
    /// hearing it finished is told nothing runs; and told as busy when jobs
    /// still wait behind it, so the panel knows what they wait for now.
    fn started(&self, lines: &[String], running: &Running, events: &Events) {
        let mut turns = self.lock();
        turns.current = Some(Current {
            lines: lines.to_vec(),
            running: running.clone(),
        });
        // Under the lock, as a job queued is told, so busy behind this run
        // is never told before it started.
        events.publish(Event::CommandStarted {
            lines: lines.to_vec(),
        });
        if turns.waiting > 0 {
            events.publish(Event::Busy {
                lines: lines.to_vec(),
                waiting: turns.waiting,
            });
        }
    }

    /// None is, whatever came of the one that was.
    fn finished(&self) {
        self.lock().current = None;
    }

    /// The run under way, if one is: taken out of the lock, so what a
    /// cancel calls runs outside it.
    fn current(&self) -> Option<Current> {
        self.lock().current.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Turns> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<B: Backend> Writer<B> {
    /// Start the thread, recording every run in `transcript`, telling
    /// `events` as each starts and finishes and as a switch is taken,
    /// having `watcher` watch what the backend names after a switch, and
    /// saying in `log`, when there is one, how long each job waited its
    /// turn and took. It lives as long as somebody can still queue a task,
    /// and ends when the last router holding this writer is dropped.
    pub(crate) fn start(
        backend: Arc<B>,
        transcript: Transcript,
        events: Events,
        watcher: Arc<Watcher>,
        log: Option<Arc<Log>>,
    ) -> io::Result<Self> {
        let (queue, jobs) = mpsc::channel::<Job<B>>();
        let underway = Underway::default();
        let on_it = underway.clone();
        let told = events.clone();
        thread::Builder::new()
            .name(String::from("mindweaver-writes"))
            .spawn(move || {
                // Nobody may be waiting any more: a client that gives up
                // does not take its task back, which has already run.
                let log = log.as_deref();
                for job in jobs {
                    on_it.taken();
                    match job {
                        Job::Run { task, sent, reply } => {
                            let took = Took { sent, log };
                            let around = Around {
                                transcript: &transcript,
                                events: &events,
                                underway: &on_it,
                            };
                            let _ = reply.send(perform(&*backend, task, around, took));
                        }
                        Job::Switch { to, sent, reply } => {
                            let took = Took { sent, log };
                            let _ = reply.send(switch(&*backend, to, &watcher, &events, took));
                        }
                    }
                }
            })?;
        Ok(Self {
            queue,
            underway,
            events: told,
        })
    }

    /// Queue a task behind every one that arrived before it, and wait for it
    /// to have run, or to have been refused.
    pub(crate) async fn run(&self, task: B::Task) -> Done {
        let (reply, done) = oneshot::channel();
        let sent = Instant::now();
        self.enqueue(Job::Run { task, sent, reply })?;
        done.await.map_err(|_| stopped())?
    }

    /// Queue a switch behind every task that arrived before it, and wait
    /// for it to have been taken, or refused.
    pub(crate) async fn switch(&self, to: B::Switch) -> Switched {
        let (reply, done) = oneshot::channel();
        let sent = Instant::now();
        self.enqueue(Job::Switch { to, sent, reply })?;
        done.await.map_err(|_| stopped())?
    }

    /// Put `job` behind every one sent before it, counted, and told as
    /// waiting when a run is under way: one at a time is the thread's rule,
    /// so a gather that takes a minute holds every write sent meanwhile,
    /// and the panel is told why nothing it sent has started.
    fn enqueue(&self, job: Job<B>) -> Result<(), Failed> {
        self.underway.queued(&self.events);
        self.queue.send(job).map_err(|_| {
            self.underway.unqueued();
            stopped()
        })
    }

    /// Ask the run under way to stop ([`Running::cancel`]), and say which
    /// run that is; or why none was asked, with nothing changed.
    ///
    /// Beside the queue rather than on it: the thread is the run's until it
    /// ends, so a cancel queued behind it would arrive once there is
    /// nothing left to stop. What it calls is the backend's, held as a
    /// call of the backend's is, so one that panics loses that answer and
    /// not the server.
    pub(crate) fn cancel(&self) -> Result<Cancelling, Failed> {
        let Current { lines, running } = self.underway.current().ok_or(Failed {
            condition: Condition::NothingRunning,
            message: "no run is under way to cancel",
        })?;
        match panic::catch_unwind(AssertUnwindSafe(|| running.cancel())) {
            Ok(Ok(())) => Ok(Cancelling { lines }),
            Ok(Err((condition, message))) => Err(Failed { condition, message }),
            Err(_) => Err(panicked("the backend panicked while cancelling the run")),
        }
    }
}

/// What a run is recorded in, told to and reached through by a cancel, as
/// it runs.
#[derive(Clone, Copy)]
struct Around<'a> {
    transcript: &'a Transcript,
    events: &'a Events,
    underway: &'a Underway,
}

/// The thread has stopped, so nothing queued on it is answered.
fn stopped() -> Failed {
    Failed {
        condition: Condition::WriterStopped,
        message: "the thread that runs tasks has stopped",
    }
}

/// When a job was queued, and the log to say how long it waited and took
/// in, if the front end gave one.
///
/// Said around the backend's calls rather than worked out from the reply:
/// a request line says only how long the whole answer took, and a run that
/// took seconds could have waited behind another, been slow to work out,
/// or been slow to run, which a log says apart (#240).
#[derive(Clone, Copy)]
struct Took<'a> {
    sent: Instant,
    log: Option<&'a Log>,
}

/// The backend panicked, as `message` says where, so there is no answer
/// to give.
fn panicked(message: &'static str) -> Failed {
    Failed {
        condition: Condition::BackendFailed,
        message,
    }
}

/// Run one task: its lines first, worked out on this thread, so they are the
/// lines of the disk the run itself meets, after every write before it.
/// A task whose lines the backend refuses is never handed to it to run, and
/// nobody is told it started, since nothing did.
///
/// One that runs is told to every subscriber as it starts and as it
/// finishes, however it finishes, a refusal or a panic included, so a panel
/// showing it running never waits for an end that is not coming, and in
/// between each step the backend says of how far it got ([`Running`]).
/// Started is told once the run is under way, so a cancel sent on hearing
/// it reaches the run. Finished is told once the run is in the transcript,
/// as the reply is sent, so whoever asks for the transcript on hearing it
/// finds the run there, and once it is no longer under way, so a cancel
/// sent on hearing it is told nothing runs.
///
/// Said in the log, when there is one, once its lines are worked out, with
/// how long it waited its turn and how long they took to work out, and
/// again once they have run, however that went, with how long they took
/// ([`Took`]): the first before any line runs, so a run that never ends
/// is said to have started.
///
/// A backend that panics has a bug, but the thread outlives it so the
/// tasks queued behind can still run. An outcome that will not serialize is
/// still a run that happened, so it is recorded as an error beside the
/// lines that ran rather than raised: raised, it would leave the transcript
/// a run short, and the client a bare 500 for lines that may well have
/// changed the disk.
fn perform<B: Backend>(backend: &B, task: B::Task, around: Around<'_>, took: Took<'_>) -> Done {
    let Around {
        transcript,
        events,
        underway,
    } = around;
    let failed = || panicked("the backend panicked while running the task");
    let queued = took.sent.elapsed();
    let working = Instant::now();
    let planned = panic::catch_unwind(AssertUnwindSafe(|| backend.preview(&task)));
    if let Some(log) = took.log {
        let planned = match &planned {
            Ok(Ok(preview)) => Planned::Lines(preview.lines.len()),
            Ok(Err(_)) => Planned::Refused,
            Err(_) => Planned::Panicked,
        };
        log.planned(queued, working.elapsed(), planned);
    }
    let lines = match planned {
        Ok(Ok(preview)) => preview.lines,
        // A refused task ran nothing, so it is no run to replay.
        Ok(Err(refusal)) => return Ok(Err(refusal)),
        Err(_) => return Err(failed()),
    };
    // Each step it says told with its lines, as its start and its finish
    // are, so a subscriber that heard neither knows which run it is of.
    let running = Running::new({
        let (events, lines) = (events.clone(), lines.clone());
        move |progress| {
            events.publish(Event::Progress {
                lines: lines.clone(),
                progress: progress.clone(),
            });
        }
    });
    underway.started(&lines, &running, events);
    let started = Instant::now();
    let ran = panic::catch_unwind(AssertUnwindSafe(|| backend.run(task, &running)));
    underway.finished();
    if let Some(log) = took.log {
        log.ran(started.elapsed(), lines.len());
    }
    let done = match ran {
        Ok(Ok(outcome)) => {
            let outcome = serde_json::to_value(outcome).unwrap_or_else(
                |error| json!({ "error": format!("the backend's outcome is not JSON: {error}") }),
            );
            let ran = Ran {
                lines: lines.clone(),
                outcome,
            };
            transcript.record(ran.clone());
            Ok(Ok(ran))
        }
        Ok(Err(refusal)) => Ok(Err(refusal)),
        Err(_) => Err(failed()),
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
fn switch<B: Backend>(
    backend: &B,
    to: B::Switch,
    watcher: &Watcher,
    events: &Events,
    took: Took<'_>,
) -> Switched {
    let queued = took.sent.elapsed();
    let switching = Instant::now();
    let switched = panic::catch_unwind(AssertUnwindSafe(move || backend.switch(to)));
    if !matches!(switched, Ok(Err(_))) {
        watcher.again();
        events.switched();
    }
    if let Some(log) = took.log {
        log.switched(queued, switching.elapsed());
    }
    switched.map_err(|_| panicked("the backend panicked while switching"))
}
