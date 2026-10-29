//! `flayer ui` end to end: the binary started as a person starts it, the
//! link read from what it prints, and the panel's requests sent to it over
//! loopback as a script sends them, with the token as Bearer. No browser is
//! opened: every launch passes --no-open.

// The panel is the `ui` feature's, which a build without it lacks.
#![cfg(feature = "ui")]

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::ChildStdout;
use std::time::Duration;

use common::*;
use serde_json::{json, Value};

/// A panel being served, with what its link said.
struct Panel {
    started: Started,
    address: SocketAddr,
    token: String,
    /// What it prints after the link.
    stdout: BufReader<ChildStdout>,
}

/// Every agent home empty, so nothing the panel runs reads a real one.
const NO_AGENT_HOMES: [&str; 4] = [
    "--claude-config-dir=",
    "--codex-home=",
    "--opencode-config-dir=",
    "--agents-dir=",
];

/// `binary ui --no-open --port 0` started from `dir`, with `home` as the
/// default workspace's home, nothing named and every agent home empty, and
/// the link it printed read back into an address and a token.
fn serve(binary: &str, dir: &Path, home: &Path) -> Panel {
    let (started, stdout, link) = launch(binary, dir, home, &[]);
    let (address, token) = link
        .strip_prefix("http://")
        .and_then(|link| link.split_once("/?token="))
        .unwrap_or_else(|| panic!("not a link: {link:?}"));
    let address: SocketAddr = address.parse().unwrap();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(address.port(), 0);
    Panel {
        address,
        token: token.to_owned(),
        started,
        stdout,
    }
}

/// `binary ui --no-open --port 0`, and `options`, started as [`serve`]
/// starts it, and the one line it printed, the link.
fn launch(
    binary: &str,
    dir: &Path,
    home: &Path,
    options: &[&str],
) -> (Started, BufReader<ChildStdout>, String) {
    let home = format!("--home={}", home.display());
    let mut args = vec!["ui", "--no-open", "--port", "0", &home, "--workspace="];
    args.extend(NO_AGENT_HOMES);
    args.extend(options);
    let mut started = start(binary, dir, &args, &[]);
    let mut stdout = BufReader::new(started.child.stdout.take().unwrap());
    let mut link = String::new();
    stdout.read_line(&mut link).unwrap();
    let Some(link) = link.strip_suffix('\n').map(str::to_owned) else {
        let mut said = String::new();
        let _ = started
            .child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut said);
        panic!("no link printed: {link:?}; stderr: {said}");
    };
    (started, stdout, link)
}

/// What a request carries to be let in.
#[derive(Clone, Copy)]
enum Auth<'a> {
    Nothing,
    /// The token, as a script sends it.
    Bearer,
    /// The cookie the link sets, with what the panel's page sends beside it,
    /// from a page at this origin.
    Cookie(&'a str),
}

impl Panel {
    /// One request over loopback: Host naming the server, what `auth` says,
    /// and a JSON body; and the status and the JSON that came back.
    fn send(&self, method: &str, path: &str, auth: Auth<'_>, body: Option<Value>) -> (u16, Value) {
        let body = body.map(|body| body.to_string()).unwrap_or_default();
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n",
            self.address,
            body.len()
        );
        match auth {
            Auth::Nothing => {}
            Auth::Bearer => request.push_str(&format!("Authorization: Bearer {}\r\n", self.token)),
            Auth::Cookie(origin) => request.push_str(&format!(
                "Cookie: mf_{}={}\r\nOrigin: {origin}\r\nSec-Fetch-Site: same-origin\r\n\
                 X-Mindflayer: 1\r\n",
                self.address.port(),
                self.token
            )),
        }
        request.push_str("\r\n");
        request.push_str(&body);

        let mut stream = TcpStream::connect(self.address).unwrap();
        // Long enough for a run on a slow runner; a server that never
        // answers fails the test rather than hanging it.
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body}"));
        (status, body)
    }

    fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.send("POST", path, Auth::Bearer, Some(body))
    }

    fn snapshot(&self) -> Value {
        let (status, snapshot) = self.post("/api/query", json!({"type": "snapshot"}));
        assert_eq!(status, 200, "{snapshot}");
        snapshot
    }
}

/// How `name` stands in the project the snapshot lists as `entry`, if it
/// holds it.
fn standing<'a>(snapshot: &'a Value, entry: &str, name: &str) -> Option<&'a str> {
    let project = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["entry"] == entry)
        .unwrap_or_else(|| panic!("no project {entry}: {snapshot}"));
    project["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|held| held["name"] == name)
        .map(|held| held["state"].as_str().unwrap_or("none"))
}

#[test]
fn the_panel_runs_what_it_is_sent_and_the_next_snapshot_shows_it() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("flayer", dir.path(), home.path());

    let before = panel.snapshot();
    assert_eq!(standing(&before, "alpha", "commit-style"), None);

    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let (status, ran) = panel.post("/api/commands", install);
    assert_eq!(status, 200, "{ran}");
    assert_eq!(ran["outcome"]["ok"], true, "{ran}");
    assert_eq!(
        ran["lines"],
        json!([format!(
            "flayer install -p {} skill:commit-style",
            dir.path().join("alpha").display()
        )])
    );

    let after = panel.snapshot();
    assert_eq!(standing(&after, "alpha", "commit-style"), Some("current"));
    assert_eq!(standing(&after, "beta", "commit-style"), None);
    let (_, transcript) = panel.send("GET", "/api/transcript", Auth::Bearer, None);
    assert_eq!(transcript, json!([ran]));
}

#[test]
fn a_request_without_the_token_is_refused_with_a_401() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("flayer", dir.path(), home.path());

    for (path, body) in [
        ("/api/query", Some(json!({"type": "snapshot"}))),
        ("/api/commands", Some(json!({"type": "shelf"}))),
    ] {
        let (status, refusal) = panel.send("POST", path, Auth::Nothing, body);
        assert_eq!(status, 401, "{path}");
        assert_eq!(
            refusal,
            json!({"error": "this needs the launch token, as the cookie its link sets or as a Bearer token"})
        );
    }
    let (status, _) = panel.send("GET", "/api/transcript", Auth::Nothing, None);
    assert_eq!(status, 401);
}

/// What a front end without a terminal does not run is a 422 that names the
/// command, with its code, and nothing of it runs.
#[test]
fn a_refused_command_is_a_422_naming_the_command() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("flayer", dir.path(), home.path());

    let edit = json!({
        "type": "edit",
        "holder": {"type": "workspace"},
        "kind": "skill",
        "name": "commit-style",
    });
    for path in ["/api/commands/preview", "/api/commands"] {
        let (status, refusal) = panel.post(path, edit.clone());
        assert_eq!(status, 422, "{path}");
        assert_eq!(
            refusal,
            json!({
                "error": "`flayer edit skill:commit-style` opens an editor, which needs a terminal to draw on",
                "code": "opens_editor",
            }),
            "{path}"
        );
    }
    let (_, transcript) = panel.send("GET", "/api/transcript", Auth::Bearer, None);
    assert_eq!(transcript, json!([]));
}

/// `mind ui` is `flayer ui` from where it is run: the panel is over the
/// workspace a command run there would use.
#[test]
fn mind_ui_in_a_project_serves_the_workspace_above_it() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mind", &dir.path().join("alpha"), home.path());

    let snapshot = panel.snapshot();

    assert_eq!(
        snapshot["workspace"]["path"],
        dir.path().to_str().unwrap(),
        "{snapshot}"
    );
    assert_eq!(snapshot["workspace"]["reason"], "above");
}

/// Through the dev server the link is the dev server's, the note says where
/// the server itself listens, which the link no longer does, and a change
/// the dev server's proxy forwards, with the browser's cookie and the dev
/// server's Origin, is taken. From any other page it is not.
#[test]
fn through_the_dev_server_the_link_is_its_own_and_its_changes_are_taken() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let (mut started, stdout, link) = launch("flayer", dir.path(), home.path(), &["--dev"]);
    let token = link
        .strip_prefix("http://127.0.0.1:5173/?token=")
        .unwrap_or_else(|| panic!("not the dev server's link: {link}"))
        .to_owned();
    let mut note = String::new();
    BufReader::new(started.child.stderr.take().unwrap())
        .read_line(&mut note)
        .unwrap();
    let address = note
        .split_once(" on ")
        .and_then(|(_, rest)| rest.split_once(", through the panel's dev server at "))
        .map(|(address, _)| address.parse().unwrap())
        .unwrap_or_else(|| panic!("no address said: {note}"));
    let panel = Panel {
        started,
        address,
        token,
        stdout,
    };
    let shelf = json!({"type": "shelf"});

    let (status, ran) = panel.send(
        "POST",
        "/api/commands",
        Auth::Cookie("http://127.0.0.1:5173"),
        Some(shelf.clone()),
    );
    assert_eq!(status, 200, "{ran}");

    let (status, refusal) = panel.send(
        "POST",
        "/api/commands",
        Auth::Cookie("http://127.0.0.1:5174"),
        Some(shelf),
    );
    assert_eq!(status, 403, "{refusal}");
    assert!(
        refusal["error"]
            .as_str()
            .is_some_and(|error| error.contains("only from its own pages")),
        "{refusal}"
    );
}

/// A port somebody holds is said as a failure, and nothing is served: no
/// link, and no other port in its place. Started rather than spawned, so a
/// panel that served after all fails the test once the wait is up, and is
/// killed, rather than hang it until a Ctrl-C.
#[test]
fn a_port_somebody_holds_is_a_failure_and_no_link() {
    let dir = workspace_with_own();
    let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = held.local_addr().unwrap().port().to_string();
    let mut args = vec![
        "ui",
        "--no-open",
        "--port",
        &port,
        "--home=",
        "--workspace=",
    ];
    args.extend(NO_AGENT_HOMES);

    let mut started = start("flayer", dir.path(), &args, &[]);
    let exited = exit(&mut started, Duration::from_secs(60));

    assert!(!exited.success(), "{exited:?}");
    let stdout = std::io::read_to_string(started.child.stdout.take().unwrap()).unwrap();
    let stderr = std::io::read_to_string(started.child.stderr.take().unwrap()).unwrap();
    assert_eq!(stdout, "");
    let said = format!("error: the panel cannot listen on 127.0.0.1:{port}: ");
    assert!(stderr.starts_with(&said), "{stderr}");
    assert!(
        stderr.ends_with("; name another with --port, or 0 for one the system picks\n"),
        "{stderr}"
    );
}

/// Ctrl-C is SIGINT here, sent by `kill`, as a terminal sends it: the panel
/// stops and exits as a command that went well does. On stdout, after the
/// link, what the TUI prints when it closes: every line that changed
/// something, with what each said, and no read. On stderr, where it served
/// and how it would stop.
#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_panel_which_prints_what_it_changed() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let mut panel = serve("flayer", dir.path(), home.path());
    let (status, _) = panel.post("/api/commands", json!({"type": "shelf"}));
    assert_eq!(status, 200);
    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let (status, ran) = panel.post("/api/commands", install);
    assert_eq!(status, 200, "{ran}");

    let pid = panel.started.child.id().to_string();
    let sent = std::process::Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .expect("run kill");
    assert!(sent.success());
    let exited = exit(&mut panel.started, Duration::from_secs(60));

    assert!(exited.success(), "{exited:?}");
    let run = &ran["outcome"]["runs"][0];
    let mut transcript = format!(
        "$ {}\n{}",
        run["command"].as_str().unwrap(),
        run["stdout"].as_str().unwrap()
    );
    for line in run["stderr"].as_array().unwrap() {
        transcript.push_str(&format!("{}\n", line["text"].as_str().unwrap()));
    }
    let mut rest = String::new();
    panel.stdout.read_to_string(&mut rest).unwrap();
    assert_eq!(rest, transcript);
    assert!(rest.starts_with("$ flayer install -p "), "{rest}");
    let mut said = String::new();
    let mut stderr = panel.started.child.stderr.take().unwrap();
    stderr.read_to_string(&mut said).unwrap();
    assert_eq!(
        said,
        format!(
            "note: serving the workspace at {} on {} until Ctrl-C\n",
            dir.path().display(),
            panel.address
        )
    );
}

/// How a started binary exited, waited for until `limit` has passed.
fn exit(started: &mut Started, limit: Duration) -> std::process::ExitStatus {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = started.child.try_wait().unwrap() {
            return status;
        }
        assert!(start.elapsed() < limit, "still running after {limit:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
