//! Changing what a project holds: adding to it, finding an artifact by what
//! somebody typed, renaming it, removing it.
//!
//! Finding is more forgiving than listing. A skill whose front matter does not
//! parse is left out of every listing, but it is exactly the one somebody
//! needs to open and fix, so an artifact is also found by where its name would
//! put it on disk, whether or not it reads.
//!
//! Each change also sees to the ledger's record of what was installed under
//! the names it touches, and says in its result what it did ([`Record`]). The
//! rule is one, and it lives here rather than in a front end, so that no
//! front end can apply it differently. Only one that changes things through
//! these functions gets it, though: [`create`] and [`create_from`], the
//! writers [`add`] is built on, see to no record, and a front end that added
//! with them would leave a stale one in place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, Reference, QUALIFIER};
use crate::create::{create, create_from, CreateError};
use crate::frontmatter;
use crate::install::{self, InstallError};
use crate::kind::{Kind, Layout};
use crate::paths;
use crate::skill::SkillManifest;
use crate::template::Template;
use crate::workspace::{replace_file, FlayerWorkspace, MindProject, WorkspaceError};

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

/// What a change did to the ledger's record that Mindflayer installed what
/// is under one name.
///
/// That record is what gives the install screen leave to overwrite and
/// delete, so it must not outlive the copy it was about: whatever somebody
/// writes there next would inherit it.
///
/// A change reports it as `Result<Record, InstallError>` beside what it made
/// rather than failing with it: by the time the record is seen to, the change
/// has been made, and reporting it as not made would be false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record {
    /// No record was changed: there was none — no workspace was consulted,
    /// the one consulted does not manage the project, or it installed nothing
    /// under that name — or the change gave no reason to touch it.
    Untouched,
    /// The record was dropped, and what is there now is its author's.
    Dropped,
}

/// What adding an artifact made, and what it did to a record under its name.
#[derive(Debug)]
pub struct Added {
    pub artifact: Artifact,
    /// A record of an install under the same name. Nothing was there for the
    /// new artifact to be written over, so the copy it was about had gone,
    /// and left in place the record would make somebody's new work look like
    /// something Mindflayer put there.
    pub stale: Result<Record, InstallError>,
}

/// Write a new artifact into `project`, from `template` when one is given
/// ([`create`], [`create_from`]), and drop any record of an install under its
/// name.
///
/// `workspace` is the one the front end consulted about the project, given
/// rather than found by walking up from it: a walk misses every workspace
/// that links the project from somewhere that is not above it. With none,
/// there is no record to drop.
pub fn add(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Added, CreateError> {
    let artifact = match template {
        None => create(project, kind, name, description),
        Some(template) => create_from(project, kind, name, description, template),
    }?;
    let stale = disowned(workspace, project, kind, name);
    Ok(Added { artifact, stale })
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

    // Only a name made of plain folder names is turned into a place: one with
    // `..` in it would otherwise be a way out of the project. Not the naming
    // rules, which are for what Mindflayer writes: a folder from before they
    // were tightened, or one somebody made by hand, is exactly what has to be
    // found to be fixed or removed.
    let name = reference.name();
    if name.split('/').all(install::is_folder_name) {
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

/// An artifact opened to be edited, with what its file held then, so that
/// finishing the edit can tell whether anything changed.
///
/// Running the editor is the front end's, since which one to run comes from
/// the environment, and core never reads that. What the edit came to, and
/// what that means for the copy's record, is core's.
#[derive(Debug, Clone)]
pub struct Edit {
    kind: Kind,
    name: String,
    file: PathBuf,
    before: Option<Vec<u8>>,
}

impl Edit {
    /// The file to open in the editor.
    pub fn file(&self) -> &Path {
        &self.file
    }
}

/// What an edit came to.
#[derive(Debug)]
pub struct Edited {
    /// Whether the file is any different from when the edit started.
    pub changed: bool,
    /// The record of the copy edited, dropped when it changed: it is no longer
    /// what was installed, so the install screen must not take it for its
    /// own to overwrite.
    pub copy: Result<Record, InstallError>,
}

/// Start editing `target`, noting what its file holds now.
pub fn start_edit(target: &Target) -> Edit {
    let file = target.file();
    Edit {
        kind: target.kind,
        name: target.name.clone(),
        before: fs::read(&file).ok(),
        file,
    }
}

/// Finish an edit of an artifact in `project` once the editor has closed,
/// dropping the record of an install of it when the file changed.
/// `workspace` is the one consulted, as for [`add`].
///
/// A front end finishes the edit whether or not the editor succeeded: one
/// that wrote the file before it failed has changed the copy all the same.
pub fn finish_edit(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    edit: Edit,
) -> Edited {
    let changed = fs::read(&edit.file).ok() != edit.before;
    let copy = if changed {
        disowned(workspace, project, edit.kind, &edit.name)
    } else {
        Ok(Record::Untouched)
    };
    Edited { changed, copy }
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

/// What removing an artifact did to the record of an install of it.
#[derive(Debug)]
pub struct Removed {
    /// Dropped when the copy had been installed: it is gone, and the record
    /// would be inherited by whatever is written under its name next.
    pub copy: Result<Record, InstallError>,
}

/// Delete an artifact: a skill's whole directory, or a rule's file and any
/// folder it leaves empty. Then drop the record of an install of it.
/// `workspace` is the one consulted, as for [`add`].
pub fn remove(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    target: &Target,
) -> Result<Removed, LifecycleError> {
    let directory = inside(project, target)?;
    match target.kind.layout() {
        Layout::Directory { .. } => {
            fs::remove_dir_all(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
        }
        Layout::Files { .. } => {
            fs::remove_file(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
            prune(&directory, target.path.parent());
        }
    }
    Ok(Removed {
        copy: disowned(workspace, project, target.kind, &target.name),
    })
}

/// What renaming an artifact made, and what it did to the records under its
/// old name and its new one.
#[derive(Debug)]
pub struct Renamed {
    /// The artifact under its new name.
    pub target: Target,
    /// The record of the copy under its old name, dropped: renamed, it is no
    /// longer what was installed.
    pub copy: Result<Record, InstallError>,
    /// A record under the new name, from an install that is no longer there:
    /// stale, and left in place it would make what was just renamed look like
    /// something Mindflayer put there.
    pub stale: Result<Record, InstallError>,
}

/// Give an artifact a new name: a skill's folder and the `name` it declares
/// together, so it still passes `validate`; a rule's file, filed by its new
/// route.
///
/// In a skill's manifest the `name` line is the only thing rewritten. The rest
/// is somebody's writing — comments, key order, the description — and is left
/// byte for byte, and the result is parsed back to make sure the one line
/// changed what it was meant to and nothing else.
///
/// Then the records under both names are dropped. `workspace` is the one
/// consulted, as for [`add`].
pub fn rename(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    target: &Target,
    to: &str,
) -> Result<Renamed, LifecycleError> {
    let issues = artifact::name_issues(to);
    if !issues.is_empty() {
        return Err(LifecycleError::Name {
            name: to.to_owned(),
            issues,
        });
    }
    let directory = inside(project, target)?;
    // Matched on the kind, so a third one has to say how it is renamed.
    let renamed = match target.kind {
        Kind::Skill => rename_skill(project, target, &directory, to),
        Kind::Rule => rename_rule(project, target, &directory, to),
    }?;
    let copy = disowned(workspace, project, target.kind, &target.name);
    let stale = disowned(workspace, project, target.kind, &renamed.name);
    Ok(Renamed {
        target: renamed,
        copy,
        stale,
    })
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
    // Held to the strict rules as it will be, the rules `add` and `install`
    // hold a skill to: what is left behind is a skill Mindflayer wrote, if
    // only its name, and one an agent would skip is not left behind. Judged as
    // renamed, because its name and folder are what a rename fixes.
    if let Some(manifest) = artifact.manifest() {
        let renamed = SkillManifest {
            name: to.to_owned(),
            ..manifest.clone()
        };
        let issues = artifact::strict_issues(&renamed, to);
        if !issues.is_empty() {
            return Err(LifecycleError::Unfit {
                name: artifact.name().to_owned(),
                issues,
            });
        }
    }

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

    // A rule's front matter holds no name, so there is nothing inside it to
    // rewrite, and one that did not read before a move does not read after it
    // either.
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

/// Drop the record `workspace` keeps of an install of `name` into `project`.
fn disowned(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
) -> Result<Record, InstallError> {
    let Some(workspace) = workspace else {
        return Ok(Record::Untouched);
    };
    Ok(if install::disown(workspace, project, kind, name)? {
        Record::Dropped
    } else {
        Record::Untouched
    })
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
    #[error("`{name}` cannot be renamed until it is fixed: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
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
