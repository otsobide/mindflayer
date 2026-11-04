//! The `Clock` contract: what the ledger asks of whatever tells it the time,
//! held of the system's clock and of the fakes a test stamps by, and what a
//! gather's records say when the ledger is handed one.
//!
//! The port is one method, so its contract is short: seconds since the Unix
//! epoch, never going back, readable from any thread. What the ledger does
//! with it, every store alike, is `ledger_store_test.rs`'s.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use mindflayer_core::adapter::SystemClock;
use mindflayer_core::gather::{self, Request};
use mindflayer_core::ledger::{Action, Outcome};
use mindflayer_core::mother::{random, GitRepositoryMother, WorkspaceMother};
use mindflayer_core::Clock;

/// 2024-01-01T00:00:00Z: no clock this build runs by can be earlier, since
/// it was written after.
const WRITTEN_AFTER: i64 = 1_704_067_200;

/// 2100-01-01T00:00:00Z: a reading past it is milliseconds or nanoseconds
/// taken for seconds.
const SECONDS_BEFORE: i64 = 4_102_444_800;

/// A moment a fake clock starts at, with room before the upper bound for
/// every tick a test reads.
fn moment() -> i64 {
    random::between(WRITTEN_AFTER as usize, SECONDS_BEFORE as usize - 10_000) as i64
}

/// A clock stopped at one moment.
#[derive(Debug)]
struct Stopped(i64);

impl Clock for Stopped {
    fn now(&self) -> i64 {
        self.0
    }
}

/// A clock that moves on a second each time it is read.
#[derive(Debug)]
struct Ticking(AtomicI64);

impl Clock for Ticking {
    fn now(&self) -> i64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// Every clock the contract is held of: the system's and the fakes.
fn clocks() -> Vec<(&'static str, Arc<dyn Clock>)> {
    let moment = moment();
    vec![
        ("the system's", Arc::new(SystemClock)),
        ("a stopped one", Arc::new(Stopped(moment))),
        ("a ticking one", Arc::new(Ticking(AtomicI64::new(moment)))),
    ]
}

/// What the operating system says, read without the port.
fn system_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock set after 1970")
        .as_secs() as i64
}

#[test]
fn no_clock_goes_back() {
    for (which, clock) in clocks() {
        let readings: Vec<i64> = (0..1000).map(|_| clock.now()).collect();
        assert!(
            readings.windows(2).all(|pair| pair[0] <= pair[1]),
            "{which} went back: {readings:?}"
        );
    }
}

#[test]
fn every_clock_reads_whole_seconds_after_this_was_written() {
    for (which, clock) in clocks() {
        let now = clock.now();
        assert!(
            (WRITTEN_AFTER..SECONDS_BEFORE).contains(&now),
            "{which} says {now}, which is no time in seconds since this was written"
        );
    }
}

#[test]
fn every_clock_is_read_from_any_thread() {
    for (which, clock) in clocks() {
        let readings: Vec<i64> = (0..8)
            .map(|_| {
                let clock = Arc::clone(&clock);
                thread::spawn(move || clock.now())
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|reading| reading.join().expect("a thread read the clock"))
            .collect();
        assert!(
            readings
                .iter()
                .all(|now| (WRITTEN_AFTER..SECONDS_BEFORE).contains(now)),
            "{which}: {readings:?}"
        );
    }
}

#[test]
fn the_systems_clock_says_what_the_operating_system_says() {
    let before = system_seconds();
    let now = SystemClock.now();
    let after = system_seconds();

    assert!(
        (before..=after).contains(&now),
        "{now} is not between {before} and {after}"
    );
}

#[test]
fn a_stopped_clock_says_one_moment_and_a_ticking_one_a_new_one_each_time() {
    let moment = moment();
    let stopped = Stopped(moment);
    let ticking = Ticking(AtomicI64::new(moment));

    let still: BTreeSet<i64> = (0..100).map(|_| stopped.now()).collect();
    let moving: Vec<i64> = (0..100).map(|_| ticking.now()).collect();

    assert_eq!(still, BTreeSet::from([moment]));
    assert_eq!(moving, (moment..moment + 100).collect::<Vec<_>>());
}

#[test]
fn a_ledger_stamps_its_log_by_the_clock_it_is_handed_and_no_other() {
    for (which, clock) in clocks() {
        let ledger = mindflayer_core::Ledger::in_memory()
            .unwrap()
            .with_clock(Arc::clone(&clock));
        let before = clock.now();

        ledger
            .log(Action::Install, Some("deploy"), Outcome::Ok, None)
            .unwrap();

        let after = clock.now();
        let at = ledger.history(1).unwrap()[0].at;
        assert!(
            (before..=after).contains(&at),
            "{which}: {at} is not between {before} and {after}"
        );
    }
}

#[test]
fn a_gather_is_stamped_by_the_clock_its_ledger_is_handed() {
    let moment = moment();
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace
        .ledger()
        .unwrap()
        .with_clock(Arc::new(Stopped(moment)));
    let source = GitRepositoryMother::with_skills(&["deploy", "review"]);

    gather::gather(
        &workspace,
        &ledger,
        &Request::git(GitRepositoryMother::url(&source)),
    )
    .unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert!(
        gathered
            .iter()
            .all(|artifact| artifact.gathered_at == moment),
        "{moment}: {gathered:#?}"
    );
    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].at, moment);
}
