//! Starting points for `mind add`: a skill folder or a rule file to begin
//! from, instead of the least that passes `validate`.
//!
//! Templates are Mindflayer's, not the agents', so they live inside the
//! markers rather than beside the code: `.mind/templates/` for one project,
//! `.mindflayer/templates/` for every project a workspace manages. A skill
//! template is a folder holding a `SKILL.md` and whatever else the skill
//! should start with; a rule template is one markdown file. Of two templates
//! of one kind and one name, the closer wins — the project's over the
//! workspace's over the one built in — the way a saved workflow or a git
//! setting is looked up.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact;
use crate::kind::{Kind, Layout};
use crate::workspace::{FlayerWorkspace, MindProject, WorkspaceError};

/// The folder inside `.mind` or `.mindflayer` that holds templates.
pub const TEMPLATES_DIR: &str = "templates";

/// Where a template comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Shipped with Mindflayer.
    BuiltIn,
    /// A skill folder or a rule file on disk.
    Path(PathBuf),
}

/// A template `mind add` can start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub kind: Kind,
    pub origin: Origin,
}

/// One file a built-in template writes: its route, and what it holds.
type File = (&'static str, &'static str);

/// The templates that ship with Mindflayer, as the files each one writes.
///
/// `full` is the shape the skill format sets aside room for: a manifest with
/// sections to fill in, a `scripts/` folder for what the skill runs and a
/// `references/` folder for what it reads on demand. The folders hold a
/// `.gitkeep` rather than a README, so they are committed empty instead of
/// holding text an agent would load as if it were reference material.
const BUILT_IN: &[(&str, Kind, &[File])] = &[(
    "full",
    Kind::Skill,
    &[
        (
            "SKILL.md",
            "---\n---\n\n# {{name}}\n\n{{description}}\n\n## When to use\n\n## Steps\n\n\
             ## Scripts and references\n\n\
             Run what is in `scripts/`; read what is in `references/` when a step points there.\n",
        ),
        ("scripts/.gitkeep", ""),
        ("references/.gitkeep", ""),
    ],
)];

impl Template {
    /// Every file in a skill template, as a route below it and what it holds.
    pub(crate) fn skill_files(&self) -> Result<Vec<(PathBuf, Vec<u8>)>, TemplateError> {
        match &self.origin {
            Origin::BuiltIn => Ok(built_in(&self.name)
                .iter()
                .map(|(route, contents)| (PathBuf::from(route), contents.as_bytes().to_vec()))
                .collect()),
            Origin::Path(folder) => {
                let mut files = Vec::new();
                collect(folder, Path::new(""), &mut files)?;
                Ok(files)
            }
        }
    }

    /// The text of a rule template.
    pub(crate) fn rule_text(&self) -> Result<String, TemplateError> {
        match &self.origin {
            Origin::BuiltIn => Ok(built_in(&self.name)
                .first()
                .map_or_else(String::new, |(_, text)| (*text).to_owned())),
            Origin::Path(file) => fs::read_to_string(file).map_err(|source| TemplateError::Read {
                path: file.clone(),
                source,
            }),
        }
    }
}

/// Every template `project` can use: its own, its workspace's, and the ones
/// built in, with a closer one hiding a further one of the same kind and
/// name. Ordered by kind, then by name.
pub fn templates(project: &MindProject) -> Result<Vec<Template>, TemplateError> {
    let mut found: Vec<Template> = Vec::new();
    let mut offer = |template: Template| {
        let hidden = found
            .iter()
            .any(|other| other.kind == template.kind && other.name == template.name);
        if !hidden {
            found.push(template);
        }
    };

    for directory in search_path(project)? {
        for template in on_disk(&directory)? {
            offer(template);
        }
    }
    for (name, kind, _) in BUILT_IN {
        offer(Template {
            name: (*name).to_owned(),
            kind: *kind,
            origin: Origin::BuiltIn,
        });
    }

    found.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    Ok(found)
}

/// The template of `kind` called `name`, from wherever is closest.
pub fn find(project: &MindProject, kind: Kind, name: &str) -> Result<Template, TemplateError> {
    // A name, not a path: it is looked up in known places and must not be
    // able to reach out of them.
    if name.contains('/') || !artifact::name_issues(name).is_empty() {
        return Err(TemplateError::BadName {
            name: name.to_owned(),
        });
    }
    let all = templates(project)?;
    if let Some(found) = all.iter().find(|t| t.kind == kind && t.name == name) {
        return Ok(found.clone());
    }
    // A template of the other kind is a different mistake from none at all,
    // and saying which one it is saves a trip to the folder.
    if let Some(other) = all.iter().find(|t| t.name == name) {
        return Err(TemplateError::WrongKind {
            name: name.to_owned(),
            is: other.kind,
            wanted: kind,
        });
    }
    Err(TemplateError::Unknown {
        name: name.to_owned(),
        kind,
        available: all
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.name.clone())
            .collect(),
    })
}

/// Where templates are looked for, closest first.
///
/// A workspace's templates count for a project it manages, found by walking
/// up — the same rule every other workspace-wide thing follows — and for the
/// workspace's own artifacts, whose holder sits at the workspace's root.
fn search_path(project: &MindProject) -> Result<Vec<PathBuf>, TemplateError> {
    let mut directories = vec![project.mind_dir().join(TEMPLATES_DIR)];
    if let Some(workspace) = FlayerWorkspace::locate(project.root())? {
        if workspace.root() == project.root() || workspace.is_linked(project.root()) {
            directories.push(workspace.flayer_dir().join(TEMPLATES_DIR));
        }
    }
    Ok(directories)
}

/// The templates in one folder: each folder holding a manifest is a skill
/// template, each markdown file a rule template. A folder that is not there
/// holds none.
fn on_disk(directory: &Path) -> Result<Vec<Template>, TemplateError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(TemplateError::Read {
                path: directory.to_path_buf(),
                source,
            })
        }
    };

    let mut found = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| TemplateError::Read {
                path: directory.to_path_buf(),
                source,
            })?
            .path();
        for kind in Kind::ALL {
            let name = match kind.layout() {
                Layout::Directory { manifest } if path.join(manifest).is_file() => {
                    path.file_name().and_then(|name| name.to_str())
                }
                Layout::Files { extension }
                    if path.is_file()
                        && path
                            .extension()
                            .is_some_and(|found| found.eq_ignore_ascii_case(extension)) =>
                {
                    path.file_stem().and_then(|stem| stem.to_str())
                }
                _ => None,
            };
            if let Some(name) = name.filter(|name| !name.starts_with('.')) {
                found.push(Template {
                    name: name.to_owned(),
                    kind,
                    origin: Origin::Path(path.clone()),
                });
            }
        }
    }
    Ok(found)
}

/// Every file below `folder`, with its route from the template's top.
fn collect(
    folder: &Path,
    route: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), TemplateError> {
    let read_error = |source| TemplateError::Read {
        path: folder.to_path_buf(),
        source,
    };
    let mut entries = fs::read_dir(folder)
        .map_err(read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_error)?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let below = route.join(entry.file_name());
        // The entry's own type, so a symlinked folder is read as the file it
        // cannot be, and fails, rather than walked into a loop.
        if entry.file_type().map_err(read_error)?.is_dir() {
            collect(&path, &below, files)?;
        } else {
            let contents = fs::read(&path).map_err(|source| TemplateError::Read {
                path: path.clone(),
                source,
            })?;
            files.push((below, contents));
        }
    }
    Ok(())
}

fn built_in(name: &str) -> &'static [File] {
    BUILT_IN
        .iter()
        .find(|(each, _, _)| *each == name)
        .map_or(&[], |(_, _, files)| files)
}

/// Why a template could not be found or read.
#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("`{name}` is not a template name: templates are named like artifacts, with no `/`")]
    BadName { name: String },
    #[error("no {kind} template called `{name}`{}", there_are(.available))]
    Unknown {
        name: String,
        kind: Kind,
        available: Vec<String>,
    },
    #[error("`{name}` is a {is} template, not a {wanted} one")]
    WrongKind {
        name: String,
        is: Kind,
        wanted: Kind,
    },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

/// "; there are full, runbook", or nothing when there are none.
fn there_are(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("; there are {}", names.join(", "))
    }
}
