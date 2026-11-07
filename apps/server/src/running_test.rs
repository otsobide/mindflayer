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
