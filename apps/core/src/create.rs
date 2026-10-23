//! Writing a new artifact into a mind project.
//!
//! The one way into a project that is not a copy from the shelf: a skill or a
//! rule somebody is about to write, scaffolded so that it passes `validate`
//! from the moment it exists. Without a template, what is written is the
//! least that does — a skill's front matter and a heading, a rule's opening
//! line. With one, the template is copied and the name and description filled
//! in.
//!
//! Nothing here records anything in the ledger. A created artifact is its
//! author's work, not something Mindflayer installed, so [`crate::install`]
//! treats it as foreign: never overwritten by a shelf entry of the same name,
//! never removed by unticking one.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::frontmatter::{self, FrontMatterError};
use crate::kind::{Kind, Layout};
use crate::skill::{SkillManifest, MAX_DESCRIPTION_LEN};
use crate::template::{Template, TemplateError};
use crate::workspace::MindProject;

/// What a template writes where the name goes.
pub const NAME_PLACEHOLDER: &str = "{{name}}";
/// What a template writes where the description goes.
pub const DESCRIPTION_PLACEHOLDER: &str = "{{description}}";

/// Create an artifact of `kind` called `name` in `project`, and load it back.
///
/// `description` is the one line saying what the artifact is: a skill's
/// `description`, a rule's opening line. Both kinds need one — a skill without
/// it is invalid and a rule without an opening line is an empty file — so it
/// is asked for rather than left as a placeholder somebody forgets to replace.
///
/// Nothing already there is written over, including a folder that is not an
/// artifact at all: it is somebody's, and a scaffold that clobbered it would
/// be the worst kind of convenience.
pub fn create(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
) -> Result<Artifact, CreateError> {
    scaffold(project, kind, name, description, None)
}

/// The same, starting from a template.
///
/// A skill template is copied whole, and in its manifest the `name` and
/// `description` are set — whatever the template had for them is replaced —
/// and [`NAME_PLACEHOLDER`] and [`DESCRIPTION_PLACEHOLDER`] in the body are
/// filled in. A rule template is filled in the same way, and opens with the
/// description as a heading when it does not say where the description goes.
/// Other files are copied as they are: a placeholder in a script is the
/// script's business.
pub fn create_from(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: &Template,
) -> Result<Artifact, CreateError> {
    if template.kind != kind {
        return Err(CreateError::Template(TemplateError::WrongKind {
            name: template.name.clone(),
            is: template.kind,
            wanted: kind,
        }));
    }
    scaffold(project, kind, name, description, Some(template))
}

fn scaffold(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Artifact, CreateError> {
    let issues = artifact::name_issues(name);
    if !issues.is_empty() {
        return Err(CreateError::Name {
            name: name.to_owned(),
            issues,
        });
    }
    let description = description.trim();
    check_description(kind, description)?;

    let directory = project.directory_for(kind);
    // Matched on the kind rather than its layout, so a third kind has to say
    // how it is scaffolded before it compiles.
    match kind {
        Kind::Skill => create_skill(project, &directory, name, description, template),
        Kind::Rule => create_rule(project, &directory, name, description, template),
    }
}

/// A skill: its own directory, holding a manifest with front matter.
fn create_skill(
    project: &MindProject,
    directory: &Path,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Artifact, CreateError> {
    // Skills are flat. A skill's directory belongs to it, scripts and
    // references included, so a `/` would file one skill inside another's
    // assets, where discovery never looks.
    if name.contains('/') {
        return Err(CreateError::Nested {
            name: name.to_owned(),
        });
    }
    let Layout::Directory { manifest } = Kind::Skill.layout() else {
        unreachable!("a skill is a directory");
    };

    // Everything is worked out before anything is written, so a template
    // whose manifest does not parse costs nothing but the error.
    let files = match template {
        None => vec![(
            PathBuf::from(manifest),
            skill_manifest(name, description, "", "")
                .map_err(|reason| CreateError::Manifest { reason })?
                .into_bytes(),
        )],
        Some(template) => from_template(template, manifest, name, description)?,
    };

    create_dir(directory)?;
    let folder = directory.join(name);
    // `create_dir`, not `create_dir_all`: failing when the folder is already
    // there is the check, with nothing in between for another write to slip
    // into.
    fs::create_dir(&folder).map_err(|source| match source.kind() {
        io::ErrorKind::AlreadyExists => CreateError::Exists {
            path: folder.clone(),
        },
        _ => CreateError::Write {
            path: folder.clone(),
            source,
        },
    })?;

    let written = files.iter().try_for_each(|(route, contents)| {
        let path = folder.join(route);
        if let Some(parent) = path.parent() {
            create_dir(parent)?;
        }
        write_new(&path, contents)
    });
    if let Err(error) = written {
        // The folder was made a moment ago, by this call, so everything in it
        // is this call's: leaving it would be a skill directory with half a
        // skill in it.
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }

    // Read back rather than trusted, so what is reported is what a listing
    // will find.
    Ok(Artifact::skill(folder, project.root())?)
}

/// A template's files, with its manifest filled in.
fn from_template(
    template: &Template,
    manifest: &str,
    name: &str,
    description: &str,
) -> Result<Vec<(PathBuf, Vec<u8>)>, CreateError> {
    let mut files = template.skill_files()?;
    let Some((_, source)) = files
        .iter_mut()
        .find(|(route, _)| route == Path::new(manifest))
    else {
        return Err(CreateError::Manifest {
            reason: format!("the template `{}` has no {manifest}", template.name),
        });
    };

    let text = String::from_utf8(std::mem::take(source)).map_err(|_| CreateError::Manifest {
        reason: format!("the template's {manifest} is not UTF-8 text"),
    })?;
    // A manifest with no front matter is all body: it gets one.
    let (front_matter, body) = match frontmatter::split(&text) {
        Ok(document) => (document.front_matter, document.body),
        Err(FrontMatterError::Missing) => ("", text.as_str()),
        Err(error) => {
            return Err(CreateError::Manifest {
                reason: format!("the template's {manifest}: {error}"),
            })
        }
    };
    let front_matter = frontmatter::without_keys(front_matter, &["name", "description"]);
    let body = fill(body, name, description);
    *source = skill_manifest(name, description, &front_matter, &body)
        .map_err(|reason| CreateError::Manifest {
            reason: format!("the template `{}`: {reason}", template.name),
        })?
        .into_bytes();
    Ok(files)
}

/// A rule: one markdown file, filed wherever its route says.
fn create_rule(
    project: &MindProject,
    directory: &Path,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Artifact, CreateError> {
    let Layout::Files { extension } = Kind::Rule.layout() else {
        unreachable!("a rule is a file");
    };

    let text = match template {
        None => format!("# {description}\n"),
        Some(template) => {
            let text = template.rule_text()?;
            // A rule's opening line is what every listing shows, so a template
            // that does not say where the description goes gets it on top.
            if text.contains(DESCRIPTION_PLACEHOLDER) {
                fill(&text, name, description)
            } else {
                format!("# {description}\n\n{}", fill(&text, name, description))
            }
        }
    };

    // The name is a route, so `git/no-force-push` lands in a `git` folder.
    // Every segment has already passed the name check, so none of them is `.`
    // or `..` and the file cannot land outside the rules directory.
    let file = directory.join(format!("{name}.{extension}"));
    if let Some(parent) = file.parent() {
        create_dir(parent)?;
    }
    write_new(&file, text.as_bytes())?;

    Ok(Artifact::rule(file, name.to_owned(), project.root())?)
}

/// Refuse a description that would not survive as what it is meant to be.
fn check_description(kind: Kind, description: &str) -> Result<(), CreateError> {
    if description.is_empty() {
        return Err(CreateError::DescriptionEmpty { kind });
    }
    // One line, because a rule's is its opening line and a skill's is the
    // sentence an agent reads to decide whether to load the rest. Anything
    // longer belongs in the file itself.
    if description.contains(['\n', '\r']) {
        return Err(CreateError::DescriptionNotOneLine);
    }
    let length = description.chars().count();
    if length > MAX_DESCRIPTION_LEN {
        return Err(CreateError::DescriptionTooLong { length });
    }
    Ok(())
}

/// A skill's manifest: `name` and `description` first, then whatever else the
/// front matter says, then the body — checked by parsing it back.
///
/// The two keys are serialized rather than formatted: a description is free
/// text, and one containing `: ` or opening with a quote would otherwise write
/// YAML that does not parse back into what was typed.
fn skill_manifest(
    name: &str,
    description: &str,
    rest_of_front_matter: &str,
    body: &str,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Declared<'a> {
        name: &'a str,
        description: &'a str,
    }

    let declared =
        serde_yaml_ng::to_string(&Declared { name, description }).map_err(|e| e.to_string())?;
    let front_matter = format!("{declared}{rest_of_front_matter}");
    let parsed = SkillManifest::parse(&front_matter)
        .map_err(|error| format!("its front matter does not parse: {error}"))?;
    if parsed.name != name || parsed.description != description {
        return Err(String::from(
            "its front matter sets `name` or `description` in a way that cannot be replaced",
        ));
    }
    // A body with nothing in it gets the heading a skill without a template
    // opens with, so a manifest is never front matter alone.
    let body = if body.trim().is_empty() {
        format!("\n# {name}\n")
    } else {
        body.to_owned()
    };
    Ok(format!("---\n{front_matter}---\n{body}"))
}

/// A template's text with the placeholders filled in.
fn fill(text: &str, name: &str, description: &str) -> String {
    text.replace(NAME_PLACEHOLDER, name)
        .replace(DESCRIPTION_PLACEHOLDER, description)
}

fn create_dir(path: &Path) -> Result<(), CreateError> {
    fs::create_dir_all(path).map_err(|source| CreateError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Write a file that must not already exist.
///
/// `create_new` rather than a prior `exists()` check: the check and the write
/// are one operation, so nothing that appears in between is overwritten.
fn write_new(path: &Path, contents: &[u8]) -> Result<(), CreateError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| match source.kind() {
            io::ErrorKind::AlreadyExists => CreateError::Exists {
                path: path.to_path_buf(),
            },
            _ => CreateError::Write {
                path: path.to_path_buf(),
                source,
            },
        })?;
    file.write_all(contents)
        .map_err(|source| CreateError::Write {
            path: path.to_path_buf(),
            source,
        })
}

/// Why an artifact could not be created.
#[derive(Debug, Error)]
pub enum CreateError {
    #[error("{}", issue_list(.issues))]
    Name {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}`: a skill's name cannot contain `/`; skills are flat, and only rules are filed in folders")]
    Nested { name: String },
    #[error("a {kind} needs a description: one line saying what it is")]
    DescriptionEmpty { kind: Kind },
    #[error("the description has to be one line; the rest belongs in the file")]
    DescriptionNotOneLine,
    #[error(
        "the description is {length} characters, over the {MAX_DESCRIPTION_LEN} character limit"
    )]
    DescriptionTooLong { length: usize },
    #[error("{path}: already exists, and is left alone")]
    Exists { path: PathBuf },
    #[error("the manifest cannot be written: {reason}")]
    Manifest { reason: String },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}
