//! Running tasks: one at a time, in the order they arrived, on the one thread
//! that does all the writing, and the transcript they leave behind.

mod common;

use common::*;
use std::time::Duration;

#[tokio::test]
async fn a_command_runs_on_the_writer_thread_and_says_what_it_was() {
    let (server, runs) = launch();

    let reply = post(
        &server.router(),
        "/api/commands",
        Auth::Bearer(server.token()),
        json!({ "name": "link beta" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json(),
        json!({
            "lines": ["fake run link beta"],
            "outcome": { "ran": "link beta", "thread": "mindflayer-writes" },
        })
    );
    assert_eq!(*runs.lock().unwrap(), ["start link beta", "end link beta"]);
}

#[tokio::test]
async fn a_task_the_backend_cannot_read_runs_nothing() {
    let (server, runs) = launch();

    let reply = post(
        &server.router(),
        "/api/commands",
        Auth::Bearer(server.token()),
        json!({ "nombre": "link beta" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reply.json()["error"].as_str().unwrap().contains("name"));
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_writes_run_one_at_a_time_in_arrival_order() {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    let hold = Arc::clone(&fake.hold);
    let server = Server::new(fake).bind().unwrap();
    let write = |task: Value| {
        let router = server.router();
        let token = server.token().to_owned();
        tokio::spawn(
            async move { post(&router, "/api/commands", Auth::Bearer(&token), task).await },
        )
    };

    let release = Release(Arc::clone(&hold));
    let first = write(json!({ "name": "first", "hold": true }));
    let reached = Arc::clone(&hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();
    // Two behind it, each given time to arrive before the next is sent, so
    // the order they arrived in is known. Two, because with one waiting any
    // lock would let it through second, ordered or not.
    let mut waiting = Vec::new();
    for name in ["second", "third"] {
        waiting.push(write(json!({ "name": name })));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(*runs.lock().unwrap(), ["start first"], "none overtook it");
    drop(release);

    let mut replies = vec![first.await.unwrap()];
    for reply in waiting {
        replies.push(reply.await.unwrap());
    }
    for (reply, name) in replies.iter().zip(["first", "second", "third"]) {
        assert_eq!(reply.json()["outcome"]["ran"], name);
    }
    assert_eq!(
        *runs.lock().unwrap(),
        [
            "start first",
            "end first",
            "start second",
            "end second",
            "start third",
            "end third",
        ]
    );

    let transcript = get(
        &server.router(),
        "/api/transcript",
        Auth::Bearer(server.token()),
    )
    .await;
    assert_eq!(
        transcript.json(),
        Value::Array(replies.iter().map(Reply::json).collect()),
        "all three, in the order they ran"
    );
}

#[tokio::test]
async fn the_transcript_keeps_runs_and_nothing_that_only_read() {
    let (server, _) = launch();
    let router = server.router();
    let token = Auth::Bearer(server.token());
    let transcript = || get(&router, "/api/transcript", token);
    assert_eq!(transcript().await.json(), json!([]));

    post(&router, "/api/query", token, json!({ "echo": "x" })).await;
    post(
        &router,
        "/api/commands/preview",
        token,
        json!({ "name": "a" }),
    )
    .await;
    post(&router, "/api/commands", token, json!({ "nombre": "nope" })).await;
    assert_eq!(transcript().await.json(), json!([]));

    post(&router, "/api/commands", token, json!({ "name": "init" })).await;
    post(
        &router,
        "/api/commands",
        token,
        json!({ "name": "link beta" }),
    )
    .await;
    let reply = transcript().await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json(),
        json!([
            {
                "lines": ["fake run init"],
                "outcome": { "ran": "init", "thread": "mindflayer-writes" },
            },
            {
                "lines": ["fake run link beta"],
                "outcome": { "ran": "link beta", "thread": "mindflayer-writes" },
            },
        ])
    );
}
