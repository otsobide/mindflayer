//! Unit tests for `lock.rs`, kept beside it rather than inside it.
//!
//! The lock is the operating system's, taken on a file a writer opens for
//! itself, so two acquisitions in one process keep each other out as two
//! processes would: each test here plays both writers on one temporary
//! `.mindweaver`.

use super::*;

use std::sync::mpsc;

use tempfile::TempDir;

use crate::mother::random;

/// A `.mindweaver` of its own, as a workspace holds one.
fn folder() -> TempDir {
    TempDir::new().expect("make a temporary folder")
}

fn holder_in(dir: &TempDir) -> PathBuf {
    dir.path().join(HOLDER_FILE)
}

#[test]
fn a_lock_nobody_holds_is_taken_and_names_this_process() {
    let dir = folder();

    let lock = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();

    assert!(lock.is_held());
    assert!(dir.path().join(LOCK_FILE).is_file());
    assert_eq!(
        fs::read_to_string(holder_in(&dir)).unwrap(),
        std::process::id().to_string()
    );
}

/// The holder's id goes while the lock is still held, so it never names a
/// process that has finished to the next writer kept waiting.
#[test]
fn dropping_the_lock_takes_the_holder_s_id_away_and_keeps_the_lock_file() {
    let dir = folder();
    let lock = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();

    drop(lock);

    assert!(!holder_in(&dir).exists());
    assert!(dir.path().join(LOCK_FILE).is_file(), "never deleted");
}

/// A writer waiting on the file it opened would otherwise lock one nobody
/// else can see, so the file is neither emptied nor replaced.
#[test]
fn the_lock_file_is_never_truncated_by_a_writer_taking_it() {
    let dir = folder();
    let held = random::text(30);
    fs::write(dir.path().join(LOCK_FILE), &held).unwrap();

    drop(WorkspaceLock::acquire(dir.path(), WAIT).unwrap());

    assert_eq!(
        fs::read_to_string(dir.path().join(LOCK_FILE)).unwrap(),
        held
    );
}

#[test]
fn a_held_lock_keeps_a_second_writer_out_and_names_the_holder() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();

    let error = WorkspaceLock::acquire(dir.path(), Duration::ZERO).unwrap_err();

    match &error {
        LockError::Busy { path, pid, waited } => {
            assert_eq!(path, &dir.path().join(LOCK_FILE));
            assert_eq!(*pid, Some(std::process::id()));
            assert_eq!(*waited, Duration::ZERO);
        }
        other => panic!("not busy: {other:?}"),
    }
    assert_eq!(error.code(), "workspace_busy");
}

/// Not re-entrant: a writer asking again for what it holds waits for
/// itself, which is why core's writers never call one another.
#[test]
fn a_writer_asking_again_for_the_lock_it_holds_is_kept_out_as_busy() {
    let dir = folder();
    let held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();

    let again = WorkspaceLock::acquire(dir.path(), Duration::from_millis(5));

    assert!(matches!(again, Err(LockError::Busy { .. })), "{again:?}");
    drop(held);
}

#[test]
fn a_writer_kept_out_waits_as_long_as_it_was_told_and_says_so() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let wait = Duration::from_millis(random::between(20, 80) as u64);

    let started = Instant::now();
    let error = WorkspaceLock::acquire(dir.path(), wait).unwrap_err();

    assert!(
        started.elapsed() >= wait,
        "{:?} < {wait:?}",
        started.elapsed()
    );
    assert!(
        matches!(error, LockError::Busy { waited, .. } if waited == wait),
        "{error:?}"
    );
}

/// The lock is let go when the holder drops it, and the next writer takes
/// it then rather than once its whole wait has passed.
#[test]
fn a_writer_kept_waiting_takes_the_lock_once_the_holder_lets_it_go() {
    let dir = folder();
    let held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let (taken, told) = mpsc::channel();
    let path = dir.path().to_path_buf();
    let waiter = thread::spawn(move || {
        let lock = WorkspaceLock::acquire(&path, WAIT);
        taken.send(Instant::now()).unwrap();
        lock
    });

    thread::sleep(Duration::from_millis(30));
    let released = Instant::now();
    drop(held);
    let lock = waiter.join().unwrap().unwrap();
    let at = told.recv().unwrap();

    assert!(lock.is_held());
    assert!(at >= released, "taken before it was let go");
    assert!(
        at.duration_since(released) < WAIT,
        "waited out the whole wait"
    );
}

#[test]
fn a_lock_let_go_is_taken_again_at_once() {
    let dir = folder();
    for _ in 0..random::between(2, 6) {
        let lock = WorkspaceLock::acquire(dir.path(), Duration::ZERO).unwrap();

        assert!(lock.is_held());
    }
}

/// The id says only who has the lock: one that reads as none leaves a
/// waiting writer saying another process holds it, never a wrong one.
#[test]
fn a_holder_file_that_names_no_process_leaves_the_holder_unnamed() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    for written in ["", "pid", "-1", "4294967296", "12 34", "0x1f"] {
        fs::write(holder_in(&dir), written).unwrap();

        let error = WorkspaceLock::acquire(dir.path(), Duration::ZERO).unwrap_err();

        assert!(
            matches!(error, LockError::Busy { pid: None, .. }),
            "{written:?} gave {error:?}"
        );
        assert!(error.to_string().contains("another process"), "{error}");
    }
}

#[test]
fn a_holder_s_id_is_read_with_the_whitespace_around_it_left_out() {
    let dir = folder();
    for _ in 0..50 {
        let pid = random::number() as u32;
        let padding = random::pick(&["", " ", "\n", "\r\n", "\t"]);
        fs::write(holder_in(&dir), format!("{padding}{pid}{padding}")).unwrap();

        assert_eq!(holding(&holder_in(&dir)), Some(pid), "{padding:?}{pid}");
    }
}

#[test]
fn no_holder_file_names_no_holder() {
    let dir = folder();

    assert_eq!(holding(&holder_in(&dir)), None);
}

#[test]
fn a_busy_workspace_says_which_process_holds_it_and_how_long_it_waited() {
    let error = LockError::Busy {
        path: PathBuf::from("/w/.mindweaver/lock"),
        pid: Some(4242),
        waited: Duration::from_secs(10),
    };

    assert_eq!(
        error.to_string(),
        "/w/.mindweaver/lock: workspace busy: waited 10s for process 4242 to finish \
         writing to it; try again once it has"
    );
}

#[test]
fn a_busy_workspace_whose_holder_is_unknown_says_another_process_holds_it() {
    let error = LockError::Busy {
        path: PathBuf::from("/w/.mindweaver/lock"),
        pid: None,
        waited: Duration::from_millis(1500),
    };

    assert_eq!(
        error.to_string(),
        "/w/.mindweaver/lock: workspace busy: waited 1.5s for another process to finish \
         writing to it; try again once it has"
    );
}

#[test]
fn a_holder_is_named_by_its_process_or_as_another() {
    for _ in 0..50 {
        let pid = random::number() as u32;

        assert_eq!(holder(Some(pid)), format!("process {pid}"));
    }
    assert_eq!(holder(None), "another process");
}

#[test]
fn a_lock_in_a_folder_that_is_not_there_cannot_be_opened() {
    let dir = folder();
    let missing = dir.path().join("gone");

    let error = WorkspaceLock::acquire(&missing, WAIT).unwrap_err();

    assert!(matches!(error, LockError::Open { .. }), "{error:?}");
    assert_eq!(error.code(), "lock_open_failed");
    assert_eq!(error.path(), missing.join(LOCK_FILE));
    assert!(
        error.to_string().starts_with(&format!(
            "{}: cannot be opened to lock the workspace: ",
            missing.join(LOCK_FILE).display()
        )),
        "{error}"
    );
    assert!(!missing.exists(), "the folder is not made for the lock");
}

#[test]
fn every_lock_error_names_the_lock_file() {
    let path = PathBuf::from(format!("/{}/.mindweaver/lock", random::word()));
    let errors = [
        LockError::Busy {
            path: path.clone(),
            pid: None,
            waited: WAIT,
        },
        LockError::Open {
            path: path.clone(),
            source: io::Error::other("denied"),
        },
    ];
    for error in errors {
        assert_eq!(error.path(), path, "{error:?}");
    }
}

#[test]
fn the_lock_s_codes_are_busy_and_unopened() {
    assert_eq!(LockError::CODES, ["workspace_busy", "lock_open_failed"]);
}

/// A wait too long to add to now has no deadline, which is a wait with no
/// end: a lock nobody holds is still taken at once.
#[test]
fn a_wait_with_no_end_still_takes_a_free_lock_at_once() {
    let dir = folder();

    let lock = WorkspaceLock::acquire(dir.path(), Duration::MAX).unwrap();

    assert!(lock.is_held());
}

/// Bounded, so a command is told what it waits for rather than hang: a
/// wait well past a person's patience would read as a hang.
#[test]
fn a_writer_waits_a_bounded_while_that_a_person_waits_out() {
    assert!(WAIT >= Duration::from_secs(1), "{WAIT:?}");
    assert!(WAIT <= Duration::from_secs(60), "{WAIT:?}");
    assert!(FIRST_PAUSE < LONGEST_PAUSE);
    assert!(LONGEST_PAUSE < WAIT);
}

// ---------------------------------------------------------------------------
// What a thread's writers waited (`waits`)
// ---------------------------------------------------------------------------

/// A writer kept out until it gives up, as [`waits`] counts it: `wait` long
/// at least, by the process holding the lock, which is this one.
fn kept_out(dir: &TempDir, wait: Duration) {
    let error = WorkspaceLock::acquire(dir.path(), wait).unwrap_err();
    assert!(matches!(error, LockError::Busy { .. }), "{error:?}");
}

#[test]
fn a_lock_nobody_holds_is_taken_with_no_wait_counted() {
    let dir = folder();

    let (lock, waited) = waits(|| WorkspaceLock::acquire(dir.path(), WAIT));

    assert!(lock.unwrap().is_held());
    assert_eq!(waited, Waited::default());
}

/// A wait that ends busy is a wait all the same, and the one most worth
/// telling: all of it is counted, with the holder it names.
#[test]
fn a_writer_kept_out_counts_its_whole_wait_and_who_held_the_lock() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let wait = Duration::from_millis(random::between(10, 40) as u64);

    let ((), waited) = waits(|| kept_out(&dir, wait));

    assert!(waited.total >= wait, "{waited:?} < {wait:?}");
    assert_eq!(waited.holder, Some(std::process::id()));
}

#[test]
fn a_writer_that_waited_for_the_holder_to_let_go_counts_the_wait() {
    let dir = folder();
    let held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let (asking, asked) = mpsc::channel();
    let path = dir.path().to_path_buf();
    let waiter = thread::spawn(move || {
        asking.send(()).unwrap();
        let started = Instant::now();
        let (lock, waited) = waits(|| WorkspaceLock::acquire(&path, WAIT));
        (lock.map(|lock| lock.is_held()), waited, started.elapsed())
    });

    asked.recv().unwrap();
    thread::sleep(Duration::from_millis(100));
    drop(held);
    let (held, waited, took) = waiter.join().unwrap();

    assert!(held.unwrap());
    assert!(waited.total > Duration::ZERO, "{waited:?}");
    assert!(waited.total <= took, "{waited:?} counted of {took:?}");
    assert_eq!(waited.holder, Some(std::process::id()));
}

/// No writer of this thread's waited, though one of another's did: what a
/// command waited is its own, whatever ran beside it.
#[test]
fn a_wait_on_another_thread_is_not_counted_on_this_one() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();

    let ((), waited) = waits(|| {
        thread::scope(|scope| {
            scope.spawn(|| kept_out(&dir, Duration::from_millis(20)));
        })
    });

    assert_eq!(waited, Waited::default());
}

#[test]
fn waits_counted_inside_others_count_toward_both() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let wait = Duration::from_millis(15);

    let (inner, outer) = waits(|| {
        kept_out(&dir, wait);
        let ((), inner) = waits(|| kept_out(&dir, wait));
        inner
    });

    assert!(inner.total >= wait, "{inner:?}");
    assert!(outer.total >= inner.total + wait, "{outer:?} of {inner:?}");
}

/// Work that panics, caught further up, leaves what was counted around it as
/// it was, with what it waited before it panicked.
#[test]
fn a_panic_inside_waits_leaves_the_count_around_it_whole() {
    let dir = folder();
    let _held = WorkspaceLock::acquire(dir.path(), WAIT).unwrap();
    let wait = Duration::from_millis(15);

    let ((), outer) = waits(|| {
        kept_out(&dir, wait);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            waits(|| {
                kept_out(&dir, wait);
                panic!("the work fails");
            })
        }));
        assert!(caught.is_err());
    });

    assert!(outer.total >= wait * 2, "{outer:?}");
}
