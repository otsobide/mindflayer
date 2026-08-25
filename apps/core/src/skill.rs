//! A single skill: its front matter, where it lives, and whether it is valid.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::frontmatter::{self, FrontMatterError};

/// The file that makes a directory a skill.
pub const SKILL_FILE: &str = "SKILL.md";

/// The longest name an agent will load.
pub const MAX_NAME_LEN: usize = 64;
/// The longest description an agent will load.
pub const MAX_DESCRIPTION_LEN: usize = 1024;

/// The front matter of a `SKILL.md`, as written.
///
/// Unknown keys are kept out of the struct rather than rejected: the format
/// grows, and a skill carrying a key this version has never heard of is still
/// a skill worth listing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkillManifest {
    /// The name the agent invokes the skill by.
    pub name: String,
    /// When to use the skill. This is the only text an agent sees before
    /// deciding to load the rest, which is why an empty one is a hard error.
    pub description: String,
    /// The tools the skill is allowed to use, if it narrows them.
    #[serde(default, rename = "allowed-tools", deserialize_with = "tool_list")]
    pub allowed_tools: Option<Vec<String>>,
    /// An SPDX identifier, when the skill declares one.
    #[serde(default)]
    pub license: Option<String>,
}

impl SkillManifest {
    /// Parse front matter that has already been separated from its body.
    pub fn parse(front_matter: &str) -> Result<Self, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(front_matter)
    }
}

/// `allowed-tools` is written both as a YAML list and as one comma separated
/// string, and both spellings mean the same thing.
fn tool_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Inline(String),
        List(Vec<String>),
    }

    let written = Option::<Written>::deserialize(deserializer)?;
    Ok(match written {
        None => None,
        Some(Written::List(tools)) => Some(tools),
        Some(Written::Inline(line)) => Some(
            line.split(',')
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
    })
}

/// A skill on disk.
///
/// Holding the manifest but not the body keeps a catalog cheap to build and
/// cheap to clone into a list: the instructions, which are the bulk of the
/// file, are read on demand by [`Skill::instructions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The parsed front matter.
    pub manifest: SkillManifest,
    /// The skill's own directory, the one holding `SKILL.md`.
    pub directory: PathBuf,
    /// The root of the mind project this was found in. A skill is always
    /// somebody's: which project it came from is what tells two identically
    /// named skills apart in a workspace listing.
    pub project: PathBuf,
}

impl Skill {
    /// Read and parse the `SKILL.md` in `directory`.
    pub fn load(
        directory: impl Into<PathBuf>,
        project: impl Into<PathBuf>,
    ) -> Result<Self, SkillError> {
        let directory = directory.into();
        let path = directory.join(SKILL_FILE);

        let source = fs::read_to_string(&path).map_err(|source| SkillError::Read {
            path: path.clone(),
            source,
        })?;
        let document = frontmatter::split(&source).map_err(|source| SkillError::FrontMatter {
            path: path.clone(),
            source,
        })?;
        let manifest =
            SkillManifest::parse(document.front_matter).map_err(|source| SkillError::Parse {
                path: path.clone(),
                source,
            })?;

        Ok(Self {
            manifest,
            directory,
            project: project.into(),
        })
    }

    /// The name of the mind project this skill belongs to.
    pub fn project_name(&self) -> Option<&str> {
        self.project.file_name()?.to_str()
    }

    /// The `SKILL.md` this was loaded from.
    pub fn path(&self) -> PathBuf {
        self.directory.join(SKILL_FILE)
    }

    /// The declared name, which is not necessarily the directory's.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// The declared description.
    pub fn description(&self) -> &str {
        &self.manifest.description
    }

    /// The name of the directory the skill lives in.
    pub fn directory_name(&self) -> Option<&str> {
        self.directory.file_name()?.to_str()
    }

    /// The markdown after the front matter: the instructions themselves.
    pub fn instructions(&self) -> Result<String, SkillError> {
        let path = self.path();
        let source = fs::read_to_string(&path).map_err(|source| SkillError::Read {
            path: path.clone(),
            source,
        })?;
        let document = frontmatter::split(&source).map_err(|source| SkillError::FrontMatter {
            path: path.clone(),
            source,
        })?;
        Ok(document.body.to_owned())
    }

    /// Everything wrong with this skill, in the order it is worth fixing.
    ///
    /// An empty result means an agent will load it. The checks are the ones
    /// that decide that: a name it can invoke, matching the directory it has
    /// to find, and a description short enough to keep in context.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let name = self.name();

        if name.is_empty() {
            issues.push(ValidationIssue::NameEmpty);
        } else {
            if !is_kebab_case(name) {
                issues.push(ValidationIssue::NameNotKebabCase {
                    name: name.to_owned(),
                });
            }
            if name.chars().count() > MAX_NAME_LEN {
                issues.push(ValidationIssue::NameTooLong {
                    length: name.chars().count(),
                });
            }
            // The directory name is how an agent locates the skill it was told
            // to invoke, so a mismatch means a skill that lists fine and never
            // loads.
            if let Some(directory) = self.directory_name() {
                if directory != name {
                    issues.push(ValidationIssue::NameDirectoryMismatch {
                        name: name.to_owned(),
                        directory: directory.to_owned(),
                    });
                }
            }
        }

        let description = self.description().trim();
        if description.is_empty() {
            issues.push(ValidationIssue::DescriptionEmpty);
        } else if description.chars().count() > MAX_DESCRIPTION_LEN {
            issues.push(ValidationIssue::DescriptionTooLong {
                length: description.chars().count(),
            });
        }

        issues
    }
}

/// Lowercase letters, digits and inner hyphens: what an agent can be asked to
/// invoke without quoting or case folding.
fn is_kebab_case(name: &str) -> bool {
    !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Something that stops a skill from loading, or will.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    NameEmpty,
    NameNotKebabCase { name: String },
    NameTooLong { length: usize },
    NameDirectoryMismatch { name: String, directory: String },
    DescriptionEmpty,
    DescriptionTooLong { length: usize },
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::NameEmpty => write!(f, "`name` is empty"),
            ValidationIssue::NameNotKebabCase { name } => write!(
                f,
                "`name` is `{name}`, but only lowercase letters, digits and inner hyphens are allowed"
            ),
            ValidationIssue::NameTooLong { length } => write!(
                f,
                "`name` is {length} characters, over the {MAX_NAME_LEN} character limit"
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
        }
    }
}

/// Why a skill could not be loaded. Every variant names the file, because a
/// catalog reports these next to each other and the path is what tells them
/// apart.
#[derive(Debug, Error)]
pub enum SkillError {
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

impl SkillError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            SkillError::Read { path, .. }
            | SkillError::FrontMatter { path, .. }
            | SkillError::Parse { path, .. } => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_required_keys() {
        let manifest = SkillManifest::parse("name: pdf-forms\ndescription: Fill PDF forms\n")
            .expect("valid front matter");
        assert_eq!(manifest.name, "pdf-forms");
        assert_eq!(manifest.description, "Fill PDF forms");
        assert_eq!(manifest.allowed_tools, None);
    }

    #[test]
    fn reads_allowed_tools_written_either_way() {
        let inline =
            SkillManifest::parse("name: a\ndescription: d\nallowed-tools: Read, Write , Bash\n")
                .unwrap();
        let list = SkillManifest::parse(
            "name: a\ndescription: d\nallowed-tools:\n  - Read\n  - Write\n  - Bash\n",
        )
        .unwrap();
        let expected = Some(vec!["Read".into(), "Write".into(), "Bash".into()]);
        assert_eq!(inline.allowed_tools, expected);
        assert_eq!(list.allowed_tools, expected);
    }

    #[test]
    fn rejects_front_matter_missing_a_required_key() {
        assert!(SkillManifest::parse("name: a\n").is_err());
        assert!(SkillManifest::parse("description: d\n").is_err());
    }

    fn skill(name: &str, description: &str, directory: &str) -> Skill {
        Skill {
            manifest: SkillManifest {
                name: name.to_owned(),
                description: description.to_owned(),
                allowed_tools: None,
                license: None,
            },
            directory: PathBuf::from(directory),
            project: PathBuf::from("/work/repo"),
        }
    }

    #[test]
    fn a_well_formed_skill_has_no_issues() {
        assert_eq!(
            skill("pdf-forms", "Fill forms", "/s/pdf-forms").validate(),
            vec![]
        );
    }

    #[test]
    fn flags_a_name_the_directory_does_not_match() {
        assert_eq!(
            skill("pdf-forms", "Fill forms", "/s/pdf").validate(),
            vec![ValidationIssue::NameDirectoryMismatch {
                name: "pdf-forms".into(),
                directory: "pdf".into(),
            }]
        );
    }

    #[test]
    fn flags_a_name_that_is_not_kebab_case() {
        let issues = skill("PDF Forms", "Fill forms", "/s/PDF Forms").validate();
        assert!(issues.contains(&ValidationIssue::NameNotKebabCase {
            name: "PDF Forms".into()
        }));
    }

    #[test]
    fn flags_names_hyphenated_at_the_edges() {
        for name in ["-lead", "trail-"] {
            let issues = skill(name, "Fill forms", &format!("/s/{name}")).validate();
            assert!(
                issues.contains(&ValidationIssue::NameNotKebabCase { name: name.into() }),
                "expected `{name}` to be rejected"
            );
        }
    }

    #[test]
    fn flags_an_empty_or_whitespace_description() {
        assert_eq!(
            skill("a", "   ", "/s/a").validate(),
            vec![ValidationIssue::DescriptionEmpty]
        );
    }

    #[test]
    fn flags_limits_by_characters_not_bytes() {
        let name = "á".repeat(MAX_NAME_LEN + 1);
        let issues = skill(&name, "d", &format!("/s/{name}")).validate();
        assert!(issues.contains(&ValidationIssue::NameTooLong {
            length: MAX_NAME_LEN + 1
        }));

        let long = "é".repeat(MAX_DESCRIPTION_LEN + 1);
        let issues = skill("a", &long, "/s/a").validate();
        assert!(issues.contains(&ValidationIssue::DescriptionTooLong {
            length: MAX_DESCRIPTION_LEN + 1
        }));
    }
}
