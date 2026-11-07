use std::sync::{Arc, Mutex};

use super::*;

fn step(step: &str, done: u64, cancellable: bool) -> Progress {
    Progress {
        step: step.to_owned(),
        message: format!("{step} {done}"),
        done,
        total: None,
        cancellable,
    }
}

#[test]
fn each_step_said_is_told_in_order_and_kept_as_the_latest() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let running = Running::new({
        let heard = Arc::clone(&heard);
        move |progress| heard.lock().unwrap().push(progress.clone())
    });
    assert_eq!(running.progress(), None);

    running.report(step("receiving", 1, true));
    running.clone().report(step("placing", 2, false));

    assert_eq!(
        *heard.lock().unwrap(),
        [step("receiving", 1, true), step("placing", 2, false)]
    );
    assert_eq!(running.progress(), Some(step("placing", 2, false)));
}

/// What hears a step may ask the run what it said, as a front end that
/// cancels from where it hears does: the step is told outside the lock.
#[test]
fn what_hears_a_step_may_ask_the_run_about_it() {
    let slot: Arc<Mutex<Option<Running>>> = Arc::default();
    let seen = Arc::new(Mutex::new(None));
    let running = Running::new({
        let (slot, seen) = (Arc::clone(&slot), Arc::clone(&seen));
        move |_| {
            let run = slot.lock().unwrap().clone().expect("the run");
            *seen.lock().unwrap() = run.progress();
        }
    });
    *slot.lock().unwrap() = Some(running.clone());

    running.report(step("receiving", 3, true));

    assert_eq!(*seen.lock().unwrap(), Some(step("receiving", 3, true)));
}

#[test]
fn a_run_nobody_follows_keeps_what_it_said_all_the_same() {
    let running = Running::default();

    running.report(step("harvesting", 4, true));

    assert_eq!(running.progress(), Some(step("harvesting", 4, true)));
}

/// A run counting the cancels that reach it.
fn cancellable() -> (Running, Arc<Mutex<u32>>) {
    let reached = Arc::new(Mutex::new(0));
    let running = Running::default();
    running.cancelled_by({
        let reached = Arc::clone(&reached);
        move || *reached.lock().unwrap() += 1
    });
    (running, reached)
}

#[test]
fn a_cancel_reaches_a_run_whose_backend_handed_one_over_each_time_it_is_sent() {
    let (running, reached) = cancellable();

    assert_eq!(running.cancel(), Ok(()));
    running.report(step("receiving", 1, true));
    assert_eq!(running.clone().cancel(), Ok(()));

    assert_eq!(*reached.lock().unwrap(), 2);
}

#[test]
fn a_run_whose_backend_handed_no_cancel_over_refuses_one() {
    let running = Running::default();
    running.report(step("receiving", 1, true));

    let refused = running.cancel().unwrap_err();

    assert_eq!(refused.0, Condition::RunUncancellable);
}

/// Past the step its backend said a cancel stops it at, none reaches it:
/// what it would set is left as it is.
#[test]
fn a_cancel_past_the_point_a_run_stops_at_is_refused_and_reaches_nothing() {
    let (running, reached) = cancellable();
    running.report(step("receiving", 1, true));
    running.report(step("placing", 0, false));

    let refused = running.cancel().unwrap_err();

    assert_eq!(refused.0, Condition::CancelTooLate);
    assert_eq!(*reached.lock().unwrap(), 0);
}

/// What a cancel calls may say a step of its own, as a backend that stops
/// says where it stopped: it is called outside the lock.
#[test]
fn what_a_cancel_calls_may_report_a_step() {
    let slot: Arc<Mutex<Option<Running>>> = Arc::default();
    let running = Running::default();
    running.cancelled_by({
        let slot = Arc::clone(&slot);
        move || {
            let run = slot.lock().unwrap().clone().expect("the run");
            run.report(step("stopping", 0, false));
        }
    });
    *slot.lock().unwrap() = Some(running.clone());

    assert_eq!(running.cancel(), Ok(()));

    assert_eq!(running.progress(), Some(step("stopping", 0, false)));
}
