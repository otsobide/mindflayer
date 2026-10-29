//! What the commands that read artifacts one by one say of each, for a
//! front end to print: `show`, one artifact in full ([`Shown`]), and
//! `validate`, what is wrong with each and the counts its verdict follows
//! ([`Validation`]).
//!
//! Built from the catalog the command read, the one its text is laid out
//! from, so the two cannot list different artifacts, and with no ledger:
//! neither command reads one, and an artifact's standing is the
//! snapshot's to say ([`crate::view`]). So neither says who put a copy
//! where it is, which read without the ledger would call an installed copy
//! somebody's. What the snapshot also says of an artifact goes by the
//! snapshot's name for it (`kind`, `name`, `summary`, `issues`), and
//! everything here is held to the rule every report is: each field by its
//! name, a contract once printed.

use std::path::PathBuf;

use serde::{Serialize, Serializer};

use crate::artifact::{Artifact, ArtifactError, Declared, ValidationIssue};
use crate::catalog::DiscoveryFailure;
use crate::kind::Kind;
use crate::paths;

/// One artifact in full, as `show` shows it: what it is, where it is, what
/// its front matter declares that `show` prints, and the body after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Shown {
    pub kind: Kind,
    pub name: String,
    /// The directory a skill is, or the file a rule is: its first copy's,
    /// the one `show` reads.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The root of what holds it: its project, or the workspace for the
    /// workspace's own.
    #[serde(serialize_with = "paths::lossy")]
    pub project: PathBuf,
    /// The file it is read from: a skill's `SKILL.md`, or the rule itself.
    #[serde(serialize_with = "paths::lossy")]
    pub file: PathBuf,
    /// The one line a listing shows; none when there is nothing to show.
    pub summary: Option<String>,
    /// What it says it is for: always for a skill, and for a rule only when
    /// its front matter says.
    pub description: Option<String>,
    /// A skill's `allowed-tools`, when it narrows them.
    pub allowed_tools: Option<Vec<String>>,
    /// A skill's `license`, when it names one.
    pub license: Option<String>,
    /// A skill's `compatibility`, when written as the string it should be.
    pub compatibility: Option<String>,
    /// Each `metadata` entry of a skill that maps a string to a string, in
    /// the order written: the rest are `validate`'s to report. None for a
    /// rule, which has no `metadata`.
    #[serde(serialize_with = "in_order")]
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<std::collections::BTreeMap<String, String>>")
    )]
    pub metadata: Option<Vec<(String, String)>>,
    /// The files a rule is about, as glob patterns, when it says.
    pub paths: Option<Vec<String>>,
    /// What follows the front matter, as the file has it now.
    pub body: String,
}

impl Shown {
    /// `artifact` in full, its body read from its file again, as `show`
    /// reads it: a file gone since it was listed is an error, as it is for
    /// `show`.
    pub fn of(artifact: &Artifact) -> Result<Shown, ArtifactError> {
        let body = artifact.contents()?;
        let mut shown = Shown {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            path: artifact.path().to_path_buf(),
            project: artifact.project().to_path_buf(),
            file: artifact.file(),
            summary: artifact.summary().map(str::to_owned),
            description: artifact.description().map(str::to_owned),
            allowed_tools: None,
            license: None,
            compatibility: None,
            metadata: None,
            paths: None,
            body,
        };
        match artifact.declared() {
            Declared::Skill(manifest) => {
                shown.allowed_tools.clone_from(&manifest.allowed_tools);
                shown.license.clone_from(&manifest.license);
                shown.compatibility = manifest.compatibility.stated().map(str::to_owned);
                shown.metadata = Some(manifest.metadata.entries.clone());
            }
            Declared::Rule(manifest) => {
                shown.paths = manifest.paths.stated().cloned();
            }
        }
        Ok(shown)
    }
}

/// Entries as an object, in the order written: a map would sort them, and a
/// list of pairs would read as something other than the mapping it is.
fn in_order<S: Serializer>(
    entries: &Option<Vec<(String, String)>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match entries {
        None => serializer.serialize_none(),
        Some(entries) => serializer.collect_map(entries.iter().map(|(key, value)| (key, value))),
    }
}

/// What `validate` found: each artifact it checked, with what is wrong with
/// it, and the counts its verdict follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Validation {
    /// Each artifact checked, in the order the catalog lists them.
    pub artifacts: Vec<Checked>,
    /// How many of them do not pass.
    pub invalid: usize,
    /// How many files could not be read at all
    /// ([`DiscoveryFailure::is_unreadable`]): none is among `artifacts`, and
    /// each is a warning beside them. Counted, so a check of a project whose
    /// files would not read does not pass for having found nothing wrong.
    pub unreadable: usize,
    /// How many of them have copies that do not all hold the same, which
    /// the agents reading them do not all load alike.
    pub differing: usize,
}

/// One artifact `validate` checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Checked {
    pub kind: Kind,
    pub name: String,
    /// The directory a skill is, or the file a rule is: its first copy's.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The root of what holds it: its project, or the workspace for the
    /// workspace's own and what is on its shelf.
    #[serde(serialize_with = "paths::lossy")]
    pub project: PathBuf,
    /// Everything wrong with it, every copy's, warnings included, each
    /// saying whether it is one.
    pub issues: Vec<ValidationIssue>,
    /// Whether it does not pass: one issue that is no warning is enough
    /// ([`ValidationIssue::is_warning`]).
    pub invalid: bool,
    /// Whether its copies do not all hold the same.
    pub copies_differ: bool,
}

impl Validation {
    /// What `validate` finds of `artifacts`, read from a catalog that met
    /// `failures` on the way.
    pub fn of(artifacts: &[&Artifact], failures: &[DiscoveryFailure]) -> Validation {
        let artifacts: Vec<Checked> = artifacts
            .iter()
            .map(|artifact| {
                let issues = artifact.validate();
                Checked {
                    kind: artifact.kind(),
                    name: artifact.name().to_owned(),
                    path: artifact.path().to_path_buf(),
                    project: artifact.project().to_path_buf(),
                    invalid: issues.iter().any(|issue| !issue.is_warning()),
                    issues,
                    copies_differ: artifact.copies_differ(),
                }
            })
            .collect();
        Validation {
            invalid: artifacts.iter().filter(|checked| checked.invalid).count(),
            unreadable: failures
                .iter()
                .filter(|failure| failure.is_unreadable())
                .count(),
            differing: artifacts
                .iter()
                .filter(|checked| checked.copies_differ)
                .count(),
            artifacts,
        }
    }

    /// Whether everything checked passes: nothing invalid, nothing that
    /// would not read, and no copies that differ.
    pub fn passes(&self) -> bool {
        self.invalid == 0 && self.unreadable == 0 && self.differing == 0
    }
}

#[cfg(test)]
#[path = "reading_test.rs"]
mod tests;
