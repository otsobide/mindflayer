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
fn a_forgotten_source_frees_its_directory_and_takes_its_artifacts() {
    let ledger = Ledger::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    ledger
        .record(
            &source,
            Kind::Skill,
            "deploy",
            "skills/h-a/deploy",
            None,
            None,
        )
        .unwrap();

    assert!(ledger.forget_source(source.id).unwrap());
    assert!(!ledger.forget_source(source.id).unwrap(), "already gone");

    assert!(ledger.gathered().unwrap().is_empty());
    assert_eq!(
        ledger
            .source(SourceKind::Git, "https://h/a", None, "skills")
            .unwrap(),
        None
    );
    // The name it had is free for the next source to reduce to it.
    let next = ledger
        .source_for(SourceKind::Git, "ssh://h/a", None, "skills")
        .unwrap();
    assert_eq!(next.directory, "h-a");
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
             INSERT INTO installations (project, kind, name, source_id, installed_at)
                 VALUES ('../p', 'skill', 'deploy', 1, 0);",
        )
        .unwrap();

    let gone = ledger.scrub_source(1).unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    assert_eq!(gathered[0].source.id, 1);
    assert_eq!(gathered[0].source.url, "https://h/a");
    assert_eq!(gathered[0].source.directory, "h-a");
    assert_eq!(stored(&ledger, "sources").lines().count(), 1);
    // Handed back so its folder can go too.
    assert_eq!(
        gone.map(|source| source.directory).as_deref(),
        Some("h-a-2")
    );
}

#[test]
fn a_row_tried_later_that_filled_no_shelf_loses_to_the_one_that_did() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    // Gathered with the token, which worked, then tried without it, which
    // failed before it took anything: the later row is the empty one.
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (2, 'git', 'https://h/a', NULL, 'skills', 'h-a-2', 30, 30);",
        )
        .unwrap();

    let gone = ledger.scrub_source(1).unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    assert_eq!(gathered[0].name, "deploy");
    assert_eq!(gathered[0].source.id, 1);
    assert_eq!(gathered[0].source.url, "https://h/a");
    assert_eq!(gone.map(|source| source.id), Some(2));
}

#[test]
fn merging_keeps_the_fresher_rows_artifacts_and_lets_the_others_go_with_its_row() {
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
             INSERT INTO installations (project, kind, name, source_id, installed_at)
                 VALUES ('../p', 'skill', 'only-here', 1, 0);",
        )
        .unwrap();

    let gone = ledger.scrub_source(1).unwrap();

    // Nothing points into the folder of the row that went, so the next
    // source given that folder cannot gather over files a record claims.
    let gathered = ledger.gathered().unwrap();
    let names: Vec<(&str, i64, &str)> = gathered
        .iter()
        .map(|g| (g.name.as_str(), g.source.id, g.path.as_str()))
        .collect();
    assert_eq!(names, [("deploy", 2, ".mindflayer/skills/h-a-2/deploy")]);
    assert_eq!(gone.map(|source| source.directory).as_deref(), Some("h-a"));
    // An installation is about the place, which is the kept row now.
    assert!(stored(&ledger, "installations").contains("|only-here|2|"));
}

#[test]
fn an_origin_is_recorded_and_read_back_without_credentials() {
    let ledger = Ledger::in_memory().unwrap();
    let mut written = installation_of("deploy", &[(None, "skills/deploy")]);
    written.origin = Some(String::from("https://me:hunter2@h/a#main"));

    ledger.installed("collapse", &written).unwrap();

    assert!(!stored(&ledger, "installations").contains("hunter2"));
    let read = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(read.origin.as_deref(), Some("https://h/a#main"));
}

#[test]
fn scrubbing_a_source_takes_the_credentials_out_of_the_origins_spelling_it() {
    let (dir, ledger) = ledger_with_a_token_in_it();
    // As schema 3 fills it in from a source an older build stored, and an
    // ssh user name, which is no credential, in another source's.
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (2, 'git', 'ssh://git@h/b', NULL, 'skills', 'h-b', 0, 0);
             INSERT INTO installations (project, kind, name, source_id, origin, installed_at)
                 VALUES ('../p', 'skill', 'deploy', 1, 'https://me:hunter2@h/a', 0),
                        ('../p', 'skill', 'lint', 2, 'ssh://git@h/b', 0);",
        )
        .unwrap();
    // Handed back without them before the scrub too.
    let origin = |name: &str| {
        ledger
            .installation("../p", Kind::Skill, name)
            .unwrap()
            .unwrap()
            .origin
    };
    assert_eq!(origin("deploy").as_deref(), Some("https://h/a"));

    ledger.scrub_source(1).unwrap();
    ledger.scrub_log().unwrap();

    let installations = stored(&ledger, "installations");
    assert!(installations.contains("|https://h/a|"), "{installations}");
    assert!(installations.contains("|ssh://git@h/b|"), "{installations}");
    assert_eq!(origin("lint").as_deref(), Some("ssh://git@h/b"));
    let file = std::fs::read(dir.path().join(LEDGER_FILE)).unwrap();
    assert!(!file.windows(7).any(|bytes| bytes == b"hunter2"));
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
fn a_ledger_another_process_migrates_meanwhile_is_left_to_that_ones_copy() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    // The copy the other one took, which only it may write.
    let backup = backup_path(&path, 1);
    std::fs::write(&backup, "the other one's copy").unwrap();

    // The other one, holding the write lock the migration takes.
    let other = Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let opening = std::thread::spawn({
        let path = path.clone();
        move || Ledger::open(path).map(drop)
    });
    // Long enough to read version 1 and wait for the lock, which is when a
    // copy taken outside it would be taken.
    std::thread::sleep(std::time::Duration::from_millis(300));
    other.execute_batch(SCHEMA_V2).unwrap();
    other.execute_batch(SCHEMA_V3).unwrap();
    other
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();
    other.execute_batch("COMMIT").unwrap();

    opening.join().unwrap().unwrap();
    assert!(
        std::fs::read(&backup).unwrap() == b"the other one's copy",
        "the other one's copy was replaced"
    );
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

#[cfg(unix)]
#[test]
fn the_copy_is_no_more_readable_than_the_ledger_before_anything_is_in_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let temporary = dir.path().join("mindflayer.db.v1.bak.tmp");

    create_like(&temporary, &path).unwrap();

    let made = std::fs::metadata(&temporary).unwrap();
    assert_eq!(made.len(), 0);
    assert_eq!(made.permissions().mode() & 0o777, 0o600);
}

/// An installation of `name` with a copy at each of `targets`.
fn installation_of(name: &str, targets: &[(Option<&str>, &str)]) -> InstallRecord {
    InstallRecord {
        kind: Kind::Skill,
        name: name.to_owned(),
        source_id: None,
        origin: Some(String::from("workspace")),
        content_hash: Some(String::from("e3b0c442")),
        targets: targets
            .iter()
            .map(|(agent, path)| Target {
                agent: agent.map(str::to_owned),
                path: (*path).to_owned(),
            })
            .collect(),
    }
}

#[test]
fn an_installation_reads_back_with_every_copy_it_wrote() {
    let ledger = Ledger::in_memory().unwrap();
    let written = installation_of(
        "deploy",
        &[
            (Some("claude"), ".claude/skills/deploy"),
            (None, ".agents/skills/deploy"),
        ],
    );

    ledger.installed("collapse", &written).unwrap();

    let read = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    // By path, whatever order they were written in.
    assert_eq!(read.targets[0].path, ".agents/skills/deploy");
    assert_eq!(read.targets[1].path, ".claude/skills/deploy");
    let mut expected = written;
    expected.targets.reverse();
    assert_eq!(read, expected);
    assert!(ledger
        .installation("collapse", Kind::Rule, "deploy")
        .unwrap()
        .is_none());
    assert!(ledger
        .installation("tanukeys", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
}

#[test]
fn installing_again_replaces_the_copies_recorded_rather_than_adding_to_them() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[(None, ".agents/skills/deploy"), (None, "skills/deploy")],
            ),
        )
        .unwrap();
    let mut again = installation_of("deploy", &[(None, "skills/deploy")]);
    again.content_hash = Some(String::from("another"));

    ledger.installed("collapse", &again).unwrap();

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(again)
    );
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["deploy"]
    );
}

#[test]
fn a_record_naming_one_copy_twice_is_refused_and_the_old_one_stands() {
    let ledger = Ledger::in_memory().unwrap();
    let first = installation_of("deploy", &[(None, "skills/deploy")]);
    ledger.installed("collapse", &first).unwrap();

    let twice = installation_of(
        "deploy",
        &[(Some("claude"), "skills/deploy"), (None, "skills/deploy")],
    );
    assert!(ledger.installed("collapse", &twice).is_err());

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(first)
    );
}

#[test]
fn forgetting_an_installation_forgets_its_copies() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[
                    (None, ".agents/skills/deploy"),
                    (None, ".claude/skills/deploy"),
                ],
            ),
        )
        .unwrap();

    assert!(ledger
        .uninstalled("collapse", Kind::Skill, "deploy")
        .unwrap());

    assert!(ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    let targets: i64 = ledger
        .connection
        .query_row("SELECT count(*) FROM installation_targets", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(targets, 0);
}

#[test]
fn forgetting_one_copy_keeps_the_installation_until_its_last_copy_goes() {
    let ledger = Ledger::in_memory().unwrap();
    let written = installation_of(
        "deploy",
        &[
            (None, ".agents/skills/deploy"),
            (Some("claude"), ".claude/skills/deploy"),
        ],
    );
    ledger.installed("collapse", &written).unwrap();

    let forgot = |path| {
        ledger
            .uninstalled_copy("collapse", Kind::Skill, "deploy", path)
            .unwrap()
    };
    assert_eq!(forgot("skills/deploy"), Forgotten::Nothing);
    assert_eq!(forgot(".claude/skills/deploy"), Forgotten::Copy);
    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            targets: written.targets[..1].to_vec(),
            ..written.clone()
        })
    );
    assert_eq!(forgot(".claude/skills/deploy"), Forgotten::Nothing);
    assert_eq!(forgot(".agents/skills/deploy"), Forgotten::Installation);
    assert!(ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    assert_eq!(forgot(".agents/skills/deploy"), Forgotten::Nothing);
}

#[test]
fn an_installation_recording_no_copy_is_forgotten_whole_by_any_one() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed("collapse", &installation_of("deploy", &[]))
        .unwrap();

    assert_eq!(
        ledger
            .uninstalled_copy("collapse", Kind::Skill, "deploy", "skills/deploy")
            .unwrap(),
        Forgotten::Installation
    );
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn retargeting_an_installation_moves_its_copies_and_keeps_the_rest_of_its_record() {
    let ledger = Ledger::in_memory().unwrap();
    let written = installation_of("deploy", &[(Some("claude"), ".claude/skills/deploy")]);
    ledger.installed("collapse", &written).unwrap();
    let when = |ledger: &Ledger| -> i64 {
        ledger
            .connection
            .query_row("SELECT installed_at FROM installations", [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    let installed_at = when(&ledger);
    let moved = installation_of("deploy", &[(None, ".agents/skills/deploy")]).targets;

    assert!(ledger
        .retargeted("collapse", Kind::Skill, "deploy", &moved)
        .unwrap());

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            targets: moved,
            ..written
        })
    );
    // Not installed again: it is the same installation, from when it was.
    assert_eq!(when(&ledger), installed_at);
}

#[test]
fn retargeting_an_installation_to_no_copy_forgets_it_and_one_never_made_is_nothing() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, ".agents/skills/deploy")]),
        )
        .unwrap();

    assert!(ledger
        .retargeted("collapse", Kind::Skill, "deploy", &[])
        .unwrap());
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
    assert!(!ledger
        .retargeted("collapse", Kind::Skill, "deploy", &[])
        .unwrap());
    assert!(!ledger
        .retargeted(
            "collapse",
            Kind::Skill,
            "review",
            &installation_of("review", &[(None, ".agents/skills/review")]).targets,
        )
        .unwrap());
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn every_copy_installed_into_a_project_is_listed_by_path_whichever_install_wrote_it() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[
                    (Some("claude"), ".claude/skills/deploy"),
                    (None, ".agents/skills/deploy"),
                ],
            ),
        )
        .unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("audit", &[(None, ".agents/skills/audit")]),
        )
        .unwrap();
    ledger
        .installed(
            "tanukeys",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        [
            ".agents/skills/audit",
            ".agents/skills/deploy",
            ".claude/skills/deploy"
        ]
    );
    assert_eq!(
        ledger.copies("tanukeys", Kind::Skill).unwrap(),
        ["skills/deploy"]
    );
    assert!(ledger.copies("collapse", Kind::Rule).unwrap().is_empty());
    assert!(ledger.copies("elsewhere", Kind::Skill).unwrap().is_empty());
}

#[test]
fn every_installation_into_a_project_reads_back_with_its_copies_in_name_order() {
    let ledger = Ledger::in_memory().unwrap();
    let deploy = installation_of(
        "deploy",
        &[
            (Some("claude"), ".claude/skills/deploy"),
            (None, ".agents/skills/deploy"),
        ],
    );
    let audit = InstallRecord {
        origin: Some(String::from("https://github.com/acme/skills")),
        source_id: None,
        ..installation_of("audit", &[(None, ".agents/skills/audit")])
    };
    let empty = installation_of("bare", &[]);
    for record in [&deploy, &audit, &empty] {
        ledger.installed("collapse", record).unwrap();
    }
    ledger
        .installed(
            "tanukeys",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    let records = ledger.records("collapse", Kind::Skill).unwrap();

    let mut deploy = deploy;
    deploy.targets.reverse();
    assert_eq!(records, [audit, empty, deploy]);
    for record in &records {
        assert_eq!(
            ledger
                .installation("collapse", Kind::Skill, &record.name)
                .unwrap()
                .as_ref(),
            Some(record)
        );
    }
    assert!(ledger.records("collapse", Kind::Rule).unwrap().is_empty());
    assert!(ledger.records("elsewhere", Kind::Skill).unwrap().is_empty());
}

#[test]
fn every_installation_reads_back_without_the_credentials_an_older_build_stored() {
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    ledger
        .connection
        .execute(
            "UPDATE installations SET origin = 'https://user:ghp_t0ken@github.com/acme/skills'",
            [],
        )
        .unwrap();

    let records = ledger.records("collapse", Kind::Skill).unwrap();

    assert_eq!(
        records[0].origin.as_deref(),
        Some("https://github.com/acme/skills")
    );
}

#[test]
fn a_ledger_waits_for_another_writer_as_long_as_a_writer_waits_for_the_workspace() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    let opened = Ledger::open(&path).unwrap();
    let existing = Ledger::open_existing(&path).unwrap().unwrap();

    // Read off each connection rather than shown by two writers racing:
    // rusqlite waits five seconds of its own accord, which is long enough
    // for any race a test can stage.
    for ledger in [&opened, &existing] {
        let waits: i64 = ledger
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(
            u128::try_from(waits).unwrap(),
            crate::lock::WAIT.as_millis()
        );
    }
}
