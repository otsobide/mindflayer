//! The request log: a line for every request, with how long it took to
//! answer, and never the launch token.

mod common;

use common::*;
use std::time::Duration;

#[tokio::test]
async fn every_request_is_logged_once_answered() {
    let (server, lines) = launch_logged();
    let token = Auth::Bearer(server.token());

    post(&server, "/api/v1/judge", token, json!({ "echo": "x" })).await;
    post(&server, "/api/v1/tasks", token, json!({ "name": "a" })).await;
    get(&server, "/api/v1/transcript?view=all", Auth::Nothing).await;
    get(&server, "/api/nowhere", token).await;

    let lines = lines.lock().unwrap();
    let said: Vec<&str> = lines.iter().map(|line| untimed(line).0).collect();
    assert_eq!(
        said,
        [
            "POST /api/v1/judge 200",
            "POST /api/v1/tasks 200",
            "GET /api/v1/transcript?view=all 401",
            "GET /api/nowhere 404",
        ]
    );
}

/// From the request reaching the server to its answer: a run held for a
/// while says at least that while, and a read sent while it is held, which
/// waits for no run, says less.
#[tokio::test(flavor = "multi_thread")]
async fn a_logged_line_says_how_long_the_answer_took() {
    let fake = Fake::default();
    let hold = Arc::clone(&fake.hold);
    let (server, lines) = launch_logging(fake);
    let server = Arc::new(server);
    let release = Release(Arc::clone(&hold));
    let sent = Arc::clone(&server);
    let run = tokio::spawn(async move {
        let token = Auth::Bearer(sent.token());
        post(
            &sent,
            "/api/v1/tasks",
            token,
            json!({ "name": "slow", "hold": true }),
        )
        .await
    });
    let reached = Arc::clone(&hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();

    get(&server, "/api/v1/transcript", Auth::Bearer(server.token())).await;
    let held = Duration::from_millis(150);
    tokio::time::sleep(held).await;
    drop(release);
    assert_eq!(run.await.unwrap().status, StatusCode::OK);

    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 2, "{lines:#?}");
    let (read, read_took) = untimed(&lines[0]);
    let (ran, run_took) = untimed(&lines[1]);
    assert_eq!(read, "GET /api/v1/transcript 200");
    assert_eq!(ran, "POST /api/v1/tasks 200");
    assert!(run_took >= held, "{lines:#?}");
    assert!(read_took < run_took, "{lines:#?}");
}

#[tokio::test]
async fn the_token_never_appears_in_a_logged_line() {
    let (server, lines) = launch_logged();
    let secret = server.token().to_owned();
    let cookie = format!("{}={secret}", server.cookie());

    // Everywhere a token can travel, and some places it should not.
    get(&server, &format!("/?token={secret}"), Auth::Nothing).await;
    get(
        &server,
        &format!("/?view=1&%74oken={secret}"),
        Auth::Nothing,
    )
    .await;
    get(&server, &format!("/?token={secret}x"), Auth::Nothing).await;
    get(&server, "/api/v1/transcript", Auth::Cookie(&cookie)).await;
    get(&server, "/api/v1/transcript", Auth::Bearer(&secret)).await;
    get(
        &server,
        &format!("/api/v1/transcript?token={secret}"),
        Auth::Nothing,
    )
    .await;
    get(&server, &format!("/api/{secret}"), Auth::Bearer(&secret)).await;
    let shouted = secret.to_ascii_uppercase();
    get(
        &server,
        &format!("/api/v1/transcript?t={shouted}"),
        Auth::Nothing,
    )
    .await;

    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 8, "{lines:#?}");
    for line in lines.iter() {
        assert!(
            !line.to_ascii_lowercase().contains(&secret),
            "the token is in {line:?}"
        );
    }
    assert_eq!(untimed(&lines[0]).0, "GET /?token=<redacted> 303");
    assert_eq!(untimed(&lines[1]).0, "GET /?view=1&%74oken=<redacted> 303");
    assert_eq!(untimed(&lines[2]).0, "GET /?token=<redacted> 401");
    assert_eq!(untimed(&lines[6]).0, "GET /api/<redacted> 404");
}

#[tokio::test]
async fn a_server_given_nowhere_to_log_answers_all_the_same() {
    let (server, _) = launch();

    let reply = get(&server, "/api/v1/transcript", Auth::Bearer(server.token())).await;

    assert_eq!(reply.status, StatusCode::OK);
}

/// `text` with every `%` and two hex digits after it turned back into the
/// byte they stand for: what a reader of the log could make of an encoded
/// token.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escape = bytes.get(at + 1..at + 3).filter(|_| bytes[at] == b'%');
        match escape.and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()) {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The parameter's name and its value each encoded as a browser may encode
/// them, any character percent-encoded in either case, among other
/// parameters; and the token pasted, in any case, under another name or in
/// the path. However it is spelled where the server reads it, the log
/// leaves it out, and no decoding of the line brings it back.
#[tokio::test]
async fn the_token_never_reaches_the_log_however_a_request_spells_it() {
    use common::mother::random;

    let (server, lines) = launch_logged();
    let token = server.token().to_owned();
    let encoded = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if random::chance() {
                    random::cased(&format!("%{:02x}", c as u32))
                } else {
                    c.to_string()
                }
            })
            .collect()
    };

    let mut sent = 0;
    for _ in 0..8 {
        let mut parameters: Vec<String> = (0..random::between(0, 3))
            .map(|_| format!("{}={}", random::word(), random::word()))
            .collect();
        parameters.push(format!("{}={}", encoded("token"), encoded(&token)));
        let link = format!("/?{}", random::shuffled(parameters).join("&"));
        let reply = get(&server, &link, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER, "{link}");

        let pasted = random::cased(&token);
        for path in [
            format!("/api/v1/transcript?{}={pasted}", random::word()),
            format!("/api/{pasted}"),
            format!("/{pasted}?token={pasted}"),
        ] {
            get(&server, &path, Auth::Bearer(&token)).await;
        }
        sent += 4;
    }

    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), sent, "{lines:#?}");
    for line in lines.iter() {
        assert!(
            !decoded(line).to_ascii_lowercase().contains(&token),
            "the token is in {line:?}"
        );
    }
}
