//! The two levels Mindflayer works at.
//!
//! A **mind project** is a directory carrying a `.mind`, the way a repository
//! carries a `.git`: it holds skills, in `.mind/skills/<name>/SKILL.md`, and
//! it travels with the code it belongs to.
//!
//! A **flayer workspace** sits above them, carrying a `.mindflayer` that
//! references the mind projects it manages, so skills can be listed and moved
//! across several projects at once.
//!
//! Both are found the way git finds a repository: by walking up from a
//! starting directory until the marker file appears. Both are described by a
//! marker file rather than by the directory alone, so an empty `.mind` left
//! behind by a failed copy is not mistaken for a project.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::paths;

/// The layout version written into new marker files.
///
/// Refusing to read a version from the future is what lets the format change
/// later without an old binary silently misreading a newer project.
pub const FORMAT_VERSION: u32 = 1;

/// The directory a mind project is identified by.
pub const MIND_DIR: &str = ".mind";
/// The marker file inside [`MIND_DIR`].
pub const MIND_CONFIG: &str = "mind.toml";
/// The folder holding one directory per skill, inside [`MIND_DIR`].
pub const SKILLS_DIR: &str = "skills";

/// The directory a flayer workspace is identified by.
pub const FLAYER_DIR: &str = ".mindflayer";
/// The marker file inside [`FLAYER_DIR`].
pub const FLAYER_CONFIG: &str = "flayer.toml";

/// What an `init` actually did.
///
/// Initialising twice is not an error, and never overwrites the marker: the
/// caller is told which of the two happened and says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Initialization {
    /// The marker did not exist and now does.
    Created,
    /// The marker was already there and was left untouched.
    AlreadyInitialized,
}

/// What a `link` actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// The project was not registered and now is.
    Added,
    /// The project was already registered; the file was not touched.
    AlreadyRegistered,
}

// ---------------------------------------------------------------------------
// Mind projects
// ---------------------------------------------------------------------------

/// The contents of `.mind/mind.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MindConfig {
    /// The layout version this project was written with.
    pub version: u32,
    /// A human name for the project. Defaults to its directory's name.
    pub name: String,
}

/// A directory holding skills, identified by its `.mind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindProject {
    root: PathBuf,
    config: MindConfig,
}

impl MindProject {
    /// Create `.mind`, its marker file and its skills folder under `root`.
    ///
    /// Existing files are never rewritten, so running this on a project that
    /// already exists is safe and only fills in what is missing.
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, Initialization), WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let dir = root.join(MIND_DIR);
        let config_path = dir.join(MIND_CONFIG);

        create_dir(&dir)?;
        create_dir(&dir.join(SKILLS_DIR))?;

        if config_path.is_file() {
            return Ok((Self::open(&root)?, Initialization::AlreadyInitialized));
        }

        let name = directory_name(&root);
        write_new(&config_path, &mind_template(&name))?;
        Ok((Self::open(&root)?, Initialization::Created))
    }

    /// Open the mind project rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let config_path = root.join(MIND_DIR).join(MIND_CONFIG);
        if !config_path.is_file() {
            return Err(WorkspaceError::NotAProject { path: root });
        }
        let config: MindConfig = read_config(&config_path)?;
        check_version(&config_path, config.version)?;
        Ok(Self { root, config })
    }

    /// Walk up from `start` looking for a mind project.
    pub fn locate(start: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        match locate_marker(start.as_ref(), MIND_DIR, MIND_CONFIG)? {
            Some(root) => Self::open(root).map(Some),
            None => Ok(None),
        }
    }

    /// The directory holding `.mind`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.mind` directory itself.
    pub fn mind_dir(&self) -> PathBuf {
        self.root.join(MIND_DIR)
    }

    /// `.mind/skills`, whether or not it exists yet.
    pub fn skills_dir(&self) -> PathBuf {
        self.mind_dir().join(SKILLS_DIR)
    }

    /// The project's declared name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// The parsed marker file.
    pub fn config(&self) -> &MindConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Flayer workspaces
// ---------------------------------------------------------------------------

/// The contents of `.mindflayer/flayer.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlayerConfig {
    /// The layout version this workspace was written with.
    pub version: u32,
    /// A human name for the workspace. Defaults to its directory's name.
    pub name: String,
    /// The mind projects this workspace manages, relative to its root.
    #[serde(default)]
    pub projects: Vec<PathBuf>,
}

/// A directory orchestrating several mind projects, identified by its
/// `.mindflayer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlayerWorkspace {
    root: PathBuf,
    config: FlayerConfig,
}

impl FlayerWorkspace {
    /// Create `.mindflayer` and its marker file under `root`.
    ///
    /// The registry starts empty: a workspace manages the projects it is told
    /// about, and guessing which neighbouring directories were meant would be
    /// the kind of surprise that is hard to undo.
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, Initialization), WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let dir = root.join(FLAYER_DIR);
        let config_path = dir.join(FLAYER_CONFIG);

        create_dir(&dir)?;

        if config_path.is_file() {
            return Ok((Self::open(&root)?, Initialization::AlreadyInitialized));
        }

        let name = directory_name(&root);
        write_new(&config_path, &flayer_template(&name))?;
        Ok((Self::open(&root)?, Initialization::Created))
    }

    /// Open the workspace rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let config_path = root.join(FLAYER_DIR).join(FLAYER_CONFIG);
        if !config_path.is_file() {
            return Err(WorkspaceError::NotAWorkspace { path: root });
        }
        let config: FlayerConfig = read_config(&config_path)?;
        check_version(&config_path, config.version)?;
        Ok(Self { root, config })
    }

    /// Walk up from `start` looking for a flayer workspace.
    pub fn locate(start: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        match locate_marker(start.as_ref(), FLAYER_DIR, FLAYER_CONFIG)? {
            Some(root) => Self::open(root).map(Some),
            None => Ok(None),
        }
    }

    /// The directory holding `.mindflayer`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.mindflayer` directory itself.
    pub fn flayer_dir(&self) -> PathBuf {
        self.root.join(FLAYER_DIR)
    }

    /// The workspace's declared name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// The parsed marker file.
    pub fn config(&self) -> &FlayerConfig {
        &self.config
    }

    /// The marker file itself.
    pub fn config_path(&self) -> PathBuf {
        self.flayer_dir().join(FLAYER_CONFIG)
    }

    /// The entry this workspace would store for a project directory: a route
    /// relative to the workspace, so the pair can be moved together, and an
    /// absolute path only when no route exists (different Windows drives).
    pub fn entry_for(&self, project_root: &Path) -> PathBuf {
        paths::relative_to(project_root, &self.root)
            .unwrap_or_else(|| paths::normalize(project_root))
    }

    /// Where a stored entry points.
    fn resolve(&self, entry: &Path) -> PathBuf {
        paths::normalize(&self.root.join(entry))
    }

    /// Register a mind project with this workspace.
    ///
    /// Idempotent, like `init`: registering a project that is already there
    /// changes nothing and says so. Matching is by where an entry *points*,
    /// not by how it is spelled, so `collapse` and `./collapse` are one entry.
    pub fn link(
        &mut self,
        project: &MindProject,
    ) -> Result<(PathBuf, Registration), WorkspaceError> {
        let entry = self.entry_for(project.root());
        let target = paths::normalize(project.root());

        if self
            .config
            .projects
            .iter()
            .any(|existing| self.resolve(existing) == target)
        {
            return Ok((entry, Registration::AlreadyRegistered));
        }

        let written =
            paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                path: entry.clone(),
            })?;
        self.edit_projects(move |array| {
            array.push(written.as_str());
            Ok(())
        })?;
        Ok((entry, Registration::Added))
    }

    /// Drop a registered project, returning the entry that was removed.
    ///
    /// Takes a path rather than a `MindProject` on purpose: the entry worth
    /// removing most often is one whose directory has moved away, and that
    /// cannot be opened as a project any more.
    ///
    /// Unlike `link` this is not idempotent. Removing something that was never
    /// there is a typo far more often than it is a no-op, and saying so is
    /// what turns a silent success into a fixable mistake.
    pub fn unlink(&mut self, project_root: &Path) -> Result<PathBuf, WorkspaceError> {
        let target = paths::normalize(project_root);
        let found = self
            .config
            .projects
            .iter()
            .position(|existing| self.resolve(existing) == target);

        let index = found.ok_or_else(|| WorkspaceError::NotRegistered {
            path: project_root.to_path_buf(),
            workspace: self.root.clone(),
        })?;
        let removed = self.config.projects[index].clone();

        // The serde-parsed Vec and the toml_edit array are the same array read
        // twice, so their indices line up. The guard is for the impossible
        // case rather than the expected one, because the alternative is a
        // panic out of Array::remove.
        self.edit_projects(move |array| {
            if index >= array.len() {
                return Err("`projects` changed on disk while unlinking".to_owned());
            }
            array.remove(index);
            Ok(())
        })?;
        Ok(removed)
    }

    /// Rewrite the `projects` array in place, leaving every other byte of the
    /// file alone: comments, key order, spacing.
    ///
    /// This is why `toml_edit` is a dependency. Reading is serde's job, but
    /// re-serializing to write would throw away the comment that explains what
    /// the file is, which is the first documentation anyone opening it reads.
    fn edit_projects<F>(&mut self, edit: F) -> Result<(), WorkspaceError>
    where
        F: FnOnce(&mut toml_edit::Array) -> Result<(), String>,
    {
        let path = self.config_path();
        let text = fs::read_to_string(&path).map_err(|source| WorkspaceError::Read {
            path: path.clone(),
            source,
        })?;
        let mut document: toml_edit::DocumentMut =
            text.parse()
                .map_err(|error: toml_edit::TomlError| WorkspaceError::Edit {
                    path: path.clone(),
                    detail: error.to_string(),
                })?;

        // A workspace whose `projects` key was deleted by hand still links.
        if !document.as_table().contains_key("projects") {
            document["projects"] = toml_edit::value(toml_edit::Array::new());
        }
        let array = document["projects"]
            .as_array_mut()
            .ok_or_else(|| WorkspaceError::Edit {
                path: path.clone(),
                detail: "`projects` is not an array".to_owned(),
            })?;

        edit(array).map_err(|detail| WorkspaceError::Edit {
            path: path.clone(),
            detail,
        })?;

        replace_file(&path, &document.to_string())?;

        // Re-read, so what is in memory is what is on disk rather than what we
        // believe we wrote.
        let config: FlayerConfig = read_config(&path)?;
        check_version(&path, config.version)?;
        self.config = config;
        Ok(())
    }

    /// Open every registered mind project.
    ///
    /// A registered path that has gone missing is reported rather than raised:
    /// one stale entry must not stop a workspace from managing the rest.
    pub fn projects(&self) -> (Vec<MindProject>, Vec<WorkspaceError>) {
        let mut projects = Vec::new();
        let mut failures = Vec::new();
        for entry in &self.config.projects {
            // Relative to the workspace root, so a workspace can be moved with
            // its projects and keep working.
            let path = self.root.join(entry);
            match MindProject::open(path) {
                Ok(project) => projects.push(project),
                Err(error) => failures.push(error),
            }
        }
        (projects, failures)
    }
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

/// Why a project or a workspace could not be created, found or read.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("{path}: cannot be created: {source}")]
    Create {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: not valid Mindflayer configuration: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{path}: not a mind project (no {MIND_DIR}/{MIND_CONFIG})")]
    NotAProject { path: PathBuf },
    #[error("{path}: not a flayer workspace (no {FLAYER_DIR}/{FLAYER_CONFIG})")]
    NotAWorkspace { path: PathBuf },
    #[error("{path}: written by a newer Mindflayer (format version {found}, this one reads {FORMAT_VERSION})")]
    Version { path: PathBuf, found: u32 },
    #[error("{path}: cannot be edited: {detail}")]
    Edit { path: PathBuf, detail: String },
    #[error("{path}: not registered in the workspace at {workspace}")]
    NotRegistered { path: PathBuf, workspace: PathBuf },
    #[error("{path}: not valid UTF-8, so it cannot be written into a TOML file")]
    NonUtf8Path { path: PathBuf },
}

impl WorkspaceError {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            WorkspaceError::Create { path, .. }
            | WorkspaceError::Read { path, .. }
            | WorkspaceError::Parse { path, .. }
            | WorkspaceError::NotAProject { path }
            | WorkspaceError::NotAWorkspace { path }
            | WorkspaceError::Version { path, .. }
            | WorkspaceError::Edit { path, .. }
            | WorkspaceError::NotRegistered { path, .. }
            | WorkspaceError::NonUtf8Path { path } => path,
        }
    }
}

/// Walk `start` and its ancestors looking for `<dir>/<file>`.
fn locate_marker(start: &Path, dir: &str, file: &str) -> Result<Option<PathBuf>, WorkspaceError> {
    // Absolute first: `ancestors()` on a relative path stops at the empty
    // component, so `mind list` run from a subdirectory would find nothing.
    let start = absolute(start)?;
    Ok(start
        .ancestors()
        .find(|candidate| candidate.join(dir).join(file).is_file())
        .map(Path::to_path_buf))
}

/// Read and parse a marker file.
fn read_config<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, WorkspaceError> {
    let text = fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| WorkspaceError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Refuse a marker written by a version that knows more than this one.
fn check_version(path: &Path, found: u32) -> Result<(), WorkspaceError> {
    if found > FORMAT_VERSION {
        return Err(WorkspaceError::Version {
            path: path.to_path_buf(),
            found,
        });
    }
    Ok(())
}

fn create_dir(path: &Path) -> Result<(), WorkspaceError> {
    fs::create_dir_all(path).map_err(|source| WorkspaceError::Create {
        path: path.to_path_buf(),
        source,
    })
}

/// Write a file that must not already exist.
///
/// `create_new` rather than a prior `exists()` check: the check and the write
/// are one operation, so nothing that appears in between is overwritten.
fn write_new(path: &Path, contents: &str) -> Result<(), WorkspaceError> {
    use std::io::Write as _;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| WorkspaceError::Create {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|source| WorkspaceError::Create {
            path: path.to_path_buf(),
            source,
        })
}

/// Replace a file's contents through a temporary file and a rename.
///
/// The rename is the point: a crash mid-write leaves either the old config or
/// the new one, never half of each. The temporary sits beside the original so
/// the rename stays inside one filesystem, which is where it is atomic.
fn replace_file(path: &Path, contents: &str) -> Result<(), WorkspaceError> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temporary = path.with_file_name(name);

    fs::write(&temporary, contents).map_err(|source| WorkspaceError::Create {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| {
        // A temporary left behind by a failed rename is litter nothing owns.
        let _ = fs::remove_file(&temporary);
        WorkspaceError::Create {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// An absolute, `..`-free path, without requiring the path to exist yet.
///
/// The normalising half is not cosmetic. A workspace resolves an entry by
/// joining it onto its own root, so a project registered as `../collapse`
/// arrives here as `<workspace>/../collapse`; `std::path::absolute` leaves
/// that `..` in place. Every root would then carry it, and two paths naming
/// one directory would stop comparing equal — which is what `in_project` and
/// the link deduplication are built on.
fn absolute(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let absolute = std::path::absolute(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(paths::normalize(&absolute))
}

/// The name to give a project or workspace created in `root`.
fn directory_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mindflayer")
        .to_owned()
}

/// The marker files are written from templates rather than serialized, so the
/// comments explaining the layout are in the file the user opens first.
fn mind_template(name: &str) -> String {
    format!(
        "# Mindflayer mind project.\n\
         #\n\
         # Skills live in {MIND_DIR}/{SKILLS_DIR}/<name>/SKILL.md. Everything under\n\
         # {MIND_DIR} is meant to be committed: it travels with the project it describes.\n\
         version = {FORMAT_VERSION}\n\
         name = {}\n",
        toml_string(name)
    )
}

fn flayer_template(name: &str) -> String {
    format!(
        "# Mindflayer workspace.\n\
         #\n\
         # It orchestrates the mind projects listed below, so their skills can be\n\
         # managed together. Paths are relative to this file's grandparent, the\n\
         # directory holding {FLAYER_DIR}.\n\
         version = {FORMAT_VERSION}\n\
         name = {}\n\
         projects = []\n",
        toml_string(name)
    )
}

/// A TOML basic string. Directory names are arbitrary, so the quoting is not
/// optional even though the common case never needs it.
fn toml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_round_trip_through_the_parser() {
        let mind: MindConfig = toml::from_str(&mind_template("collapse")).unwrap();
        assert_eq!(mind.version, FORMAT_VERSION);
        assert_eq!(mind.name, "collapse");

        let flayer: FlayerConfig = toml::from_str(&flayer_template("projects")).unwrap();
        assert_eq!(flayer.version, FORMAT_VERSION);
        assert_eq!(flayer.name, "projects");
        assert!(flayer.projects.is_empty());
    }

    #[test]
    fn a_name_needing_quotes_still_round_trips() {
        let awkward = "quote\" and \\ backslash";
        let mind: MindConfig = toml::from_str(&mind_template(awkward)).unwrap();
        assert_eq!(mind.name, awkward);
    }
}
