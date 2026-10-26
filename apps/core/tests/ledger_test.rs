//! Opening a ledger an older Mindflayer wrote: what it keeps, what it gains,
//! and the copy of it left behind.

use std::path::{Path, PathBuf};

use mindflayer_core::install::Offer;
use mindflayer_core::ledger::{backup_path, SCHEMA_VERSION};
use mindflayer_core::{FlayerWorkspace, Kind, Ledger, LEDGER_FILE};
use rusqlite::types::Value;
use rusqlite::Connection;
use tempfile::TempDir;

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
