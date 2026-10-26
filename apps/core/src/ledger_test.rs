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
fn a_database_from_the_future_is_refused_by_its_version() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    let future = SCHEMA_VERSION + 1;
    {
        let ledger = Ledger::open(&path).unwrap();
        ledger
            .connection
            .pragma_update(None, "user_version", future)
            .unwrap();
    }

    for error in [
        Ledger::open(&path).unwrap_err(),
        Ledger::open_existing(&path).unwrap_err(),
    ] {
        assert!(
            matches!(error, LedgerError::Version { found, .. } if found == future),
            "{error}"
        );
        assert!(
            error.to_string().contains(&format!(
                "schema version {future}, this one reads {SCHEMA_VERSION}"
            )),
            "{error}"
        );
    }
    // Left exactly as it is, and nothing copied for a step this build
    // cannot take.
    assert_eq!(version_of(&path), future);
    assert!(!backup_path(&path, future).exists());
}

#[test]
fn opening_only_an_existing_ledger_makes_nothing_when_there_is_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let folder = dir.path().join(".mindflayer");
    let path = folder.join(LEDGER_FILE);

    assert!(Ledger::open_existing(&path).unwrap().is_none());

    // Not the database, and not the folder it would have gone in either.
    assert!(!folder.exists());
}

#[test]
fn opening_only_an_existing_ledger_still_brings_an_older_one_up_to_date() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(SCHEMA_V1).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
    }

    let ledger = Ledger::open_existing(&path).unwrap().unwrap();

    let version: u32 = ledger
        .connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn a_step_that_fails_leaves_the_ledger_as_it_was() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        // A version 1 ledger that already has an index by the name the second
        // step gives its own, so that step fails after creating its table.
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(SCHEMA_V1).unwrap();
        connection
            .execute_batch("CREATE INDEX installations_by_project ON sources (url);")
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
    }

    let error = Ledger::open(&path).unwrap_err();

    assert!(
        error.to_string().contains("adding the installations table"),
        "{error}"
    );
    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'installations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0, "the table the failed step made is still there");
}

/// A ledger as version 1 left it, with one source in it.
fn version_1_at(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(SCHEMA_V1).unwrap();
    connection
        .execute(
            "INSERT INTO sources
                 (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen)
             VALUES ('git', 'https://h/a', NULL, 'skills', NULL, 'h-a', 1, 1)",
            [],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
}

fn version_of(path: &Path) -> u32 {
    Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn an_older_ledger_is_copied_aside_before_it_is_migrated() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindflayer.db.v1.bak");
    assert_eq!(backup_path(&path, 1), backup);
    // Opened raw: opening it as a ledger would migrate the copy too.
    assert_eq!(version_of(&backup), 1);
    let copy = Connection::open(&backup).unwrap();
    let url: String = copy
        .query_row("SELECT url FROM sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(url, "https://h/a");
    let installations: i64 = copy
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'installations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(installations, 0, "the copy was taken after the step");
    assert_eq!(version_of(&path), SCHEMA_VERSION);
}

#[test]
fn a_ledger_made_now_or_already_current_is_not_copied() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);

    Ledger::open(&path).unwrap();
    Ledger::open(&path).unwrap();

    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [LEDGER_FILE], "{names:?}");
}

#[test]
fn what_an_earlier_attempt_left_beside_the_ledger_is_replaced() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    let backup = backup_path(&path, 1);
    std::fs::write(&backup, "an older copy").unwrap();
    std::fs::write(dir.path().join("mindflayer.db.v1.bak.tmp"), "half a copy").unwrap();
    std::fs::write(dir.path().join("mindflayer.db.v1.bak.tmp-journal"), "junk").unwrap();

    Ledger::open(&path).unwrap();

    assert_eq!(version_of(&backup), 1);
    assert!(!dir.path().join("mindflayer.db.v1.bak.tmp").exists());
    assert!(!dir.path().join("mindflayer.db.v1.bak.tmp-journal").exists());
}

#[test]
fn a_ledger_that_cannot_be_copied_is_not_migrated() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    // A folder holding something, where the copy would go, which no rename
    // replaces.
    let backup = backup_path(&path, 1);
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("keep"), "mine").unwrap();

    let error = Ledger::open(&path).unwrap_err();

    assert!(matches!(error, LedgerError::Backup { .. }), "{error}");
    assert!(
        error.to_string().contains("so it is left as it is"),
        "{error}"
    );
    assert_eq!(version_of(&path), 1);
    assert!(backup.join("keep").is_file());
}

#[cfg(unix)]
#[test]
fn the_copy_is_no_more_readable_than_the_ledger() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    Ledger::open(&path).unwrap();

    let mode = std::fs::metadata(backup_path(&path, 1))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
