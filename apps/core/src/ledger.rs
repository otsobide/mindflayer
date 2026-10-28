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
//!
//! Nor does it write a credential. Every URL is [`redacted`] on the way in,
//! whichever caller is writing, and again on the way out, for a row a
//! Mindflayer from before the redaction wrote, which stays in the file until
//! a command that writes scrubs it: the ledger is the one place everything
//! stored passes through, so it is where the guarantee is cheapest to keep.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::code::codes;
use crate::kind::Kind;
use crate::settings::ProjectId;
use crate::url::{carries_credentials, redacted};

/// The schema version this build writes.
///
/// Refused if the file on disk is newer, for the same reason a marker file is:
/// an old binary silently misreading a newer database is the failure that has
/// no error message.
pub const SCHEMA_VERSION: u32 = 5;

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

/// By its slug, for the reason a [`Kind`] is: the word the database and a
/// report already hold, which a renamed variant cannot change.
impl Serialize for SourceKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

/// A place artifacts have been gathered from.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name, but the row id, and the URL only through [`redacted`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    /// Its row id, which is what gathered artifacts point at.
    ///
    /// Not serialized: it is the ledger's, numbered as rows were written, so
    /// no front end can name a source by it on a command line, and the URL
    /// and the branch already tell one source from another.
    #[serde(skip)]
    pub id: i64,
    pub kind: SourceKind,
    /// The URL as the user typed it, without the credentials [`redacted`]
    /// takes out.
    #[serde(serialize_with = "crate::url::serialize_redacted")]
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
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// Following a project the workspace links to where it moved.
    Relink,
    /// Bringing a skill somebody kept in a project into the library, its
    /// copies recorded as installed from there.
    Adopt,
}

impl Action {
    pub const fn slug(self) -> &'static str {
        match self {
            Action::Gather => "gather",
            Action::Install => "install",
            Action::Uninstall => "uninstall",
            Action::Relink => "relink",
            Action::Adopt => "adopt",
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
    /// ([`crate::install::Offer::origin`]), without the credentials
    /// [`redacted`] takes out. `None` only in a record from before schema 3
    /// of something that came from no source.
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
    /// What this copy holds, when it is not what the installation's
    /// `content_hash` names: a rule's section in `AGENTS.md` and its file
    /// for Claude hold different bytes, each with a hash of its own. `None`
    /// for a skill's copies, every one of which holds the tree the
    /// installation's hash names.
    pub content_hash: Option<String>,
}

/// Where one installation's copies are once its project has moved from a
/// `mind.toml` to its settings ([`Ledger::migrated`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub kind: Kind,
    pub name: String,
    /// Every copy it has now, in place of those it recorded; none forgets it.
    pub targets: Vec<Target>,
    /// What every one of them holds ([`crate::hash::tree`]), for a record
    /// from before hashes were kept, which the migration found current by
    /// its offer: kept only where the record has none.
    pub content_hash: Option<String>,
}

/// What [`Ledger::uninstalled_copy`] forgot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgotten {
    /// No installation of that name records a copy there.
    Nothing,
    /// The copy, from an installation that still records others.
    Copy,
    /// The installation, whose last copy it was, or which recorded none.
    Installation,
}

/// What [`Ledger::renamed_copy`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    /// No installation of that name records a copy there.
    Nothing,
    /// The copy is recorded under its new name and at its new place.
    Moved,
    /// Another installation holds the new name, so the copy is recorded by
    /// neither, and its installation goes with it when it was the last.
    Forgotten,
}

/// One line of the action log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it happened, in seconds since the Unix epoch.
    pub at: i64,
    pub action: String,
    /// What the action was about: a URL, a name. A URL is [`redacted`].
    pub target: Option<String>,
    pub outcome: String,
    /// What it said, successful or not, with every URL in it [`redacted`].
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
        Self::open_knowing(path, Vec::new)
    }

    /// [`open`](Self::open), with `known` asked, only when a step to schema 5
    /// is taken, for the id each project it may hold records of holds and
    /// the route the ledger names it by: what a workspace's linked projects
    /// say ([`crate::workspace::FlayerWorkspace::ledger`]), which no SQL in
    /// the file can know.
    pub fn open_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Self, LedgerError> {
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
        // A connection that finds another one writing waits for it, as long
        // as a writer waits for the workspace, rather than failing with
        // "database is locked": the CLI, the TUI and the server can each have
        // the ledger open. rusqlite waits five seconds of its own accord, but
        // as a library's default; set here, the wait is Mindflayer's.
        connection
            .busy_timeout(crate::lock::WAIT)
            .map_err(|source| LedgerError::Open {
                path: path.clone(),
                source,
            })?;
        let ledger = Self { connection, path };
        ledger.migrate(known)?;
        // And write-ahead logging, so that readers and a writer do not wait
        // for each other at all: a reader goes on reading the ledger as the
        // last write left it while the next is made. It needs every process
        // using the file on one machine, sharing memory through
        // `mindflayer.db-shm`, so a workspace on a network filesystem used
        // from two machines at once is beyond it, as it is beyond the
        // workspace lock.
        //
        // Only once `migrate` has accepted the version: the mode is kept in
        // the file's header, so switching first would change a ledger a newer
        // build wrote before refusing it. And never a reason to refuse the
        // ledger, since every mode keeps it whole: a filesystem that cannot
        // keep the log stays in the journal it had, where readers and writers
        // take turns instead, and a switch that finds another connection
        // writing fails at once, without the busy wait, because waiting there
        // could deadlock with it. That one is left to the next open.
        let _ = ledger
            .connection
            .pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()));
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
        Self::open_existing_knowing(path, Vec::new)
    }

    /// [`open_existing`](Self::open_existing), with `known` asked as
    /// [`open_knowing`](Self::open_knowing) asks it.
    pub fn open_existing_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Option<Self>, LedgerError> {
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
        // Waiting for a writer as `open` does, but in whatever journal the
        // file keeps: changing it is a write, and a read makes none but a
        // migration.
        connection
            .busy_timeout(crate::lock::WAIT)
            .map_err(|source| LedgerError::Open {
                path: path.clone(),
                source,
            })?;
        let ledger = Self { connection, path };
        match ledger.migrate(known) {
            Ok(()) => Ok(Some(ledger)),
            Err(error) if cannot_make_the_log(&ledger.path, &error) => {
                Self::open_immutable(&ledger.path).map(Some).ok_or(error)
            }
            Err(error) => Err(error),
        }
    }

    /// A ledger that keeps a write-ahead log, read where the log's files
    /// cannot be made, as a file nobody is changing.
    ///
    /// SQLite makes `mindflayer.db-wal` and `mindflayer.db-shm` to read a
    /// ledger that keeps its log, and takes them away when the last
    /// connection closes. So in a folder this process may read and not write
    /// (a sandboxed agent's view of the workspace, a read-only mount,
    /// somebody else's workspace), reading fails though nothing is to be
    /// written, where the journal the ledger had before let it be read.
    /// Called only when there is no log, which is when everything is in the
    /// file itself, and SQLite's `immutable` then reads it without the log or
    /// any lock. What somebody who can write there writes goes into a log
    /// this does not see, so this reads the ledger as their last checkpoint
    /// left it; a checkpoint made between two of its reads is the one thing
    /// that could change the file under it.
    ///
    /// `None` when it cannot be opened that way either, or migrating it would
    /// need a write, and the error that sent it here is the one to tell.
    fn open_immutable(path: &Path) -> Option<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(immutable_uri(path)?, flags).ok()?;
        let ledger = Self {
            connection,
            path: path.to_path_buf(),
        };
        ledger.migrate(Vec::new).ok()?;
        Some(ledger)
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
        ledger.migrate(Vec::new)?;
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
    ///
    /// `known` is asked only by the step to schema 5, for each project's id
    /// and route ([`open_knowing`](Self::open_knowing)), and not at all by a
    /// ledger already there, so opening a current one reads no project.
    fn migrate(&self, known: impl FnOnce() -> Vec<(ProjectId, String)>) -> Result<(), LedgerError> {
        self.connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|source| self.failed("enabling foreign keys", source))?;
        // Zeroes what a row leaves behind when it is rewritten or deleted.
        // Without it a URL the scrub rewrites would keep its credentials in
        // the file's free space, readable by anything that reads the bytes
        // rather than the rows, until SQLite happened to reuse the page.
        self.connection
            .pragma_update(None, "secure_delete", true)
            .map_err(|source| self.failed("turning on secure delete", source))?;

        // Read before taking the write lock, so that opening a database that
        // is already current (every open but the first after an upgrade)
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
        if found < 4 {
            transaction.execute_batch(SCHEMA_V4).map_err(|source| {
                self.failed("giving each copy a hash and noting the files made", source)
            })?;
        }
        if found < 5 {
            let failed = |source| self.failed("naming each project by its id", source);
            transaction.execute_batch(SCHEMA_V5).map_err(failed)?;
            // What no SQL in the file can know: which id the project at a
            // route holds. Each is taken once, the first for an id or a
            // route two of them claim, which the caller leaves out anyway,
            // since two projects holding one id are a copy to tell apart,
            // not a record to fill in.
            for (id, route) in known() {
                let taken = transaction
                    .execute(
                        "INSERT OR IGNORE INTO projects (id, last_route) VALUES (?1, ?2)",
                        rusqlite::params![id.as_str(), route],
                    )
                    .map_err(failed)?;
                if taken > 0 {
                    transaction
                        .execute(
                            "UPDATE installations SET project_id = ?1 WHERE project = ?2",
                            rusqlite::params![id.as_str(), route],
                        )
                        .map_err(failed)?;
                }
            }
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
        let url = redacted(url);
        self.connection
            .query_row(
                "SELECT id, kind, url, reference, subdirectory, revision, directory
                   FROM sources
                  WHERE kind = ?1 AND url = ?2 AND subdirectory = ?3
                    AND reference IS ?4",
                rusqlite::params![kind.slug(), url.as_ref(), subdirectory, reference],
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
        let url = redacted(url);
        if let Some(existing) = self.source(kind, &url, reference, subdirectory)? {
            return Ok(existing);
        }

        let directory = self.free_directory(&slug(&url))?;
        let at = now();
        self.connection
            .execute(
                "INSERT INTO sources
                     (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6)",
                rusqlite::params![
                    kind.slug(),
                    url.as_ref(),
                    reference,
                    subdirectory,
                    directory,
                    at
                ],
            )
            .map_err(|source| self.failed("registering a source", source))?;

        Ok(Source {
            id: self.connection.last_insert_rowid(),
            kind,
            url: url.into_owned(),
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

    /// Forget a source, and with it what it brought; an installation from it
    /// keeps its `origin` but no longer points at it (see the schema's notes
    /// on what a forgotten source takes with it). Whether there was one to
    /// forget.
    ///
    /// For a gather that registered a source and then was cancelled, which
    /// leaves the ledger as it found it: a source nothing came from would
    /// otherwise hold on to its folder name. Its files are the caller's.
    pub(crate) fn forget_source(&self, id: i64) -> Result<bool, LedgerError> {
        self.connection
            .execute("DELETE FROM sources WHERE id = ?1", [id])
            .map(|deleted| deleted > 0)
            .map_err(|source| self.failed("forgetting a source", source))
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
                    url: redacted(&row.url).into_owned(),
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
        // An origin spells a source's URL, so it goes in as every URL does.
        let origin = installation.origin.as_deref().map(redacted);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let id: i64 = transaction
            .query_row(
                "INSERT INTO installations
                     (project, kind, name, source_id, origin, content_hash, installed_at,
                      project_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                         (SELECT id FROM projects WHERE last_route = ?1))
                 ON CONFLICT (project, kind, name) DO UPDATE SET
                     source_id = excluded.source_id,
                     origin = excluded.origin,
                     content_hash = excluded.content_hash,
                     installed_at = excluded.installed_at,
                     project_id = excluded.project_id
                 RETURNING id",
                rusqlite::params![
                    project,
                    installation.kind.slug(),
                    installation.name,
                    installation.source_id,
                    origin.as_deref(),
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
        insert_targets(&transaction, id, &installation.targets).map_err(failed)?;
        transaction.commit().map_err(failed)
    }

    /// Record what a render of a project's `kind` leaves it: each of
    /// `records` with its hash and copies in place of what was recorded of
    /// it, every other installation of `kind` in the project forgotten, and
    /// for each of `files`, a route from the project, whether Mindflayer made
    /// it ([`Ledger::set_created`]).
    ///
    /// For a kind the project is given as one render of all of it, as its
    /// rules are a block of one file: what is recorded of each is what that
    /// render wrote, so the whole is written at once, in one transaction, and
    /// a render's record is never half there. An installation keeps the time
    /// it was first recorded at, and its origin is taken in as every URL is.
    pub fn rendered(
        &self,
        project: &str,
        kind: Kind,
        records: &[InstallRecord],
        files: &[(&str, bool)],
    ) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording what a render wrote", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let recorded: Vec<(i64, String)> = {
            let mut statement = transaction
                .prepare("SELECT id, name FROM installations WHERE project = ?1 AND kind = ?2")
                .map_err(failed)?;
            let rows = statement
                .query_map(rusqlite::params![project, kind.slug()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };
        for (id, name) in recorded {
            if !records.iter().any(|record| record.name == name) {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
            }
        }
        for record in records {
            let origin = record.origin.as_deref().map(redacted);
            let id: i64 = transaction
                .query_row(
                    "INSERT INTO installations
                         (project, kind, name, source_id, origin, content_hash, installed_at,
                          project_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                             (SELECT id FROM projects WHERE last_route = ?1))
                     ON CONFLICT (project, kind, name) DO UPDATE SET
                         source_id = excluded.source_id,
                         origin = excluded.origin,
                         content_hash = excluded.content_hash,
                         project_id = excluded.project_id
                     RETURNING id",
                    rusqlite::params![
                        project,
                        kind.slug(),
                        record.name,
                        record.source_id,
                        origin.as_deref(),
                        record.content_hash,
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
            insert_targets(&transaction, id, &record.targets).map_err(failed)?;
        }
        for (path, created) in files {
            set_created_in(&transaction, project, path, *created).map_err(failed)?;
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
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get(3)?,
                    ))
                },
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(None);
        };
        // And comes out as every URL does, for one schema 3 filled in from a
        // source's URL as an older build stored it, until the scrub rewrites it.
        let origin = origin.map(|text| redacted(&text).into_owned());

        let mut statement = self
            .connection
            .prepare(
                "SELECT agent, path, content_hash FROM installation_targets
                  WHERE installation_id = ?1 ORDER BY path",
            )
            .map_err(failed)?;
        let targets = statement
            .query_map([id], |row| {
                Ok(Target {
                    agent: row.get(0)?,
                    path: row.get(1)?,
                    content_hash: row.get(2)?,
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

    /// Forget the copy an installation of `name` wrote at `path`, and the
    /// installation once it records no other, saying which went.
    ///
    /// For a change that reached one copy: the others are still what was
    /// installed, and forgetting the whole installation would leave them with
    /// no record, never updated or removed again. A skill's installation
    /// recording no copy at all, which only a build from before installations
    /// had targets wrote, is forgotten whole, as [`Ledger::uninstalled`]
    /// would. A rule's is one a build writes, for a rule that reaches no
    /// agent the project names (`install::rules`), never one from before
    /// targets, since rules were installed only after: a change at some other
    /// path leaves it what was installed, and it stays. One transaction, so
    /// an install of the same name at the same moment never finds half of
    /// this done.
    pub fn uninstalled_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
    ) -> Result<Forgotten, LedgerError> {
        let failed = |source| self.failed("forgetting an installed copy", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let Some(id) = transaction
            .query_row(
                "SELECT id FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(Forgotten::Nothing);
        };
        let copy = transaction
            .execute(
                "DELETE FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
            )
            .map_err(failed)?;
        let left: i64 = transaction
            .query_row(
                "SELECT count(*) FROM installation_targets WHERE installation_id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(failed)?;
        let forgotten = if left == 0 && (copy > 0 || kind != Kind::Rule) {
            transaction
                .execute("DELETE FROM installations WHERE id = ?1", [id])
                .map_err(failed)?;
            Forgotten::Installation
        } else if copy > 0 {
            Forgotten::Copy
        } else {
            Forgotten::Nothing
        };
        transaction.commit().map_err(failed)?;
        Ok(forgotten)
    }

    /// Record that the copy an installation of `name` wrote at `path` is
    /// called `to_name` now, at `to_path`, saying what became of its record.
    ///
    /// For a rename that reached one copy: it is what was installed, renamed,
    /// and stays Mindflayer's to tell apart (D-04), as an edit does, from
    /// where it came, with the hash of what was written. An installation it
    /// was the last copy of takes the new name whole, keeping when it was
    /// installed; one that records others keeps them under the old name,
    /// still what was installed, and the copy is recorded under the new one
    /// as an installation of its own from the same place. A name is one
    /// installation a project, so where another holds the new name already
    /// the copy is forgotten instead, as [`Ledger::uninstalled_copy`] would.
    /// A skill's installation recording no copy at all, which only a build
    /// from before installations had targets wrote, is forgotten whole as
    /// there. One transaction, so an install of either name at the same
    /// moment never finds half of this done.
    pub fn renamed_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
        to_name: &str,
        to_path: &str,
    ) -> Result<Carried, LedgerError> {
        let failed = |source| self.failed("recording a renamed copy", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let Some(id) = transaction
            .query_row(
                "SELECT id FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(Carried::Nothing);
        };
        let count = |transaction: &Transaction<'_>| -> rusqlite::Result<i64> {
            transaction.query_row(
                "SELECT count(*) FROM installation_targets WHERE installation_id = ?1",
                [id],
                |row| row.get(0),
            )
        };
        let copy: Option<(Option<String>, Option<String>)> = transaction
            .query_row(
                "SELECT agent, content_hash FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(failed)?;
        let Some((agent, held)) = copy else {
            if kind != Kind::Rule && count(&transaction).map_err(failed)? == 0 {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
                transaction.commit().map_err(failed)?;
                return Ok(Carried::Forgotten);
            }
            return Ok(Carried::Nothing);
        };
        transaction
            .execute(
                "DELETE FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
            )
            .map_err(failed)?;
        let last = count(&transaction).map_err(failed)? == 0;
        let taken = transaction
            .query_row(
                "SELECT 1 FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), to_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(failed)?
            .is_some();
        let carried = if taken {
            if last {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
            }
            Carried::Forgotten
        } else {
            let to = if last {
                transaction
                    .execute(
                        "UPDATE installations SET name = ?1 WHERE id = ?2",
                        rusqlite::params![to_name, id],
                    )
                    .map_err(failed)?;
                id
            } else {
                transaction
                    .query_row(
                        "INSERT INTO installations
                             (project, kind, name, source_id, origin, content_hash, installed_at,
                              project_id)
                         SELECT project, kind, ?1, source_id, origin, content_hash, installed_at,
                                project_id
                           FROM installations WHERE id = ?2
                         RETURNING id",
                        rusqlite::params![to_name, id],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(failed)?
            };
            transaction
                .execute(
                    "INSERT INTO installation_targets (installation_id, agent, path, content_hash)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![to, agent, to_path, held],
                )
                .map_err(failed)?;
            Carried::Moved
        };
        transaction.commit().map_err(failed)?;
        Ok(carried)
    }

    /// Record that the installation of `name` has its copies at `targets`
    /// now, in place of those it recorded, and forget it when `targets` is
    /// empty. Whether there was one.
    ///
    /// For copies that moved without the installation changing: a project
    /// that names another agent is given a copy where that agent reads, and
    /// one that stops naming an agent loses the copy there, while what was
    /// installed, from where, its hash and when stay what they were. Writing
    /// the whole record again ([`Ledger::installed`]) would say it was
    /// installed just now. One transaction, so an install of the same name
    /// at the same moment never finds half of this done.
    pub fn retargeted(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        targets: &[Target],
    ) -> Result<bool, LedgerError> {
        let failed = |source| self.failed("recording where an installation's copies are", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        if retarget_in(&transaction, project, kind, name, targets)
            .map_err(failed)?
            .is_none()
        {
            return Ok(false);
        }
        transaction.commit().map_err(failed)?;
        Ok(true)
    }

    /// Record where each of a project's installations has its copies once
    /// the project has moved from a `mind.toml` to its settings, a hash for
    /// each record that had none and was found current, the `id` its
    /// settings hold now, as [`Ledger::knew`] records one, and `log`, each
    /// line a log entry about the project.
    ///
    /// All of it in one transaction, where [`Ledger::retargeted`] takes one
    /// an installation: a migration is all or nothing for its project, and
    /// one that fails here puts the project's `mind.toml` back, so a ledger
    /// left with some installations moved would name copies where that
    /// marker installs none. An installation it does not record any more is
    /// passed over, as `retargeted` finds nothing to do for one.
    pub fn migrated(
        &self,
        project: &str,
        id: &ProjectId,
        moved: &[Moved],
        log: &[(Action, String)],
    ) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording a project's move to its settings", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        know_in(&transaction, id, project, &[]).map_err(failed)?;
        for one in moved {
            let id = retarget_in(&transaction, project, one.kind, &one.name, &one.targets)
                .map_err(failed)?;
            if let (Some(id), Some(hash)) = (id, &one.content_hash) {
                transaction
                    .execute(
                        "UPDATE installations SET content_hash = ?1
                          WHERE id = ?2 AND content_hash IS NULL",
                        rusqlite::params![hash, id],
                    )
                    .map_err(failed)?;
            }
        }
        for (action, detail) in log {
            insert_log(
                &transaction,
                *action,
                Some(project),
                Outcome::Ok,
                Some(detail),
            )
            .map_err(failed)?;
        }
        transaction.commit().map_err(failed)
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

    /// Every installation into a project of `kind`, each read as
    /// [`Ledger::installation`] reads one, in name order.
    ///
    /// For a report on everything a project was given, which has to name
    /// what was installed even where no copy of it is left to find, and asks
    /// once rather than once a name.
    pub fn records(&self, project: &str, kind: Kind) -> Result<Vec<InstallRecord>, LedgerError> {
        let failed = |source| self.failed("reading the installations", source);
        let mut statement = self
            .connection
            .prepare(
                "SELECT installations.id, installations.name, installations.source_id,
                        installations.origin, installations.content_hash,
                        installation_targets.agent, installation_targets.path,
                        installation_targets.content_hash
                   FROM installations
                   LEFT JOIN installation_targets
                     ON installation_targets.installation_id = installations.id
                  WHERE installations.project = ?1 AND installations.kind = ?2
                  ORDER BY installations.name, installation_targets.path",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(failed)?;
        let mut records: Vec<(i64, InstallRecord)> = Vec::new();
        for row in rows {
            let (id, name, source_id, origin, content_hash, agent, path, held) =
                row.map_err(failed)?;
            // One row a copy, an installation's together, since a name is
            // one installation a project and the rows come in name order.
            if records.last().is_none_or(|(last, _)| *last != id) {
                records.push((
                    id,
                    InstallRecord {
                        kind,
                        name,
                        source_id,
                        // Out as every URL comes out, as `installation` has it.
                        origin: origin.map(|text| redacted(&text).into_owned()),
                        content_hash,
                        targets: Vec::new(),
                    },
                ));
            }
            // None for an installation recording no copy, which the join
            // still gives a row.
            if let (Some(path), Some((_, record))) = (path, records.last_mut()) {
                record.targets.push(Target {
                    agent,
                    path,
                    content_hash: held,
                });
            }
        }
        Ok(records.into_iter().map(|(_, record)| record).collect())
    }

    /// Every copy this ledger says Mindflayer wrote into a project for
    /// `kind`, whichever installation wrote it, by path relative to the
    /// project, `/` separated, in path order, each once: the rules of a
    /// project share its `AGENTS.md`, a section each.
    ///
    /// By copy rather than by name, for a listing that has to say which of
    /// the folders holding a skill hold Mindflayer's copy: a name alone
    /// would claim a copy somebody keeps in one agent's folder because
    /// Mindflayer installed one of that name into another's. One query
    /// rather than one per installation, since a listing asks it of every
    /// project.
    pub fn copies(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError> {
        let failed = |source| self.failed("reading the copies installed", source);
        let mut statement = self
            .connection
            .prepare(
                "SELECT DISTINCT installation_targets.path
                   FROM installation_targets
                   JOIN installations
                     ON installations.id = installation_targets.installation_id
                  WHERE installations.project = ?1 AND installations.kind = ?2
                  ORDER BY installation_targets.path",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| row.get(0))
            .map_err(failed)?;
        rows.collect::<Result<Vec<String>, _>>().map_err(failed)
    }

    /// Whether Mindflayer made the file at `path`, a route from the project,
    /// rather than wrote into one that was there: what a render was told
    /// when it made it ([`crate::agents_md::Written::Created`]), kept until
    /// the file stops being Mindflayer's.
    pub fn created(&self, project: &str, path: &str) -> Result<bool, LedgerError> {
        self.connection
            .query_row(
                "SELECT 1 FROM created_files WHERE project = ?1 AND path = ?2",
                rusqlite::params![project, path],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(|source| self.failed("reading which files Mindflayer made", source))
    }

    /// Record whether Mindflayer made the file at `path`, a route from the
    /// project: kept when it did, forgotten when it did not or the file has
    /// stopped being Mindflayer's. A file noted twice keeps the time it was
    /// first noted.
    pub fn set_created(&self, project: &str, path: &str, created: bool) -> Result<(), LedgerError> {
        set_created_in(&self.connection, project, path, created)
            .map_err(|source| self.failed("noting which files Mindflayer made", source))
    }

    /// The id the ledger last saw the project at `route` hold, if it saw one
    /// there: a route from the workspace, spelled as a record spells its
    /// project.
    ///
    /// An id somebody edited into a form no project holds is no id, rather
    /// than a failure: nothing can hold it, so nothing is found by it.
    pub fn project_at(&self, route: &str) -> Result<Option<ProjectId>, LedgerError> {
        self.connection
            .query_row(
                "SELECT id FROM projects WHERE last_route = ?1",
                [route],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|id| id.as_deref().and_then(ProjectId::parse))
            .map_err(|source| self.failed("reading which project a route holds", source))
    }

    /// Every id the ledger knows, with the route it last saw it at, in route
    /// order: what a scan matches the projects it finds against.
    pub fn known_projects(&self) -> Result<Vec<(ProjectId, String)>, LedgerError> {
        let failed = |source| self.failed("reading the projects known by their id", source);
        let mut statement = self
            .connection
            .prepare("SELECT id, last_route FROM projects ORDER BY last_route")
            .map_err(failed)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed)?;
        let mut known = Vec::new();
        for row in rows {
            let (id, route) = row.map_err(failed)?;
            if let Some(id) = ProjectId::parse(&id) {
                known.push((id, route));
            }
        }
        Ok(known)
    }

    /// Whether anything is recorded of the project at `route`: an
    /// installation, or a file Mindflayer made there. What a project the
    /// ledger knows no id of has to hold nothing of for its entry to be
    /// handed to another directory ([`Ledger::relinked`]), since nothing
    /// says which project those records were of.
    pub fn holds(&self, route: &str) -> Result<bool, LedgerError> {
        self.connection
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM installations WHERE project = ?1)
                     OR EXISTS (SELECT 1 FROM created_files WHERE project = ?1)",
                [route],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|source| self.failed("reading what is recorded of a project", source))
    }

    /// Record that each project of `seen` holds its id at its route, and
    /// say how many of them the ledger did not know there already.
    ///
    /// For what a workspace's projects say now, read whenever it is opened
    /// to write ([`crate::workspace::FlayerWorkspace::ledger_to_write`]): the
    /// ledger follows them, so it knows each linked project by the id it
    /// holds. An id it last saw at another route moves here, unless that
    /// route is one of `stale`, the entries the workspace links that open
    /// no project any more: an id seen there is what finds that entry's
    /// project once it has moved, and only `flayer relink` takes it from
    /// there ([`Ledger::relinked`]). Whatever id the ledger knew at the
    /// route goes, as the project there holds another now, and the records
    /// there are the new one's: they name copies in the directory it holds.
    ///
    /// One transaction, taken before anything is read, so a relink at the
    /// same moment comes wholly before this or wholly after it.
    pub fn knew(
        &self,
        seen: &[(ProjectId, String)],
        stale: &[String],
    ) -> Result<usize, LedgerError> {
        let failed = |source| self.failed("recording which project each route holds", source);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(failed)?;
        let mut learned = 0;
        for (id, route) in seen {
            if know_in(&transaction, id, route, stale).map_err(failed)? {
                learned += 1;
            }
        }
        transaction.commit().map_err(failed)?;
        Ok(learned)
    }

    /// Record that the project the ledger knew at the route `old` is at
    /// `new` now, holding `id`: every installation and file made recorded
    /// there moves with it, and `id` is known there. `write`, the change to
    /// the workspace's marker that goes with it, runs inside the same
    /// transaction, which is kept only when it succeeds.
    ///
    /// So the entry and the records move together or not at all: an entry
    /// moved without them would leave them under a route no entry names,
    /// and records moved without it would name a directory the workspace
    /// does not link. What was recorded at `new` of some other project,
    /// one linked there once and unlinked, goes first: it names copies in a
    /// directory that holds `id`'s project now, and the records moving in
    /// would collide with it, name for name.
    ///
    /// Which project `old` was, and whether `new` holds it, is the caller's
    /// to have checked ([`crate::relink`]); this only moves the rows. Taken
    /// before anything is read, as [`Ledger::knew`] is, and logged. Hands
    /// back what `write` did and how many installations moved, or what it
    /// failed with, nothing moved.
    pub fn relinked<T, E>(
        &self,
        old: &str,
        new: &str,
        id: &ProjectId,
        write: impl FnOnce() -> Result<T, E>,
    ) -> Result<Result<(T, usize), E>, LedgerError> {
        let failed = |source| self.failed("moving a project's records to where it is", source);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM installations WHERE project = ?1 AND project_id IS NOT ?2",
                rusqlite::params![new, id.as_str()],
            )
            .map_err(failed)?;
        transaction
            .execute("DELETE FROM created_files WHERE project = ?1", [new])
            .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM projects WHERE last_route = ?1 AND id != ?2",
                rusqlite::params![new, id.as_str()],
            )
            .map_err(failed)?;
        // Known before the records name it, which they may only once it is.
        transaction
            .execute(
                "INSERT INTO projects (id, last_route) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET last_route = excluded.last_route",
                rusqlite::params![id.as_str(), new],
            )
            .map_err(failed)?;
        let moved = transaction
            .execute(
                "UPDATE installations SET project = ?2, project_id = ?3 WHERE project = ?1",
                rusqlite::params![old, new, id.as_str()],
            )
            .map_err(failed)?;
        transaction
            .execute(
                "UPDATE created_files SET project = ?2 WHERE project = ?1",
                rusqlite::params![old, new],
            )
            .map_err(failed)?;
        insert_log(
            &transaction,
            Action::Relink,
            Some(new),
            Outcome::Ok,
            Some(&format!("{old} to {new}")),
        )
        .map_err(failed)?;
        let written = match write() {
            Ok(written) => written,
            // Dropped, so nothing above is kept.
            Err(error) => return Ok(Err(error)),
        };
        transaction.commit().map_err(failed)?;
        Ok(Ok((written, moved)))
    }

    /// The sources whose stored URL still carries credentials, which only a
    /// Mindflayer from before they were refused could have stored.
    ///
    /// Each is handed back with the URL [`Ledger::scrub_source`] will give it,
    /// never the one it has: nothing the ledger returns carries a credential.
    pub fn tainted_sources(&self) -> Result<Vec<Source>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, kind, url, reference, subdirectory, revision, directory
                   FROM sources WHERE instr(url, '@') > 0 ORDER BY id",
            )
            .map_err(|source| self.failed("looking for credentials in sources", source))?;
        let rows = statement
            .query_map([], |row| {
                let stored: String = row.get(2)?;
                Ok((carries_credentials(&stored), read_source(row)?))
            })
            .map_err(|source| self.failed("looking for credentials in sources", source))?;

        let mut tainted = Vec::new();
        for row in rows {
            let (carries, source) =
                row.map_err(|source| self.failed("looking for credentials in sources", source))?;
            if carries {
                tainted.push(source);
            }
        }
        Ok(tainted)
    }

    /// Store a source's URL without the credentials in it, handing back the
    /// row that went if that made two rows one.
    ///
    /// When another source already holds that URL, with the same branch and
    /// folder, the two are one place gathered twice, so they become one row
    /// rather than two with the same address: two rows would make the shelf
    /// offer each skill twice under one origin, and `--from` could not tell
    /// them apart. The one whose shelf was filled most recently is kept, since
    /// its folder holds the newer files; a row whose gather failed before it
    /// filled anything loses to one that did, however recently it was tried.
    /// What the other one gathered goes with it, rather than moving to the
    /// kept row: its files are in the other's folder, and a record pointing
    /// there would outlive the folder's owner, so the next source given that
    /// folder would gather over files the kept row still claims. An
    /// installation that pointed at the other points at the kept one, which
    /// is the same place. The row that went is handed back for its folders to
    /// go too, which nothing points into any more.
    ///
    /// An installation's origin is rewritten with the URL it spells: schema 3
    /// filled it in from the URL as it was stored, credentials and all, and
    /// nothing else would take them out of it.
    pub fn scrub_source(&self, id: i64) -> Result<Option<Source>, LedgerError> {
        let failed = |source| self.failed("rewriting a source without credentials", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;

        let Some((kind, stored, reference, subdirectory)) = transaction
            .query_row(
                "SELECT kind, url, reference, subdirectory FROM sources WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(None);
        };
        let clean = redacted(&stored);
        if clean == stored {
            return Ok(None);
        }

        let other: Option<i64> = transaction
            .query_row(
                "SELECT id FROM sources
                  WHERE kind = ?1 AND url = ?2 AND subdirectory = ?3 AND reference IS ?4
                    AND id != ?5
                  ORDER BY last_seen DESC, id DESC LIMIT 1",
                rusqlite::params![kind, clean.as_ref(), subdirectory, reference, id],
                |row| row.get(0),
            )
            .optional()
            .map_err(failed)?;

        let (kept, dropped) = match other {
            None => (id, None),
            Some(other) => {
                // Newest harvest first, `None` for a shelf never filled, then
                // the last attempt, then the later row.
                let freshness = |source: i64| {
                    transaction.query_row(
                        "SELECT (SELECT MAX(gathered_at) FROM artifacts WHERE source_id = ?1),
                                last_seen, id
                           FROM sources WHERE id = ?1",
                        [source],
                        |row| {
                            Ok((
                                row.get::<_, Option<i64>>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )
                };
                let (kept, dropped) =
                    if freshness(id).map_err(failed)? >= freshness(other).map_err(failed)? {
                        (id, other)
                    } else {
                        (other, id)
                    };
                let gone = transaction
                    .query_row(
                        "SELECT id, kind, url, reference, subdirectory, revision, directory
                           FROM sources WHERE id = ?1",
                        [dropped],
                        read_source,
                    )
                    .map_err(failed)?;
                transaction
                    .execute(
                        "UPDATE installations SET source_id = ?1 WHERE source_id = ?2",
                        [kept, dropped],
                    )
                    .map_err(failed)?;
                // What it gathered goes with it, by the cascade.
                transaction
                    .execute("DELETE FROM sources WHERE id = ?1", [dropped])
                    .map_err(failed)?;
                (kept, Some(gone))
            }
        };
        let origins: Vec<(i64, String)> = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, origin FROM installations
                      WHERE source_id = ?1 AND instr(origin, '@') > 0",
                )
                .map_err(failed)?;
            let rows = statement
                .query_map([kept], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };
        for (installation, origin) in origins {
            let clean = redacted(&origin);
            if clean != origin {
                transaction
                    .execute(
                        "UPDATE installations SET origin = ?2 WHERE id = ?1",
                        rusqlite::params![installation, clean.as_ref()],
                    )
                    .map_err(failed)?;
            }
        }
        transaction
            .execute(
                "UPDATE sources SET url = ?2 WHERE id = ?1",
                rusqlite::params![kept, clean.as_ref()],
            )
            .map_err(failed)?;
        transaction.commit().map_err(failed)?;
        Ok(dropped)
    }

    /// Rewrite every action log entry without the credentials of any URL in
    /// it, returning how many there were.
    pub fn scrub_log(&self) -> Result<usize, LedgerError> {
        let failed = |source| self.failed("rewriting the action log without credentials", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;

        let rows: Vec<(i64, Option<String>, Option<String>)> = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, target, detail FROM actions
                      WHERE instr(target, '@') > 0 OR instr(detail, '@') > 0",
                )
                .map_err(failed)?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };

        let mut scrubbed = 0;
        for (id, target, detail) in rows {
            let clean_target = target.as_deref().map(redacted);
            let clean_detail = detail.as_deref().map(redacted);
            if clean_target.as_deref() == target.as_deref()
                && clean_detail.as_deref() == detail.as_deref()
            {
                continue;
            }
            transaction
                .execute(
                    "UPDATE actions SET target = ?2, detail = ?3 WHERE id = ?1",
                    rusqlite::params![id, clean_target.as_deref(), clean_detail.as_deref()],
                )
                .map_err(failed)?;
            scrubbed += 1;
        }
        transaction.commit().map_err(failed)?;
        Ok(scrubbed)
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
        insert_log(&self.connection, action, target, outcome, detail)
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
                let target: Option<String> = row.get(2)?;
                let detail: Option<String> = row.get(4)?;
                Ok(Entry {
                    at: row.get(0)?,
                    action: row.get(1)?,
                    target: target.map(|text| redacted(&text).into_owned()),
                    outcome: row.get(3)?,
                    detail: detail.map(|text| redacted(&text).into_owned()),
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

/// [`Ledger::knew`] of one project, through `connection`, which a caller
/// holds a transaction on: whether the ledger did not know `id` at `route`
/// already.
fn know_in(
    connection: &Connection,
    id: &ProjectId,
    route: &str,
    stale: &[String],
) -> rusqlite::Result<bool> {
    let at: Option<String> = connection
        .query_row(
            "SELECT last_route FROM projects WHERE id = ?1",
            [id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    match at {
        Some(at) if at == route || stale.contains(&at) => return Ok(false),
        Some(_) => {
            connection.execute("DELETE FROM projects WHERE id = ?1", [id.as_str()])?;
        }
        None => {}
    }
    connection.execute("DELETE FROM projects WHERE last_route = ?1", [route])?;
    connection.execute(
        "INSERT INTO projects (id, last_route) VALUES (?1, ?2)",
        rusqlite::params![id.as_str(), route],
    )?;
    connection.execute(
        "UPDATE installations SET project_id = ?1 WHERE project = ?2",
        rusqlite::params![id.as_str(), route],
    )?;
    Ok(true)
}

/// Give the installation of `name` the copies `targets` in place of those
/// it records, or forget it when `targets` is empty, through `connection`,
/// which a caller holds a transaction on: its id, or `None` when nothing
/// records it.
fn retarget_in(
    connection: &Connection,
    project: &str,
    kind: Kind,
    name: &str,
    targets: &[Target],
) -> rusqlite::Result<Option<i64>> {
    let Some(id) = connection
        .query_row(
            "SELECT id FROM installations
              WHERE project = ?1 AND kind = ?2 AND name = ?3",
            rusqlite::params![project, kind.slug(), name],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    if targets.is_empty() {
        connection.execute("DELETE FROM installations WHERE id = ?1", [id])?;
        return Ok(Some(id));
    }
    connection.execute(
        "DELETE FROM installation_targets WHERE installation_id = ?1",
        [id],
    )?;
    insert_targets(connection, id, targets)?;
    Ok(Some(id))
}

/// Record `targets` as the copies of the installation `id`, through
/// `connection`, which a caller holds a transaction on, and which holds none
/// of them yet.
fn insert_targets(connection: &Connection, id: i64, targets: &[Target]) -> rusqlite::Result<()> {
    for target in targets {
        connection.execute(
            "INSERT INTO installation_targets (installation_id, agent, path, content_hash)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, target.agent, target.path, target.content_hash],
        )?;
    }
    Ok(())
}

/// [`Ledger::set_created`], through `connection`, which may hold a
/// transaction a caller keeps it in.
fn set_created_in(
    connection: &Connection,
    project: &str,
    path: &str,
    created: bool,
) -> rusqlite::Result<()> {
    if created {
        connection.execute(
            "INSERT INTO created_files (project, path, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (project, path) DO NOTHING",
            rusqlite::params![project, path, now()],
        )?;
    } else {
        connection.execute(
            "DELETE FROM created_files WHERE project = ?1 AND path = ?2",
            rusqlite::params![project, path],
        )?;
    }
    Ok(())
}

/// Append one entry to the action log through `connection`, every URL in
/// what it is about and what it says [`redacted`], as [`Ledger::log`] does.
fn insert_log(
    connection: &Connection,
    action: Action,
    target: Option<&str>,
    outcome: Outcome,
    detail: Option<&str>,
) -> rusqlite::Result<()> {
    let target = target.map(redacted);
    let detail = detail.map(redacted);
    connection
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                now(),
                action.slug(),
                target.as_deref(),
                outcome.slug(),
                detail.as_deref()
            ],
        )
        .map(|_| ())
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
/// Whether `error` is SQLite failing to make the files a write-ahead log
/// needs beside the ledger at `path`, with no log there to read.
fn cannot_make_the_log(path: &Path, error: &LedgerError) -> bool {
    let LedgerError::Query {
        source: rusqlite::Error::SqliteFailure(failure, _),
        ..
    } = error
    else {
        return false;
    };
    let mut log = path.file_name().unwrap_or_default().to_os_string();
    log.push("-wal");
    matches!(
        failure.code,
        rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::CannotOpen
    ) && !path.with_file_name(log).exists()
}

/// `path` as the URI SQLite opens as [`immutable`](Ledger::open_immutable):
/// `None` for one that is not UTF-8, which a URI cannot spell.
///
/// Only what SQLite reads as part of the URI's syntax is escaped (`%`, and
/// `?` and `#`, which would end the path), since it takes everything else
/// as it is. An absolute path goes after an empty authority, `file:///`, so
/// one that starts with two slashes is not read as naming a host; on Windows
/// a slash goes before it too (`file:///C:/...`), which SQLite drops again.
fn immutable_uri(path: &Path) -> Option<String> {
    let mut uri = String::from("file:");
    if path.is_absolute() {
        uri.push_str("//");
        if cfg!(windows) {
            uri.push('/');
        }
    }
    for character in path.to_str()?.chars() {
        match character {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            '#' => uri.push_str("%23"),
            other => uri.push(other),
        }
    }
    uri.push_str("?immutable=1");
    Some(uri)
}

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
    let url: String = row.get(2)?;
    Ok(Source {
        id: row.get(0)?,
        kind,
        url: redacted(&url).into_owned(),
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
    /// The workspace could not be had to scrub clones in, opening its ledger
    /// to write ([`crate::workspace::FlayerWorkspace::ledger_to_write`]).
    #[error(transparent)]
    Lock(#[from] crate::lock::LockError),
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

codes!(LedgerError {
    Self::Create { .. } => "ledger_create_failed",
    Self::Open { .. } => "ledger_open_failed",
    Self::Version { .. } => "ledger_too_new",
    Self::Backup { .. } => "ledger_backup_failed",
    Self::Unmade { .. } => "ledger_unmade",
    Self::Query { .. } => "ledger_query_failed",
} carried {
    Self::Lock(error) => error,
});

impl LedgerError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            LedgerError::Lock(error) => error.path(),
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
/// source's offer at this version (the address, with `#` and the branch when
/// one was asked for), and here rather than through the code that spells it
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

/// The fourth schema: what installing a rule writes (#45).
///
/// A rule reaches a project marked by its settings as a section of the block
/// in its `AGENTS.md` and, for Claude, as a file in
/// `.claude/rules/mindflayer`, which hold different bytes: the section is the
/// rule without its front matter, the file the rule as written. So a target
/// gains a `content_hash` of its own, which is what tells a section or a file
/// edited since from the one written (D-04). A skill's copies leave it empty:
/// every one of them holds the tree the installation's `content_hash` names,
/// which is why version 3 kept one hash an installation.
///
/// `created_files` names each file Mindflayer made in a project, by its
/// route from the project, rather than wrote into: an `AGENTS.md` that was
/// not there until a rule was installed. The block is taken out of a file
/// with the last rule, and only a file Mindflayer made goes with it, so
/// whether it made the file is kept until then. A table of its own rather
/// than a column of an installation's, since one file serves every rule of
/// the project, and outlives each of them but the last.
///
/// Nothing older is filled in: no build before this one installed a rule,
/// and every target it recorded is a skill's.
const SCHEMA_V4: &str = "
ALTER TABLE installation_targets ADD COLUMN content_hash TEXT;

CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
";

/// The fifth schema: a project known by its id, so that one that moves is
/// found again (#57).
///
/// Every record names its project by its route from the workspace, which
/// moving the project changes: its entry goes stale, and its records name a
/// place nothing is at. `projects` keeps the id ([`ProjectId`]) the ledger
/// last saw at each route, one id a route and one route an id, so a project
/// found somewhere else holding an id the ledger last saw at a stale entry
/// is that entry's project, moved, and `flayer relink` takes its records
/// along ([`Ledger::relinked`]). `installations.project_id` says which id
/// each record was made for, the route's when it was written, emptied when
/// the ledger stops knowing that id and moved with it.
///
/// The route stays the key, rather than giving way to the id: a project a
/// `mind.toml` marks has no id, and keeps being found by its route as
/// before, and so does one whose id nothing has recorded yet.
///
/// What a version before it held is filled in by the step, but not from
/// the file, which kept no id: from the projects the workspace links, each
/// opened for the id its settings hold ([`Ledger::open_knowing`]), the one
/// fact no SQL could know. A project a `mind.toml` marks, an entry that
/// would not open, and two projects holding one id get none, and their
/// records keep their route alone.
const SCHEMA_V5: &str = "
CREATE TABLE projects (
    id         TEXT    PRIMARY KEY,
    last_route TEXT    NOT NULL UNIQUE
);

ALTER TABLE installations
    ADD COLUMN project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL;

CREATE INDEX installations_by_project_id ON installations (project_id);
";

#[cfg(test)]
#[path = "ledger_test.rs"]
mod tests;
