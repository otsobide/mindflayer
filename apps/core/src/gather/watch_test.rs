//! Unit tests for `watch.rs`, kept beside it rather than inside it.

use std::sync::Mutex;

use super::*;

#[test]
fn a_watch_reports_each_step_to_its_sink_in_order() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });

    watch.report(Progress::Connecting);
    watch.report(Progress::Harvesting { done: 1, total: 2 });

    assert_eq!(
        *seen.lock().unwrap(),
        [
            Progress::Connecting,
            Progress::Harvesting { done: 1, total: 2 }
        ]
    );
}

#[test]
fn a_watch_with_no_sink_reports_to_nobody() {
    // Nothing to assert beyond not panicking: a gather nobody follows still
    // reports, and has to be able to.
    Watch::new().report(Progress::Connecting);
}

#[test]
fn a_cancel_reaches_every_clone_of_a_watch() {
    let watch = Watch::new();
    let kept = watch.clone();
    assert!(!watch.is_cancelled());

    kept.cancel();

    assert!(watch.is_cancelled());
    assert!(format!("{watch:?}").contains("cancelled: true"));
}
