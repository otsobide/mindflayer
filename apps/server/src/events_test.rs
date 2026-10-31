use std::time::Duration;

use serde_json::json;

use super::*;

/// The next event, or a failed test once long enough has passed that none
/// is coming.
async fn next(subscription: &mut Subscription) -> Option<Event> {
    tokio::time::timeout(Duration::from_secs(10), subscription.next())
        .await
        .expect("an event, or the end, in time")
}

/// What a subscriber hears first while nothing has switched.
const RESYNC: Event = Event::Resync { switches: 0 };

fn changed(scope: &str) -> Event {
    Event::Changed {
        scopes: vec![json!(scope)],
    }
}

#[tokio::test]
async fn a_subscriber_is_told_to_resync_and_then_every_event_in_order() {
    let events = Events::new();
    let mut subscription = events.subscribe();

    events.publish(changed("one"));
    events.publish(changed("two"));

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(changed("one")));
    assert_eq!(next(&mut subscription).await, Some(changed("two")));
}

#[tokio::test]
async fn a_subscriber_is_told_how_things_are_watched_once_something_has_said() {
    let events = Events::new();
    let mut before = events.subscribe();
    let unwatched = Event::Unwatched {
        reason: String::from("the limit"),
    };
    events.publish(unwatched.clone());
    events.publish(changed("one"));

    let mut after = events.subscribe();

    assert_eq!(next(&mut before).await, Some(RESYNC));
    assert_eq!(next(&mut before).await, Some(unwatched.clone()));
    assert_eq!(next(&mut after).await, Some(RESYNC));
    assert_eq!(next(&mut after).await, Some(unwatched));
    events.publish(Event::Watching);
    assert_eq!(next(&mut after).await, Some(Event::Watching));
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_is_told_to_resync_and_how_things_are_watched() {
    let events = Events::new();
    events.publish(Event::Watching);
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));

    for each in 0..CAPACITY + 10 {
        events.publish(changed(&each.to_string()));
    }

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));
    // Then what the channel still holds, the latest it kept.
    assert_eq!(next(&mut subscription).await, Some(changed("10")));
}

/// A subscriber that missed a switch, subscribing after it, or falling
/// behind past it, learns of it from the count its resync carries, the
/// one every switch told carries.
#[tokio::test]
async fn every_switch_is_counted_and_a_resync_carries_the_count() {
    let events = Events::new();
    let mut heard = events.subscribe();
    assert_eq!(next(&mut heard).await, Some(RESYNC));

    events.switched();
    events.switched();

    assert_eq!(
        next(&mut heard).await,
        Some(Event::Switched { switches: 1 })
    );
    assert_eq!(
        next(&mut heard).await,
        Some(Event::Switched { switches: 2 })
    );
    let mut after = events.subscribe();
    assert_eq!(next(&mut after).await, Some(Event::Resync { switches: 2 }));

    events.switched();
    for each in 0..CAPACITY + 10 {
        events.publish(changed(&each.to_string()));
    }

    assert_eq!(
        next(&mut after).await,
        Some(Event::Resync { switches: 3 }),
        "fallen behind past the switch"
    );
}

#[tokio::test]
async fn closing_ends_every_subscription_and_every_one_after() {
    let events = Events::new();
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));

    events.close();
    events.publish(changed("late"));

    assert_eq!(next(&mut subscription).await, None);
    assert_eq!(next(&mut events.subscribe()).await, None);
}

#[test]
fn each_event_is_tagged_by_its_type_in_snake_case() {
    let shapes = [
        (
            changed("one"),
            json!({"type": "changed", "scopes": ["one"]}),
        ),
        (
            Event::CommandStarted {
                lines: vec![String::from("flayer link beta")],
            },
            json!({"type": "command_started", "lines": ["flayer link beta"]}),
        ),
        (
            Event::CommandFinished {
                lines: vec![String::from("flayer link beta")],
            },
            json!({"type": "command_finished", "lines": ["flayer link beta"]}),
        ),
        (
            Event::Resync { switches: 2 },
            json!({"type": "resync", "switches": 2}),
        ),
        (
            Event::Switched { switches: 3 },
            json!({"type": "switched", "switches": 3}),
        ),
        (Event::Watching, json!({"type": "watching"})),
        (
            Event::Unwatched {
                reason: String::from("the limit"),
            },
            json!({"type": "unwatched", "reason": "the limit"}),
        ),
    ];
    for (event, shape) in shapes {
        assert_eq!(serde_json::to_value(&event).unwrap(), shape);
    }
}
