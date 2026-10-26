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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use thiserror::Error;

use crate::kind::Kind;

/// The schema version this build writes.
///
/// Refused if the file on disk is newer, for the same reason a marker file is:
/// an old binary silently misreading a newer database is the failure that has
/// no error message.
pub const SCHEMA_VERSION: u32 = 3;

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

/// What the ledger records of one installation into a project: what went
/// in, from where, and every copy it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    pub kind: Kind,
    pub name: String,
    /// The shelf source it was copied from, while the ledger still has it;
    /// `None` for the workspace's own and a loaded workspace's.
    pub source_id: Option<i64>,
    /// Where it came from, spelled as `--from` takes it
    /// ([`crate::install::Offer::origin`]). `None` only in a record from
    /// before schema 3 of something that came from no source.
    pub origin: Option<String>,
    /// [`crate::hash::tree`] of what was written, which every target holds.
    /// `None` in a record from before schema 3, which D-04 reads as current
    /// only while the copy matches its offer byte for byte.
    pub content_hash: Option<String>,
    /// Every copy it wrote, by path.
    pub targets: Vec<Target>,
}

/// One copy an installation wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The one agent the copy is for, or `None` for a copy no one agent owns:
    /// the folder a v2 project's marker names, or `.agents/skills`, which two
    /// agents read.
    pub agent: Option<String>,
    /// Where the copy is, relative to the project, `/` separated.
    pub path: String,
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
            fs::create_dir_all(parent).map_err(|source| LedgerError::Create {
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

    /// Open the ledger at `path` only if it is there: for a command that only
    /// reads it, which is no reason to create a database, or the folder it
    /// would sit in.
    ///
    /// One that is there is still brought up to [`SCHEMA_VERSION`], as
    /// [`open`](Self::open) brings it. A build reads its own schema and no
    /// other, so declining to migrate would leave an older ledger unreadable
    /// rather than untouched: the step is upkeep of Mindflayer's own file, not
    /// a change anybody asked for, and the one write a read may make.
    pub fn open_existing(path: impl Into<PathBuf>) -> Result<Option<Self>, LedgerError> {
        let path = path.into();
        if !path.is_file() {
            return Ok(None);
        }
        // Without `SQLITE_OPEN_CREATE`, so a file removed since the check is
        // an error to report rather than an empty database left in its place.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection =
            Connection::open_with_flags(&path, flags).map_err(|source| LedgerError::Open {
                path: path.clone(),
                source,
            })?;
        let ledger = Self { connection, path };
        ledger.migrate()?;
        Ok(Some(ledger))
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
    ///
    /// A database that already holds something is [backed up](Self::back_up)
    /// first, whichever step it is about to take. A step to come is one more
    /// block below, and the copy is taken for it without its asking.
    fn migrate(&self) -> Result<(), LedgerError> {
        self.connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|source| self.failed("enabling foreign keys", source))?;

        // Read before taking the write lock, so that opening a database that
        // is already current — every open but the first after an upgrade —
        // writes nothing, and still works on a file it may only read.
        let found = self.schema_version()?;
        if found == SCHEMA_VERSION {
            return Ok(());
        }

        // Every step and the stamp in one transaction. Run one statement at a
        // time, a step that fails, or a process that dies halfway, would leave
        // a database half migrated under its old number, which the next open
        // would try to migrate again and fail on; this way it is left as it
        // was.
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(|source| self.failed("starting the migration", source))?;
        // Read again under the lock, because another process may have
        // migrated it in the meantime.
        let found = self.schema_version()?;
        if found == SCHEMA_VERSION {
            return Ok(());
        }
        // Copied under the lock too, not before it: a copy taken after the
        // first read could be of what another process has since migrated,
        // and would replace that process's copy of the old version with one of
        // the new. Holding the lock, this is the one process migrating, and
        // the file is at the version just read. Version 0 is a database just
        // created, with nothing in it to keep.
        if found > 0 {
            self.back_up(found)?;
        }

        // One block per version, applied in order, so a database created two
        // versions ago arrives at the same schema as one created today.
        if found < 1 {
            transaction
                .execute_batch(SCHEMA_V1)
                .map_err(|source| self.failed("creating the schema", source))?;
        }
        if found < 2 {
            transaction
                .execute_batch(SCHEMA_V2)
                .map_err(|source| self.failed("adding the installations table", source))?;
        }
        if found < 3 {
            transaction
                .execute_batch(SCHEMA_V3)
                .map_err(|source| self.failed("giving installations their targets", source))?;
        }

        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|source| self.failed("stamping the schema version", source))?;
        transaction
            .commit()
            .map_err(|source| self.failed("finishing the migration", source))
    }

    /// Copy the database, at schema `version`, to [`backup_path`] before a
    /// step changes it.
    ///
    /// So that a step that turns out wrong, or a build that misreads what it
    /// wrote, leaves the file it started from to go back to: the copy opens at
    /// the old version, and an older Mindflayer reads it as it always did. One
    /// copy per version, replaced if an earlier attempt at the same step left
    /// one, since both are of the same file at the same version and the newer
    /// is the one the step is about to change. Nothing is migrated unless the
    /// copy is whole.
    ///
    /// Taken by SQLite's online backup rather than by copying the file:
    /// opening and closing the file outside SQLite while this connection
    /// holds it would drop the connection's locks on Unix, where they belong
    /// to the process. Read through a connection of its own, because this
    /// one holds the migration's write lock and SQLite will not copy from a
    /// connection in the middle of writing; reading beside that lock is
    /// allowed. Written to a temporary beside it and renamed, so an
    /// interrupted copy is never mistaken for a whole one, and made with the
    /// database's permissions before anything is written to it, so a ledger
    /// kept private is never readable through its copy, not even while the
    /// copy is being written.
    ///
    /// Called with the write lock held, which is what makes the temporary's
    /// one fixed name safe: no other process is copying, so whatever is
    /// there is what an interrupted attempt left.
    fn back_up(&self, version: u32) -> Result<(), LedgerError> {
        let backup = backup_path(&self.path, version);
        let failed = |source: io::Error| LedgerError::Backup {
            path: self.path.clone(),
            backup: backup.clone(),
            source,
        };
        let mut name = backup.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        let temporary = backup.with_file_name(name);

        // What an interrupted attempt left, its journal included, which
        // SQLite would otherwise replay into the fresh copy.
        for leftover in [temporary.clone(), journal_of(&temporary)] {
            match fs::remove_file(&leftover) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(failed(error)),
                _ => {}
            }
        }
        if let Err(error) = copy_database(&self.path, &temporary, version) {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(journal_of(&temporary));
            return Err(failed(error));
        }
        // Exactly the database's once it is whole: it was made with the
        // owner's read and write added, and less whatever the umask took.
        if let Ok(metadata) = fs::metadata(&self.path) {
            let _ = fs::set_permissions(&temporary, metadata.permissions());
        }
        fs::rename(&temporary, &backup).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            failed(error)
        })
    }

    /// The schema version on disk, refused when it is newer than this build's.
    fn schema_version(&self) -> Result<u32, LedgerError> {
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
        Ok(found)
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

    /// Record that an artifact was put into a project, replacing whatever was
    /// recorded of it before.
    ///
    /// Its targets are replaced as a whole rather than added to: they are the
    /// copies this install wrote, and a target an earlier install had and this
    /// one did not is no longer Mindflayer's to overwrite or delete. All of it
    /// or none, so a record never names some of an install's copies.
    pub fn installed(
        &self,
        project: &str,
        installation: &InstallRecord,
    ) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording an installation", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let id: i64 = transaction
            .query_row(
                "INSERT INTO installations
                     (project, kind, name, source_id, origin, content_hash, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (project, kind, name) DO UPDATE SET
                     source_id = excluded.source_id,
                     origin = excluded.origin,
                     content_hash = excluded.content_hash,
                     installed_at = excluded.installed_at
                 RETURNING id",
                rusqlite::params![
                    project,
                    installation.kind.slug(),
                    installation.name,
                    installation.source_id,
                    installation.origin,
                    installation.content_hash,
                    now()
                ],
                |row| row.get(0),
            )
            .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM installation_targets WHERE installation_id = ?1",
                [id],
            )
            .map_err(failed)?;
        for target in &installation.targets {
            transaction
                .execute(
                    "INSERT INTO installation_targets (installation_id, agent, path)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, target.agent, target.path],
                )
                .map_err(failed)?;
        }
        transaction.commit().map_err(failed)
    }

    /// What the ledger records of installing `name` into a project, if it
    /// records anything.
    pub fn installation(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
    ) -> Result<Option<InstallRecord>, LedgerError> {
        let failed = |source| self.failed("reading an installation", source);
        let Some((id, source_id, origin, content_hash)) = self
            .connection
            .query_row(
                "SELECT id, source_id, origin, content_hash FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| Ok((row.get::<_, i64>(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(None);
        };

        let mut statement = self
            .connection
            .prepare(
                "SELECT agent, path FROM installation_targets
                  WHERE installation_id = ?1 ORDER BY path",
            )
            .map_err(failed)?;
        let targets = statement
            .query_map([id], |row| {
                Ok(Target {
                    agent: row.get(0)?,
                    path: row.get(1)?,
                })
            })
            .map_err(failed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(failed)?;

        Ok(Some(InstallRecord {
            kind,
            name: name.to_owned(),
            source_id,
            origin,
            content_hash,
            targets,
        }))
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

/// Where a ledger at `path` is copied before a schema step takes it on from
/// `version`: beside it, as `mindflayer.db.v2.bak` for version 2.
///
/// Named after the version it holds rather than numbered or dated, so which
/// build reads a copy is in its name, and a ledger brought forward by several
/// builds in turn keeps one copy for each version it had, side by side.
pub fn backup_path(path: &Path, version: u32) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".v{version}.bak"));
    path.with_file_name(name)
}

/// Copy the database at `from`, at schema `version`, into a new one at `to`,
/// closed once it is whole.
fn copy_database(from: &Path, to: &Path, version: u32) -> Result<(), io::Error> {
    // Read-only and never created: a file gone since it was opened is an
    // error, not an empty database to copy.
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let source = Connection::open_with_flags(from, flags).map_err(io::Error::other)?;
    create_like(to, from)?;
    let mut copy = Connection::open(to).map_err(io::Error::other)?;
    {
        let copying = Backup::new(&source, &mut copy).map_err(io::Error::other)?;
        // Every page in one step, so the copy is one moment of the database
        // rather than pages from either side of a write.
        match copying.step(-1).map_err(io::Error::other)? {
            StepResult::Done => {}
            other => {
                return Err(io::Error::other(format!(
                    "something else was writing to it ({other:?}), so try again"
                )))
            }
        }
    }
    // What the copy is for is opening at the old version, so that is checked
    // rather than assumed: a file that is not the one being migrated is no
    // way back from migrating it.
    let copied: u32 = copy
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(io::Error::other)?;
    if copied != version {
        return Err(io::Error::other(format!(
            "the copy is at schema version {copied}, not {version}, so it is not of the file being migrated"
        )));
    }
    Ok(())
}

/// Create `path` empty, no more readable than the file at `like`, for SQLite
/// to copy that file into.
///
/// Made with that mode rather than given it once the copy is written: SQLite
/// creates a file anyone the umask allows may read, and whoever opened it
/// before its mode changed could read the whole ledger through that handle
/// afterwards. The owner may always read and write it, which SQLite needs
/// and nobody else gains by. SQLite opens an empty file as an empty
/// database, and gives the journal it keeps beside it the same mode.
fn create_like(path: &Path, like: &Path) -> Result<(), io::Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode((fs::metadata(like)?.permissions().mode() & 0o777) | 0o600);
    }
    // Elsewhere a new file takes its folder's access rules, and the one
    // flag left, read-only, is given to the copy once it is whole.
    #[cfg(not(unix))]
    let _ = like;
    options.open(path).map(drop)
}

/// The rollback journal SQLite keeps beside a database while writing it.
fn journal_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push("-journal");
    path.with_file_name(name)
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
        source: io::Error,
    },
    #[error("{path}: cannot be opened: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("{path}: written by a newer Mindflayer (schema version {found}, this one reads {SCHEMA_VERSION})")]
    Version { path: PathBuf, found: u32 },
    #[error("{path}: cannot be copied to {backup} before its schema changes, so it is left as it is: {source}")]
    Backup {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: no workspace has been made here yet, so there is no ledger to write to")]
    Unmade { path: PathBuf },
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
            | LedgerError::Backup { path, .. }
            | LedgerError::Unmade { path }
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

/// The third schema: one installation, a copy in each place its agents read
/// (D-07), and enough about what it wrote to tell it apart later (D-04).
///
/// An installation keeps one row, and every copy it wrote is a target of it:
/// installed, updated and removed together, since an agent that reads two of
/// the copies (OpenCode reads `.agents/skills` and `.claude/skills`) must
/// never see two versions. A target's `path` is relative to the project and
/// `/` separated, as the installation's own was, which goes: kept in both
/// places, the two could disagree. `agent` names the one agent a copy is for,
/// and is empty for a copy no one agent owns: the folder a v2 project's
/// marker names, or `.agents/skills`, which two agents read. One row per path,
/// so such a copy is never recorded twice.
///
/// `origin` is where the installation came from, spelled as `--from` takes
/// it, which the workspace's own and a loaded workspace's had no other way to
/// say: their `source_id` is empty. `content_hash` is [`crate::hash::tree`]
/// of what was written, once for the installation rather than once a target,
/// because every target is written from one copy and is compared with it.
///
/// What version 2 recorded becomes one target at the path it had. Its origin
/// is filled in where it points at a source, spelled as `--from` takes a
/// source's offer at this version — the address, with `#` and the branch when
/// one was asked for — and here rather than through the code that spells it
/// now, so a later change to that spelling cannot change what this step
/// writes. An install from the workspace or a load gets none, since version 2
/// kept nothing that says which. No hash is made up for a copy nobody hashed
/// when it was written: D-04 reads an empty one as current only while the
/// copy matches its offer byte for byte.
const SCHEMA_V3: &str = "
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL,
    UNIQUE (installation_id, path)
);

INSERT INTO installation_targets (installation_id, agent, path)
    SELECT id, NULL, path FROM installations;

ALTER TABLE installations DROP COLUMN path;
ALTER TABLE installations ADD COLUMN origin TEXT;
ALTER TABLE installations ADD COLUMN content_hash TEXT;

UPDATE installations
   SET origin = (SELECT s.url || coalesce('#' || s.reference, '')
                   FROM sources s
                  WHERE s.id = installations.source_id)
 WHERE source_id IS NOT NULL;
";

#[cfg(test)]
#[path = "ledger_test.rs"]
mod tests;
