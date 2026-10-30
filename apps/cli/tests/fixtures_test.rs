//! The snapshots the panel's tests render from, the answers to what its
//! page of an artifact asks, and what previewing and running a few tasks is
//! answered with, written from the real thing: workspaces built by running
//! the commands, read and changed through the panel's backend as the server
//! answers it, and written into the panel's `tests/fixtures` (`make
//! web/fixtures`), committed beside its tests.
//!
//! Written rather than drawn by hand, so that a test of the panel renders
//! what core says and nothing a person imagined it says: a field added,
//! renamed or holding something else reaches the fixtures the next time
//! they are written, and `make web/fixtures/check`, which CI's `types drift
//! (web)` runs, fails until they are committed. Under the `ts` feature, as
//! the export of the panel's types is, so a plain `cargo test` writes
//! nothing anywhere.
//!
//! What would differ from one run to the next is pinned: the temporary
//! folder reads as `/home`, each project's id is one of [`IDS`], and every
//! time the ledger keeps is [`AT`] or a minute after the one before.

#![cfg(feature = "ts")]

mod common;

use common::*;
use mindflayer_cli::backend::{PanelBackend, Query};
use mindflayer_core::ledger::{Action, Outcome as Logged};
use mindflayer_core::SourceKind;
use mindflayer_server::Backend;
use serde_json::{json, Value};

/// Where the fixtures go: the panel's tests, beside this crate, whichever
/// folder cargo was started in.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/tests/fixtures");

/// What the temporary folder a fixture is built in reads as.
const HOME: &str = "/home";

/// The id each project reads as, by its route from the workspace, in place
/// of the random one `mind init` gave it: the workspace's own, then those
/// [`candidates`] makes, which only some fixtures build.
const IDS: [(&str, &str); 6] = [
    ("api", "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"),
    ("web", "0123456789abcdef0123456789abcdef"),
    ("docs", "fedcba9876543210fedcba9876543210"),
    ("blog", "b10b10b10b10b10b10b10b10b10b10b1"),
    ("drafts", "d4af7ed4af7ed4af7ed4af7ed4af7ed4"),
    ("archive/old", "01d01d01d01d01d01d01d01d01d01d01"),
];

/// The first time the ledger keeps, in seconds since the epoch: an
/// afternoon in October 2026.
const AT: i64 = 1_792_850_400;

/// The source the shelf is filled from, and the commit it was taken at.
const SOURCE: &str = "https://github.com/acme/skills";
const REVISION: &str = "8d1c3f0a2b4e6d8f0a1c3e5b7d9f1a3c5e7b9d2f";

/// What the library's `review` says, as somebody would write a skill: front
/// matter beyond the two keys every skill has, and a body with something of
/// everything markdown draws, a table, a fence, a link to a file of its own
/// and one to another site among them.
const REVIEW: &str = "---
name: review
description: Read a change before it lands
license: MIT
metadata:
  owner: platform
---

# review

Read the diff **before** it lands, and say what you would change.

## Steps

1. Run `scripts/diff.sh` to see what changed.
2. Go through [the checklist](references/checklist.md).
3. Leave one comment per finding, and *say why*.

| check | why |
| :--- | ---: |
| tests | they pass |

The long version is at <https://example.com/review>.

```sh
scripts/diff.sh main
```
";

/// `you/Projects`, a workspace holding something of everything the panel
/// shows, built as somebody would build it, a command at a time:
///
/// - its library holds the skills `review`, with a script and a reference
///   beside its `SKILL.md`, and `lint`, and the rules `git/commits` and
///   `style/comments`;
/// - `api`, for Claude and Codex and local, authors `deploy`, and received
///   `review` and `git/commits`, as they were written: nothing to do;
/// - `web`, for Claude and OpenCode and shared, received `lint`, changed in
///   the library since, and `review`, one copy of which was edited since,
///   and keeps a skill by hand in Claude's folder: something to do;
/// - `docs`, for Codex, installs skills into `.skills`, which Codex does not
///   read, received `review` there, and authors a skill whose file does not
///   read: broken;
/// - `gone` was linked, and its folder has gone since: stale;
/// - it loads `team`, beside it, which has gone too;
/// - its shelf holds `format` and `triage`, gathered from one source.
fn workspace(home: &Path) -> PathBuf {
    let root = home.join("Projects");
    fs::create_dir_all(&root).unwrap();
    flayer(&root, &["init"]).unwrap();
    for (kind, name, description) in [
        ("skill", "review", "Read a change before it lands"),
        ("skill", "lint", "Tidy the code before a commit"),
        ("rule", "git/commits", "Write commit messages that say why"),
        ("rule", "style/comments", "Comment the why, not the what"),
    ] {
        flayer(&root, &["add", kind, name, description]).unwrap();
    }
    // Before anything installs it, so every copy is of this one.
    write_files(
        &own_directory_for(&root, Kind::Skill).join("review"),
        &[
            ("SKILL.md", REVIEW),
            ("scripts/diff.sh", "git diff \"$1\"...HEAD\n"),
            (
                "references/checklist.md",
                "# Checklist\n\n- Does it say why?\n- Is it tested?\n",
            ),
        ],
    );

    project(&root, "api", &["--agents", "claude,codex"]);
    mind(
        &root.join("api"),
        &["add", "skill", "deploy", "Ship a release"],
    )
    .unwrap();
    install(&root, "api", &["review", "git/commits"]);

    project(&root, "web", &["--agents", "claude,opencode", "--shared"]);
    install(&root, "web", &["review", "lint"]);
    let lint = own_directory_for(&root, Kind::Skill).join("lint/SKILL.md");
    let text = fs::read_to_string(&lint).unwrap();
    fs::write(&lint, format!("{text}\nRun the formatter first.\n")).unwrap();
    let web = root.join("web");
    let review = web.join(".claude/skills/review/SKILL.md");
    let text = fs::read_to_string(&review).unwrap();
    fs::write(&review, format!("{text}\nAsk for a second pair of eyes.\n")).unwrap();
    write_files(
        &web,
        &[(
            ".claude/skills/legacy/SKILL.md",
            &skill_file("legacy", "Kept by hand from before"),
        )],
    );

    project(&root, "docs", &["--agents", "codex"]);
    let docs = root.join("docs");
    mind(&docs, &["targets", "set", "skill", ".skills", "--yes"]).unwrap();
    install(&root, "docs", &["review"]);
    write_files(
        &docs,
        &[("skills/notes/SKILL.md", "Notes, with no front matter\n")],
    );

    project(&root, "gone", &[]);
    fs::remove_dir_all(root.join("gone")).unwrap();

    let team = home.join("team");
    fs::create_dir_all(&team).unwrap();
    flayer(&team, &["init"]).unwrap();
    flayer(&root, &["load", "../team"]).unwrap();
    fs::remove_dir_all(&team).unwrap();

    shelve(&root);
    pin_times(&root);
    root
}

/// `name`, a folder of `root`, made a repository and a project with
/// `options` given to `mind init`, and linked.
fn project(root: &Path, name: &str, options: &[&str]) {
    let project = root.join(name);
    fs::create_dir(&project).unwrap();
    gix::init(&project).expect("initialize a repository");
    let mut init = vec!["init"];
    init.extend_from_slice(options);
    mind(&project, &init).unwrap();
    flayer(root, &["link", name]).unwrap();
}

/// Each of `names`, the workspace's own, into the project `entry`.
fn install(root: &Path, entry: &str, names: &[&str]) {
    for name in names {
        let installed = flayer(root, &["install", "-p", entry, name]).unwrap();
        assert!(installed.ok, "{name} into {entry}: {installed:?}");
    }
}

/// What there is to find beside the projects of the workspace at `root`,
/// left as somebody would leave it, for a scan to find and the panel to
/// link:
///
/// - `blog` and `drafts`, projects nobody linked;
/// - `notes`, a repository holding an `AGENTS.md`, and `scratch`, one
///   holding nothing an agent reads, neither a project yet;
/// - `old`, linked with `review` installed, which records its id, for
///   [`moved`] to move;
/// - `legacy`, linked, which a `mind.toml` marks, as an older Mindflayer
///   made a project.
fn candidates(root: &Path) {
    for name in ["blog", "drafts"] {
        let project = root.join(name);
        fs::create_dir(&project).unwrap();
        gix::init(&project).expect("initialize a repository");
        mind(&project, &["init"]).unwrap();
    }
    for (name, files) in [
        ("notes", &[("AGENTS.md", "# Agents\n")][..]),
        ("scratch", &[][..]),
    ] {
        let repository = root.join(name);
        fs::create_dir(&repository).unwrap();
        gix::init(&repository).expect("initialize a repository");
        write_files(&repository, files);
    }
    project(root, "old", &[]);
    // Said with the warning that the load `team` has gone, as every install
    // here is, so held to what it did rather than to how it exited.
    let installed = flayer(root, &["install", "-p", "old", "review"]).unwrap();
    assert!(
        installed.stdout.starts_with("installed review into old"),
        "{installed:?}"
    );
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    older_project(&legacy);
    flayer(root, &["link", "legacy"]).unwrap();
}

/// `old`, which [`candidates`] linked, moved to `archive/old`, leaving its
/// entry stale: apart, since a stale entry is a warning beside whatever
/// reads every project, and the first tasks the panel runs are written
/// from before it.
fn moved(root: &Path) {
    fs::create_dir(root.join("archive")).unwrap();
    fs::rename(root.join("old"), root.join("archive/old")).unwrap();
}

/// `files`, each a route from `root` and what it holds.
fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (route, text) in files {
        let path = root.join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// Two skills on the shelf of the workspace at `root`, from [`SOURCE`],
/// recorded as a gather records them: a gather itself would clone, and the
/// URL of a repository made in a temporary folder is a temporary path.
fn shelve(root: &Path) {
    let workspace = FlayerWorkspace::open(root).unwrap();
    let ledger = workspace.ledger().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, SOURCE, None, "skills")
        .unwrap();
    ledger.saw_source(source.id, Some(REVISION)).unwrap();
    for (name, summary) in [
        ("format", "Format the code the way the team does"),
        ("triage", "Sort new issues by what they need"),
    ] {
        let shelf = format!(".mindflayer/skills/{}/{name}", source.directory);
        write_files(
            &root.join(&shelf),
            &[("SKILL.md", &skill_file(name, summary))],
        );
        ledger
            .record(
                &source,
                Kind::Skill,
                name,
                &shelf,
                Some(summary),
                Some(REVISION),
            )
            .unwrap();
    }
    ledger
        .log(
            Action::Gather,
            Some(SOURCE),
            Logged::Ok,
            Some("2 added, 0 updated, 0 unchanged, 0 failed"),
        )
        .unwrap();
}

/// Every time the ledger at `root` keeps, set from [`AT`] on: each action a
/// minute after the one before it, and the shelf at the first.
fn pin_times(root: &Path) {
    let workspace = FlayerWorkspace::open(root).unwrap();
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE artifacts SET gathered_at = {AT}; UPDATE actions SET at = {AT} + id * 60;"
        ))
        .unwrap();
}

/// What the panel is answered with when it asks the backend launched from
/// `directory` with `lookup` for the snapshot, as the server sends it.
fn answered(directory: &Path, lookup: &Lookup) -> String {
    let backend = PanelBackend::launch(directory, lookup).unwrap();
    let snapshot = backend
        .query(Query::Snapshot {})
        .unwrap_or_else(|refusal| panic!("{}: {}", refusal.code, refusal.message));
    serde_json::to_string_pretty(&snapshot).unwrap() + "\n"
}

/// `query`, as the panel sends it, with what `backend` answers.
fn asked(backend: &PanelBackend, query: Value) -> Value {
    let read: Query = serde_json::from_value(query.clone()).unwrap();
    let answer = backend
        .query(read)
        .unwrap_or_else(|refusal| panic!("{query}: {}: {}", refusal.code, refusal.message));
    json!({"query": query, "answer": answer})
}

/// `json` as it reads wherever it was made: `dir` as [`HOME`], each
/// project's id as [`IDS`] says, `/` between the parts of a path.
fn pinned(json: &str, dir: &Path, root: Option<&Path>) -> String {
    let escaped = serde_json::to_string(&dir.to_string_lossy()).unwrap();
    let mut json = json.replace(escaped.trim_matches('"'), HOME);
    if let Some(root) = root {
        // Only those this fixture built: the candidates are not in every
        // one.
        for (route, id) in IDS {
            let Ok(project) = MindProject::open(root.join(route)) else {
                continue;
            };
            let random = project.settings().unwrap().id.to_string();
            json = json.replace(&random, id);
        }
    }
    if cfg!(windows) {
        json.replace("\\\\", "/")
    } else {
        json
    }
}

/// Write `json` as the fixture `name`.
fn write(name: &str, json: &str) {
    let folder = Path::new(FIXTURES);
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join(format!("{name}.json")), json).unwrap();
}

/// The workspace [`workspace`] builds, as the panel is answered with it.
#[test]
fn write_the_snapshot_of_a_workspace_holding_something_of_everything() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("workspace", &json);
}

/// The workspace [`workspace`] builds, with [`candidates`] beside its
/// projects and `old` [`moved`], as the panel is answered with it: the one
/// [`write_what_scanning_the_workspace_is_answered_with`] scans, so the
/// page of what could be linked draws a snapshot and a scan of one
/// workspace, and a project's page draws `legacy`, which a `mind.toml`
/// marks, as core reads it rather than as a test would draw it.
#[test]
fn write_the_snapshot_of_a_workspace_with_more_to_find() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    moved(&root);
    // Again, for what `candidates` installed.
    pin_times(&root);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let snapshot: Value = serde_json::from_str(&json).unwrap();
    let legacy = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["entry"] == "legacy")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(legacy["migrates"], true, "{json}");
    write("candidates", &json);
}

/// What opening each of the library's own asks, as the panel's page of one
/// asks it, with what the backend answers: its files, then each file of
/// them, the one it is read from among them, every query of every one, in
/// the order the snapshot lists them.
#[test]
fn write_what_opening_each_of_the_librarys_own_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let snapshot = backend.query(Query::Snapshot {}).unwrap();

    let mut answers = Vec::new();
    for held in snapshot["library"]["artifacts"].as_array().unwrap() {
        let reference = format!(
            "{}:{}",
            held["kind"].as_str().unwrap(),
            held["name"].as_str().unwrap()
        );
        let holder = json!({"type": "workspace"});
        let files = asked(
            &backend,
            json!({"type": "files", "holder": holder, "reference": reference}),
        );
        let read: Vec<Value> = files["answer"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["folder"] != true)
            .map(|entry| {
                asked(
                    &backend,
                    json!({
                        "type": "contents",
                        "holder": holder,
                        "reference": reference,
                        "file": entry["path"],
                    }),
                )
            })
            .collect();
        answers.push(files);
        answers.extend(read);
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    assert!(json.contains("references/checklist.md"), "{json}");
    write("library", &json);
}

/// The default workspace in a home where nothing has made it: what the
/// panel shows before anything is linked, gathered or added.
#[test]
fn write_the_snapshot_of_a_home_where_nothing_is_made() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    fs::create_dir_all(&home).unwrap();
    let lookup = Lookup {
        home: Some(home.clone()),
        ..Lookup::default()
    };

    let json = answered(&home, &lookup);

    let json = pinned(&json, dir.path(), None);
    assert!(!home.join(FLAYER_DIR).exists(), "the read made a workspace");
    write("unmade", &json);
}

/// What the panel is answered with when its page of what could be linked
/// scans the workspace [`workspace`] builds, with [`candidates`] beside its
/// projects: the query as the page sends it, every folder under the
/// workspace to core's own depth, and the answer.
#[test]
fn write_what_scanning_the_workspace_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    moved(&root);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();

    let answers = vec![asked(
        &backend,
        json!({"type": "scan", "roots": [root.to_str().unwrap()], "depth": null}),
    )];

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    for (route, offers) in [("drafts", "link"), ("notes", "init_and_link")] {
        let found = answers[0]["answer"]["found"]
            .as_array()
            .unwrap()
            .iter()
            .find(|suggestion| suggestion["found"]["route"] == route)
            .unwrap_or_else(|| panic!("{route}: {json}"));
        assert_eq!(found["offers"], offers, "{json}");
    }
    write("scan", &json);
}

/// What the panel is answered with when it previews and runs each of a few
/// tasks in turn on the workspace [`workspace`] builds, with `blog` made a
/// project beside the others and not linked: each task as the panel sends
/// it, its preview, and what its run is answered with, its lines and what
/// they produced, as the server sends both.
///
/// A link, which asks nothing; an unlink, which the panel has somebody
/// confirm; a validate of `docs`, which fails and says why with codes; a
/// removal everywhere, which takes two things away; then what the page of
/// what could be linked and a project's page send, against [`candidates`]
/// beside the projects: `drafts` and `notes` marked, as the scan
/// [`write_what_scanning_the_workspace_is_answered_with`] writes offers
/// them, the second made a project first; `old` relinked to where the scan
/// found it moved; `web`'s toggles turned to name Codex too; and `legacy`
/// migrated, its dry run first.
#[test]
fn write_what_previewing_and_running_a_few_tasks_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    let blog = root.join("blog");
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let mut answers = Vec::new();
    let mut run = |sent: Value| {
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        answers.push(json!({"task": sent, "preview": preview, "ran": ran}));
    };

    for sent in [
        json!({"type": "link", "path": blog.to_str().unwrap()}),
        json!({"type": "unlink", "entry": "gone"}),
        json!({"type": "validate", "holder": {"type": "project", "entry": "docs"}}),
        json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "lint", "everywhere": true,
        }),
    ] {
        run(sent);
    }

    moved(&root);
    let scan = PanelBackend::launch(&root, &Lookup::default())
        .unwrap()
        .query(Query::Scan {
            roots: vec![root.to_str().unwrap().to_owned()],
            depth: None,
        })
        .unwrap();
    let found = scan["found"].as_array().unwrap();
    // In the order the scan lists them, as the page marks them.
    let marks: Vec<Value> = found
        .iter()
        .filter(|suggestion| ["drafts", "notes"].contains(&suggestion["found"]["route"].as_str().unwrap()))
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    let old = found
        .iter()
        .find(|suggestion| suggestion["found"]["route"] == "archive/old")
        .unwrap_or_else(|| panic!("{scan}"));
    for sent in [
        json!({"type": "marks", "marks": marks}),
        json!({"type": "relink", "old": old["moved_from"], "new": old["found"]["path"]}),
        json!({
            "type": "toggled",
            "holder": {"type": "project", "entry": "web"},
            "was": {"agents": ["claude", "opencode"], "share": "shared", "edit_tracked": false},
            "now": {"agents": ["claude", "codex", "opencode"], "share": "shared", "edit_tracked": false},
        }),
        json!({"type": "migrate", "project": "legacy", "yes": false}),
        json!({"type": "migrate", "project": "legacy", "yes": true}),
    ] {
        run(sent);
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    for (index, ok) in [true, true, false, true, true, true, true, true, true]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            answers[index]["ran"]["outcome"]["ok"], ok,
            "{index}: {json}"
        );
    }
    write("commands", &json);
}
