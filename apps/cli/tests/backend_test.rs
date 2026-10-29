//! The panel's backend, driven as the server drives it: every query and task
//! as the JSON a browser sends, read into the backend's own types, and
//! handed to the `Backend` trait. No port is bound: the server's own suites
//! cover the transport, over a fake backend.

// The backend is the panel's, which a build without the `ui` feature lacks.
#![cfg(feature = "ui")]

mod common;

use std::collections::BTreeMap;

use common::*;
use mindflayer_cli::backend::{Outcome, PanelBackend, Query};
use mindflayer_server::{Backend, Refusal};
use serde_json::{json, Value};

/// The backend over the workspace at `root`, launched there with nothing
/// named, activated or at home, and no agent home: it can reach no
/// workspace but the fixture's.
fn backend(root: &Path) -> PanelBackend {
    PanelBackend::launch(root, &Lookup::default()).unwrap()
}

/// What a browser sent, read as the server reads it.
fn task(sent: Value) -> Task {
    serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
}

fn query(sent: Value) -> Query {
    serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
}

/// Every file under `dir` with what it holds, to tell that nothing changed.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                folders.push(entry.path());
            } else if kind.is_file() {
                found.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    found
}

/// A path under `dir`, spelled with the platform's separator.
fn under(dir: &Path, parts: &[&str]) -> String {
    let mut text = dir.to_str().unwrap().to_owned();
    for part in parts {
        text.push(std::path::MAIN_SEPARATOR);
        text.push_str(part);
    }
    text
}

fn add_to(holder: Value) -> Value {
    json!({
        "type": "add",
        "holder": holder,
        "kind": "skill",
        "name": "deploy",
        "description": "Ship it",
        "template": null,
    })
}

/// A browser's task, shaped to make a line act somewhere it should not:
/// each refused with a code by its preview and by its run, and nothing of it
/// run, the disk left as it was.
#[test]
fn a_task_shaped_to_escape_is_refused_with_a_code_and_nothing_runs() {
    let dir = workspace_with_two();
    let root = dir.path();
    let alpha = under(root, &["alpha"]);
    let gamma = under(root, &["gamma"]);
    let flayer = under(root, &[FLAYER_DIR]);
    fs::create_dir(&gamma).unwrap();
    let backend = backend(root);
    let before = files(root);

    let cases: Vec<(&str, Value, &str)> = vec![
        // Values that start with a dash.
        (
            "a path",
            json!({"type": "link", "path": "--yes"}),
            "path_relative",
        ),
        (
            "a -C",
            json!({"type": "link", "path": "-C"}),
            "path_relative",
        ),
        (
            "an entry",
            json!({"type": "unlink", "entry": "-rf"}),
            "not_registered",
        ),
        (
            "a holder",
            add_to(json!({"type": "project", "entry": "--home=/etc"})),
            "not_registered",
        ),
        (
            "a project",
            json!({"type": "install", "projects": ["-W"], "kind": "skill", "name": "alpha", "from": null}),
            "not_registered",
        ),
        (
            "a load",
            json!({"type": "unload", "entry": "--workspace="}),
            "not_loaded",
        ),
        // `..` parts.
        (
            "a link climbing out",
            json!({"type": "link", "path": under(root, &["alpha", "..", "..", "etc"])}),
            "path_unnormalized",
        ),
        (
            "a load climbing out",
            json!({"type": "load", "path": under(root, &[".."])}),
            "path_unnormalized",
        ),
        (
            "a holder climbing back",
            add_to(json!({"type": "project", "entry": "alpha/.."})),
            "not_registered",
        ),
        (
            "a holder from beside",
            json!({"type": "validate", "holder": {"type": "elsewhere", "entry": "../alpha"}}),
            "not_registered",
        ),
        // Relative paths.
        (
            "a bare name",
            json!({"type": "link", "path": "gamma"}),
            "path_relative",
        ),
        (
            "a path from here",
            json!({"type": "init_and_link", "path": "./gamma"}),
            "path_relative",
        ),
        (
            "a mark",
            json!({"type": "minds", "init": ["gamma"], "link": ["gamma"], "unlink": []}),
            "path_relative",
        ),
        (
            "a new place",
            json!({"type": "relink", "old": "alpha", "new": "elsewhere"}),
            "path_relative",
        ),
        // Entries nothing links.
        (
            "an unlink",
            json!({"type": "unlink", "entry": "gamma"}),
            "not_registered",
        ),
        (
            "a holder",
            json!({"type": "validate", "holder": {"type": "elsewhere", "entry": "gamma"}}),
            "not_registered",
        ),
        (
            "a holder by its path",
            json!({"type": "update", "holder": {"type": "project", "entry": alpha}, "reference": null}),
            "not_registered",
        ),
        (
            "one project of two",
            json!({"type": "uninstall", "projects": ["alpha", "gamma"], "kind": "skill", "name": "alpha"}),
            "not_registered",
        ),
        (
            "a mark to unlink",
            json!({"type": "minds", "init": [], "link": [], "unlink": [gamma]}),
            "not_registered",
        ),
        (
            "an unload",
            json!({"type": "unload_here", "path": gamma}),
            "not_loaded",
        ),
        // Inside a `.mindflayer`.
        (
            "the folder itself",
            json!({"type": "link", "path": flayer}),
            "path_in_mindflayer",
        ),
        (
            "its library",
            json!({"type": "load", "path": under(root, &[FLAYER_DIR, "skills"])}),
            "path_in_mindflayer",
        ),
        (
            "a clone on its shelf",
            json!({"type": "init_and_link", "path": under(root, &[FLAYER_DIR, "shelf", "x"])}),
            "path_in_mindflayer",
        ),
        // URLs.
        (
            "a file URL",
            json!({"type": "link", "path": "file:///etc"}),
            "path_url",
        ),
        (
            "a file URL",
            json!({"type": "load", "path": "file:///etc/"}),
            "path_url",
        ),
        (
            "a gather",
            json!({"type": "gather", "url": "file:///etc", "folder": "skills", "reference": null}),
            "gather_unwatched",
        ),
        // What no front end without a terminal runs.
        (
            "an editor",
            json!({"type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x"}),
            "opens_editor",
        ),
        ("a screen", json!({"type": "open_install"}), "opens_screen"),
        (
            "an editor in a batch",
            json!({"type": "apply_install", "changes": [
                {"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "alpha"},
                {"type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x"},
            ]}),
            "opens_editor",
        ),
    ];

    for (what, sent, code) in cases {
        let preview = backend.preview(&task(sent.clone()));
        let run = backend.run(task(sent.clone()));
        assert_eq!(
            preview.map_err(|refusal| refusal.code),
            Err(code),
            "{what}: {sent}"
        );
        assert_eq!(
            run.map_err(|refusal| refusal.code),
            Err(code),
            "{what}: {sent}"
        );
    }
    assert_eq!(files(root), before, "something ran");
}

/// A refusal of a command says which line it would have been, so the
/// panel can say which command it will not run.
#[test]
fn a_refused_command_is_named_in_what_the_refusal_says() {
    let dir = workspace_with_two();

    let refusal = backend(dir.path())
        .preview(&task(json!({
            "type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x",
        })))
        .unwrap_err();

    assert_eq!(
        refusal,
        Refusal {
            code: "opens_editor",
            message: String::from(
                "`flayer edit skill:x` opens an editor, which needs a terminal to draw on"
            ),
        }
    );
}

/// The browser never supplies them: a task has no field for any, and one
/// with fields sent with one does not read as a task at all. One with none,
/// `shelf` say, passes it over, as serde does for a variant with no fields
/// to hold it to; nothing of it reaches a line either way.
#[test]
fn a_task_cannot_carry_a_directory_a_home_a_workspace_or_an_agent_home() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    for field in [
        "directory",
        "home",
        "workspace",
        "claude_config_dir",
        "codex_home",
        "opencode_config_dir",
        "agents_dir",
    ] {
        let mut sent = json!({"type": "validate", "holder": {"type": "workspace"}});
        sent[field] = json!("/etc");
        assert!(
            serde_json::from_value::<Task>(sent.clone()).is_err(),
            "{sent}"
        );
        let mut sent = json!({"type": "shelf"});
        sent[field] = json!("/etc");
        assert_eq!(
            backend.preview(&task(sent)),
            Ok(vec![String::from("flayer gather list")])
        );
    }
}

/// The holder's entry becomes the `-C` the backend resolved, absolute, and
/// the line's only one; the change lands in that project and nowhere else.
#[test]
fn a_task_held_by_a_linked_project_runs_in_that_project_with_the_directory_resolved() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    let beta = dir.path().join("beta");
    let backend = backend(dir.path());
    let sent = add_to(json!({"type": "project", "entry": "alpha"}));
    let line = format!("mind -C {} add skill deploy 'Ship it'", alpha.display());
    let beta_before = files(&beta);

    assert_eq!(backend.preview(&task(sent.clone())), Ok(vec![line.clone()]));
    let outcome = backend.run(task(sent)).unwrap();

    assert_eq!(outcome.runs.len(), 1);
    let run = &outcome.runs[0];
    assert_eq!(run.command, line);
    assert_eq!(line.split(' ').filter(|word| *word == "-C").count(), 1);
    assert!(outcome.ok, "{outcome:?}");
    assert!(
        run.stdout.starts_with("created skill deploy at "),
        "{:?}",
        run.stdout
    );
    assert!(directory_for(&alpha, Kind::Skill)
        .join("deploy")
        .join("SKILL.md")
        .is_file());
    assert_eq!(files(&beta), beta_before);
}

/// What came of each line, with the code of what went wrong beside it, in
/// JSON as the server sends it on.
#[test]
fn a_line_that_fails_says_so_with_its_code() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());

    let outcome = backend
        .run(task(json!({
            "type": "remove",
            "holder": {"type": "project", "entry": "alpha"},
            "kind": "skill",
            "name": "missing",
            "everywhere": false,
        })))
        .unwrap();

    assert!(!outcome.ok);
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["runs"][0]["error"]["code"], "not_found");
    assert_eq!(json["runs"][0]["ok"], false);
    let mut keys: Vec<&String> = json["runs"][0].as_object().unwrap().keys().collect();
    keys.sort_unstable();
    assert_eq!(keys, ["command", "error", "ok", "stderr", "stdout"]);
}

/// What `flayer ui` prints once it stops: every line of a task that changed
/// something, failed ones included, with what each said, as the TUI's
/// transcript lays it out; a read changed nothing, and is left out. Kept by
/// a clone, as `ui` keeps one while the server holds the backend.
#[test]
fn the_transcript_holds_the_lines_that_changed_something_and_no_read() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    let kept = backend.clone();

    backend.run(task(json!({"type": "shelf"}))).unwrap();
    let added = backend
        .run(task(add_to(json!({"type": "workspace"}))))
        .unwrap();
    let missing = backend
        .run(task(json!({
            "type": "remove",
            "holder": {"type": "workspace"},
            "kind": "skill",
            "name": "missing",
            "everywhere": false,
        })))
        .unwrap();

    let added = &added.runs[0];
    let missing = &missing.runs[0];
    assert!(added.ok && !missing.ok);
    let mut expected = format!("$ {}\n{}", added.command, added.stdout);
    for line in &added.stderr {
        expected.push_str(&format!("{}\n", line.text));
    }
    expected.push_str(&format!("$ {}\n", missing.command));
    for line in &missing.stderr {
        expected.push_str(&format!("{}\n", line.text));
    }
    let error = missing.error.as_ref().unwrap();
    expected.push_str(&format!("error: {}\n", error.message));
    assert_eq!(kept.transcript(), expected);
    assert!(!expected.contains("gather list"));
}

#[test]
fn a_snapshot_is_of_the_workspace_the_panel_was_launched_on() {
    let dir = workspace_with_two();
    let backend = backend(&dir.path().join("alpha"));

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();

    assert_eq!(snapshot["format"], 1);
    assert_eq!(snapshot["workspace"]["path"], dir.path().to_str().unwrap());
    assert_eq!(snapshot["workspace"]["reason"], "above");
    let entries: Vec<&Value> = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|project| &project["entry"])
        .collect();
    assert_eq!(entries, [&json!("alpha"), &json!("beta")]);
}

#[test]
fn a_query_sent_with_a_field_it_does_not_have_is_refused() {
    for sent in [
        json!({"type": "snapshot", "home": "/etc"}),
        json!({"type": "contents", "holder": {"type": "workspace"}, "reference": "x", "file": "SKILL.md", "raw": true}),
        json!({"type": "contents", "holder": {"type": "workspace"}, "reference": "x"}),
    ] {
        assert!(
            serde_json::from_value::<Query>(sent.clone()).is_err(),
            "{sent}"
        );
    }
}

/// `alpha`'s own skill, holding beside its `SKILL.md` what a gathered skill
/// might carry to run in a browser.
fn contents_fixture() -> (TempDir, PanelBackend, PathBuf) {
    let dir = workspace_with_two();
    let skill = directory_for(&dir.path().join("alpha"), Kind::Skill).join("alpha");
    let backend = backend(dir.path());
    (dir, backend, skill)
}

fn contents(backend: &PanelBackend, file: &str) -> Result<Value, Refusal> {
    backend.query(query(json!({
        "type": "contents",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "skill:alpha",
        "file": file,
    })))
}

#[test]
fn a_skills_file_comes_back_as_text_inside_json() {
    let (_dir, backend, _skill) = contents_fixture();

    let answer = contents(&backend, "SKILL.md").unwrap();

    assert_eq!(answer["text"], skill_file("alpha", "A skill"));
    assert_eq!(answer["withheld"], Value::Null);
    assert_eq!(answer["kind"], "skill");
}

/// Markup a browser would render, or run, reaches it only as a string in
/// JSON: nothing in the answer says what type of file it is, so nothing can
/// serve it as one.
#[test]
fn an_html_asset_or_an_svg_holding_a_script_is_not_served_as_content() {
    let (_dir, backend, skill) = contents_fixture();
    let page = "<!doctype html><script>fetch('/api/commands',{method:'POST'})</script>\n";
    let image =
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(document.cookie)</script></svg>\n";
    fs::write(skill.join("page.html"), page).unwrap();
    fs::write(skill.join("logo.svg"), image).unwrap();

    for (file, markup) in [("page.html", page), ("logo.svg", image)] {
        let answer = contents(&backend, file).unwrap();
        assert_eq!(answer["text"], markup, "{file}");
        let mut keys: Vec<&String> = answer.as_object().unwrap().keys().collect();
        keys.sort_unstable();
        assert_eq!(keys, ["file", "kind", "name", "size", "text", "withheld"]);
        // What goes over the wire is a JSON object, the markup a string in it.
        let wire = serde_json::to_string(&answer).unwrap();
        assert!(wire.starts_with('{'), "{wire}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_links_out_of_its_holder_is_not_served_as_content() {
    let (dir, backend, skill) = contents_fixture();
    let secret = dir.path().join("secret.env");
    fs::write(&secret, "TOKEN=1\n").unwrap();
    std::os::unix::fs::symlink(&secret, skill.join("notes.md")).unwrap();

    let refusal = contents(&backend, "notes.md").unwrap_err();

    assert_eq!(refusal.code, "contents_outside");
}

/// `show` prints an artifact whole, read through its links, which is what
/// a terminal's user asked for and not what a browser may: the file goes to
/// the browser only through the contents query, which refuses it.
#[cfg(unix)]
#[test]
fn a_rule_that_links_out_of_its_project_is_not_shown_to_the_browser() {
    let (dir, backend, _skill) = contents_fixture();
    let secret = dir.path().join("secret.txt");
    fs::write(&secret, "TOP SECRET KEY\n").unwrap();
    let rules = directory_for(&dir.path().join("alpha"), Kind::Rule);
    fs::create_dir_all(&rules).unwrap();
    std::os::unix::fs::symlink(&secret, rules.join("leak.md")).unwrap();
    let show = json!({
        "type": "show",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "rule:leak",
    });

    for refusal in [
        backend.preview(&task(show.clone())).unwrap_err(),
        backend.run(task(show.clone())).unwrap_err(),
    ] {
        assert_eq!(refusal.code, "show_unheld");
        assert!(!refusal.message.contains("TOP SECRET"), "{refusal:?}");
    }
    let asked = backend.query(query(json!({
        "type": "contents",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "rule:leak",
        "file": "leak.md",
    })));
    assert_eq!(
        asked.map_err(|refusal| refusal.code),
        Err("contents_outside")
    );
}

#[test]
fn an_oversized_file_is_not_served_as_content() {
    let (_dir, backend, skill) = contents_fixture();
    let cap = mindflayer_core::contents::CAP;
    fs::write(
        skill.join("huge.md"),
        "a".repeat(usize::try_from(cap).unwrap() + 1),
    )
    .unwrap();

    let answer = contents(&backend, "huge.md").unwrap();

    assert_eq!(answer["text"], Value::Null);
    assert_eq!(answer["withheld"], "too_large");
    assert_eq!(answer["size"], cap + 1);
}

#[test]
fn contents_are_asked_of_a_linked_project_or_the_workspace_and_nothing_else() {
    let (dir, backend, _skill) = contents_fixture();
    let ask = |holder: Value, reference: &str, file: &str| {
        backend.query(query(json!({
            "type": "contents", "holder": holder, "reference": reference, "file": file,
        })))
    };

    for entry in [
        "gamma",
        "../alpha",
        "alpha/..",
        dir.path().join("alpha").to_str().unwrap(),
    ] {
        assert_eq!(
            ask(
                json!({"type": "project", "entry": entry}),
                "alpha",
                "SKILL.md"
            )
            .map_err(|refusal| refusal.code),
            Err("not_registered"),
            "{entry}"
        );
    }
    assert_eq!(
        ask(
            json!({"type": "project", "entry": "alpha"}),
            "alpha",
            "../../beta/skills/beta/SKILL.md"
        )
        .map_err(|refusal| refusal.code),
        Err("contents_route_unfit")
    );
    // The workspace's own library holds nothing of `alpha`'s.
    assert_eq!(
        ask(json!({"type": "workspace"}), "alpha", "SKILL.md").map_err(|refusal| refusal.code),
        Err("not_found")
    );
}

/// Launched on the default workspace before anything made it, the backend
/// reads it as it will be, and its first change makes it first, by a line of
/// its own, as the TUI's does.
#[test]
fn the_default_workspace_is_made_by_the_first_change_and_said_first() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let fresh = dir.path().join("fresh");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&fresh).unwrap();
    new_project(&fresh);
    let backend = PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap();

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();
    assert_eq!(snapshot["workspace"]["made"], false);
    assert_eq!(snapshot["workspace"]["reason"], "home");
    let link = json!({"type": "link", "path": fresh.to_str().unwrap()});
    assert_eq!(
        backend.preview(&task(link.clone())).unwrap(),
        [
            String::from("flayer init"),
            format!("flayer link {}", fresh.display())
        ]
    );
    assert!(!home.join(FLAYER_DIR).exists(), "a preview made it");

    let outcome: Outcome = backend.run(task(link)).unwrap();

    assert!(outcome.ok, "{outcome:?}");
    let workspace = FlayerWorkspace::open(&home).unwrap();
    assert!(workspace.is_linked(&fresh));
}

/// Its own making is one `flayer init`, not a second in front of it.
#[test]
fn the_default_workspace_asked_to_be_made_is_made_by_one_line() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let backend = PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap();
    let init = json!({"type": "init_workspace"});

    assert_eq!(
        backend.preview(&task(init.clone())).unwrap(),
        [String::from("flayer init")]
    );
    let outcome: Outcome = backend.run(task(init)).unwrap();

    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.runs.len(), 1, "{outcome:?}");
    assert!(FlayerWorkspace::open(&home).is_ok());
}

/// Pinned to the one it was launched on: a workspace that has gone since is
/// refused, never made again by the first change.
#[test]
fn a_workspace_gone_since_launch_is_refused_rather_than_made_again() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    fs::remove_dir_all(dir.path().join(FLAYER_DIR)).unwrap();

    let refusal = backend
        .preview(&task(
            json!({"type": "link", "path": dir.path().join("alpha").to_str().unwrap()}),
        ))
        .unwrap_err();

    assert_eq!(refusal.code, "not_a_workspace");
    assert!(!dir.path().join(FLAYER_DIR).exists());
}
