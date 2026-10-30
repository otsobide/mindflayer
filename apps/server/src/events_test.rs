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

    assert_eq!(next(&mut subscription).await, Some(Event::Resync));
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

    assert_eq!(next(&mut before).await, Some(Event::Resync));
    assert_eq!(next(&mut before).await, Some(unwatched.clone()));
    assert_eq!(next(&mut after).await, Some(Event::Resync));
    assert_eq!(next(&mut after).await, Some(unwatched));
    events.publish(Event::Watching);
    assert_eq!(next(&mut after).await, Some(Event::Watching));
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_is_told_to_resync_and_how_things_are_watched() {
    let events = Events::new();
    events.publish(Event::Watching);
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(Event::Resync));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));

    for each in 0..CAPACITY + 10 {
        events.publish(changed(&each.to_string()));
    }

    assert_eq!(next(&mut subscription).await, Some(Event::Resync));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));
    // Then what the channel still holds, the latest it kept.
    assert_eq!(next(&mut subscription).await, Some(changed("10")));
}

#[tokio::test]
async fn closing_ends_every_subscription_and_every_one_after() {
    let events = Events::new();
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(Event::Resync));

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
        (Event::Resync, json!({"type": "resync"})),
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
