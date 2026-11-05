//! The lock a writer takes on a workspace, so that two writers take turns.
//!
//! The commands were written for a person at one terminal, running one at a
//! time. The TUI beside a terminal, and the server the panel runs on beside
//! both, are more than one: two writers that each read `weaver.toml`, add an
//! entry and write it back keep only the entry written second, and two gathers
//! of one source clone into one folder. So a writer takes `.mindweaver/lock`
//! before it reads what it is about to change, and holds it until the change
//! is written ([`crate::workspace::WeaverWorkspace::lock`]).
//!
//! Readers never take it. A marker is replaced by a rename, and a read of the
//! ledger sees only what was committed, so each file a reader opens is whole,
//! as it was before a write or after it. A change made in several steps is
//! not: a read between an install's copy and its record finds the copy
//! without the record. The ledger keeps a write-ahead log, so a read does not
//! wait for a write to it either, except on a filesystem that cannot keep
//! one, where it waits for the write to be committed.
//!
//! The lock is the operating system's, through `std`'s `File::lock`, for the
//! reason a stage's is ([`crate::copy::Stage`]): the system drops it when its
//! process dies, however it dies, so nothing has to decide whether a process
//! named in a file is still running, which each platform answers differently.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::code::codes;

/// How long a writer waits for another to finish before it gives up.
///
/// Bounded, because the writer holding the lock may be a gather waiting on
/// the network, or a process that has stopped answering, and a command that
/// hangs without a word is worse than one that says what it is waiting for.
/// Longer than anything but a gather takes, and short enough that a person
/// at the other end is told rather than left to wonder.
pub const WAIT: Duration = Duration::from_secs(10);

/// The file a writer locks, inside `.mindweaver`.
pub const LOCK_FILE: &str = "lock";

/// Where the writer holding [`LOCK_FILE`] writes its process id, beside it,
/// for a writer kept waiting to name.
///
/// A file of its own rather than the lock file's contents, because a lock is
/// mandatory on Windows: while one process holds it, no other may read a byte
/// of the file it locks, and the id is wanted by exactly the processes that
/// cannot have the lock.
pub const HOLDER_FILE: &str = "lock.pid";

/// The first pause between two tries at a lock another writer holds, doubled
/// after each try up to [`LONGEST_PAUSE`].
///
/// Polled with `try_lock` because `std` cannot wait for a lock with a
/// deadline, and a thread blocked in `lock` could not be told to stop. Short
/// at first, because most changes take milliseconds; capped, so a writer that
/// has waited a while still notices within a tenth of a second that the lock
/// is free.
const FIRST_PAUSE: Duration = Duration::from_millis(2);
const LONGEST_PAUSE: Duration = Duration::from_millis(100);

/// How long writers were kept out of a workspace by a lock another writer
/// held, and by which process: what [`waits`] says of the work it ran.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Waited {
    /// Every wait added up, one that ended busy among them: zero when every
    /// lock the work asked for was free.
    pub total: Duration,
    /// The process the last wait found holding the lock, when the holder
    /// had said who it was ([`HOLDER_FILE`]), as [`LockError::Busy`] names
    /// it.
    pub holder: Option<u32>,
}

impl Waited {
    const NONE: Waited = Waited {
        total: Duration::ZERO,
        holder: None,
    };

    /// These waits and then `later`'s, the holder the latest one named.
    fn then(self, later: Waited) -> Waited {
        Waited {
            total: self.total.saturating_add(later.total),
            holder: later.holder.or(self.holder),
        }
    }
}

thread_local! {
    /// The waits of the writers this thread has run since the innermost
    /// [`waits`] began. Per thread, since a writer runs on the thread that
    /// asked it to, and a wait on another is another command's.
    static WAITED: Cell<Waited> = const { Cell::new(Waited::NONE) };
}

/// Run `work`, and say how long the writers it ran on this thread were kept
/// waiting for a workspace's lock another writer held, and by which
/// process.
///
/// For a front end that runs commands for somebody watching, to tell a
/// command that was slow from one that waited its turn behind another
/// writer, which neither says of itself once it has finished (#240).
/// Counted here rather than handed back by every writer, which would carry
/// a duration through each of core's writers and every front end's call of
/// one, for the one front end that asks. Nested, the outer one counts the
/// inner one's waits too.
pub fn waits<T>(work: impl FnOnce() -> T) -> (T, Waited) {
    /// Puts back what was counted before, with what was counted since added,
    /// however `work` ends: a panic caught further up leaves the count
    /// around it with this one's waits in it, rather than lose them, or
    /// everything counted before.
    struct Resume(Waited);
    impl Drop for Resume {
        fn drop(&mut self) {
            WAITED.with(|waited| waited.set(self.0.then(waited.get())));
        }
    }
    let resume = Resume(WAITED.with(|waited| waited.replace(Waited::NONE)));
    let value = work();
    let waited = WAITED.with(Cell::get);
    drop(resume);
    (value, waited)
}

/// Count a wait of `total` for a lock `holder` held on this thread.
fn count(total: Duration, holder: Option<u32>) {
    WAITED.with(|waited| waited.set(waited.get().then(Waited { total, holder })));
}

/// A workspace held for a change, until this is dropped.
///
/// Not re-entrant: a writer holding it that asks for it again waits for
/// itself, and fails as busy. Core's writers take it themselves, so a front
/// end holding it must not ask core to write.
#[derive(Debug)]
#[must_use = "the workspace is unlocked as soon as this is dropped"]
pub struct WorkspaceLock {
    holder: PathBuf,
    /// `None` on a filesystem without locks ([`WorkspaceLock::is_held`]).
    file: Option<File>,
}

impl WorkspaceLock {
    /// Take the lock in `dir`, a workspace's `.mindweaver`, waiting up to
    /// `wait` for a writer that holds it.
    pub(crate) fn acquire(dir: &Path, wait: Duration) -> Result<Self, LockError> {
        let path = dir.join(LOCK_FILE);
        // Never truncated, and never deleted, even by the writer that made
        // it: a writer waiting on the file it opened would otherwise go on to
        // lock a file nobody else can see, beside the one the next writer
        // makes, and the two would write at once.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| LockError::Open {
                path: path.clone(),
                source,
            })?;
        let holder = dir.join(HOLDER_FILE);
        let started = Instant::now();
        // None for a wait too long to add to now, which is a wait with no end.
        let deadline = started.checked_add(wait);
        let mut pause = FIRST_PAUSE;
        // Once another writer has kept this one out: the last holder that
        // said who it was, if any did.
        let mut kept: Option<Option<u32>> = None;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    if let Some(by) = kept {
                        count(started.elapsed(), by);
                    }
                    // Best effort: the lock is what keeps writers apart, and
                    // the id only says who has it. One that cannot be written
                    // costs a waiting writer the name, not the wait.
                    let _ = fs::write(&holder, std::process::id().to_string());
                    return Ok(WorkspaceLock {
                        holder,
                        file: Some(file),
                    });
                }
                Err(TryLockError::WouldBlock) => {
                    let pid = holding(&holder);
                    let by = pid.or(kept.flatten());
                    kept = Some(by);
                    let left = deadline.map_or(LONGEST_PAUSE, |deadline| {
                        deadline.saturating_duration_since(Instant::now())
                    });
                    if left.is_zero() {
                        count(started.elapsed(), by);
                        return Err(LockError::Busy {
                            path,
                            pid,
                            waited: wait,
                        });
                    }
                    thread::sleep(pause.min(left));
                    pause = (pause * 2).min(LONGEST_PAUSE);
                }
                // A filesystem without locks. Refusing would refuse every
                // change there, so writers go ahead unlocked, as they did
                // before there was a lock; the names their temporaries and
                // stages are given still keep their files apart. Any error,
                // not only `Unsupported`, for the reason a stage gives: NFS
                // without its lock daemon says "no locks available", which
                // `std` leaves uncategorized.
                Err(TryLockError::Error(_)) => {
                    return Ok(WorkspaceLock { holder, file: None });
                }
            }
        }
    }

    /// Whether the system took the lock: false only on a filesystem that
    /// has no locks, where this writer goes ahead with nothing keeping
    /// others out.
    pub fn is_held(&self) -> bool {
        self.file.is_some()
    }
}

impl WorkspaceLock {
    /// Say that the folder holding the lock is `dir` now, for one renamed
    /// while it is held, as a workspace moved to Mindweaver's names is: the
    /// lock is on the file, wherever its folder went, and the holder's id is
    /// taken away from where it is once the lock is let go.
    pub(crate) fn moved(&mut self, dir: &Path) {
        self.holder = dir.join(HOLDER_FILE);
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            // Taken away while the lock is still held, so it never names this
            // process to a writer kept waiting by the next one.
            let _ = fs::remove_file(&self.holder);
            // Released here rather than when the handle closes, which
            // Windows may get round to some time later.
            let _ = file.unlock();
        }
    }
}

/// The process id the holder wrote, if it has written one yet and it reads
/// as one.
fn holding(holder: &Path) -> Option<u32> {
    fs::read_to_string(holder).ok()?.trim().parse().ok()
}

/// Why a writer could not have the workspace.
#[derive(Debug, Error)]
pub enum LockError {
    #[error("{path}: workspace busy: waited {waited:?} for {} to finish writing to it; try again once it has", holder(*.pid))]
    Busy {
        /// The lock file.
        path: PathBuf,
        /// The process holding it, when it could be read.
        pid: Option<u32>,
        /// How long this writer waited.
        waited: Duration,
    },
    #[error("{path}: cannot be opened to lock the workspace: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(LockError {
    Self::Busy { .. } => "workspace_busy",
    Self::Open { .. } => "lock_open_failed",
});

impl LockError {
    /// The lock file.
    pub fn path(&self) -> &Path {
        match self {
            LockError::Busy { path, .. } | LockError::Open { path, .. } => path,
        }
    }
}

/// Who a busy error says is writing.
fn holder(pid: Option<u32>) -> String {
    match pid {
        Some(pid) => format!("process {pid}"),
        None => String::from("another process"),
    }
}

#[cfg(test)]
#[path = "lock_test.rs"]
mod tests;
