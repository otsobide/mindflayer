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

    /// The entry this workspace would store for a project directory.
    ///
    /// A route relative to the workspace when one genuinely resolves, so the
    /// pair can be moved together; the absolute path otherwise.
    pub fn entry_for(&self, project_root: &Path) -> PathBuf {
        let absolute = paths::normalize(project_root);
        match paths::relative_to(&absolute, &self.root) {
            Some(route) if self.route_reaches(&route, &absolute) => route,
            _ => absolute,
        }
    }

    /// Whether following `route` from this workspace really lands on `target`.
    ///
    /// The route is arithmetic, and arithmetic on paths is only true when no
    /// component is a symlink: if the workspace root is spelled through one,
    /// a `..` climbs out of the link's target rather than out of the directory
    /// the name suggests, and the stored entry points somewhere that does not
    /// exist. `/tmp` is a symlink to `/private/tmp` on every Mac, so this is
    /// not an exotic case.
    ///
    /// The filesystem is consulted here and only here. The answer decides
    /// which spelling to store; it is never stored itself, so entries stay
    /// portable rather than being frozen to one machine's symlink layout.
    fn route_reaches(&self, route: &Path, target: &Path) -> bool {
        match (
            fs::canonicalize(self.root.join(route)),
            fs::canonicalize(target),
        ) {
            (Ok(followed), Ok(wanted)) => followed == wanted,
            // Nothing to compare against: a route that only descends is safe
            // whatever the symlinks do, and one that climbs is not worth a
            // guess.
            _ => !climbs(route),
        }
    }

    /// Register a mind project with this workspace.
    ///
    /// Idempotent, like `init`: registering a project that is already there
    /// changes nothing, says so, and reports the spelling the file actually
    /// uses rather than the one this call would have written.
    pub fn link(
        &mut self,
        project: &MindProject,
    ) -> Result<(PathBuf, Registration), WorkspaceError> {
        let entry = self.entry_for(project.root());
        let target = paths::normalize(project.root());
        let written =
            paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                path: entry.clone(),
            })?;
        let root = self.root.clone();

        // Matched against the array being edited, not against the copy parsed
        // when this workspace was opened: the file is meant to be editable by
        // hand, so the copy can be stale by the time we get here.
        self.edit_projects(move |array| {
            if let Some(existing) = entries(array)
                .into_iter()
                .find(|entry| points_at(&root, entry, &target))
            {
                return Ok((existing, Registration::AlreadyRegistered));
            }
            array.push(written.as_str());
            Ok((PathBuf::from(&written), Registration::Added))
        })
    }

    /// Drop every entry pointing at a project, returning what was removed.
    ///
    /// Every entry, not the first: two spellings of one directory are one
    /// project, and removing half of them while reporting success would leave
    /// it registered and the user believing otherwise.
    ///
    /// Takes a path rather than a `MindProject` on purpose: the entry worth
    /// removing most often is one whose directory has moved away, and that
    /// cannot be opened as a project any more.
    ///
    /// Unlike `link` this is not idempotent. Removing something that was never
    /// there is a typo far more often than it is a no-op, and saying so is
    /// what turns a silent success into a fixable mistake.
    pub fn unlink(&mut self, project_root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
        let target = paths::normalize(project_root);
        let root = self.root.clone();

        let removed = self.edit_projects(move |array| {
            let mut removed = Vec::new();
            let mut index = 0;
            while index < array.len() {
                match array.get(index).and_then(|value| value.as_str()) {
                    Some(entry) if points_at(&root, Path::new(entry), &target) => {
                        removed.push(PathBuf::from(entry));
                        array.remove(index);
                    }
                    _ => index += 1,
                }
            }
            Ok(removed)
        })?;

        if removed.is_empty() {
            return Err(WorkspaceError::NotRegistered {
                path: project_root.to_path_buf(),
                workspace: self.root.clone(),
            });
        }
        Ok(removed)
    }

    /// Rewrite the `projects` array in place, leaving every other byte of the
    /// file alone: comments, key order, spacing.
    ///
    /// This is why `toml_edit` is a dependency. Reading is serde's job, but
    /// re-serializing to write would throw away the comment that explains what
    /// the file is, which is the first documentation anyone opening it reads.
    ///
    /// The array is read from disk here rather than taken from `self.config`,
    /// so an edit decides what to do from the file it is about to write, not
    /// from a copy that may be minutes old.
    fn edit_projects<F, T>(&mut self, edit: F) -> Result<T, WorkspaceError>
    where
        F: FnOnce(&mut toml_edit::Array) -> Result<T, String>,
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

        let value = edit(array).map_err(|detail| WorkspaceError::Edit {
            path: path.clone(),
            detail,
        })?;

        // Only write when the edit changed something. An already-registered
        // link and a failed unlink both leave the file untouched, down to its
        // modification time.
        let rewritten = document.to_string();
        if rewritten != text {
            replace_file(&path, &rewritten)?;
        }

        // Re-read, so what is in memory is what is on disk rather than what we
        // believe we wrote.
        let config: FlayerConfig = read_config(&path)?;
        check_version(&path, config.version)?;
        self.config = config;
        Ok(value)
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

/// The entries currently written in a `projects` array.
fn entries(array: &toml_edit::Array) -> Vec<PathBuf> {
    array
        .iter()
        .filter_map(|value| value.as_str())
        .map(PathBuf::from)
        .collect()
}

/// Whether a stored entry names the same directory as `target`.
///
/// By where they point, not how they are spelled, so `collapse` and
/// `./collapse` are one entry. Arithmetic settles most of it; two spellings
/// that only the filesystem can equate — a symlinked parent, `/tmp` on a Mac —
/// need canonicalising, and a path that does not exist cannot be equated that
/// way at all.
fn points_at(root: &Path, entry: &Path, target: &Path) -> bool {
    let resolved = paths::normalize(&root.join(entry));
    if resolved == target {
        return true;
    }
    match (fs::canonicalize(&resolved), fs::canonicalize(target)) {
        (Ok(followed), Ok(wanted)) => followed == wanted,
        _ => false,
    }
}

/// Whether a route has to climb out of its base to get where it is going.
fn climbs(route: &Path) -> bool {
    route
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

/// Replace a file's contents through a temporary file and a rename.
///
/// The rename is the point: a crash mid-write leaves either the old config or
/// the new one, never half of each. The temporary sits beside the original so
/// the rename stays inside one filesystem, which is where it is atomic.
fn replace_file(path: &Path, contents: &str) -> Result<(), WorkspaceError> {
    // Follow a symlinked config to the file it names. Replacing the link
    // itself would silently detach a workspace from a config someone chose to
    // share, and leave the original holding stale contents.
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = resolved.as_path();

    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temporary = path.with_file_name(name);

    fs::write(&temporary, contents).map_err(|source| WorkspaceError::Create {
        path: temporary.clone(),
        source,
    })?;

    // A rename carries the temporary file's permissions with it, so the
    // original's are copied across first: a config chmodded to 600 must not
    // come back world readable. Best effort — a filesystem that cannot say is
    // not a reason to refuse the edit.
    if let Ok(metadata) = fs::metadata(path) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
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
