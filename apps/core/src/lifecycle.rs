//! Changing what a project already holds: finding an artifact by what somebody
//! typed, renaming it, removing it.
//!
//! Finding is more forgiving than listing. A skill whose front matter does not
//! parse is left out of every listing, but it is exactly the one somebody
//! needs to open and fix, so an artifact is also found by where its name would
//! put it on disk, whether or not it reads.
//!
//! None of this touches the ledger: whether a change leaves a stale record of
//! an installation behind is [`crate::install::disown`]'s question, asked by
//! whoever made the change.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, Reference, QUALIFIER};
use crate::frontmatter;
use crate::kind::{Kind, Layout};
use crate::paths;
use crate::skill::SkillManifest;
use crate::workspace::{replace_file, MindProject, WorkspaceError};

/// An artifact found by what somebody typed.
#[derive(Debug, Clone)]
pub struct Target {
    pub kind: Kind,
    /// What it declares, when it reads; otherwise the name its place on disk
    /// gives it.
    pub name: String,
    /// The directory a skill is, or the file a rule is.
    pub path: PathBuf,
    /// The artifact itself, when it can be read.
    pub artifact: Option<Artifact>,
}

impl Target {
    /// The file to open to change it: a skill's manifest, or the rule itself.
    pub fn file(&self) -> PathBuf {
        file_of(self.kind, &self.path)
    }

    /// `skill:deploy`.
    pub fn qualified_name(&self) -> String {
        format!("{}{QUALIFIER}{}", self.kind.slug(), self.name)
    }
}

/// The one artifact in `project` a reference names.
///
/// By what it declares first, then by where the name would put it — so a
/// skill that does not read is still found by its folder, and so is one whose
/// folder disagrees with what it declares. More than one match is an error
/// rather than a choice made quietly: changing the wrong artifact is not
/// something to guess about.
pub fn find(project: &MindProject, reference: &Reference) -> Result<Target, LifecycleError> {
    let kinds = reference
        .kind()
        .map_or_else(|| Kind::ALL.to_vec(), |kind| vec![kind]);
    let catalog = Catalog::discover_kinds(std::slice::from_ref(project), &kinds);

    let mut found: Vec<Target> = catalog
        .find(reference)
        .into_iter()
        .map(|artifact| Target {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            path: artifact.path().to_path_buf(),
            artifact: Some(artifact.clone()),
        })
        .collect();

    // Only a usable name is turned into a place: one with `..` in it would
    // otherwise be a way out of the project.
    let name = reference.name();
    if artifact::name_issues(name).is_empty() {
        for kind in kinds {
            let path = place(project, kind, name);
            if !holds(kind, &path) || found.iter().any(|target| target.path == path) {
                continue;
            }
            let artifact = load(project, kind, &path, name).ok();
            found.push(Target {
                kind,
                name: artifact.as_ref().map_or(name, Artifact::name).to_owned(),
                path,
                artifact,
            });
        }
    }

    match found.len() {
        0 => Err(LifecycleError::NotFound {
            typed: reference.typed().to_owned(),
        }),
        1 => Ok(found.remove(0)),
        _ => Err(LifecycleError::Ambiguous {
            typed: reference.typed().to_owned(),
            matches: found
                .iter()
                .map(|target| {
                    let at = paths::relative_to(&target.path, project.root())
                        .unwrap_or_else(|| target.path.clone());
                    format!("{} at {}", target.qualified_name(), at.display())
                })
                .collect(),
        }),
    }
}

/// Where an artifact of `kind` called `name` lives in `project`, whether or
/// not it is there.
pub fn place(project: &MindProject, kind: Kind, name: &str) -> PathBuf {
    let directory = project.directory_for(kind);
    match kind.layout() {
        Layout::Directory { .. } => directory.join(name),
        Layout::Files { extension } => directory.join(format!("{name}.{extension}")),
    }
}

/// Load the artifact at `path`, named `name` if it is the kind whose name
/// comes from where it sits.
pub fn load(
    project: &MindProject,
    kind: Kind,
    path: &Path,
    name: &str,
) -> Result<Artifact, ArtifactError> {
    match kind {
        Kind::Skill => Artifact::skill(path, project.root()),
        Kind::Rule => Artifact::rule(path, name.to_owned(), project.root()),
    }
}

/// Which artifact a file belongs to, when it sits where one would.
///
/// What lets a file that cannot be read still be named — and so opened,
/// renamed or removed — when all a listing has for it is the error.
pub fn identify(project: &MindProject, file: &Path) -> Option<(Kind, String)> {
    Kind::ALL.into_iter().find_map(|kind| {
        let below = file.strip_prefix(project.directory_for(kind)).ok()?;
        match kind.layout() {
            Layout::Directory { manifest } => {
                let mut parts = below.components();
                let folder = parts.next()?.as_os_str().to_str()?;
                let file = parts.next()?.as_os_str();
                (file == manifest && parts.next().is_none()).then(|| (kind, folder.to_owned()))
            }
            Layout::Files { extension } => {
                let matches = below
                    .extension()
                    .is_some_and(|found| found.eq_ignore_ascii_case(extension));
                let name = paths::to_config_string(&below.with_extension(""))?;
                matches.then_some((kind, name))
            }
        }
    })
}

/// How many files removing it would delete.
pub fn files(target: &Target) -> usize {
    fn count(path: &Path) -> usize {
        match fs::read_dir(path) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| match entry.file_type() {
                    Ok(kind) if kind.is_dir() => count(&entry.path()),
                    _ => 1,
                })
                .sum(),
            Err(_) => 1,
        }
    }
    match target.kind.layout() {
        Layout::Directory { .. } => count(&target.path),
        Layout::Files { .. } => 1,
    }
}

/// Delete an artifact: a skill's whole directory, or a rule's file and any
/// folder it leaves empty.
pub fn remove(project: &MindProject, target: &Target) -> Result<(), LifecycleError> {
    let directory = inside(project, target)?;
    match target.kind.layout() {
        Layout::Directory { .. } => {
            fs::remove_dir_all(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })
        }
        Layout::Files { .. } => {
            fs::remove_file(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
            prune(&directory, target.path.parent());
            Ok(())
        }
    }
}

/// Give an artifact a new name: a skill's folder and the `name` it declares
/// together, so it still passes `validate`; a rule's file, filed by its new
/// route.
///
/// In a skill's manifest the `name` line is the only thing rewritten. The rest
/// is somebody's writing — comments, key order, the description — and is left
/// byte for byte, and the result is parsed back to make sure the one line
/// changed what it was meant to and nothing else.
pub fn rename(project: &MindProject, target: &Target, to: &str) -> Result<Target, LifecycleError> {
    let issues = artifact::name_issues(to);
    if !issues.is_empty() {
        return Err(LifecycleError::Name {
            name: to.to_owned(),
            issues,
        });
    }
    let directory = inside(project, target)?;
    // Matched on the kind, so a third one has to say how it is renamed.
    match target.kind {
        Kind::Skill => rename_skill(project, target, &directory, to),
        Kind::Rule => rename_rule(project, target, &directory, to),
    }
}

fn rename_skill(
    project: &MindProject,
    target: &Target,
    directory: &Path,
    to: &str,
) -> Result<Target, LifecycleError> {
    if to.contains('/') {
        return Err(LifecycleError::Nested {
            name: to.to_owned(),
        });
    }
    // The name lives in the front matter too, and front matter that does not
    // parse cannot be trusted to have its name rewritten.
    let Some(artifact) = &target.artifact else {
        return Err(LifecycleError::Unreadable {
            path: target.file(),
        });
    };

    let folder = directory.join(to);
    let moves = folder != target.path;
    let renames = artifact.name() != to;
    if !moves && !renames {
        return Err(LifecycleError::Unchanged {
            name: to.to_owned(),
        });
    }
    if moves && occupied(&folder) {
        return Err(LifecycleError::Exists { path: folder });
    }

    let manifest = target.file();
    let rewritten = if renames {
        let source = fs::read_to_string(&manifest).map_err(|source| LifecycleError::Io {
            path: manifest.clone(),
            source,
        })?;
        Some(
            rewrite_name(&source, to).map_err(|reason| LifecycleError::Rewrite {
                path: manifest.clone(),
                reason,
            })?,
        )
    } else {
        None
    };

    if moves {
        fs::rename(&target.path, &folder).map_err(|source| LifecycleError::Io {
            path: target.path.clone(),
            source,
        })?;
    }
    if let Some(text) = rewritten {
        if let Err(error) = replace_file(&file_of(Kind::Skill, &folder), &text) {
            // Half a rename is a skill whose folder and name disagree: put the
            // folder back rather than leave that behind.
            if moves {
                let _ = fs::rename(&folder, &target.path);
            }
            return Err(error.into());
        }
    }

    let artifact = Artifact::skill(&folder, project.root())?;
    Ok(Target {
        kind: Kind::Skill,
        name: artifact.name().to_owned(),
        path: folder,
        artifact: Some(artifact),
    })
}

fn rename_rule(
    project: &MindProject,
    target: &Target,
    directory: &Path,
    to: &str,
) -> Result<Target, LifecycleError> {
    let file = place(project, Kind::Rule, to);
    if file == target.path {
        return Err(LifecycleError::Unchanged {
            name: to.to_owned(),
        });
    }
    if occupied(&file) {
        return Err(LifecycleError::Exists { path: file });
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|source| LifecycleError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&target.path, &file).map_err(|source| LifecycleError::Io {
        path: target.path.clone(),
        source,
    })?;
    prune(directory, target.path.parent());

    // A rule declares nothing, so there is nothing inside it to rewrite, and
    // one that did not read before a move does not read after it either.
    Ok(Target {
        kind: Kind::Rule,
        name: to.to_owned(),
        artifact: Artifact::rule(&file, to.to_owned(), project.root()).ok(),
        path: file,
    })
}

/// A manifest with its `name` set to `to` and every other byte as it was.
fn rewrite_name(source: &str, to: &str) -> Result<String, String> {
    let document = frontmatter::split(source).map_err(|error| error.to_string())?;
    let front_matter = document.front_matter;
    let before = SkillManifest::parse(front_matter).map_err(|error| error.to_string())?;

    // Serialized, so a name YAML would read as something else — `true`,
    // `123` — is quoted.
    let value = serde_yaml_ng::to_string(to).map_err(|error| error.to_string())?;
    let rewritten = frontmatter::with_key(front_matter, "name", value.trim_end())
        .ok_or_else(|| String::from("its `name` is not on a line of its own"))?;

    let after = SkillManifest::parse(&rewritten).map_err(|error| error.to_string())?;
    let wanted = SkillManifest {
        name: to.to_owned(),
        ..before
    };
    if after != wanted {
        return Err(String::from(
            "rewriting that line would change more than the name",
        ));
    }

    // The front matter is a slice of `source`, so where it starts in `source`
    // is where the rewritten one goes: the fences, and anything before them,
    // stay exactly as they were.
    let start = front_matter.as_ptr() as usize - source.as_ptr() as usize;
    Ok(format!(
        "{}{rewritten}{}",
        &source[..start],
        &source[start + front_matter.len()..]
    ))
}

/// The folder a target's kind lives in, having checked the target is inside
/// it: nothing here deletes or moves anything anywhere else.
fn inside(project: &MindProject, target: &Target) -> Result<PathBuf, LifecycleError> {
    let directory = project.directory_for(target.kind);
    if target.path == directory || !target.path.starts_with(&directory) {
        return Err(LifecycleError::Outside {
            path: target.path.clone(),
            directory,
        });
    }
    Ok(directory)
}

/// Whether anything at all is at `path`, a dangling symlink included.
fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Remove the folders a rule leaves empty, up to but not including the folder
/// the kind lives in. Folders under it only group, so an empty one groups
/// nothing.
fn prune(root: &Path, from: Option<&Path>) {
    let mut current = from;
    while let Some(folder) = current {
        if folder == root || !folder.starts_with(root) || fs::remove_dir(folder).is_err() {
            break;
        }
        current = folder.parent();
    }
}

/// The file an artifact at `path` is read from.
fn file_of(kind: Kind, path: &Path) -> PathBuf {
    match kind.layout() {
        Layout::Directory { manifest } => path.join(manifest),
        Layout::Files { .. } => path.to_path_buf(),
    }
}

/// Whether an artifact of `kind` sits at `path`.
fn holds(kind: Kind, path: &Path) -> bool {
    file_of(kind, path).is_file()
}

/// Why an artifact could not be found, renamed or removed.
#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("nothing named `{typed}` here")]
    NotFound { typed: String },
    #[error("`{typed}` names more than one thing — {} — so say which: by kind, as in `skill:name`, or by folder", .matches.join(", "))]
    Ambiguous { typed: String, matches: Vec<String> },
    #[error("{}", issue_list(.issues))]
    Name {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}`: a skill's name cannot contain `/`; skills are flat, and only rules are filed in folders")]
    Nested { name: String },
    #[error("it is already called `{name}`")]
    Unchanged { name: String },
    #[error("{path}: already exists, and is left alone")]
    Exists { path: PathBuf },
    #[error("{path}: its front matter cannot be read, so its name cannot be rewritten; fix it with `mind edit` first")]
    Unreadable { path: PathBuf },
    #[error("{path}: the name cannot be rewritten on its own — {reason}; change it by hand")]
    Rewrite { path: PathBuf, reason: String },
    #[error("{path}: not inside {directory}, so it is not changed from here")]
    Outside { path: PathBuf, directory: PathBuf },
    #[error("{path}: cannot be changed: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}
