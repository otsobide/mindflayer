//! Unit tests for `adapter.rs`, kept beside it rather than inside it: the
//! system clock, the one adapter that lives in the module itself.

use super::*;

/// Seconds since the epoch the system says, read beside the clock's.
fn system_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("this machine's clock is past 1970")
        .as_secs() as i64
}

/// Seconds rather than milliseconds: every stamp the ledger holds is in
/// seconds, and `datetime(at, 'unixepoch')` reads it as one.
#[test]
fn the_system_clock_tells_the_seconds_the_system_says() {
    let before = system_seconds();
    let now = SystemClock.now();
    let after = system_seconds();

    assert!(
        (before..=after).contains(&now),
        "{before} <= {now} <= {after}"
    );
}

/// A time before this code was written is a clock reading something other
/// than seconds since 1970.
#[test]
fn the_system_clock_reads_a_time_after_this_was_written() {
    // 2026-01-01T00:00:00Z.
    assert!(SystemClock.now() > 1_767_225_600, "{}", SystemClock.now());
}

/// A clock is handed to the ledger as a trait object shared across
/// threads, so the default one must be one.
#[test]
fn the_system_clock_is_a_clock_the_ledger_can_hold() {
    let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(SystemClock);

    assert!(clock.now() > 0);
    assert_eq!(format!("{:?}", SystemClock), "SystemClock");
}
