//! One artifact of any kind: what it declares, where it lives, and whether it
//! is well formed.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::frontmatter::{self, FrontMatterError};
use crate::kind::{Kind, Layout};
use crate::skill::{SkillManifest, MAX_DESCRIPTION_LEN, MAX_NAME_SEGMENT_LEN};

/// What an artifact declared about itself.
///
/// This enum, not a field on [`Artifact`], is where the kinds differ — and it
/// *is* the kind, so an artifact cannot carry a discriminant that disagrees
/// with what was parsed, because there is only one. A rule that declares a
/// description is not a state this type can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared {
    /// A skill's front matter.
    Skill(SkillManifest),
    /// A rule declares nothing. It is a markdown file and that is all.
    Rule,
}

impl Declared {
    /// The kind this is a declaration for.
    pub fn kind(&self) -> Kind {
        match self {
            Declared::Skill(_) => Kind::Skill,
            Declared::Rule => Kind::Rule,
        }
    }
}

/// An artifact on disk.
///
/// The body is not held: it is the bulk of the file and a listing never shows
/// it. What a listing does show — one line saying what the thing is — is
/// captured at load, because the file had to be read anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    name: String,
    path: PathBuf,
    project: PathBuf,
    declared: Declared,
    summary: Option<String>,
}

impl Artifact {
    /// Load a skill from its directory.
    pub fn skill(
        directory: impl Into<PathBuf>,
        project: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactError> {
        let directory = directory.into();
        let path = directory.join(manifest_of(Kind::Skill));
        let source = read(&path)?;
        let document =
            frontmatter::split(&source).map_err(|source| ArtifactError::FrontMatter {
                path: path.clone(),
                source,
            })?;
        let manifest =
            SkillManifest::parse(document.front_matter).map_err(|source| ArtifactError::Parse {
                path: path.clone(),
                source,
            })?;

        // A skill says what it is, so nothing is derived: the summary is the
        // first line of the description its author wrote.
        let summary = first_line(&manifest.description);
        Ok(Self {
            name: manifest.name.clone(),
            path: directory,
            project: project.into(),
            declared: Declared::Skill(manifest),
            summary,
        })
    }

    /// Load a rule from its file, named by its route under the rules folder.
    ///
    /// The name is passed in rather than taken from the path because only the
    /// caller knows which folder the route is relative to.
    pub fn rule(
        file: impl Into<PathBuf>,
        name: String,
        project: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactError> {
        let path = file.into();
        let source = read(&path)?;
        // No front matter, by definition: a rule is a markdown file that gives
        // context and declares nothing. Its opening line is the closest thing
        // to a description it has, so that is what a listing shows.
        Ok(Self {
            name,
            path,
            project: project.into(),
            declared: Declared::Rule,
            summary: opening_line(&source),
        })
    }

    /// What kind of artifact this is.
    pub fn kind(&self) -> Kind {
        self.declared.kind()
    }

    /// The name it is referred to by.
    ///
    /// Declared, for a skill. For a rule, its route under the rules folder
    /// without the extension, so `git/no-force-push` is a name and the folders
    /// above it are grouping and nothing more.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `skill/commit-style`: the name qualified by its kind.
    pub fn qualified_name(&self) -> String {
        format!("{}/{}", self.kind().slug(), self.name)
    }

    /// The directory a skill is, or the file a rule is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The root of the mind project this was found in.
    pub fn project(&self) -> &Path {
        &self.project
    }

    /// The name of that project.
    pub fn project_name(&self) -> Option<&str> {
        self.project.file_name()?.to_str()
    }

    /// What this artifact declared.
    pub fn declared(&self) -> &Declared {
        &self.declared
    }

    /// One line saying what this is: declared by a skill, derived from the
    /// opening line for a rule, and absent when there is nothing to show.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// The description, for the kinds that declare one.
    pub fn description(&self) -> Option<&str> {
        match &self.declared {
            Declared::Skill(manifest) => Some(&manifest.description),
            Declared::Rule => None,
        }
    }

    /// The skill front matter, when this is a skill.
    pub fn manifest(&self) -> Option<&SkillManifest> {
        match &self.declared {
            Declared::Skill(manifest) => Some(manifest),
            Declared::Rule => None,
        }
    }

    /// The markdown an agent reads: the body after a skill's front matter, or
    /// the whole of a rule's file.
    pub fn contents(&self) -> Result<String, ArtifactError> {
        let path = self.file();
        let source = read(&path)?;
        match self.declared {
            Declared::Skill(_) => {
                let document =
                    frontmatter::split(&source).map_err(|source| ArtifactError::FrontMatter {
                        path: path.clone(),
                        source,
                    })?;
                Ok(document.body.to_owned())
            }
            Declared::Rule => Ok(source),
        }
    }

    /// The file this artifact was read from.
    pub fn file(&self) -> PathBuf {
        match self.kind().layout() {
            Layout::Directory { manifest } => self.path.join(manifest),
            Layout::Files { .. } => self.path.clone(),
        }
    }

    /// Everything wrong with this artifact, in the order it is worth fixing.
    ///
    /// Pure: every fact it judges was captured when the file was read, so
    /// checking cannot fail on its own and a caller never has to handle an
    /// error from asking whether something is valid.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Every kind is looked up by name, so every kind is checked for one.
        if self.name.is_empty() {
            issues.push(ValidationIssue::NameEmpty);
        } else {
            for segment in self.name.split('/') {
                if !is_kebab_case(segment) {
                    issues.push(ValidationIssue::NameNotKebabCase {
                        segment: segment.to_owned(),
                    });
                }
                if segment.chars().count() > MAX_NAME_SEGMENT_LEN {
                    issues.push(ValidationIssue::NameSegmentTooLong {
                        segment: segment.to_owned(),
                        length: segment.chars().count(),
                    });
                }
            }
        }

        match &self.declared {
            Declared::Skill(manifest) => {
                // The directory name is how an agent locates the skill it was
                // told to invoke, so a mismatch means one that lists fine and
                // never loads.
                if let Some(directory) = self.path.file_name().and_then(|name| name.to_str()) {
                    if directory != self.name {
                        issues.push(ValidationIssue::NameDirectoryMismatch {
                            name: self.name.clone(),
                            directory: directory.to_owned(),
                        });
                    }
                }
                let description = manifest.description.trim();
                if description.is_empty() {
                    issues.push(ValidationIssue::DescriptionEmpty);
                } else if description.chars().count() > MAX_DESCRIPTION_LEN {
                    issues.push(ValidationIssue::DescriptionTooLong {
                        length: description.chars().count(),
                    });
                }
            }
            Declared::Rule => {
                // A rule declares nothing, so there is exactly one thing left
                // that can be wrong with it: having nothing to say. `summary`
                // is None only when no line held any text.
                if self.summary.is_none() {
                    issues.push(ValidationIssue::Empty);
                }
            }
        }

        issues
    }
}

/// Read a file, naming it if that fails.
fn read(path: &Path) -> Result<String, ArtifactError> {
    fs::read_to_string(path).map_err(|source| ArtifactError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// The manifest file a directory-shaped kind holds.
fn manifest_of(kind: Kind) -> &'static str {
    match kind.layout() {
        Layout::Directory { manifest } => manifest,
        Layout::Files { .. } => unreachable!("{kind} is not a directory-shaped kind"),
    }
}

/// The first line of a declared description.
fn first_line(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// The first line of a document that carries any text.
///
/// Leading `#` are stripped, so a document that opens with a heading is
/// summarised by that heading rather than by a row of hashes — which is what
/// most markdown files opening with a title actually want.
fn opening_line(source: &str) -> Option<String> {
    source
        .lines()
        .map(|line| line.trim_start().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Lowercase letters, digits and inner hyphens: what an agent can be asked to
/// invoke without quoting or case folding.
fn is_kebab_case(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Something that stops an artifact from being usable, or will.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    NameEmpty,
    NameNotKebabCase { segment: String },
    NameSegmentTooLong { segment: String, length: usize },
    NameDirectoryMismatch { name: String, directory: String },
    DescriptionEmpty,
    DescriptionTooLong { length: usize },
    Empty,
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::NameEmpty => write!(f, "the name is empty"),
            ValidationIssue::NameNotKebabCase { segment } => write!(
                f,
                "`{segment}` is not usable as a name: only lowercase letters, digits and inner hyphens are"
            ),
            ValidationIssue::NameSegmentTooLong { segment, length } => write!(
                f,
                "`{segment}` is {length} characters, over the {MAX_NAME_SEGMENT_LEN} character limit"
            ),
            ValidationIssue::NameDirectoryMismatch { name, directory } => write!(
                f,
                "`name` is `{name}` but the directory is `{directory}`; they have to match"
            ),
            ValidationIssue::DescriptionEmpty => write!(f, "`description` is empty"),
            ValidationIssue::DescriptionTooLong { length } => write!(
                f,
                "`description` is {length} characters, over the {MAX_DESCRIPTION_LEN} character limit"
            ),
            ValidationIssue::Empty => write!(f, "the file has no content"),
        }
    }
}

/// Why an artifact could not be loaded. Every variant names the file, because
/// a catalog reports these next to each other and the path tells them apart.
#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}")]
    FrontMatter {
        path: PathBuf,
        #[source]
        source: FrontMatterError,
    },
    #[error("{path}: invalid front matter: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },
}

impl ArtifactError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            ArtifactError::Read { path, .. }
            | ArtifactError::FrontMatter { path, .. }
            | ArtifactError::Parse { path, .. } => path,
        }
    }
}
