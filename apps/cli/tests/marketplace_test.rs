//! `mindweaver gather marketplace`: a Claude Code plugin marketplace's
//! catalog listed and recorded, and the plugins picked gathered, each into
//! a source of its own.
//!
//! Every marketplace here is a repository on the disk, built in-process,
//! and every plugin gathered is in it: a catalog naming another repository
//! names it by a remote URL, which these tests list and never fetch, so
//! nothing reaches the network. Core's suite gathers those, through a remote
//! of its own.

mod common;

use common::*;
use mindweaver_core::Kind;

/// A marketplace as anthropics/skills keeps one, beside a plugin in a
/// repository of its own and two entries no gather makes.
fn marketplace() -> TempDir {
    repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{
  "name": "anthropic-agent-skills",
  "plugins": [
    {"name": "document-skills", "source": "./", "skills": ["./skills/xlsx", "./skills/docx"]},
    {"name": "example-skills", "source": "./", "skills": ["./skills/art"]},
    {"name": "tools", "source": {"source": "github", "repo": "acme/tools", "ref": "v2", "sha": "0123456789abcdef0123456789abcdef01234567"}},
    {"name": "pkg", "source": {"source": "npm", "package": "@acme/pkg"}},
    {"name": "run-me", "source": {"source": "command", "command": "rm -rf ~"}}
  ]
}"#,
        ),
        ("skills/xlsx/SKILL.md", &skill_file("xlsx", "Spreadsheets")),
        ("skills/docx/SKILL.md", &skill_file("docx", "Documents")),
        ("skills/art/SKILL.md", &skill_file("art", "Art")),
        (".mcp.json", "{}"),
    ])
}

/// A workspace in a folder of its own.
fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    dir
}

/// The commit `repository`'s HEAD is at, as a report shortens it.
fn short_head(repository: &TempDir) -> String {
    gix::open(repository.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string()[..7]
        .to_owned()
}

/// The warnings an entry no gather makes gives, as `gather marketplace`
/// says each.
const ENTRY_WARNINGS: [&str; 2] = [
    "warning: anthropic-agent-skills: `pkg`: the npm package `@acme/pkg`, which is not gathered",
    "warning: anthropic-agent-skills: `run-me`: its plugin is made by running a command, which \
     is never run, so it is refused",
];

#[test]
fn listing_a_marketplace_says_each_plugin_and_where_it_is_and_gathers_none() {
    let market = marketplace();
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();

    let listed = weaver(dir.path(), &["gather", "marketplace", &url, "--list"]).unwrap();

    assert_eq!(
        listed.stdout,
        format!(
            "anthropic-agent-skills: {url} at {}\n\
             \x20 document-skills  relative  .\n\
             \x20 example-skills   relative  .\n\
             \x20 tools            github    acme/tools  ref v2 pinned 0123456\n\
             \n\
             5 plugins listed; nothing gathered: gather some with --plugin <name>, or every one \
             with --all\n",
            short_head(&market)
        )
    );
    assert_eq!(listed.stderr, ENTRY_WARNINGS);
    assert_eq!(
        stderr_codes(&listed),
        [
            Some(String::from("marketplace_npm_not_gathered")),
            Some(String::from("marketplace_command_refused")),
        ]
    );
    assert!(
        !listed.ok,
        "an entry no gather makes fails it, as a skill does"
    );
    let sources = weaver(dir.path(), &["gather", "sources"]).unwrap();
    assert!(
        sources.stdout.starts_with("anthropic-agent-skills  "),
        "{}",
        sources.stdout
    );
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        shelf.stdout.starts_with("nothing gathered yet"),
        "{}",
        shelf.stdout
    );
}

#[test]
fn a_plugin_picked_is_gathered_into_a_source_called_after_its_marketplace() {
    let market = marketplace();
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();
    let head = short_head(&market);

    let gathered = weaver(
        dir.path(),
        &["gather", "marketplace", &url, "--plugin", "document-skills"],
    )
    .unwrap();

    assert_eq!(
        gathered.stdout,
        format!(
            "anthropic-agent-skills: {url} at {head}\n\
             \n\
             anthropic-agent-skills.document-skills: {url} at {head}\n\
             \x20 added  docx  Documents\n\
             \x20 added  xlsx  Spreadsheets\n\
             \n\
             2 skills: 2 added, 0 updated, 0 unchanged\n"
        )
    );
    let mut said = ENTRY_WARNINGS.map(String::from).to_vec();
    said.push(String::from(
        "note: anthropic-agent-skills.document-skills: its plugin holds MCP servers too, which \
         are not gathered",
    ));
    assert_eq!(gathered.stderr, said);
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(
        shelf.stdout,
        "docx  anthropic-agent-skills.document-skills  unknown  Documents\n\
         xlsx  anthropic-agent-skills.document-skills  unknown  Spreadsheets\n"
    );
}

#[test]
fn a_plugin_installs_from_the_source_its_marketplace_gathered_it_into() {
    let market = repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{"name": "m", "plugins": [{"name": "kit", "source": "./kit", "license": "MIT"}]}"#,
        ),
        (
            "kit/skills/deploy/SKILL.md",
            &skill_file("deploy", "Ship it"),
        ),
    ]);
    let dir = workspace_with_two();
    let url = market.path().to_string_lossy().into_owned();
    weaver(dir.path(), &["gather", "marketplace", &url, "--all"]).unwrap();

    let installed = weaver(
        dir.path(),
        &["install", "-p", "alpha", "deploy", "--from", "m.kit"],
    )
    .unwrap();

    assert!(installed.ok, "{:?}", installed.stderr);
    assert!(
        installed.stdout.starts_with("installed deploy into alpha"),
        "{}",
        installed.stdout
    );
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(shelf.stdout, "deploy  m.kit  MIT  Ship it\n");
}

#[test]
fn a_plugin_the_catalog_does_not_list_is_refused_and_none_gathered() {
    let market = marketplace();
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();

    let failure = weaver(
        dir.path(),
        &["gather", "marketplace", &url, "--plugin", "nothing-like-it"],
    )
    .unwrap_err();

    assert_eq!(failure.error.code(), "marketplace_plugin_not_listed");
    assert_eq!(
        failure.error.to_string(),
        "the marketplace anthropic-agent-skills lists no plugin called `nothing-like-it`: \
         `--list` lists them"
    );
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        shelf.stdout.starts_with("nothing gathered yet"),
        "{}",
        shelf.stdout
    );
}

/// A marketplace listing `count` plugins, each in a folder of its own
/// holding one skill.
fn listing(count: usize) -> TempDir {
    let entries: Vec<String> = (0..count)
        .map(|n| format!(r#"{{"name": "p{n:02}", "source": "./p{n:02}"}}"#))
        .collect();
    let catalog = format!(r#"{{"name": "big", "plugins": [{}]}}"#, entries.join(","));
    let mut files: Vec<(String, String)> = (0..count)
        .map(|n| {
            (
                format!("p{n:02}/SKILL.md"),
                skill_file(&format!("p{n:02}"), "One of many"),
            )
        })
        .collect();
    files.push((String::from(".claude-plugin/marketplace.json"), catalog));
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    repository(&borrowed)
}

#[test]
fn every_plugin_of_a_catalog_listing_more_than_twenty_five_waits_for_a_yes_and_nothing_is_written()
{
    let market = listing(30);
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();
    let before = names_in(&dir.path().join(".mindweaver"));

    let looked = weaver(dir.path(), &["gather", "marketplace", &url, "--all"]).unwrap();

    assert!(looked.ok, "{:?}", looked.stderr);
    assert!(
        looked
            .stdout
            .ends_with("\n30 plugins to gather; nothing gathered: add --yes to gather every one\n"),
        "{}",
        looked.stdout
    );
    assert!(
        looked.stdout.contains("  p29  relative  p29\n"),
        "{}",
        looked.stdout
    );
    // No source, no clone, and no ledger made to say there is none.
    assert_eq!(names_in(&dir.path().join(".mindweaver")), before);

    let gathered = weaver(
        dir.path(),
        &["gather", "marketplace", &url, "--all", "--yes"],
    )
    .unwrap();

    assert!(gathered.ok, "{:?}", gathered.stderr);
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(shelf.stdout.lines().count(), 30, "{}", shelf.stdout);
}

#[test]
fn every_plugin_of_a_catalog_listing_twenty_five_is_gathered_without_a_yes() {
    let market = listing(25);
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();

    let gathered = weaver(dir.path(), &["gather", "marketplace", &url, "--all"]).unwrap();

    assert!(gathered.ok, "{:?}", gathered.stderr);
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(shelf.stdout.lines().count(), 25, "{}", shelf.stdout);
}

#[test]
fn a_marketplace_is_picked_from_by_one_of_list_plugin_and_all_and_yes_goes_with_all() {
    let dir = workspace();

    for args in [
        vec!["gather", "marketplace", "acme/market"],
        vec!["gather", "marketplace", "acme/market", "--list", "--all"],
        vec![
            "gather",
            "marketplace",
            "acme/market",
            "--list",
            "--plugin",
            "x",
        ],
        vec![
            "gather",
            "marketplace",
            "acme/market",
            "--plugin",
            "x",
            "--yes",
        ],
    ] {
        let mut line = vec!["mindweaver"];
        line.extend(&args);
        assert!(
            mindweaver_cli::WeaverCli::try_parse_from(&line).is_err(),
            "{args:?}"
        );
    }
    assert!(weaver(dir.path(), &["gather", "list"]).unwrap().ok);
}

#[test]
fn a_repository_holding_no_catalog_is_said_to_be_no_marketplace() {
    let plain = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = workspace();
    let url = plain.path().to_string_lossy().into_owned();

    let failure = weaver(dir.path(), &["gather", "marketplace", &url, "--list"]).unwrap_err();

    assert_eq!(failure.error.code(), "marketplace_catalog_missing");
    assert_eq!(
        failure.error.to_string(),
        ".claude-plugin/marketplace.json: no such file, so this is no Claude Code marketplace"
    );
    let sources = weaver(dir.path(), &["gather", "sources"]).unwrap();
    assert!(
        sources.stdout.starts_with("no sources yet"),
        "{}",
        sources.stdout
    );
}

#[test]
fn a_marketplace_named_on_the_command_line_is_called_that() {
    let market = marketplace();
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();

    let listed = weaver(
        dir.path(),
        &[
            "gather",
            "marketplace",
            &url,
            "--name",
            "anthropic",
            "--list",
        ],
    )
    .unwrap();

    assert!(
        listed.stdout.starts_with("anthropic: "),
        "{}",
        listed.stdout
    );
}

#[test]
fn what_a_catalog_names_is_said_in_printable_text() {
    let market = repository(&[(
        ".claude-plugin/marketplace.json",
        "{\"name\": \"m\", \"plugins\": [\
          {\"name\": \"a\\rb\", \"source\": \"./missing\"},\
          {\"name\": \"c\\u001b[2Jd\", \"source\": {\"source\": \"npm\", \"package\": \"x\\ny\"}}\
        ]}",
    )]);
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();

    let listed = weaver(dir.path(), &["gather", "marketplace", &url, "--list"]).unwrap();

    for line in &listed.stderr {
        assert!(!line.text.chars().any(char::is_control), "{line:?}");
    }
    assert_eq!(listed.stderr.len(), 2, "{:?}", listed.stderr);
}

// ---------------------------------------------------------------------------
// The marketplace's own commands
// ---------------------------------------------------------------------------

/// A marketplace of two plugins in its own repository, `a` and `b`, both
/// gathered into a workspace, and its URL.
fn both_gathered() -> (TempDir, TempDir, String) {
    let market = repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{"name": "m", "plugins": [{"name": "a", "source": "./a"}, {"name": "b", "source": "./b"}]}"#,
        ),
        ("a/skills/one/SKILL.md", &skill_file("one", "First")),
        ("b/skills/two/SKILL.md", &skill_file("two", "Second")),
    ]);
    let dir = workspace();
    let url = market.path().to_string_lossy().into_owned();
    let gathered = weaver(dir.path(), &["gather", "marketplace", &url, "--all"]).unwrap();
    assert!(gathered.ok, "{:?}", gathered.stderr);
    (dir, market, url)
}

/// What `gather sources --json` prints of one source, as far as a
/// marketplace's tests ask.
#[derive(Debug, serde::Deserialize)]
struct Listed {
    kind: String,
    name: String,
    parent: Option<String>,
    entry: Option<String>,
}

#[test]
fn gather_sources_lists_each_plugin_of_a_marketplace_under_it() {
    let (dir, _market, url) = both_gathered();

    let text = weaver(dir.path(), &["gather", "sources"]).unwrap();
    let json = weaver(dir.path(), &["gather", "sources", "--json"]).unwrap();

    let rows: Vec<Vec<&str>> = text
        .stdout
        .lines()
        .map(|line| line.split_whitespace().take(4).collect())
        .collect();
    assert_eq!(
        rows,
        [
            ["m", url.as_str(), "2", "plugins"],
            ["m.a", url.as_str(), "1", "skill"],
            ["m.b", url.as_str(), "1", "skill"],
        ],
        "{}",
        text.stdout
    );
    assert!(
        text.stdout.lines().nth(1).unwrap().starts_with("  m.a"),
        "{}",
        text.stdout
    );
    let (listed, _): (Vec<Listed>, _) = enveloped(&json);
    let said: Vec<(&str, &str, Option<&str>, Option<&str>)> = listed
        .iter()
        .map(|it| {
            (
                it.kind.as_str(),
                it.name.as_str(),
                it.parent.as_deref(),
                it.entry.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("marketplace", "m", None, None),
            ("git", "m.a", Some("m"), Some("a")),
            ("git", "m.b", Some("m"), Some("b")),
        ]
    );
}

#[test]
fn gather_update_of_a_marketplace_gathers_each_plugin_of_it_again() {
    let (dir, market, url) = both_gathered();
    let head = short_head(&market);

    let updated = weaver(dir.path(), &["gather", "update", "m"]).unwrap();

    assert!(updated.ok, "{:?}", updated.stderr);
    assert_eq!(
        updated.stdout,
        format!(
            "m: {url} at {head}\n\
             \n\
             m.a: {url} at {head}\n\
             \x20 unchanged  one  First\n\
             \n\
             1 skill: 0 added, 0 updated, 1 unchanged\n\
             \n\
             m.b: {url} at {head}\n\
             \x20 unchanged  two  Second\n\
             \n\
             1 skill: 0 added, 0 updated, 1 unchanged\n"
        )
    );
}

#[test]
fn gather_remove_of_a_marketplace_says_each_plugin_of_it_and_takes_them_with_it() {
    let (dir, _market, url) = both_gathered();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();
    let folder = |name: &str| {
        let source = mindweaver_core::sources::named(&ledger, name).unwrap();
        (
            workspace.cache_dir().join(&source.directory),
            workspace.gathered_dir(Kind::Skill).join(&source.directory),
        )
    };
    let (clone, _) = folder("m");
    let ((_, a), (_, b)) = (folder("m.a"), folder("m.b"));

    let planned = weaver(dir.path(), &["gather", "remove", "m"]).unwrap();

    assert_eq!(
        planned.stdout,
        format!(
            "would remove the marketplace m, gathered from {url}, and the 2 plugins of it gathered\n\
             would remove its plugin m.a, and the 1 skill it holds on the shelf\n\
             would delete {}\n\
             would remove its plugin m.b, and the 1 skill it holds on the shelf\n\
             would delete {}\n\
             would delete {}\n\
             nothing removed: add --yes to go ahead\n",
            a.display(),
            b.display(),
            clone.display()
        )
    );
    assert!(clone.is_dir() && a.is_dir() && b.is_dir());

    let removed = weaver(dir.path(), &["gather", "remove", "m", "--yes"]).unwrap();

    assert!(removed.ok, "{:?}", removed.stderr);
    assert!(!clone.exists() && !a.exists() && !b.exists());
    let sources = weaver(dir.path(), &["gather", "sources"]).unwrap();
    assert!(
        sources.stdout.starts_with("no sources yet"),
        "{}",
        sources.stdout
    );
}
