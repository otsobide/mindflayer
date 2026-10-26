//! One artifact of any kind: what it declares, where it lives, and whether it
//! is well formed.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::frontmatter::{self, FrontMatterError};
use crate::kind::{Kind, Layout};
use crate::rule::{Lenient, RuleManifest};
use crate::skill::{
    Compatibility, SkillManifest, MAX_COMPATIBILITY_LEN, MAX_DESCRIPTION_LEN, MAX_NAME_SEGMENT_LEN,
};

/// What an artifact declared about itself.
///
/// This enum, not a field on [`Artifact`], is where the kinds differ — and it
/// *is* the kind, so an artifact cannot carry a discriminant that disagrees
/// with what was parsed, because there is only one. Each kind's payload says
/// what that kind requires: a skill without a description is not a state this
/// type can hold, while a rule's description is as optional as the rest of its
/// front matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared {
    /// A skill's front matter.
    Skill(SkillManifest),
    /// A rule's front matter, which is optional: most rules are a markdown
    /// file and that is all, and declare nothing.
    Rule(RuleManifest),
}

impl Declared {
    /// The kind this is a declaration for.
    pub fn kind(&self) -> Kind {
        match self {
            Declared::Skill(_) => Kind::Skill,
            Declared::Rule(_) => Kind::Rule,
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
        // Reading it never fails, as a skill's can: a rule's front matter is
        // optional, so whatever cannot be read as front matter is content.
        let (manifest, body) = RuleManifest::read(&source);
        // The opening line of the body, not of the file, and not a declared
        // description: the body is what an agent reads once the front matter
        // is taken off, so its opening line is what the agent meets first.
        Ok(Self {
            name,
            path,
            project: project.into(),
            declared: Declared::Rule(manifest),
            summary: opening_line(body),
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

    /// `skill:commit-style`: the name qualified by its kind.
    pub fn qualified_name(&self) -> String {
        format!(
            "{}{}{}",
            self.kind().slug(),
            crate::catalog::QUALIFIER,
            self.name
        )
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
    /// opening line of its body for a rule, and absent when there is nothing
    /// to show.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// The description, when this declares one: always for a skill, and for
    /// a rule only when its front matter says.
    pub fn description(&self) -> Option<&str> {
        match &self.declared {
            Declared::Skill(manifest) => Some(&manifest.description),
            Declared::Rule(manifest) => manifest.description.stated().map(String::as_str),
        }
    }

    /// The skill front matter, when this is a skill.
    pub fn manifest(&self) -> Option<&SkillManifest> {
        match &self.declared {
            Declared::Skill(manifest) => Some(manifest),
            Declared::Rule(_) => None,
        }
    }

    /// The markdown an agent reads: the body after the front matter, or the
    /// whole file when there is none.
    ///
    /// What a renderer puts in front of an agent, so a rule's front matter is
    /// never pasted into a file that has no use for it, and what `show` prints
    /// beneath what the front matter declares.
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
            // Decided again from the file as it is now, by the same reading
            // that loaded it.
            Declared::Rule(_) => Ok(RuleManifest::read(&source).1.to_owned()),
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
        match &self.declared {
            Declared::Skill(manifest) => skill_issues(
                manifest,
                self.path.file_name().and_then(|name| name.to_str()),
            ),
            Declared::Rule(manifest) => {
                // Every kind is looked up by name, so every kind is checked
                // for one.
                let mut issues = name_issues(&self.name);
                // A rule's front matter is optional, so the one thing that
                // makes it unusable is having nothing to say. `summary` is
                // None only when no line of the body held any text.
                if self.summary.is_none() {
                    issues.push(ValidationIssue::Empty);
                }
                issues.extend(rule_issues(manifest));
                issues
            }
        }
    }
}

/// Everything wrong with a skill that declares `manifest`, filed in a folder
/// called `folder` when that folder has a name to compare.
///
/// Apart from [`Artifact::validate`] so that a skill can be judged where it is
/// going rather than where it is: [`strict_issues`].
fn skill_issues(manifest: &SkillManifest, folder: Option<&str>) -> Vec<ValidationIssue> {
    // Every kind is looked up by name, so every kind is checked for one.
    let mut issues = name_issues(&manifest.name);

    // The directory name is how an agent locates the skill it was told to
    // invoke, so a mismatch means one that lists fine and never loads.
    if let Some(folder) = folder {
        if folder != manifest.name {
            issues.push(ValidationIssue::NameDirectoryMismatch {
                name: manifest.name.clone(),
                directory: folder.to_owned(),
            });
        }
    }
    let description = manifest.description.trim();
    let length = description.chars().count();
    if description.is_empty() {
        issues.push(ValidationIssue::DescriptionEmpty);
    } else if length > MAX_DESCRIPTION_LEN {
        issues.push(ValidationIssue::DescriptionTooLong { length });
    }
    // Optional, but once written it has to say something and fit: an agent
    // may show it to the model when the skill is loaded.
    match &manifest.compatibility {
        Compatibility::Unstated => {}
        Compatibility::Stated(compatibility) => {
            let compatibility = compatibility.trim();
            let length = compatibility.chars().count();
            if compatibility.is_empty() {
                issues.push(ValidationIssue::CompatibilityEmpty);
            } else if length > MAX_COMPATIBILITY_LEN {
                issues.push(ValidationIssue::CompatibilityTooLong { length });
            }
        }
        Compatibility::NotAString => issues.push(ValidationIssue::CompatibilityNotAString),
    }
    // Read leniently, so the skill still lists; said here, so the entries an
    // agent expecting strings would trip on are known.
    if manifest.metadata.not_a_map {
        issues.push(ValidationIssue::MetadataNotAMap);
    }
    issues.extend(
        manifest
            .metadata
            .ignored
            .iter()
            .map(|key| ValidationIssue::MetadataNotAString { key: key.clone() }),
    );
    issues.extend(
        manifest
            .metadata
            .repeated
            .iter()
            .map(|key| ValidationIssue::MetadataRepeated { key: key.clone() }),
    );
    issues
}

/// What a rule's front matter left unread, each only worth a warning: what is
/// not read is still in the file, and the rule still says what it says.
///
/// And each refused when Mindflayer writes a rule from a template, which is
/// the time to find out that its front matter would not be read.
pub(crate) fn rule_issues(manifest: &RuleManifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Some(reason) = &manifest.unparsed {
        issues.push(ValidationIssue::FrontMatterUnparsed {
            reason: reason.clone(),
        });
    }
    if manifest.paths == Lenient::Ignored {
        issues.push(ValidationIssue::PathsNotStrings);
    }
    if manifest.description == Lenient::Ignored {
        issues.push(ValidationIssue::DescriptionNotAString);
    }
    issues
}

/// Whichever strict rules ([`ValidationIssue::is_strict`]) a skill declaring
/// `manifest` breaks once it is filed in a folder called `folder`.
///
/// Asked of the skill as it is about to be written, rather than as it sits:
/// the folder it is filed in is the one Mindflayer is about to name, which is
/// not always the one it was read from.
pub(crate) fn strict_issues(manifest: &SkillManifest, folder: &str) -> Vec<ValidationIssue> {
    skill_issues(manifest, Some(folder))
        .into_iter()
        .filter(ValidationIssue::is_strict)
        .collect()
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

/// The first line of a declared description that carries any text.
///
/// Not literally the first line: a description written as a YAML block scalar
/// often opens with a blank one, and listing that skill with no summary at all
/// would be a formatting accident showing through.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
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

/// Everything wrong with a name, whatever kind it names.
///
/// Shared by [`Artifact::validate`], which judges a name found on disk, and
/// [`crate::create`], which refuses to write one: a name this tool creates has
/// to pass the check this tool applies, and one rule cannot drift from itself.
pub(crate) fn name_issues(name: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if name.is_empty() {
        issues.push(ValidationIssue::NameEmpty);
        return issues;
    }
    for segment in name.split('/') {
        if !is_kebab_case(segment) {
            issues.push(ValidationIssue::NameNotKebabCase {
                segment: segment.to_owned(),
            });
        }
        // Characters, not bytes: the limit exists to keep a name readable,
        // and counting bytes would reject a name in a script that spends more
        // than one per character at half the length.
        let length = segment.chars().count();
        if length > MAX_NAME_SEGMENT_LEN {
            issues.push(ValidationIssue::NameSegmentTooLong {
                segment: segment.to_owned(),
                length,
            });
        }
    }
    issues
}

/// Every problem with a name, as one sentence.
pub(crate) fn issue_list(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Lowercase letters and digits, in words joined by single hyphens —
/// `^[a-z0-9]+(-[a-z0-9]+)*$`, the Agent Skills rule for a skill's name: what
/// an agent can be asked to invoke without quoting or case folding.
///
/// Split on the hyphen rather than checked character by character, because
/// that is what makes a leading, trailing or doubled hyphen an empty word, and
/// so wrong, without a rule of its own for each.
fn is_kebab_case(segment: &str) -> bool {
    segment.split('-').all(|word| {
        !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
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
    CompatibilityEmpty,
    CompatibilityTooLong { length: usize },
    CompatibilityNotAString,
    MetadataNotAMap,
    MetadataNotAString { key: String },
    MetadataRepeated { key: String },
    Empty,
    FrontMatterUnparsed { reason: String },
    PathsNotStrings,
    DescriptionNotAString,
}

impl ValidationIssue {
    /// Whether this breaks one of the strict rules: the ones Mindflayer will
    /// not break when it writes a skill into a project — creating, renaming
    /// or installing one — rather than only report.
    ///
    /// The name, the folder it is filed in, and the description, because
    /// those are what agents refuse or trip on: Codex skips a skill whose name
    /// is over 64 characters or whose description is empty, and an agent finds
    /// a skill by its folder or by its name, so one whose two disagree is
    /// found by one and not the other. Not `compatibility` and `metadata`,
    /// which an agent reads past.
    pub fn is_strict(&self) -> bool {
        matches!(
            self,
            ValidationIssue::NameEmpty
                | ValidationIssue::NameNotKebabCase { .. }
                | ValidationIssue::NameSegmentTooLong { .. }
                | ValidationIssue::NameDirectoryMismatch { .. }
                | ValidationIssue::DescriptionEmpty
                | ValidationIssue::DescriptionTooLong { .. }
        )
    }

    /// Whether this is only worth a warning: something an agent reads past,
    /// so the artifact still works, and `validate` says so without calling it
    /// invalid.
    ///
    /// `compatibility` and `metadata`, the same ones `is_strict` lets through:
    /// were they invalid, a skill installed with nested metadata would fail
    /// its project's `validate` from then on, and the only way to clear it
    /// would be to edit the copy, which stops Mindflayer from updating it.
    /// And whatever a rule's front matter leaves unread, since a rule needs
    /// none to be read: front matter that does not parse is content, and a key
    /// written in another shape is ignored. Matched in full rather than with
    /// `matches!`, so a new issue has to say which it is.
    pub fn is_warning(&self) -> bool {
        match self {
            ValidationIssue::CompatibilityEmpty
            | ValidationIssue::CompatibilityTooLong { .. }
            | ValidationIssue::CompatibilityNotAString
            | ValidationIssue::MetadataNotAMap
            | ValidationIssue::MetadataNotAString { .. }
            | ValidationIssue::MetadataRepeated { .. }
            | ValidationIssue::FrontMatterUnparsed { .. }
            | ValidationIssue::PathsNotStrings
            | ValidationIssue::DescriptionNotAString => true,
            ValidationIssue::NameEmpty
            | ValidationIssue::NameNotKebabCase { .. }
            | ValidationIssue::NameSegmentTooLong { .. }
            | ValidationIssue::NameDirectoryMismatch { .. }
            | ValidationIssue::DescriptionEmpty
            | ValidationIssue::DescriptionTooLong { .. }
            | ValidationIssue::Empty => false,
        }
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::NameEmpty => write!(f, "the name is empty"),
            ValidationIssue::NameNotKebabCase { segment } => write!(
                f,
                "`{segment}` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens"
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
            ValidationIssue::CompatibilityEmpty => write!(
                f,
                "`compatibility` is empty; leave it out when the skill needs nothing in particular"
            ),
            ValidationIssue::CompatibilityTooLong { length } => write!(
                f,
                "`compatibility` is {length} characters, over the {MAX_COMPATIBILITY_LEN} character limit"
            ),
            ValidationIssue::CompatibilityNotAString => write!(
                f,
                "`compatibility` is not a string, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataNotAMap => write!(
                f,
                "`metadata` is not a map of strings, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataNotAString { key } => write!(
                f,
                "`metadata` entry `{key}` is not a string, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataRepeated { key } => write!(
                f,
                "`metadata` entry `{key}` is written more than once, so only the first is read"
            ),
            ValidationIssue::Empty => write!(f, "the file has no content beyond any front matter"),
            ValidationIssue::FrontMatterUnparsed { reason } => write!(
                f,
                "the front matter does not parse, so it is read as content, fences and all: {reason}"
            ),
            ValidationIssue::PathsNotStrings => write!(
                f,
                "`paths` is not a pattern or a list of patterns, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::DescriptionNotAString => write!(
                f,
                "`description` is not a string, so it is ignored, and left in the file as written"
            ),
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

#[cfg(test)]
#[path = "artifact_test.rs"]
mod tests;
