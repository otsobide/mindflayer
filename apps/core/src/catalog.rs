//! Every artifact held by a set of mind projects.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{Artifact, ArtifactError};
use crate::kind::{Kind, Layout};
use crate::paths;
use crate::workspace::MindProject;

/// The artifacts discovered across some mind projects, and what went wrong.
///
/// Failures are collected rather than returned, because one unreadable file
/// must not hide the forty next to it: a front end lists what it found and
/// reports the rest.
#[derive(Debug, Default)]
pub struct Catalog {
    artifacts: Vec<Artifact>,
    failures: Vec<DiscoveryFailure>,
}

impl Catalog {
    /// Load every kind held by every given project.
    pub fn discover(projects: &[MindProject]) -> Self {
        Self::discover_kinds(projects, &Kind::ALL)
    }

    /// The same, restricted to some kinds.
    ///
    /// Restricting here rather than filtering the result is what keeps a
    /// listing from reporting failures in a kind nobody asked about.
    pub fn discover_kinds(projects: &[MindProject], kinds: &[Kind]) -> Self {
        let mut catalog = Catalog::default();

        for project in projects {
            for kind in kinds {
                let root = project.directory_for(*kind);
                match kind.layout() {
                    Layout::Directory { manifest } => {
                        catalog.take_directories(&root, manifest, *kind, project.root());
                    }
                    Layout::Files { extension } => {
                        catalog.take_files(&root, extension, project.root());
                    }
                }
            }
        }

        // Grouped by kind, then by name. Kind first because a mixed listing
        // reads as blocks of like things rather than an alphabetical shuffle
        // of two different sorts of thing; name second because that is what
        // the reader is scanning within a block.
        catalog.artifacts.sort_by(|a, b| {
            a.kind()
                .cmp(&b.kind())
                .then_with(|| a.name().cmp(b.name()))
                .then_with(|| a.project().cmp(b.project()))
        });
        catalog.failures.sort_by(|a, b| a.path().cmp(b.path()));
        catalog
    }

    /// One artifact per immediate subdirectory holding `manifest`.
    ///
    /// Only the immediate ones: the directory belongs to the artifact, assets
    /// and all, so walking into it would turn its own files into artifacts.
    fn take_directories(&mut self, root: &Path, manifest: &str, kind: Kind, project: &Path) {
        let mut directories = match self.read_dir(root) {
            Some(entries) => entries,
            None => return,
        };
        directories.sort();
        for path in directories {
            if !path.join(manifest).is_file() {
                // Not a broken artifact: not one. Shared assets and stray
                // dotfolders both land here.
                continue;
            }
            debug_assert_eq!(kind, Kind::Skill, "only skills are directories today");
            match Artifact::skill(path, project) {
                Ok(artifact) => self.artifacts.push(artifact),
                Err(error) => self.failures.push(error.into()),
            }
        }
    }

    /// One artifact per file with `extension`, at any depth under `root`.
    ///
    /// The name is the route below `root` without the extension, so folders
    /// group and mean nothing else.
    fn take_files(&mut self, root: &Path, extension: &str, project: &Path) {
        let mut pending = vec![root.to_path_buf()];
        let mut files = Vec::new();

        while let Some(directory) = pending.pop() {
            let Some(entries) = self.read_dir(&directory) else {
                continue;
            };
            for path in entries {
                // A symlinked directory is not followed: a loop would hang
                // discovery, and nothing about a rule needs one.
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.is_dir() {
                    if !is_hidden(&path) {
                        pending.push(path);
                    }
                } else if metadata.is_file()
                    && path.extension().is_some_and(|found| found == extension)
                    && !is_hidden(&path)
                {
                    files.push(path);
                }
            }
        }

        files.sort();
        for path in files {
            let Some(name) = route(root, &path) else {
                self.failures.push(DiscoveryFailure::UnusableName { path });
                continue;
            };
            match Artifact::rule(&path, name, project) {
                Ok(artifact) => self.artifacts.push(artifact),
                Err(error) => self.failures.push(error.into()),
            }
        }
    }

    /// The entries of a directory, or nothing when it is absent.
    ///
    /// A kind's folder that does not exist is not a failure: it is a kind
    /// nobody has added anything to. One that exists and cannot be listed is.
    /// Whatever was read before a mid-walk failure is kept.
    fn read_dir(&mut self, path: &Path) -> Option<Vec<PathBuf>> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(source) => {
                self.failures.push(DiscoveryFailure::Directory {
                    path: path.to_path_buf(),
                    source,
                });
                return None;
            }
        };

        let mut found = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => found.push(entry.path()),
                Err(source) => self.failures.push(DiscoveryFailure::Directory {
                    path: path.to_path_buf(),
                    source,
                }),
            }
        }
        Some(found)
    }

    /// The artifacts found, grouped by kind and ordered by name.
    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    /// What could not be read, ordered by path.
    pub fn failures(&self) -> &[DiscoveryFailure] {
        &self.failures
    }

    /// Whether anything at all was found.
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    /// Every kind present, in report order.
    ///
    /// What a front end needs to decide whether naming a kind tells the reader
    /// anything: with one kind in play, it does not.
    pub fn kinds(&self) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = self.artifacts.iter().map(Artifact::kind).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// Every artifact a reference names.
    ///
    /// More than one is possible and is not by itself an error: the same name
    /// in two projects is ordinary in a workspace that manages both, and one
    /// name can belong to two kinds. Which one wins is a decision for whoever
    /// is orchestrating them, so all are returned and the caller says so.
    pub fn find(&self, reference: &Reference) -> Vec<&Artifact> {
        self.artifacts
            .iter()
            .filter(|artifact| reference.matches(artifact))
            .collect()
    }
}

/// What a user typed to name one artifact.
///
/// The split is at the first `/`, and only when what precedes it is a kind
/// word: `rule/git/no-force-push` is the rule `git/no-force-push`, while
/// `git/no-force-push` alone is that name in any kind. A rules folder named
/// after a kind therefore shadows, and is reached by qualifying. Deterministic
/// beats clever: a reference that means different things depending on what
/// happens to exist is the bug this codebase already refused once, when `list`
/// stopped guessing its own scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    kind: Option<Kind>,
    name: String,
}

impl Reference {
    /// Read what a user typed.
    pub fn parse(typed: &str) -> Self {
        match typed.split_once('/') {
            Some((head, rest)) => match head.parse::<Kind>() {
                Ok(kind) => Self {
                    kind: Some(kind),
                    name: rest.to_owned(),
                },
                Err(_) => Self {
                    kind: None,
                    name: typed.to_owned(),
                },
            },
            None => Self {
                kind: None,
                name: typed.to_owned(),
            },
        }
    }

    /// The kind it was qualified with, if any.
    pub fn kind(&self) -> Option<Kind> {
        self.kind
    }

    /// The name half.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this names that artifact.
    fn matches(&self, artifact: &Artifact) -> bool {
        artifact.name() == self.name && self.kind.is_none_or(|kind| kind == artifact.kind())
    }
}

/// Why an artifact, or a whole folder, could not be read.
#[derive(Debug, Error)]
pub enum DiscoveryFailure {
    #[error("{path}: cannot be listed: {source}")]
    Directory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: its route is not valid UTF-8, so it cannot be named")]
    UnusableName { path: PathBuf },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

impl DiscoveryFailure {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            DiscoveryFailure::Directory { path, .. } | DiscoveryFailure::UnusableName { path } => {
                path
            }
            DiscoveryFailure::Artifact(error) => error.path(),
        }
    }
}

/// A file's route below `root`, without its extension, `/` separated.
fn route(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let without_extension = relative.with_extension("");
    paths::to_config_string(&without_extension)
}

/// Whether a path's own name starts with a dot.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}
