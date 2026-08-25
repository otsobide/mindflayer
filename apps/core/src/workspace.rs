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
            | WorkspaceError::Version { path, .. } => path,
        }
    }
}

/// Walk `start` and its ancestors looking for `<dir>/<file>`.
fn locate_marker(start: &Path, dir: &str, file: &str) -> Result<Option<PathBuf>, WorkspaceError> {
    // Absolute first: `ancestors()` on a relative path stops at the empty
    // component, so `mf list` run from a subdirectory would find nothing.
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

/// An absolute path, without requiring the path to exist yet.
fn absolute(path: &Path) -> Result<PathBuf, WorkspaceError> {
    std::path::absolute(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })
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
