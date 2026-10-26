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
            Ok((0..columns)
                .map(|column| row.get::<_, Option<String>>(column).ok().flatten())
                .map(Option::unwrap_or_default)
                .collect::<Vec<_>>()
                .join("|"))
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
