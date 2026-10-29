//! Running tasks: one at a time, in the order they arrived, on the one thread
//! that does all the writing, and the transcript they leave behind.

mod common;

use common::*;
use std::time::Duration;

#[tokio::test]
async fn a_command_runs_on_the_writer_thread_and_says_what_it_was() {
    let (server, runs) = launch();

    let reply = post(
        &server,
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
        &server,
        "/api/commands",
        Auth::Bearer(server.token()),
        json!({ "nombre": "link beta" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reply.json()["error"].as_str().unwrap().contains("name"));
    assert!(runs.lock().unwrap().is_empty());
}

/// Refused when its lines are asked for, on the writer thread just before
/// it would run: it is never handed over to run, and leaves the transcript,
/// which is runs only, as it was.
#[tokio::test]
async fn a_task_the_backend_refuses_runs_nothing_and_is_no_run() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    let reply = post(
        &server,
        "/api/commands",
        token,
        json!({ "name": "unlink -rf", "refuse": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "unlink -rf is refused", "code": "fake_refused" })
    );
    assert!(runs.lock().unwrap().is_empty());
    assert_eq!(
        get(&server, "/api/transcript", token).await.json(),
        json!([])
    );
}

/// Refused by the run itself, its lines having passed: the backend's last
/// word, which the reply and the transcript take as they take a refusal of
/// its lines.
#[tokio::test]
async fn a_task_the_backend_refuses_when_it_is_run_is_no_run_either() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    let reply = post(
        &server,
        "/api/commands",
        token,
        json!({ "name": "link gone", "refuse_run": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reply.json()["code"], "fake_refused");
    assert!(runs.lock().unwrap().is_empty());
    assert_eq!(
        get(&server, "/api/transcript", token).await.json(),
        json!([])
    );
}

#[tokio::test]
async fn a_run_whose_outcome_is_not_json_is_still_transcribed() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    let reply = post(
        &server,
        "/api/commands",
        token,
        json!({ "name": "odd", "unwritable": true }),
    )
    .await;

    let entry = json!({
        "lines": ["fake run odd"],
        "outcome": {
            "error": "the backend's outcome is not JSON: this part cannot be written",
        },
    });
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json(), entry);
    assert_eq!(*runs.lock().unwrap(), ["start odd", "end odd"]);
    let transcript = get(&server, "/api/transcript", token).await;
    assert_eq!(transcript.json(), json!([entry]));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_writes_run_one_at_a_time_in_arrival_order() {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    let hold = Arc::clone(&fake.hold);
    let server = Arc::new(Server::new(fake).bind().unwrap());
    let write = |task: Value| {
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            post(&server, "/api/commands", Auth::Bearer(server.token()), task).await
        })
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

    let transcript = get(&server, "/api/transcript", Auth::Bearer(server.token())).await;
    assert_eq!(
        transcript.json(),
        Value::Array(replies.iter().map(Reply::json).collect()),
        "all three, in the order they ran"
    );
}

#[tokio::test]
async fn the_transcript_keeps_runs_and_nothing_that_only_read() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());
    let transcript = || get(&server, "/api/transcript", token);
    assert_eq!(transcript().await.json(), json!([]));

    post(&server, "/api/query", token, json!({ "echo": "x" })).await;
    post(
        &server,
        "/api/commands/preview",
        token,
        json!({ "name": "a" }),
    )
    .await;
    post(&server, "/api/commands", token, json!({ "nombre": "nope" })).await;
    assert_eq!(transcript().await.json(), json!([]));

    post(&server, "/api/commands", token, json!({ "name": "init" })).await;
    post(
        &server,
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
