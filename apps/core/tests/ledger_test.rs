//! The ledger: what an install records in it, and opening one an older
//! Mindflayer wrote — what it keeps, what it gains, and the copy of it left
//! behind.

mod common;

use std::path::{Path, PathBuf};

use common::{repository, skill, url};
use mindflayer_core::gather::{self, Request};
use mindflayer_core::install::{self, Offer};
use mindflayer_core::ledger::{backup_path, InstallRecord, Target, SCHEMA_VERSION};
use mindflayer_core::{FlayerWorkspace, Kind, Ledger, MindProject, LEDGER_FILE};
use rusqlite::types::Value;
use rusqlite::Connection;
use tempfile::TempDir;

/// Under one directory: `target`, a workspace linking `target/collapse`,
/// keeping `notes` as its own and with `deploy` on its shelf; and `team`, a
/// workspace `target` loads, keeping `lint` as its own.
fn three_places() -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut target, _) = FlayerWorkspace::init(dir.path().join("target")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/collapse")).unwrap();
    target.link(&project).unwrap();
    mindflayer_core::create(&target.own(), Kind::Skill, "notes", "How we take notes").unwrap();
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    mindflayer_core::create(&team.own(), Kind::Skill, "lint", "How we lint").unwrap();
    target.load(&team).unwrap();

    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let ledger = target.ledger().unwrap();
    gather::gather(&target, &ledger, &Request::git(url(&source))).unwrap();
    (dir, target, project, ledger)
}

/// Install the one thing called `name` that is offered to `project`.
fn install(workspace: &FlayerWorkspace, ledger: &Ledger, project: &MindProject, name: &str) {
    let candidate = install::offered(workspace, ledger, project, Kind::Skill, name, None).unwrap();
    install::install(workspace, ledger, project, &candidate).unwrap();
}

#[test]
fn an_install_records_one_copy_at_the_folder_the_project_names() {
    let (_dir, workspace, project, ledger) = three_places();

    install(&workspace, &ledger, &project, "deploy");

    let recorded = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(
        recorded.targets,
        [Target {
            agent: None,
            path: String::from("skills/deploy"),
        }]
    );
}

/// What schema 2 wrote, dumped from a ledger it made.
const VERSION_2: &str = include_str!("fixtures/ledger-v2.sql");

/// A ledger file in `dir` holding what `fixture` describes, untouched by
/// anything of this build's.
fn written_at(dir: &Path, fixture: &str) -> PathBuf {
    let path = dir.join(LEDGER_FILE);
    Connection::open(&path)
        .unwrap()
        .execute_batch(fixture)
        .unwrap();
    path
}

/// The same, in memory: what the rows were, to compare with what they are.
fn as_written(fixture: &str) -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(fixture).unwrap();
    connection
}

fn rows(connection: &Connection, query: &str) -> Vec<Vec<Value>> {
    let mut statement = connection.prepare(query).unwrap();
    let width = statement.column_count();
    statement
        .query_map([], |row| {
            (0..width)
                .map(|column| row.get::<_, Value>(column))
                .collect()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn version_of(connection: &Connection) -> u32 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

#[test]
fn a_version_2_ledger_keeps_every_row_it_had() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT id, project, kind, name, source_id, installed_at
           FROM installations ORDER BY id",
    ] {
        let kept = rows(&after, query);
        assert_eq!(kept, rows(&before, query), "{query}");
        assert!(!kept.is_empty(), "{query} reads nothing to compare");
    }
    assert_eq!(ledger.history(100).unwrap().len(), 11);
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["commit-style", "deploy", "lint", "notes"]
    );
    assert_eq!(
        ledger.installations("tanukeys", Kind::Skill).unwrap(),
        ["notes", "review"]
    );
}

#[test]
fn a_version_2_installation_becomes_one_target_at_the_path_it_had() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(
        rows(
            &after,
            "SELECT installation_id, agent, path FROM installation_targets
              ORDER BY installation_id"
        ),
        rows(
            &before,
            "SELECT id, NULL, path FROM installations ORDER BY id"
        )
    );
}

#[test]
fn a_version_2_installation_from_a_source_gets_the_origin_its_offer_has() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    let skills = text("https://github.com/acme/skills#next");
    let tools = text("https://github.com/acme/tools");
    assert_eq!(
        rows(
            &after,
            "SELECT name, origin, content_hash FROM installations ORDER BY id"
        ),
        [
            [text("deploy"), skills.clone(), Value::Null],
            [text("commit-style"), tools, Value::Null],
            // From the workspace and from a load: version 2 kept nothing
            // that says which.
            [text("notes"), Value::Null, Value::Null],
            [text("lint"), Value::Null, Value::Null],
            [text("review"), skills, Value::Null],
            [text("notes"), Value::Null, Value::Null],
        ]
    );
    // The spelling an install from the shelf records now, so a record the
    // step filled in and one written since compare alike.
    for gathered in ledger.gathered().unwrap() {
        let origin = Offer::Shelf(gathered.clone()).origin();
        let recorded: Vec<String> = after
            .prepare("SELECT DISTINCT origin FROM installations WHERE source_id = ?1")
            .unwrap()
            .query_map([gathered.source.id], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            recorded.iter().all(|recorded| *recorded == origin),
            "{recorded:?} is not {origin}"
        );
    }
}

#[test]
fn a_version_2_installation_reads_back_through_the_ledger() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            kind: Kind::Skill,
            name: String::from("deploy"),
            source_id: Some(1),
            origin: Some(String::from("https://github.com/acme/skills#next")),
            content_hash: None,
            targets: vec![Target {
                agent: None,
                path: String::from("skills/deploy"),
            }],
        })
    );
}

#[test]
fn a_version_2_ledger_is_copied_aside_and_the_copy_opens_at_version_2() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindflayer.db.v2.bak");
    assert_eq!(backup_path(&path, 2), backup);
    // Opened raw: opening it as a ledger would migrate the copy as well.
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version_of(&copy), 2);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

#[test]
fn reading_a_version_2_ledger_migrates_it_through_the_same_copy() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    written_at(&dir.path().join(".mindflayer"), VERSION_2);

    let ledger = workspace.ledger_for_reading().unwrap();

    assert_eq!(ledger.path(), workspace.ledger_path());
    assert_eq!(ledger.history(100).unwrap().len(), 11);
    let copy = Connection::open(backup_path(&workspace.ledger_path(), 2)).unwrap();
    assert_eq!(version_of(&copy), 2);
}
