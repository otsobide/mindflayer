//! Cancelling the run under way, `POST /api/v1/commands/cancel`: what a run
//! that takes one says as it goes and how it stops, what is refused, with a
//! code and nothing changed, and that the cancel is held to every guard a
//! change is.

mod common;

use common::*;
use std::time::Duration;

const CANCEL: &str = "/api/v1/commands/cancel";

/// A server over a fake whose runs may be held, and the hold.
fn launch_holding() -> (Arc<Listening>, Arc<common::Hold>) {
    let (server, hold, _) = launch_counting();
    (server, hold)
}

/// The same, and the fake's record of its runs.
fn launch_counting() -> (Arc<Listening>, Arc<common::Hold>, Arc<Mutex<Vec<String>>>) {
    let fake = Fake::default();
    let hold = Arc::clone(&fake.hold);
    let runs = Arc::clone(&fake.runs);
    (Arc::new(Server::new(fake).bind().unwrap()), hold, runs)
}

/// Run `task` in the background, and wait until it is held.
async fn held(server: &Arc<Listening>, hold: &Arc<common::Hold>, task: Value) -> Pending {
    let reply = {
        let server = Arc::clone(server);
        tokio::spawn(async move {
            post(&server, "/api/v1/tasks", Auth::Bearer(server.token()), task).await
        })
    };
    let reached = Arc::clone(hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();
    Pending { reply }
}

/// A run sent in the background, its reply to come.
struct Pending {
    reply: tokio::task::JoinHandle<Reply>,
}

impl Pending {
    async fn outcome(self) -> Value {
        let reply = tokio::time::timeout(PATIENCE, self.reply)
            .await
            .expect("the run ended")
            .unwrap();
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
        reply.json()["outcome"].clone()
    }
}

/// The panel's own cancel: by cookie, with what its page sends beside it.
fn by_cookie(server: &Listening, cookie: &str) -> Request<Body> {
    request(
        server,
        Method::POST,
        CANCEL,
        Auth::Cookie(cookie),
        Some(json!({})),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_takes_a_cancel_says_how_far_it_got_and_stops_on_one() {
    let (server, hold) = launch_holding();
    let _release = Release(Arc::clone(&hold));
    let cookie = format!("{}={}", server.cookie(), server.token());
    let mut stream = opened(&server, Auth::Cookie(&cookie)).await;

    let run = held(
        &server,
        &hold,
        json!({
            "name": "gather acme",
            "hold": true,
            "takes_cancel": true,
            "steps": [{"step": "receiving", "cancellable": true}],
        }),
    )
    .await;
    let lines = json!(["fake run gather acme"]);
    assert_eq!(
        stream.until("progress").await,
        [
            json!({"type": "command_started", "lines": lines}),
            json!({"type": "progress", "lines": lines, "progress": {
                "step": "receiving",
                "message": "receiving 1/1",
                "done": 1,
                "total": 1,
                "cancellable": true,
            }}),
        ]
    );

    let cancelled = deliver(&server, by_cookie(&server, &cookie)).await;

    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.text());
    assert_eq!(cancelled.json(), json!({ "lines": lines }));
    assert_eq!(run.outcome().await["cancelled"], true);
    assert_eq!(
        stream.next().await,
        json!({"type": "command_finished", "lines": lines})
    );
    // Over, so there is nothing left to stop.
    let again = deliver(&server, by_cookie(&server, &cookie)).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json()["code"], "nothing_running");
}

#[tokio::test]
async fn a_cancel_with_nothing_running_is_refused_and_changes_nothing() {
    let (server, runs) = launch();

    let reply = send(
        &server,
        Method::POST,
        CANCEL,
        Auth::Bearer(server.token()),
        None,
    )
    .await;

    assert_eq!(reply.status, StatusCode::CONFLICT);
    assert_eq!(
        reply.json(),
        json!({"code": "nothing_running", "error": "no run is under way to cancel"})
    );
    assert!(runs.lock().unwrap().is_empty());
    let transcript = get(&server, "/api/v1/transcript", Auth::Bearer(server.token())).await;
    assert_eq!(transcript.json(), json!([]));
}

/// Past the point its backend said a cancel stops it, a run goes on to its
/// end as if none was sent, and so does one that takes none at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_too_late_or_to_a_run_that_takes_none_is_refused_and_changes_nothing() {
    for (task, code) in [
        (
            json!({
                "name": "gather acme",
                "hold": true,
                "takes_cancel": true,
                "steps": [
                    {"step": "receiving", "cancellable": true},
                    {"step": "placing"},
                ],
            }),
            "cancel_too_late",
        ),
        (
            json!({"name": "install review", "hold": true}),
            "run_uncancellable",
        ),
    ] {
        let (server, hold) = launch_holding();
        let release = Release(Arc::clone(&hold));
        let run = held(&server, &hold, task).await;

        let reply = send(
            &server,
            Method::POST,
            CANCEL,
            Auth::Bearer(server.token()),
            None,
        )
        .await;

        assert_eq!(reply.status, StatusCode::CONFLICT, "{code}");
        assert_eq!(reply.json()["code"], code);
        assert!(reply.json()["error"].is_string(), "{code}");
        drop(release);
        let outcome = run.outcome().await;
        assert_eq!(outcome.get("cancelled"), None, "{code}: {outcome}");
    }
}

/// A cancel is a change, held to the token and to every guard the panel's
/// page passes, by cookie as a change to `/tasks` is: none of those it
/// turns away reaches the run, which the one it lets through stops.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_is_held_to_every_guard_a_change_is() {
    let (server, hold, runs) = launch_counting();
    let _release = Release(Arc::clone(&hold));
    let port = server.address().port();
    let cookie = format!("{}={}", server.cookie(), server.token());
    let run = held(
        &server,
        &hold,
        json!({"name": "gather acme", "hold": true, "takes_cancel": true}),
    )
    .await;
    let changed = |name: &str, value: Option<String>| {
        let mut request = by_cookie(&server, &cookie);
        match value {
            Some(value) => {
                let name = header::HeaderName::from_bytes(name.as_bytes()).unwrap();
                request.headers_mut().insert(name, value.parse().unwrap());
            }
            None => {
                request.headers_mut().remove(name);
            }
        }
        request
    };

    let refused = [
        (
            "no token",
            request(
                &server,
                Method::POST,
                CANCEL,
                Auth::Nothing,
                Some(json!({})),
            ),
            StatusCode::UNAUTHORIZED,
            "token_required",
        ),
        (
            "another server's token",
            request(
                &server,
                Method::POST,
                CANCEL,
                Auth::Bearer("0123456789abcdef"),
                None,
            ),
            StatusCode::UNAUTHORIZED,
            "token_required",
        ),
        (
            "another host",
            changed("host", Some(format!("attacker.example:{port}"))),
            StatusCode::FORBIDDEN,
            "unknown_host",
        ),
        (
            "another page",
            changed(
                "origin",
                Some(format!("http://127.0.0.1:{}", port.wrapping_add(1))),
            ),
            StatusCode::FORBIDDEN,
            "foreign_origin",
        ),
        (
            "from another port, as Fetch Metadata tells it",
            changed("sec-fetch-site", Some(String::from("same-site"))),
            StatusCode::FORBIDDEN,
            "same_origin_required",
        ),
        (
            "without X-Mindweaver",
            changed("x-mindweaver", None),
            StatusCode::FORBIDDEN,
            "x_mindflayer_required",
        ),
        (
            "as text",
            changed("content-type", Some(String::from("text/plain"))),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
        ),
    ];
    for (case, request, status, code) in refused {
        let reply = deliver(&server, request).await;
        assert_eq!(reply.status, status, "{case}: {}", reply.text());
        assert_eq!(reply.json()["code"], code, "{case}");
    }
    // Given the time a cancel that reached it would take to let it go.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(*runs.lock().unwrap(), ["start gather acme"], "still held");

    let taken = deliver(&server, by_cookie(&server, &cookie)).await;
    assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    assert_eq!(run.outcome().await["cancelled"], true);
}
