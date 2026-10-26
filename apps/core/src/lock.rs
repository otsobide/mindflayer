//! The lock a writer takes on a workspace, so that two writers take turns.
//!
//! The commands were written for a person at one terminal, running one at a
//! time. The TUI beside a terminal, and the server the panel runs on beside
//! both, are more than one: two writers that each read `flayer.toml`, add an
//! entry and write it back keep only the entry written second, and two gathers
//! of one source clone into one folder. So a writer takes `.mindflayer/lock`
//! before it reads what it is about to change, and holds it until the change
//! is written ([`crate::workspace::FlayerWorkspace::lock`]).
//!
//! Readers never take it, and never wait. A marker is replaced by a rename and
//! the ledger keeps a write-ahead log, so a reader finds what was there before
//! a write or what is there after it, never part of one.
//!
//! The lock is the operating system's, through `std`'s `File::lock`, for the
//! reason a stage's is ([`crate::copy::Stage`]): the system drops it when its
//! process dies, however it dies, so nothing has to decide whether a process
//! named in a file is still running, which each platform answers differently.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

/// How long a writer waits for another to finish before it gives up.
///
/// Bounded, because the writer holding the lock may be a gather waiting on
/// the network, or a process that has stopped answering, and a command that
/// hangs without a word is worse than one that says what it is waiting for.
/// Longer than anything but a gather takes, and short enough that a person
/// at the other end is told rather than left to wonder.
pub const WAIT: Duration = Duration::from_secs(10);

/// The file a writer locks, inside `.mindflayer`.
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
    /// Take the lock in `dir`, a workspace's `.mindflayer`, waiting up to
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
        let deadline = Instant::now() + wait;
        let mut pause = FIRST_PAUSE;
        loop {
            match file.try_lock() {
                Ok(()) => {
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
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(LockError::Busy {
                            path,
                            pid: holding(&holder),
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
