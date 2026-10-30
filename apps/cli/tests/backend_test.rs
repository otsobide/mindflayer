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
use mindflayer_cli::task::Caution;
use mindflayer_core::watch;
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
            json!({"type": "update", "holder": {"type": "project", "entry": alpha}, "reference": null, "force": false}),
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
        let preview = backend
            .preview(&task(sent.clone()))
            .map(|preview| preview.lines);
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
            backend.preview(&task(sent)).map(|preview| preview.lines),
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

    assert_eq!(
        backend
            .preview(&task(sent.clone()))
            .map(|preview| preview.lines),
        Ok(vec![line.clone()])
    );
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
            "yes": true,
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
            "yes": true,
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
        json!({"type": "files", "holder": {"type": "workspace"}, "reference": "x", "file": "SKILL.md"}),
        json!({"type": "files", "holder": {"type": "workspace"}}),
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
    // Cut where core cuts it, so the panel draws what it declares apart.
    assert_eq!(answer["manifest"]["front_matter"][0]["key"], "name");
    assert_eq!(answer["manifest"]["front_matter"][0]["value"], "alpha");
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
        assert_eq!(
            keys,
            ["file", "kind", "manifest", "name", "size", "text", "withheld"]
        );
        // No file but the one the skill is read from is cut into parts.
        assert_eq!(answer["manifest"], Value::Null, "{file}");
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

/// What a skill holds is listed by the route each file is asked for by, of
/// a linked project or the workspace's own and nothing else, as a file is.
#[test]
fn an_artifacts_files_are_listed_by_the_route_each_is_read_by() {
    let (dir, backend, skill) = contents_fixture();
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("references").join("guide.md"), "# Guide\n").unwrap();
    let files = |holder: Value, reference: &str| {
        backend.query(query(json!({
            "type": "files", "holder": holder, "reference": reference,
        })))
    };

    let listed = files(json!({"type": "project", "entry": "alpha"}), "skill:alpha").unwrap();

    assert_eq!(listed["manifest"], "SKILL.md");
    let routes: Vec<&Value> = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| &entry["path"])
        .collect();
    assert_eq!(
        routes,
        [
            &json!("SKILL.md"),
            &json!("references"),
            &json!("references/guide.md")
        ]
    );
    let guide = contents(&backend, "references/guide.md").unwrap();
    assert_eq!(guide["text"], "# Guide\n");
    for entry in ["gamma", dir.path().join("alpha").to_str().unwrap()] {
        assert_eq!(
            files(json!({"type": "project", "entry": entry}), "alpha")
                .map_err(|refusal| refusal.code),
            Err("not_registered"),
            "{entry}"
        );
    }
    assert_eq!(
        files(json!({"type": "workspace"}), "alpha").map_err(|refusal| refusal.code),
        Err("not_found")
    );
}

/// A link in a skill that leads out of its project is listed with why,
/// and nothing of where it leads reaches the browser.
#[cfg(unix)]
#[test]
fn a_link_out_of_its_holder_is_listed_with_why_and_never_followed() {
    let (dir, backend, skill) = contents_fixture();
    let secrets = dir.path().join("secrets");
    fs::create_dir_all(&secrets).unwrap();
    fs::write(secrets.join("token"), "TOP SECRET\n").unwrap();
    std::os::unix::fs::symlink(&secrets, skill.join("secrets")).unwrap();

    let listed = backend
        .query(query(json!({
            "type": "files",
            "holder": {"type": "project", "entry": "alpha"},
            "reference": "skill:alpha",
        })))
        .unwrap();

    let entries = listed["entries"].as_array().unwrap();
    let secrets = entries
        .iter()
        .find(|entry| entry["path"] == "secrets")
        .unwrap();
    assert_eq!(secrets["refused"]["code"], "contents_outside");
    assert_eq!(secrets["size"], Value::Null);
    let wire = serde_json::to_string(&listed).unwrap();
    assert!(
        !wire.contains("token") && !wire.contains("TOP SECRET"),
        "{wire}"
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
        backend.preview(&task(link.clone())).unwrap().lines,
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
        backend.preview(&task(init.clone())).unwrap().lines,
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

/// What the server watches is what core names for a snapshot, each path's
/// scope the holder a query reads it from, so the server learns no path of
/// its own.
#[test]
fn the_backend_watches_what_core_names_each_by_the_holder_a_query_names() {
    let dir = workspace_with_two();
    let root = dir.path();
    let workspace = FlayerWorkspace::open(root).unwrap();

    let watched = backend(root).watched_paths();

    let named: Vec<(PathBuf, Value)> = watch::watched(&workspace, None)
        .into_iter()
        .map(|each| {
            let scope = match each.scope {
                watch::Scope::Workspace => json!({"type": "workspace"}),
                watch::Scope::Project(entry) => json!({"type": "project", "entry": entry}),
            };
            (each.path, scope)
        })
        .collect();
    assert!(!named.is_empty());
    assert_eq!(
        watched
            .into_iter()
            .map(|each| (each.path, each.scope))
            .collect::<Vec<_>>(),
        named
    );
    let marker = root.join(FLAYER_DIR).join("flayer.toml");
    assert!(named.contains(&(marker, json!({"type": "workspace"}))));
    let alpha = root.join("alpha").join(".mind").join("settings.toml");
    assert!(named.contains(&(alpha, json!({"type": "project", "entry": "alpha"}))));
}

/// Gone since launch: watched where its marker would be, for its coming
/// back, rather than not at all.
#[test]
fn a_workspace_gone_since_launch_is_watched_for_its_marker() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    fs::remove_dir_all(dir.path().join(FLAYER_DIR)).unwrap();

    let watched = backend.watched_paths();

    let marker = dir.path().join(FLAYER_DIR).join("flayer.toml");
    assert!(watched
        .iter()
        .any(|each| each.path == marker && each.scope == json!({"type": "workspace"})));
}

/// What the panel has somebody confirm comes with the lines, each by its
/// place among them and in the CLI's words; a line that takes nothing away
/// asks nothing.
#[test]
fn a_preview_says_what_its_lines_take_away_for_the_panel_to_confirm() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    let unlink = Caution::Unlinks.to_string();
    let removes = Caution::Removes.to_string();
    let uninstalls = Caution::Uninstalls.to_string();

    for (sent, confirm) in [
        (
            json!({"type": "unlink", "entry": "alpha"}),
            json!([{"line": 0, "caution": "unlinks", "message": unlink}]),
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy", "everywhere": true, "yes": true,
            }),
            json!([
                {"line": 0, "caution": "removes", "message": removes},
                {"line": 0, "caution": "uninstalls", "message": uninstalls},
            ]),
        ),
        // Its dry run, which takes nothing.
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy", "everywhere": true, "yes": false,
            }),
            json!([]),
        ),
        // Its `mind init`, its `flayer link` and its `flayer unlink`.
        (
            json!({
                "type": "minds",
                "init": [under(dir.path(), &["gamma"])],
                "link": [under(dir.path(), &["gamma"])],
                "unlink": [under(dir.path(), &["beta"])],
            }),
            json!([{"line": 2, "caution": "unlinks", "message": unlink}]),
        ),
        (add_to(json!({"type": "workspace"})), json!([])),
        (
            json!({"type": "link", "path": under(dir.path(), &["alpha"])}),
            json!([]),
        ),
    ] {
        let preview = backend.preview(&task(sent.clone())).unwrap();
        assert_eq!(Value::Array(preview.confirm), confirm, "{sent}");
    }
}

/// Counted among the lines the panel shows, the `flayer init` that makes
/// the default workspace first among them.
#[test]
fn what_to_confirm_counts_the_line_that_makes_the_default_workspace() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let lookup = Lookup {
        home: Some(home.clone()),
        ..Lookup::default()
    };
    let backend = PanelBackend::launch(&home, &lookup).unwrap();

    let preview = backend
        .preview(&task(json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "deploy", "everywhere": false, "yes": true,
        })))
        .unwrap_or_else(|refusal| panic!("{refusal:?}"));

    assert_eq!(
        preview.lines,
        ["flayer init", "flayer remove --yes skill:deploy"]
    );
    assert_eq!(preview.confirm.len(), 1, "{preview:?}");
    assert_eq!(preview.confirm[0]["line"], 1);
    assert!(!home.join(FLAYER_DIR).exists(), "a preview made it");
}

/// What a scan the panel asks for is answered with: what `flayer scan
/// --suggest --json` finds, each directory as it prints it and in its
/// order, the words the command says why in, and what marking each asks
/// for; nothing for one linked.
#[test]
fn a_scan_is_answered_with_what_flayer_scan_suggest_finds_in_its_order_and_words() {
    let dir = workspace_with_candidates();
    let backend = backend(dir.path());

    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();

    let suggested = flayer(dir.path(), &["scan", "--suggest", "--json"]).unwrap();
    let printed: Value = serde_json::from_str(&suggested.stdout).unwrap();
    let found: Vec<&Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| &suggestion["found"])
        .collect();
    assert_eq!(
        Value::Array(found.into_iter().cloned().collect()),
        printed["data"]["found"]
    );
    let offered: Vec<(&str, &Value, &Value, &Value)> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| {
            (
                suggestion["found"]["route"].as_str().unwrap(),
                &suggestion["why"],
                &suggestion["offers"],
                &suggestion["moved_from"],
            )
        })
        .collect();
    assert_eq!(
        offered,
        [
            (
                "gamma",
                &json!("mind project with an id"),
                &json!("link"),
                &Value::Null
            ),
            (
                "repo",
                &json!("git repository"),
                &json!("init_and_link"),
                &Value::Null
            ),
            ("alpha", &Value::Null, &Value::Null, &Value::Null),
            ("beta", &Value::Null, &Value::Null, &Value::Null),
        ]
    );
    assert_eq!(answer["roots"], json!([dir.path().to_str().unwrap()]));
    assert_eq!(answer["depth"], 3);
    assert_eq!(answer["warnings"], json!([]));
}

/// Only the folders the browser names, each as a folder to link is named,
/// and only read: one the policy refuses is refused with its code before
/// anything is walked, and a scan leaves the disk as it found it.
#[test]
fn a_scan_walks_only_folders_the_policy_allows_and_writes_nothing() {
    let dir = workspace_with_candidates();
    let backend = backend(dir.path());
    let before = files(dir.path());
    let climbing = under(dir.path(), &["repo", ".."]);
    let flayer_dir = under(dir.path(), &[FLAYER_DIR]);

    for (root, code) in [
        ("repo", "path_relative"),
        ("file:///etc", "path_url"),
        (climbing.as_str(), "path_unnormalized"),
        (flayer_dir.as_str(), "path_in_mindflayer"),
    ] {
        let refusal = backend
            .query(query(
                json!({"type": "scan", "roots": [under(dir.path(), &["repo"]), root], "depth": 1}),
            ))
            .unwrap_err();
        assert_eq!(refusal.code, code, "{root}");
    }

    let answer = backend
        .query(query(
            json!({"type": "scan", "roots": [under(dir.path(), &["repo"])], "depth": 0}),
        ))
        .unwrap();
    assert_eq!(answer["found"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(answer["depth"], 0);
    assert_eq!(files(dir.path()), before);
    // A field a scan does not have is refused, as a task's is.
    assert!(serde_json::from_value::<Query>(
        json!({"type": "scan", "roots": [], "depth": 1, "deep": true})
    )
    .is_err());
}

/// A folder named that is not there is a warning with its code beside an
/// answer, as `flayer scan` warns of it, never an empty answer passed off as
/// complete.
#[test]
fn a_scan_says_what_it_could_not_read_as_flayer_scan_warns_of_it() {
    let dir = workspace_with_candidates();
    let missing = under(dir.path(), &["wrok"]);

    let answer = backend(dir.path())
        .query(query(
            json!({"type": "scan", "roots": [missing.clone()], "depth": 1}),
        ))
        .unwrap();

    let scan = flayer(dir.path(), &["scan", &missing]).unwrap();
    assert_eq!(answer["found"], json!([]));
    let said: Vec<Value> = scan
        .stderr
        .iter()
        .map(|line| json!({"text": line.text, "code": line.code}))
        .collect();
    assert_eq!(answer["warnings"], Value::Array(said));
    assert_eq!(answer["warnings"][0]["code"], "scan_folder_unlisted");
}

/// A project found moved offers no link beside the entry it left stale,
/// but names that entry as the marker spells it, which a relink takes.
#[test]
fn a_scan_names_the_entry_a_moved_project_left_for_its_relink() {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    let collapse = projects.join("collapse");
    fs::create_dir_all(&collapse).unwrap();
    flayer(&projects, &["init"]).unwrap();
    flayer(&projects, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    new_project(&collapse);
    flayer(&projects, &["link", "collapse"]).unwrap();
    flayer(&projects, &["install", "-p", "collapse", "deploy"]).unwrap();
    fs::create_dir_all(projects.join("apps")).unwrap();
    let moved = projects.join("apps/collapse");
    fs::rename(&collapse, &moved).unwrap();
    let backend = backend(&projects);

    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();

    let suggestion = &answer["found"][0];
    assert_eq!(suggestion["found"]["moved"], "collapse");
    assert_eq!(suggestion["moved_from"], "collapse");
    assert_eq!(suggestion["offers"], Value::Null);
    assert_eq!(suggestion["why"], "moved: collapse -> apps/collapse");
    let relink = task(json!({
        "type": "relink",
        "old": suggestion["moved_from"],
        "new": suggestion["found"]["path"],
    }));
    assert_eq!(
        backend.preview(&relink).unwrap().lines,
        [format!(
            "flayer relink collapse {}",
            moved.to_str().unwrap()
        )]
    );
    let outcome = backend.run(relink).unwrap();
    assert!(outcome.ok, "{outcome:?}");
    assert!(registry(&projects).contains("\"apps/collapse\""));
}

/// The panel's link batch is the minds screen's: for the same directories
/// marked, the lines the backend previews for what the panel sends are the
/// lines the screen's task is, each directory by its path, from a scan of a
/// folder outside the workspace, where both spell it so. Inside it they
/// spell a folder apart, and the batch is the same folder for folder
/// ([`inside_the_workspace_the_panels_link_batch_is_the_minds_screens_folder_for_folder`]).
#[test]
fn the_panels_link_batch_is_the_minds_screens_for_the_same_selection() {
    let dir = workspace_with_candidates();
    let work = TempDir::new().unwrap();
    let delta = work.path().join("acme/delta");
    fs::create_dir_all(&delta).unwrap();
    new_project(&delta);
    fs::create_dir_all(work.path().join("plain/.git")).unwrap();
    let root = work.path().to_str().unwrap();
    let backend = backend(dir.path());

    // Every directory found and not linked, marked as the scan offers it.
    let answer = backend
        .query(query(
            json!({"type": "scan", "roots": [root], "depth": null}),
        ))
        .unwrap();
    let marks: Vec<Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|suggestion| !suggestion["offers"].is_null())
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    assert_eq!(marks.len(), 2, "{answer}");
    let previewed = backend
        .preview(&task(json!({"type": "marks", "marks": marks})))
        .unwrap()
        .lines;

    // The same ones ticked on the screen, opened on the same folder.
    let mut app = tui::minds(dir.path(), &[work.path().to_path_buf()], &Lookup::default()).unwrap();
    for _ in 0..2 {
        press_key(&mut app, KeyCode::Char(' '));
        press_key(&mut app, KeyCode::Down);
    }
    let tui::state::Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let screened: Vec<String> = screen
        .task()
        .unwrap()
        .commands()
        .iter()
        .map(|line| mindflayer_cli::task::shell(line))
        .collect();

    assert_eq!(previewed, screened);
    assert_eq!(
        previewed,
        [
            format!("mind -C {} init", under(work.path(), &["plain"])),
            format!(
                "flayer link {} {}",
                under(work.path(), &["acme", "delta"]),
                under(work.path(), &["plain"])
            ),
        ]
    );
}

/// Inside the workspace the two spell a folder apart: the screen by the
/// route the scan gives from the root, where its lines run, and the panel
/// by its path, which is all the path policy lets a browser name. The same
/// batch all the same, folder for folder, and run, each leaves the
/// workspace linking the same entries, the repository made a project first.
#[test]
fn inside_the_workspace_the_panels_link_batch_is_the_minds_screens_folder_for_folder() {
    let panel = workspace_with_candidates();
    let screened = workspace_with_candidates();
    let backend = backend(panel.path());

    // Every directory found and not linked, marked as the scan offers it,
    // under the workspace's own folder, where the page starts.
    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();
    let marks: Vec<Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|suggestion| !suggestion["offers"].is_null())
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    assert_eq!(marks.len(), 2, "{answer}");
    let marked = task(json!({"type": "marks", "marks": marks}));
    let previewed = backend.preview(&marked).unwrap().lines;

    // The same ones ticked on the screen, in a workspace laid out the same.
    let mut app = tui::minds(screened.path(), &[], &Lookup::default()).unwrap();
    for _ in 0..2 {
        press_key(&mut app, KeyCode::Char(' '));
        press_key(&mut app, KeyCode::Down);
    }
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let ticked = screen.task().unwrap();
    let Task::Minds { init, link, unlink } = &ticked else {
        panic!("{ticked:?}")
    };
    assert_eq!(link, &["gamma", "repo"]);
    // Each route read from the root the screen's lines run from.
    let spelled = |routes: &[String]| -> Vec<String> {
        routes
            .iter()
            .map(|route| under(panel.path(), &[route]))
            .collect()
    };
    let by_path = Task::Minds {
        init: spelled(init),
        link: spelled(link),
        unlink: spelled(unlink),
    };
    let by_path: Vec<String> = by_path
        .commands()
        .iter()
        .map(|line| mindflayer_cli::task::shell(line))
        .collect();
    assert_eq!(previewed, by_path);

    let outcome = backend.run(marked).unwrap();
    assert!(outcome.ok, "{outcome:?}");
    tui::step(&mut app, ticked);
    tui::wait(&mut app);

    let entries = |dir: &Path| -> Vec<PathBuf> {
        FlayerWorkspace::open(dir)
            .unwrap()
            .members()
            .into_iter()
            .map(|member| member.entry)
            .collect()
    };
    assert_eq!(entries(panel.path()), entries(screened.path()));
    assert_eq!(
        entries(panel.path()),
        ["alpha", "beta", "gamma", "repo"].map(PathBuf::from)
    );
    for dir in [panel.path(), screened.path()] {
        assert!(
            MindProject::is_marked(&dir.join("repo")),
            "{}",
            dir.display()
        );
    }
}

// ---------------------------------------------------------------------------
// A plan of boxes ticked and unticked
// ---------------------------------------------------------------------------

/// Under the folder it returns: a workspace at `home`, with `deploy` and
/// `review` of its own, loading `team` beside it, whose `deploy` is the
/// team's; linking `alpha` and `beta`, each a repository given `mind init`,
/// `alpha` holding the workspace's `deploy` and `beta` its `review`. Its
/// second value is `home`.
fn assigning() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let team = dir.path().join("team");
    for root in [&home, &team] {
        fs::create_dir_all(root).unwrap();
        flayer(root, &["init"]).unwrap();
    }
    flayer(&home, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    flayer(&home, &["add", "skill", "review", "Review a change"]).unwrap();
    flayer(&team, &["add", "skill", "deploy", "Ship it the team's way"]).unwrap();
    flayer(&home, &["load", "../team"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = home.join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        flayer(&home, &["link", name]).unwrap();
    }
    flayer(
        &home,
        &[
            "install",
            "-p",
            "alpha",
            "--from",
            "workspace",
            "skill:deploy",
        ],
    )
    .unwrap();
    flayer(&home, &["install", "-p", "beta", "skill:review"]).unwrap();
    (dir, home)
}

fn plan(backend: &PanelBackend, selection: Value) -> Result<Value, Refusal> {
    backend.query(query(json!({"type": "plan", "selection": selection})))
}

fn cell(project: &str, reference: &str, origin: &str, wanted: bool) -> Value {
    json!({"project": project, "reference": reference, "origin": origin, "wanted": wanted})
}

/// What the projects under `home` hold: each file outside a `.git`, a
/// `.mind` or the `.mindflayer`, by its route from `home`, and what the
/// workspace records installed in each, by kind and name, with where it
/// came from.
#[allow(clippy::type_complexity)]
fn held(
    home: &Path,
) -> (
    BTreeMap<PathBuf, Vec<u8>>,
    Vec<(String, Kind, String, Option<String>)>,
) {
    let kept = |route: &Path| {
        !route.components().any(|part| {
            [".git", MIND_DIR, FLAYER_DIR].contains(&part.as_os_str().to_str().unwrap_or(""))
        })
    };
    let copies = files(home)
        .into_iter()
        .map(|(path, bytes)| (path.strip_prefix(home).unwrap().to_path_buf(), bytes))
        .filter(|(route, _)| kept(route))
        .collect();
    let workspace = FlayerWorkspace::locate(home).unwrap().unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();
    let mut records = Vec::new();
    for project in ["alpha", "beta"] {
        for kind in Kind::ALL {
            for record in ledger.records(project, kind).unwrap() {
                records.push((project.to_owned(), kind, record.name, record.origin));
            }
        }
    }
    (copies, records)
}

#[test]
fn a_plan_is_answered_with_the_tasks_that_carry_it_out_and_changes_nothing() {
    let (dir, home) = assigning();
    let backend = backend(&home);
    let before = files(dir.path());

    let answer = plan(
        &backend,
        json!([
            cell("beta", "skill:deploy", "load:../team", true),
            cell("alpha", "skill:deploy", "workspace", false),
            // Held from there already: as it is.
            cell("beta", "skill:review", "workspace", true),
        ]),
    )
    .unwrap();

    assert_eq!(
        answer,
        json!({
            "tasks": [
                {
                    "type": "install",
                    "projects": ["beta"],
                    "kind": "skill",
                    "name": "deploy",
                    "from": "load:../team",
                },
                {"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "deploy"},
            ],
            "conflicts": [],
            "locked": [],
        })
    );
    assert_eq!(files(dir.path()), before);

    // Sent back as one batch, each project named by the folder its entry
    // points at, as any task's are.
    let batch = task(json!({"type": "apply_install", "changes": answer["tasks"]}));
    assert_eq!(
        backend.preview(&batch).unwrap().lines,
        [
            format!(
                "flayer install -p {} --from load:../team skill:deploy",
                under(&home, &["beta"])
            ),
            format!(
                "flayer uninstall -p {} skill:deploy",
                under(&home, &["alpha"])
            ),
        ]
    );
}

#[test]
fn a_plan_names_only_projects_the_workspace_links_whatever_a_value_starts_with() {
    let (dir, home) = assigning();
    let backend = backend(&home);
    let before = files(dir.path());
    let alpha = under(&home, &["alpha"]);

    for project in [
        "--yes",
        "-C",
        "-W",
        "--home=/etc",
        "gamma",
        "../team",
        &alpha,
    ] {
        let refusal = plan(
            &backend,
            json!([
                cell("alpha", "skill:review", "workspace", true),
                cell(project, "skill:deploy", "workspace", true),
            ]),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "not_registered", "{project}");
    }

    // Anywhere else a value that starts with a dash is that value, which
    // nothing offers.
    let answer = plan(
        &backend,
        json!([cell("alpha", "--yes", "--from=/etc", true)]),
    )
    .unwrap();
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(
        answer["locked"][0]["why"]["code"], "not_offered",
        "{answer}"
    );
    assert_eq!(files(dir.path()), before);

    for sent in [
        json!({"type": "plan", "selection": [], "home": "/etc"}),
        json!({"type": "plan", "selection": [
            {"project": "alpha", "reference": "skill:deploy", "origin": "workspace", "wanted": true, "force": true},
        ]}),
        json!({"type": "plan", "selection": [
            {"project": "alpha", "reference": "skill:deploy", "origin": "workspace"},
        ]}),
    ] {
        assert!(
            serde_json::from_value::<Query>(sent.clone()).is_err(),
            "{sent}"
        );
    }
}

#[test]
fn a_plan_says_what_clashes_and_what_is_locked_by_the_codes_core_gives() {
    let (_dir, home) = assigning();
    for copy in copies(&home.join("beta"), "review") {
        fs::write(copy.join("SKILL.md"), skill_file("review", "Mine now")).unwrap();
    }
    let backend = backend(&home);

    let answer = plan(
        &backend,
        json!([
            cell("alpha", "skill:deploy", "workspace", true),
            cell("alpha", "skill:deploy", "load:../team", true),
            cell("beta", "skill:review", "workspace", false),
        ]),
    )
    .unwrap();

    assert_eq!(answer["tasks"], json!([]));
    let conflicts = answer["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{answer}");
    assert_eq!(conflicts[0]["why"]["code"], "wanted_twice");
    assert_eq!(conflicts[0]["cells"].as_array().unwrap().len(), 2);
    assert_eq!(
        answer["locked"],
        json!([{
            "cell": cell("beta", "skill:review", "workspace", false),
            "why": {
                "code": "edited",
                "message": "`review` in beta was changed since it was installed, so it is left \
                            alone unless a change to it is forced",
            },
        }])
    );
}

/// Put the screen's cursor on `project`'s box of `name` from `origin`, and
/// press space, as a person marking it does.
fn mark(screen: &mut Screen, project: &str, name: &str, origin: &str) {
    screen.cursor = screen
        .targets
        .iter()
        .position(|target| target.name() == project)
        .unwrap();
    let target = &mut screen.targets[screen.cursor];
    target.cursor = target
        .rows
        .iter()
        .position(|row| row.candidate.name() == name && row.candidate.origin() == origin)
        .unwrap();
    screen.focus = Focus::Offers;
    press(screen, &[KeyCode::Char(' ')]);
}

/// The panel's batch and the install screen's are one plan: the same boxes
/// marked on each, in two workspaces made alike, leave both holding the
/// same copies, recorded alike, by the same tasks.
#[test]
fn applying_a_plans_tasks_leaves_what_the_install_screen_leaves_for_the_same_marks() {
    let (_screened, screened) = assigning();
    let (_panel, panel) = assigning();

    let workspace = FlayerWorkspace::locate(&screened).unwrap().unwrap();
    let (projects, _) = install_screen::registered(&workspace);
    let mut screen = install_screen::open(&workspace, projects).unwrap();
    // The team's `deploy` in the place of the workspace's, which unticks it.
    mark(&mut screen, "alpha", "deploy", "load:../team");
    mark(&mut screen, "alpha", "review", "workspace");
    mark(&mut screen, "beta", "deploy", "workspace");
    mark(&mut screen, "beta", "review", "workspace");
    let applied = install_screen::apply(&workspace, &screen, &Lookup::default());
    assert!(applied.ok, "{}", applied.stdout);

    let backend = backend(&panel);
    let answer = plan(
        &backend,
        json!([
            cell("alpha", "skill:deploy", "workspace", false),
            cell("alpha", "skill:deploy", "load:../team", true),
            cell("alpha", "skill:review", "workspace", true),
            cell("beta", "skill:deploy", "workspace", true),
            cell("beta", "skill:review", "workspace", false),
        ]),
    )
    .unwrap();
    assert_eq!(
        (&answer["conflicts"], &answer["locked"]),
        (&json!([]), &json!([]))
    );
    let ran = backend
        .run(task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap();
    assert!(ran.ok, "{ran:?}");

    // The same tasks, in whichever order each was marked.
    let Task::ApplyInstall { changes } = screen.task() else {
        panic!("{:?}", screen.task())
    };
    let sorted = |tasks: Vec<Value>| {
        let mut said: Vec<String> = tasks.iter().map(Value::to_string).collect();
        said.sort();
        said
    };
    assert_eq!(
        sorted(serde_json::from_value(serde_json::to_value(&changes).unwrap()).unwrap()),
        sorted(serde_json::from_value(answer["tasks"].clone()).unwrap())
    );
    let state = held(&panel);
    assert_eq!(state, held(&screened));
    let (_, records) = state;
    assert_eq!(
        records,
        [
            (
                String::from("alpha"),
                Kind::Skill,
                String::from("deploy"),
                Some(String::from("load:../team"))
            ),
            (
                String::from("alpha"),
                Kind::Skill,
                String::from("review"),
                Some(String::from("workspace"))
            ),
            (
                String::from("beta"),
                Kind::Skill,
                String::from("deploy"),
                Some(String::from("workspace"))
            ),
        ]
    );
    let team = fs::read_to_string(copies(&panel.join("alpha"), "deploy")[0].join("SKILL.md"));
    assert!(team.unwrap().contains("the team's way"));
}

/// Everything the ledger of the workspace at `home` records of what was
/// installed, and what it logged, as text a row to a line: each
/// installation and the targets it wrote, with every hash, each file it
/// made, and each action with how it went. Without the times, which no two
/// runs share.
fn recorded(home: &Path) -> Vec<String> {
    let workspace = FlayerWorkspace::open(home).unwrap();
    let ledger = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    let mut rows = Vec::new();
    for sql in [
        "SELECT project, kind, name, coalesce(source_id, ''), coalesce(origin, ''), \
                coalesce(content_hash, '') \
           FROM installations ORDER BY project, kind, name",
        "SELECT i.project || ' ' || i.kind || ' ' || i.name, coalesce(t.agent, ''), t.path, \
                coalesce(t.content_hash, ''), '', '' \
           FROM installation_targets t JOIN installations i ON i.id = t.installation_id \
          ORDER BY i.project, i.kind, i.name, t.path",
        "SELECT project, path, '', '', '', '' FROM created_files ORDER BY project, path",
        "SELECT action, coalesce(target, ''), outcome, coalesce(detail, ''), '', '' \
           FROM actions ORDER BY id",
    ] {
        let mut statement = ledger.prepare(sql).unwrap();
        let found = statement
            .query_map([], |row| {
                (0..6)
                    .map(|at| row.get::<_, rusqlite::types::Value>(at))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        for row in found {
            rows.push(format!("{:?}", row.unwrap()));
        }
    }
    rows
}

/// What the panel applies is what its lines do typed into a shell: the same
/// boxes planned in two workspaces made alike, the batch applied in one and
/// its previewed lines run as `flayer` commands in the other, leave the same
/// copies and the same ledger, every hash and every action logged alike.
#[test]
fn applying_a_batch_leaves_the_ledger_as_running_its_lines_in_the_cli() {
    let (_panel, panel) = assigning();
    let (_typed, typed) = assigning();
    let selection = json!([
        cell("alpha", "skill:deploy", "workspace", false),
        cell("alpha", "skill:deploy", "load:../team", true),
        cell("alpha", "skill:review", "workspace", true),
        cell("beta", "skill:deploy", "workspace", true),
        cell("beta", "skill:review", "workspace", false),
    ]);

    let backend = backend(&panel);
    let answer = plan(&backend, selection.clone()).unwrap();
    let applied = backend
        .run(task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap();
    assert!(applied.ok, "{applied:?}");

    let other = self::backend(&typed);
    let answer = plan(&other, selection).unwrap();
    let lines = other
        .preview(&task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap()
        .lines;
    assert_eq!(lines.len(), applied.runs.len(), "{lines:?}");
    for line in &lines {
        // A line names a folder by its path, which a temporary one spells
        // with nothing a shell would quote.
        assert!(!line.contains(['\'', '"', '\\']), "{line}");
        let words: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(words[0], "flayer", "{line}");
        let ran = flayer(&typed, &words[1..]).unwrap();
        assert!(ran.ok, "{line}: {ran:?}");
    }

    assert_eq!(held(&panel), held(&typed));
    let ledger = recorded(&panel);
    assert_eq!(ledger, recorded(&typed));
    assert!(
        ledger.iter().any(|row| row.contains("load:../team")),
        "{ledger:#?}"
    );
}

/// Somebody's copy is locked by the plan, and refused by the command all
/// the same when a task that would change it is sent without one: the plan
/// says it before anything runs, and the run is what keeps it.
#[test]
fn a_task_reaching_somebodys_copy_is_refused_by_its_line_and_writes_nothing() {
    let (dir, home) = assigning();
    for copy in copies(&home.join("alpha"), "review") {
        fs::create_dir_all(&copy).unwrap();
        fs::write(copy.join("SKILL.md"), skill_file("review", "Mine")).unwrap();
    }
    let backend = backend(&home);
    let answer = plan(
        &backend,
        json!([cell("alpha", "skill:review", "workspace", false)]),
    )
    .unwrap();
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(answer["locked"][0]["why"]["code"], "foreign", "{answer}");
    let before = files(dir.path());
    let ledger = recorded(&home);

    for change in [
        json!({"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "review"}),
        json!({
            "type": "install", "projects": ["alpha"], "kind": "skill", "name": "review",
            "from": "workspace",
        }),
    ] {
        let ran = backend
            .run(task(json!({"type": "apply_install", "changes": [change]})))
            .unwrap();
        assert!(!ran.ok, "{change}: {ran:?}");
        let said: Vec<Option<&str>> = ran.runs[0].stderr.iter().map(|line| line.code).collect();
        let refused = ran.runs[0]
            .error
            .as_ref()
            .map(|error| error.code)
            .into_iter()
            .chain(said.into_iter().flatten())
            .collect::<Vec<_>>();
        assert!(refused.contains(&"foreign"), "{change}: {ran:?}");
    }
    assert_eq!(files(dir.path()), before);
    assert_eq!(recorded(&home), ledger);
}

// ---------------------------------------------------------------------------
// What an update would do, and the tasks the page of updates sends
// ---------------------------------------------------------------------------

/// Under the folder it returns: a workspace at `home` with the skills
/// `lint`, `review` and `old` of its own, linking `alpha` and `beta`, each a
/// repository given `mind init`: both received `lint`, changed in the
/// library since, `beta`'s copy of it in Claude's folder edited by hand;
/// `alpha` received `review`, its copy in `.agents/skills` deleted since;
/// and `beta` received `old`, removed from the library alone since. Its
/// second value is `home`.
fn updating() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    flayer(&home, &["init"]).unwrap();
    for (name, description) in [
        ("lint", "Tidy the code"),
        ("review", "Review a change"),
        ("old", "Done the old way"),
    ] {
        flayer(&home, &["add", "skill", name, description]).unwrap();
    }
    for (project, installs) in [("alpha", &["lint", "review"]), ("beta", &["lint", "old"])] {
        let root = home.join(project);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        flayer(&home, &["link", project]).unwrap();
        for name in installs {
            let installed = flayer(&home, &["install", "-p", project, name]).unwrap();
            assert!(installed.ok, "{installed:?}");
        }
    }
    let changed = |file: &Path, line: &str| {
        let text = fs::read_to_string(file).unwrap();
        fs::write(file, format!("{text}\n{line}\n")).unwrap();
    };
    changed(
        &own_directory_for(&home, Kind::Skill).join("lint/SKILL.md"),
        "Run the formatter first.",
    );
    changed(
        &home.join("beta/.claude/skills/lint/SKILL.md"),
        "Our own way.",
    );
    fs::remove_dir_all(home.join("alpha/.agents/skills/review")).unwrap();
    let removed = flayer(&home, &["remove", "--yes", "skill:old"]).unwrap();
    assert!(removed.stdout.starts_with("removed"), "{removed:?}");
    (dir, home)
}

#[test]
fn what_an_update_would_do_is_answered_as_core_surveys_it_and_changes_nothing() {
    let (dir, home) = updating();
    let backend = backend(&home);
    let before = files(dir.path());

    let answer = backend.query(query(json!({"type": "updates"}))).unwrap();

    assert_eq!(files(dir.path()), before);
    assert_eq!(
        answer["states"],
        json!([
            {"state": "outdated", "count": 1},
            {"state": "edited", "count": 1},
            {"state": "missing", "count": 1},
            {"state": "orphaned", "count": 1},
        ])
    );
    assert_eq!(answer["writes"], true);
    let listed: Vec<(&str, &str, &str, &str)> = answer["projects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| {
            project["installations"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |pending| {
                    (
                        project["entry"].as_str().unwrap(),
                        pending["name"].as_str().unwrap(),
                        pending["state"].as_str().unwrap(),
                        pending["does"].as_str().unwrap(),
                    )
                })
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("alpha", "lint", "outdated", "refresh"),
            ("alpha", "review", "missing", "put_back"),
            ("beta", "lint", "edited", "leave"),
            ("beta", "old", "orphaned", "orphaned"),
        ]
    );
    let left: Vec<&Value> = answer["projects"][1]["installations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pending| &pending["left"]["code"])
        .collect();
    assert_eq!(left, [&json!("edited"), &json!("no_longer_offered")]);
}

/// Asked of the default workspace before anything has made it: nothing to
/// update, and nothing made to say so, as a dry run of `update --all`.
#[test]
fn what_an_update_would_do_in_a_home_where_nothing_is_made_is_nothing_and_makes_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let backend = PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap();

    let answer = backend.query(query(json!({"type": "updates"}))).unwrap();

    assert_eq!(answer["projects"], json!([]));
    assert_eq!(answer["writes"], false);
    assert_eq!(answer["unread"], json!([]));
    assert!(!home.join(FLAYER_DIR).exists(), "asking made the workspace");
}

#[test]
fn a_query_of_what_an_update_would_do_takes_no_field() {
    let refused = serde_json::from_value::<Query>(json!({"type": "updates", "all": true}));

    assert!(refused.is_err(), "{refused:?}");
}

/// Each change the page of updates and the page of one of the library's own
/// offer, as the browser sends it, is the line somebody would type for it,
/// each project named by the folder its entry points at; and what takes
/// something away, a forced update, an uninstall and a removal told yes, is
/// asked about first, while a removal's dry run is not.
#[test]
fn each_change_the_pages_of_updates_and_of_the_library_send_is_the_line_the_cli_takes() {
    let (_dir, home) = updating();
    let backend = backend(&home);
    let alpha = under(&home, &["alpha"]);
    let beta = under(&home, &["beta"]);
    let project = |entry: &str| json!({"type": "project", "entry": entry});

    for (sent, line, confirm) in [
        (
            json!({"type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false}),
            String::from("flayer update --all --yes"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("alpha"), "reference": null, "force": false}),
            format!("mind -C {alpha} update --yes"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("alpha"), "reference": "skill:lint", "force": false}),
            format!("mind -C {alpha} update --yes skill:lint"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("beta"), "reference": "skill:lint", "force": true}),
            format!("mind -C {beta} update --yes --force skill:lint"),
            vec!["forces"],
        ),
        (
            json!({"type": "uninstall", "projects": ["beta"], "kind": "skill", "name": "old"}),
            format!("flayer uninstall -p {beta} skill:old"),
            vec!["uninstalls"],
        ),
        (
            json!({"type": "uninstall_orphaned", "holder": {"type": "workspace"}}),
            String::from("flayer uninstall --all-projects --orphaned"),
            vec!["uninstalls"],
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "lint", "everywhere": true, "yes": false,
            }),
            String::from("flayer remove --everywhere skill:lint"),
            Vec::new(),
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "lint", "everywhere": true, "yes": true,
            }),
            String::from("flayer remove --everywhere --yes skill:lint"),
            vec!["removes", "uninstalls"],
        ),
    ] {
        let preview = backend.preview(&task(sent.clone())).unwrap();

        assert_eq!(preview.lines, [line], "{sent}");
        let asked: Vec<&str> = preview
            .confirm
            .iter()
            .map(|confirm| confirm["caution"].as_str().unwrap())
            .collect();
        assert_eq!(asked, confirm, "{sent}");
    }
}

/// Updating every project from the panel leaves what `flayer update --all
/// --yes` leaves, in two workspaces made alike: the outdated copy and the
/// one gone written from the library, the edited one left as it was, and
/// the edit said by the code an install refuses it with.
#[test]
fn updating_every_project_from_the_panel_leaves_what_the_cli_leaves() {
    let (_typed, typed) = updating();
    let (_panel, panel) = updating();

    let cli = flayer(&typed, &["update", "--all", "--yes"]).unwrap();
    let ran = backend(&panel)
        .run(task(json!({
            "type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false,
        })))
        .unwrap();

    assert!(!cli.ok);
    assert!(!ran.ok);
    let run = &ran.runs[0];
    assert_eq!(run.stdout, cli.stdout);
    assert_eq!(run.stderr, cli.stderr);
    assert!(run
        .stderr
        .iter()
        .any(|line| line.code == Some("edited") && line.text.contains("beta")));
    let state = |home: &Path| {
        files(home)
            .into_iter()
            .filter_map(|(path, bytes)| {
                let route = path.strip_prefix(home).unwrap().to_path_buf();
                let kept = !route.components().any(|part| {
                    [".git", MIND_DIR, FLAYER_DIR]
                        .contains(&part.as_os_str().to_str().unwrap_or(""))
                });
                kept.then_some((route, bytes))
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(state(&panel), state(&typed));
    let edited = fs::read_to_string(panel.join("beta/.claude/skills/lint/SKILL.md")).unwrap();
    assert!(edited.contains("Our own way."), "{edited}");
    let after = backend(&panel)
        .query(query(json!({"type": "updates"})))
        .unwrap();
    assert_eq!(after["states"][0], json!({"state": "outdated", "count": 0}));
    assert_eq!(after["states"][2], json!({"state": "missing", "count": 0}));
}
