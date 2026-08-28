//! The workspace's record of what it gathered, from where, and what it did.
//!
//! Everything else Mindflayer writes is a file a person is meant to open: the
//! marker files are TOML with comments explaining themselves. This is the one
//! thing that is not, because it answers questions a file cannot: which of two
//! identically named skills came from which repository, at which revision, and
//! what happened the last four times a gather was run. It sits beside
//! `flayer.toml` in `.mindflayer/`, so it travels with the workspace it
//! describes and two workspaces never share a history.
//!
//! Nothing here decides *what* is worth gathering. It records what
//! [`crate::gather`] did.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use thiserror::Error;

use crate::kind::Kind;

/// The schema version this build writes.
///
/// Refused if the file on disk is newer, for the same reason a marker file is:
/// an old binary silently misreading a newer database is the failure that has
/// no error message.
pub const SCHEMA_VERSION: u32 = 2;

/// The file name, under `.mindflayer/`.
pub const LEDGER_FILE: &str = "mindflayer.db";

/// What kind of place an artifact was gathered from.
///
/// A closed enum, like [`Kind`]: every source ships in this crate, so adding
/// the next one is a variant the compiler then asks about everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A git repository, cloned into the workspace cache.
    Git,
}

impl SourceKind {
    /// How it is written in the database and in a report.
    pub const fn slug(self) -> &'static str {
        match self {
            SourceKind::Git => "git",
        }
    }

    fn parse(slug: &str) -> Option<Self> {
        match slug {
            "git" => Some(SourceKind::Git),
            _ => None,
        }
    }
}

/// A place artifacts have been gathered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Its row id, which is what gathered artifacts point at.
    pub id: i64,
    pub kind: SourceKind,
    /// The URL as the user typed it.
    pub url: String,
    /// The branch or tag asked for, or `None` for whatever the remote's HEAD
    /// points at.
    pub reference: Option<String>,
    /// The folder inside the source that was harvested, `skills` by default.
    pub subdirectory: String,
    /// The commit last gathered from, once one has been resolved.
    pub revision: Option<String>,
    /// The folder this source owns under `.mindflayer/<kind folder>/`.
    ///
    /// Stored rather than derived: two URLs can reduce to the same readable
    /// name, and where a source's files went is a fact about the past that
    /// must not move when the naming rule changes.
    pub directory: String,
}

/// One gathered artifact, and the source it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gathered {
    pub kind: Kind,
    pub name: String,
    /// Where it sits, relative to the workspace root, `/` separated.
    pub path: String,
    /// The one line a listing shows, as it read when it was gathered.
    pub summary: Option<String>,
    /// The revision it was taken at.
    pub revision: Option<String>,
    /// When it was gathered, in seconds since the Unix epoch.
    pub gathered_at: i64,
    pub source: Source,
}

/// Something the user asked for, whether or not it worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Harvesting artifacts from a source into the workspace.
    Gather,
    /// Copying one from the workspace shelf into a mind project.
    Install,
    /// Taking one back out of a project.
    Uninstall,
}

impl Action {
    pub const fn slug(self) -> &'static str {
        match self {
            Action::Gather => "gather",
            Action::Install => "install",
            Action::Uninstall => "uninstall",
        }
    }
}

/// How an action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed,
}

impl Outcome {
    pub const fn slug(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Failed => "failed",
        }
    }
}

/// One line of the action log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it happened, in seconds since the Unix epoch.
    pub at: i64,
    pub action: String,
    /// What the action was about: a URL, a name.
    pub target: Option<String>,
    pub outcome: String,
    /// What it said, successful or not.
    pub detail: Option<String>,
}

/// The workspace's database.
#[derive(Debug)]
pub struct Ledger {
    connection: Connection,
    path: PathBuf,
}

impl Ledger {
    /// Open the ledger at `path`, creating and migrating it if needed.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, LedgerError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| LedgerError::Create {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(&path).map_err(|source| LedgerError::Open {
            path: path.clone(),
            source,
        })?;
        let ledger = Self { connection, path };
        ledger.migrate()?;
        Ok(ledger)
    }

    /// An in-memory ledger, for tests and for anything that must not persist.
    pub fn in_memory() -> Result<Self, LedgerError> {
        let connection = Connection::open_in_memory().map_err(|source| LedgerError::Open {
            path: PathBuf::from(":memory:"),
            source,
        })?;
        let ledger = Self {
            connection,
            path: PathBuf::from(":memory:"),
        };
        ledger.migrate()?;
        Ok(ledger)
    }

    /// The file this ledger is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bring an empty or older database up to [`SCHEMA_VERSION`].
    ///
    /// `user_version` rather than a table of our own: SQLite carries the
    /// integer in the file header, so reading it costs nothing and it cannot
    /// itself be missing from a database that was half created.
    fn migrate(&self) -> Result<(), LedgerError> {
        self.connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|source| self.failed("enabling foreign keys", source))?;

        let found: u32 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|source| self.failed("reading the schema version", source))?;

        if found > SCHEMA_VERSION {
            return Err(LedgerError::Version {
                path: self.path.clone(),
                found,
            });
        }
        if found == SCHEMA_VERSION {
            return Ok(());
        }

        // One block per version, applied in order, so a database created two
        // versions ago arrives at the same schema as one created today.
        if found < 1 {
            self.connection
                .execute_batch(SCHEMA_V1)
                .map_err(|source| self.failed("creating the schema", source))?;
        }
        if found < 2 {
            self.connection
                .execute_batch(SCHEMA_V2)
                .map_err(|source| self.failed("adding the installations table", source))?;
        }

        self.connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|source| self.failed("stamping the schema version", source))
    }

    /// The source matching this coordinate, if it has been gathered before.
    pub fn source(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Option<Source>, LedgerError> {
        self.connection
            .query_row(
                "SELECT id, kind, url, reference, subdirectory, revision, directory
                   FROM sources
                  WHERE kind = ?1 AND url = ?2 AND subdirectory = ?3
                    AND reference IS ?4",
                rusqlite::params![kind.slug(), url, subdirectory, reference],
                read_source,
            )
            .optional()
            .map_err(|source| self.failed("looking up a source", source))
    }

    /// The source for this coordinate, registering it the first time.
    ///
    /// Its `directory` is chosen once, here, and never recomputed: it is where
    /// files already are.
    pub fn source_for(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Source, LedgerError> {
        if let Some(existing) = self.source(kind, url, reference, subdirectory)? {
            return Ok(existing);
        }

        let directory = self.free_directory(&slug(url))?;
        let at = now();
        self.connection
            .execute(
                "INSERT INTO sources
                     (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6)",
                rusqlite::params![kind.slug(), url, reference, subdirectory, directory, at],
            )
            .map_err(|source| self.failed("registering a source", source))?;

        Ok(Source {
            id: self.connection.last_insert_rowid(),
            kind,
            url: url.to_owned(),
            reference: reference.map(str::to_owned),
            subdirectory: subdirectory.to_owned(),
            revision: None,
            directory,
        })
    }

    /// A readable directory name no other source has taken.
    fn free_directory(&self, wanted: &str) -> Result<String, LedgerError> {
        for attempt in 1.. {
            let candidate = if attempt == 1 {
                wanted.to_owned()
            } else {
                format!("{wanted}-{attempt}")
            };
            let taken = self
                .connection
                .query_row(
                    "SELECT 1 FROM sources WHERE directory = ?1",
                    [&candidate],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|source| self.failed("checking a source directory", source))?
                .is_some();
            if !taken {
                return Ok(candidate);
            }
        }
        unreachable!("the loop returns as soon as a name is free")
    }

    /// Note which revision a source was last gathered at.
    pub fn saw_source(&self, id: i64, revision: Option<&str>) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "UPDATE sources SET revision = ?2, last_seen = ?3 WHERE id = ?1",
                rusqlite::params![id, revision, now()],
            )
            .map(|_| ())
            .map_err(|source| self.failed("updating a source", source))
    }

    /// Record one gathered artifact, replacing what that source held under the
    /// same name.
    ///
    /// The same name from a *different* source is a different row, on purpose:
    /// two repositories offering `commit-style` are two things to choose
    /// between, and this is what remembers which is which.
    pub fn record(
        &self,
        source: &Source,
        kind: Kind,
        name: &str,
        path: &str,
        summary: Option<&str>,
        revision: Option<&str>,
    ) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "INSERT INTO artifacts (kind, name, source_id, path, summary, revision, gathered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (kind, name, source_id) DO UPDATE SET
                     path = excluded.path,
                     summary = excluded.summary,
                     revision = excluded.revision,
                     gathered_at = excluded.gathered_at",
                rusqlite::params![kind.slug(), name, source.id, path, summary, revision, now()],
            )
            .map(|_| ())
            .map_err(|error| self.failed("recording an artifact", error))
    }

    /// Everything gathered, ordered the way a listing reads it: by kind, then
    /// name, then source.
    pub fn gathered(&self) -> Result<Vec<Gathered>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT a.kind, a.name, a.path, a.summary, a.revision, a.gathered_at,
                        s.id, s.kind, s.url, s.reference, s.subdirectory, s.revision, s.directory
                   FROM artifacts a
                   JOIN sources s ON s.id = a.source_id
                  ORDER BY a.kind, a.name, s.url",
            )
            .map_err(|source| self.failed("listing what was gathered", source))?;

        // The two slug columns are read as the strings they are and turned
        // into types afterwards, so a row naming a kind this build has never
        // heard of is skipped rather than failing the whole listing. Opening
        // refuses a newer database outright, so such a row was edited by hand.
        let rows = statement
            .query_map([], |row| {
                Ok(Row {
                    kind: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    summary: row.get(3)?,
                    revision: row.get(4)?,
                    gathered_at: row.get(5)?,
                    source_id: row.get(6)?,
                    source_kind: row.get(7)?,
                    url: row.get(8)?,
                    reference: row.get(9)?,
                    subdirectory: row.get(10)?,
                    source_revision: row.get(11)?,
                    directory: row.get(12)?,
                })
            })
            .map_err(|source| self.failed("listing what was gathered", source))?;

        let mut found = Vec::new();
        for row in rows {
            let row = row.map_err(|source| self.failed("reading a gathered artifact", source))?;
            let (Ok(kind), Some(source_kind)) = (
                row.kind.parse::<Kind>(),
                SourceKind::parse(&row.source_kind),
            ) else {
                continue;
            };
            found.push(Gathered {
                kind,
                name: row.name,
                path: row.path,
                summary: row.summary,
                revision: row.revision,
                gathered_at: row.gathered_at,
                source: Source {
                    id: row.source_id,
                    kind: source_kind,
                    url: row.url,
                    reference: row.reference,
                    subdirectory: row.subdirectory,
                    revision: row.source_revision,
                    directory: row.directory,
                },
            });
        }
        Ok(found)
    }

    /// Record that an artifact was put into a project.
    pub fn installed(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        source_id: Option<i64>,
        path: &str,
    ) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "INSERT INTO installations (project, kind, name, source_id, path, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (project, kind, name) DO UPDATE SET
                     source_id = excluded.source_id,
                     path = excluded.path,
                     installed_at = excluded.installed_at",
                rusqlite::params![project, kind.slug(), name, source_id, path, now()],
            )
            .map(|_| ())
            .map_err(|source| self.failed("recording an installation", source))
    }

    /// Forget an installation, returning whether there was one.
    pub fn uninstalled(&self, project: &str, kind: Kind, name: &str) -> Result<bool, LedgerError> {
        self.connection
            .execute(
                "DELETE FROM installations WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
            )
            .map(|rows| rows > 0)
            .map_err(|source| self.failed("forgetting an installation", source))
    }

    /// The names this ledger says Mindflayer put into a project.
    ///
    /// What separates an artifact this tool installed from one somebody wrote
    /// by hand, which is what stops an uninstall from deleting the second.
    pub fn installations(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT name FROM installations
                  WHERE project = ?1 AND kind = ?2 ORDER BY name",
            )
            .map_err(|source| self.failed("reading installations", source))?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| row.get(0))
            .map_err(|source| self.failed("reading installations", source))?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(|source| self.failed("reading installations", source))
    }

    /// Append to the action log.
    ///
    /// Failures are logged too. A log that only records what worked cannot
    /// answer the question anybody actually opens it with.
    pub fn log(
        &self,
        action: Action,
        target: Option<&str>,
        outcome: Outcome,
        detail: Option<&str>,
    ) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![now(), action.slug(), target, outcome.slug(), detail],
            )
            .map(|_| ())
            .map_err(|source| self.failed("writing to the action log", source))
    }

    /// The most recent actions, newest first.
    pub fn history(&self, limit: usize) -> Result<Vec<Entry>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT at, action, target, outcome, detail
                   FROM actions ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|source| self.failed("reading the action log", source))?;
        let rows = statement
            .query_map([limit as i64], |row| {
                Ok(Entry {
                    at: row.get(0)?,
                    action: row.get(1)?,
                    target: row.get(2)?,
                    outcome: row.get(3)?,
                    detail: row.get(4)?,
                })
            })
            .map_err(|source| self.failed("reading the action log", source))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|source| self.failed("reading the action log", source))
    }

    fn failed(&self, doing: &str, source: rusqlite::Error) -> LedgerError {
        LedgerError::Query {
            path: self.path.clone(),
            doing: doing.to_owned(),
            source,
        }
    }
}

/// One joined row, before its slugs mean anything.
struct Row {
    kind: String,
    name: String,
    path: String,
    summary: Option<String>,
    revision: Option<String>,
    gathered_at: i64,
    source_id: i64,
    source_kind: String,
    url: String,
    reference: Option<String>,
    subdirectory: String,
    source_revision: Option<String>,
    directory: String,
}

/// What a `SELECT` over `sources` maps to.
fn read_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    let slug: String = row.get(1)?;
    // No path of ours can write a kind this build does not know: opening
    // refuses a newer database outright. A row edited by hand still can, so it
    // is reported rather than guessed at.
    let kind = SourceKind::parse(&slug).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            format!("unknown source kind `{slug}`").into(),
        )
    })?;
    Ok(Source {
        id: row.get(0)?,
        kind,
        url: row.get(2)?,
        reference: row.get(3)?,
        subdirectory: row.get(4)?,
        revision: row.get(5)?,
        directory: row.get(6)?,
    })
}

/// A URL as a readable directory name.
///
/// The scheme, any credentials and a trailing `.git` carry nothing a person
/// browsing `.mindflayer/skills/` wants to read, and anything that is not a
/// plain character becomes a hyphen, so the result is a name every filesystem
/// accepts and nobody has to quote.
fn slug(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    // `git@github.com:acme/skills` and `https://user:token@host/x` both put
    // what is worth keeping after the last `@`.
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let rest = rest.strip_suffix(".git").unwrap_or(rest);

    let mut out = String::new();
    for c in rest.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        String::from("source")
    } else {
        trimmed.to_owned()
    }
}

/// Seconds since the Unix epoch.
///
/// An integer rather than a formatted timestamp: it needs no date library, it
/// sorts and compares as itself, and SQLite renders it for anyone reading the
/// file by hand with `datetime(at, 'unixepoch')`.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Why the ledger could not be opened, read or written.
#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("{path}: cannot be created: {source}")]
    Create {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: cannot be opened: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("{path}: written by a newer Mindflayer (schema version {found}, this one reads {SCHEMA_VERSION})")]
    Version { path: PathBuf, found: u32 },
    #[error("{path}: {doing} failed: {source}")]
    Query {
        path: PathBuf,
        doing: String,
        #[source]
        source: rusqlite::Error,
    },
}

impl LedgerError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            LedgerError::Create { path, .. }
            | LedgerError::Open { path, .. }
            | LedgerError::Version { path, .. }
            | LedgerError::Query { path, .. } => path,
        }
    }
}

/// The first schema.
///
/// Timestamps are Unix seconds. `ON DELETE CASCADE` on the artifacts means
/// forgetting a source forgets what it brought, which is the only sane reading
/// of dropping one.
const SCHEMA_V1: &str = "
CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL,
    url          TEXT    NOT NULL,
    reference    TEXT,
    subdirectory TEXT    NOT NULL,
    revision     TEXT,
    directory    TEXT    NOT NULL UNIQUE,
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL,
    UNIQUE (kind, url, reference, subdirectory)
);

CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL,
    UNIQUE (kind, name, source_id)
);

CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);

CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
";

/// The second schema: what has been put into a mind project.
///
/// `project` is stored relative to the workspace, the way a registered project
/// is, so moving the workspace and its projects together keeps the record
/// true. `source_id` is nulled rather than cascaded when a source is
/// forgotten: the file is still in the project, and saying "installed, origin
/// no longer known" is truer than pretending it was never installed.
const SCHEMA_V2: &str = "
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    path         TEXT    NOT NULL,
    installed_at INTEGER NOT NULL,
    UNIQUE (project, kind, name)
);

CREATE INDEX installations_by_project ON installations (project);
";

#[cfg(test)]
mod tests {
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
}
