//! One entry of a config file on disk: read it, change it, and write the file
//! back whole.
//!
//! The file is named the way a project names its artifact directories: a
//! route of plain folder names from a folder the caller trusts, which no
//! `..` or absolute path can lead out of. Nothing on that route may be a
//! symbolic link. An agent's folder in a project, `.claude/` or `.codex/`, is
//! somebody's to point anywhere, and a gathered repository can hold one that
//! points at the user's own `~/.claude`: an entry written through it would
//! land in a file of the user's, outside the project, that the project's
//! ledger row then claims. Refused rather than followed, as `copy::tree`
//! refuses a link that leaves its source.
//!
//! The caller holds the workspace's lock, as an install does: two writers
//! that both read the file before either writes would each put back only
//! their own entry. Nothing here knows about workspaces, so it takes none.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::{edit_document, get, Edit, EntryError, Format, Value};

/// A config file, found by its route from a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    root: PathBuf,
    route: PathBuf,
    format: Format,
}

/// What [`ConfigFile::edit`] did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// The file was not there, and now holds the entry. Made with
    /// `create_new`, so a file somebody made meanwhile is not written over.
    Created,
    /// The file was replaced whole, through a temporary and a rename, with
    /// its permissions kept.
    Changed,
    /// The entry already held what was asked, so the file was left alone, its
    /// modification time included.
    Unchanged,
}

/// Why a config file could not be read or written.
#[derive(Debug, Error)]
pub enum FileError {
    #[error("{route}: not a route of folder names down from {root}")]
    Route { root: PathBuf, route: PathBuf },
    #[error("{path}: a symbolic link, which is not followed: what it points at is not this folder's to change")]
    Symlink { path: PathBuf },
    #[error("{path}: not a folder")]
    NotAFolder { path: PathBuf },
    #[error("{path}: not a file")]
    NotAFile { path: PathBuf },
    #[error("{path}: not UTF-8 text")]
    NotText { path: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}, so it was not changed")]
    Entry {
        path: PathBuf,
        #[source]
        source: EntryError,
    },
}

impl ConfigFile {
    /// The file at `route` below `root`, written in `format`.
    ///
    /// `route` must be relative and plain folder names all the way down,
    /// `.mcp.json` or `.codex/config.toml`: checked here, by its spelling,
    /// so a route that could lead out of `root` is refused before anything
    /// is read. `root` itself is the caller's and may be reached through a
    /// link, as a project often is.
    pub fn new(
        root: impl Into<PathBuf>,
        route: impl Into<PathBuf>,
        format: Format,
    ) -> Result<Self, FileError> {
        let (root, route) = (root.into(), route.into());
        let plain = route.components().next().is_some()
            && route
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !plain {
            return Err(FileError::Route { root, route });
        }
        Ok(ConfigFile {
            root,
            route,
            format,
        })
    }

    /// Where the file is.
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.route)
    }

    pub fn format(&self) -> Format {
        self.format
    }

    /// The entry at `key`, or `None` when it is not there, and when the file
    /// is not there either: a file nobody has made yet holds no entries, and
    /// that is not a failure.
    pub fn get(&self, key: &[&str]) -> Result<Option<Value>, FileError> {
        let Some(text) = self.read()? else {
            return Ok(None);
        };
        get(self.format, &text, key).map_err(|source| self.entry_error(source))
    }

    /// Make `edit` to the entry at `key`, and write the file back with every
    /// byte outside the entry as it was.
    ///
    /// A file that is not valid in its format is refused and left as it is,
    /// never rewritten into something that parses: what it was meant to say
    /// is its owner's to settle. An insert into a file that is not there yet
    /// makes it, and the folders on its route.
    pub fn edit(&self, key: &[&str], edit: &Edit) -> Result<Written, FileError> {
        let text = self.read()?;
        let edited = edit_document(self.format, text.as_deref(), key, edit)
            .map_err(|source| self.entry_error(source))?;
        let path = self.path();
        match text {
            Some(text) if text == edited => Ok(Written::Unchanged),
            Some(_) => {
                crate::workspace::replace_bytes(&path, edited.as_bytes())
                    .map_err(|(path, source)| FileError::Write { path, source })?;
                Ok(Written::Changed)
            }
            None => {
                self.make_folders()?;
                create(&path, edited.as_bytes())?;
                Ok(Written::Created)
            }
        }
    }

    /// The file's text, `None` when it is not there, after looking at every
    /// step of its route for a link.
    fn read(&self) -> Result<Option<String>, FileError> {
        let mut path = self.root.clone();
        let mut steps = self.route.components().peekable();
        while let Some(step) = steps.next() {
            path.push(step);
            let last = steps.peek().is_none();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(source) => return Err(FileError::Read { path, source }),
            };
            if metadata.file_type().is_symlink() {
                return Err(FileError::Symlink { path });
            }
            if !last && !metadata.is_dir() {
                return Err(FileError::NotAFolder { path });
            }
            if last && !metadata.is_file() {
                return Err(FileError::NotAFile { path });
            }
        }
        let bytes = fs::read(&path).map_err(|source| FileError::Read {
            path: path.clone(),
            source,
        })?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| FileError::NotText { path })
    }

    /// Make the folders on the route that are missing, each with
    /// `create_dir`, and refuse one that turns out to be a link.
    fn make_folders(&self) -> Result<(), FileError> {
        let mut path = self.root.clone();
        let Some(parent) = self.route.parent() else {
            return Ok(());
        };
        for step in parent.components() {
            path.push(step);
            match fs::create_dir(&path) {
                Ok(()) => continue,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(FileError::Write { path, source }),
            }
            // There already, perhaps only since the file was read.
            let metadata = fs::symlink_metadata(&path).map_err(|source| FileError::Read {
                path: path.clone(),
                source,
            })?;
            if metadata.file_type().is_symlink() {
                return Err(FileError::Symlink { path });
            }
            if !metadata.is_dir() {
                return Err(FileError::NotAFolder { path });
            }
        }
        Ok(())
    }

    fn entry_error(&self, source: EntryError) -> FileError {
        FileError::Entry {
            path: self.path(),
            source,
        }
    }
}

/// Write a new file whole, refusing one that is there by now.
///
/// `create_new` rather than a temporary renamed into place: a rename would
/// put this file over one an agent's own CLI made after the read found none.
/// A write that fails takes away what it made, so no half of a file is left
/// to be refused as malformed later.
fn create(path: &Path, contents: &[u8]) -> Result<(), FileError> {
    let write_error = |source| FileError::Write {
        path: path.to_path_buf(),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(write_error)?;
    if let Err(source) = file.write_all(contents) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(write_error(source));
    }
    Ok(())
}
