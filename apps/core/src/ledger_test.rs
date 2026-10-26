//! Unit tests for `ledger.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_url_becomes_a_readable_directory_name() {
    assert_eq!(
        slug("https://github.com/acme/skills.git"),
        "github.com-acme-skills"
    );
    assert_eq!(
        slug("git@github.com:acme/skills.git"),
        "github.com-acme-skills"
    );
    assert_eq!(slug("https://user:token@host/x"), "host-x");
    assert_eq!(slug("///"), "source");
}

#[test]
fn registering_the_same_source_twice_returns_the_same_row() {
    let ledger = Ledger::in_memory().unwrap();
    let first = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let again = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    assert_eq!(first, again);
}

#[test]
fn two_urls_with_one_readable_name_get_separate_directories() {
    let ledger = Ledger::in_memory().unwrap();
    // Both reduce to `h-a`, and both hold files, so they cannot share one.
    let first = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let second = ledger
        .source_for(SourceKind::Git, "ssh://h/a", None, "skills")
        .unwrap();
    assert_eq!(first.directory, "h-a");
    assert_eq!(second.directory, "h-a-2");
}

#[test]
fn a_reference_is_part_of_what_makes_a_source() {
    let ledger = Ledger::in_memory().unwrap();
    let main = ledger
        .source_for(SourceKind::Git, "https://h/a", Some("main"), "skills")
        .unwrap();
    let next = ledger
        .source_for(SourceKind::Git, "https://h/a", Some("next"), "skills")
        .unwrap();
    assert_ne!(main.id, next.id);
}

#[test]
fn one_name_from_two_sources_is_two_rows() {
    let ledger = Ledger::in_memory().unwrap();
    let a = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let b = ledger
        .source_for(SourceKind::Git, "https://h/b", None, "skills")
        .unwrap();
    ledger
        .record(&a, Kind::Skill, "deploy", "p/a", Some("From a"), Some("1"))
        .unwrap();
    ledger
        .record(&b, Kind::Skill, "deploy", "p/b", Some("From b"), Some("2"))
        .unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert_eq!(gathered[0].source.url, "https://h/a");
    assert_eq!(gathered[1].source.url, "https://h/b");
}

#[test]
fn gathering_the_same_name_again_updates_rather_than_duplicates() {
    let ledger = Ledger::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    ledger
        .record(&source, Kind::Skill, "deploy", "p", Some("Old"), Some("1"))
        .unwrap();
    ledger
        .record(&source, Kind::Skill, "deploy", "p", Some("New"), Some("2"))
        .unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].summary.as_deref(), Some("New"));
    assert_eq!(gathered[0].revision.as_deref(), Some("2"));
}

#[test]
fn the_log_keeps_failures_too_and_reads_back_newest_first() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://h/a"),
            Outcome::Ok,
            Some("3 skills"),
        )
        .unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://h/b"),
            Outcome::Failed,
            Some("no such host"),
        )
        .unwrap();

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].target.as_deref(), Some("https://h/b"));
    assert_eq!(history[0].outcome, "failed");
    assert_eq!(history[1].outcome, "ok");
}

#[test]
fn a_database_from_the_future_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        let ledger = Ledger::open(&path).unwrap();
        ledger
            .connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    let error = Ledger::open(&path).unwrap_err();
    assert!(matches!(error, LedgerError::Version { .. }), "{error}");
}

/// Every text column `table` holds, as stored, for asserting on what reached
/// the file rather than on what the ledger hands back.
fn stored(ledger: &Ledger, table: &str) -> String {
    let mut statement = ledger
        .connection
        .prepare(&format!("SELECT * FROM {table}"))
        .unwrap();
    let columns = statement.column_count();
    let rows = statement
        .query_map([], |row| {
            let mut values = Vec::new();
            for column in 0..columns {
                values.push(match row.get_ref(column)? {
                    rusqlite::types::ValueRef::Text(text) => {
                        String::from_utf8_lossy(text).into_owned()
                    }
                    rusqlite::types::ValueRef::Integer(number) => number.to_string(),
                    _ => String::new(),
                });
            }
            Ok(values.join("|"))
        })
        .unwrap();
    rows.map(Result::unwrap).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_source_is_stored_and_found_without_the_credentials_in_its_url() {
    let ledger = Ledger::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://me:hunter2@h/a", None, "skills")
        .unwrap();

    assert_eq!(source.url, "https://h/a");
    assert!(!stored(&ledger, "sources").contains("hunter2"));
    // The same place, asked for without them, is the same source.
    let again = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    assert_eq!(again, source);
}

#[test]
fn the_log_keeps_no_credential_in_what_it_was_about_or_what_it_said() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://me:hunter2@h/a"),
            Outcome::Failed,
            Some("https://me:hunter2@h/a: cannot be fetched"),
        )
        .unwrap();

    assert!(!stored(&ledger, "actions").contains("hunter2"));
    let history = ledger.history(1).unwrap();
    assert_eq!(history[0].target.as_deref(), Some("https://h/a"));
    assert_eq!(
        history[0].detail.as_deref(),
        Some("https://h/a: cannot be fetched")
    );
}

#[test]
fn a_row_an_older_build_wrote_with_credentials_is_read_back_without_them() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (1, 'git', 'https://me:hunter2@h/a', NULL, 'skills', 'h-a', 0, 0);
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'deploy', 1, '.mindflayer/skills/h-a/deploy', 0);
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (0, 'gather', 'https://me:hunter2@h/a', 'ok', 'from https://me:hunter2@h/a');",
        )
        .unwrap();

    assert_eq!(ledger.gathered().unwrap()[0].source.url, "https://h/a");
    let history = ledger.history(1).unwrap();
    assert_eq!(history[0].target.as_deref(), Some("https://h/a"));
    assert_eq!(history[0].detail.as_deref(), Some("from https://h/a"));
}

/// A ledger on disk, as a Mindflayer from before the redaction left it: one
/// source gathered with a token in its URL, and the log of that gather.
fn ledger_with_a_token_in_it() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::TempDir::new().unwrap();
    let ledger = Ledger::open(dir.path().join(LEDGER_FILE)).unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (1, 'git', 'https://me:hunter2@h/a', NULL, 'skills', 'h-a', 0, 0);
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'deploy', 1, '.mindflayer/skills/h-a/deploy', 0);
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (0, 'gather', 'https://me:hunter2@h/a', 'ok', '1 added');",
        )
        .unwrap();
    (dir, ledger)
}

#[test]
fn only_a_url_with_credentials_is_tainted_and_it_is_handed_back_without_them() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES ('git', 'ssh://git@h/b', NULL, 'skills', 'h-b', 0, 0),
                        ('git', 'https://h/c', NULL, 'skills', 'h-c', 0, 0);",
        )
        .unwrap();

    let tainted = ledger.tainted_sources().unwrap();

    assert_eq!(tainted.len(), 1, "{tainted:?}");
    assert_eq!(tainted[0].id, 1);
    assert_eq!(tainted[0].url, "https://h/a");
}

#[test]
fn a_scrubbed_source_leaves_no_copy_of_its_credentials_in_the_file() {
    let (dir, ledger) = ledger_with_a_token_in_it();

    ledger.scrub_source(1).unwrap();

    assert!(ledger.tainted_sources().unwrap().is_empty());
    assert_eq!(ledger.gathered().unwrap()[0].source.url, "https://h/a");
    // Found by its URL now, which is what the next gather looks it up by.
    assert!(ledger
        .source(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap()
        .is_some());
    ledger.scrub_log().unwrap();
    let file = std::fs::read(dir.path().join(LEDGER_FILE)).unwrap();
    assert!(!file.windows(7).any(|bytes| bytes == b"hunter2"));
    // Again is nothing.
    ledger.scrub_source(1).unwrap();
    assert_eq!(ledger.gathered().unwrap().len(), 1);
}

#[test]
fn a_source_scrubbed_onto_one_already_at_its_url_becomes_one_row() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    // Gathered first without the token, which failed and left an empty row,
    // then with it, which worked: the kept row is the later one.
    ledger
        .connection
        .execute_batch(
            "UPDATE sources SET last_seen = 20 WHERE id = 1;
             INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (2, 'git', 'https://h/a', NULL, 'skills', 'h-a-2', 10, 10);
             INSERT INTO installations (project, kind, name, source_id, path, installed_at)
                 VALUES ('../p', 'skill', 'deploy', 1, 'skills/deploy', 0);",
        )
        .unwrap();

    ledger.scrub_source(1).unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    assert_eq!(gathered[0].source.id, 1);
    assert_eq!(gathered[0].source.url, "https://h/a");
    assert_eq!(gathered[0].source.directory, "h-a");
    assert_eq!(stored(&ledger, "sources").lines().count(), 1);
}

#[test]
fn merging_keeps_the_newer_rows_artifacts_and_moves_what_only_the_other_had() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    // A branch is part of the coordinate, so this is the case SQLite's UNIQUE
    // would refuse if the URL were rewritten before the other row went.
    ledger
        .connection
        .execute_batch(
            "UPDATE sources SET reference = 'main' WHERE id = 1;
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'only-here', 1, '.mindflayer/skills/h-a/only-here', 0);
             INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (2, 'git', 'https://h/a', 'main', 'skills', 'h-a-2', 10, 10);
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'deploy', 2, '.mindflayer/skills/h-a-2/deploy', 10);
             INSERT INTO installations (project, kind, name, source_id, path, installed_at)
                 VALUES ('../p', 'skill', 'only-here', 1, 'skills/only-here', 0);",
        )
        .unwrap();

    ledger.scrub_source(1).unwrap();

    let gathered = ledger.gathered().unwrap();
    let names: Vec<(&str, i64, &str)> = gathered
        .iter()
        .map(|g| (g.name.as_str(), g.source.id, g.path.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("deploy", 2, ".mindflayer/skills/h-a-2/deploy"),
            ("only-here", 2, ".mindflayer/skills/h-a/only-here"),
        ]
    );
    assert!(stored(&ledger, "installations").contains("|only-here|2|"));
}

#[test]
fn scrubbing_the_log_rewrites_only_the_entries_with_credentials_and_counts_them() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    ledger
        .log(Action::Gather, Some("ssh://git@h/b"), Outcome::Ok, None)
        .unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (1, 'gather', 'https://h/c', 'failed', 'https://me:hunter2@h/c: refused');",
        )
        .unwrap();

    assert_eq!(ledger.scrub_log().unwrap(), 2);

    assert!(!stored(&ledger, "actions").contains("hunter2"));
    assert!(stored(&ledger, "actions").contains("ssh://git@h/b"));
    assert_eq!(ledger.scrub_log().unwrap(), 0);
}
