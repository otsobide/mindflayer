//! What kinds of artifact a mind project holds, and what each looks like on
//! disk.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserialize, Deserializer};
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::code::codes;

/// A kind of artifact.
///
/// A closed enum on purpose. Every kind ships in this crate, so every `match`
/// is exhaustive and the compiler is the checklist for adding the next one. A
/// registry that took kinds at runtime would trade that away for an
/// extensibility nobody has asked for.
///
/// Declaration order is report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A directory of instructions an agent can be asked to follow.
    Skill,
    /// A markdown file that gives an agent context, with optional front
    /// matter that most rules go without.
    Rule,
}

impl Kind {
    /// Every kind, in report order.
    pub const ALL: [Kind; 2] = [Kind::Skill, Kind::Rule];

    /// How a kind is written when it qualifies a name: `skill:commit-style`.
    pub const fn slug(self) -> &'static str {
        match self {
            Kind::Skill => "skill",
            Kind::Rule => "rule",
        }
    }

    /// The name of the folder holding this kind: beside the code by default
    /// ([`Directories::default_for`](crate::workspace::Directories::default_for)),
    /// in a workspace's library for its own
    /// ([`Directories::library_for`](crate::workspace::Directories::library_for)),
    /// and inside `.mindflayer` on a workspace's shelf. A marker's
    /// `[directories]` is keyed by it, which is how a project or a workspace
    /// puts the folder somewhere else.
    ///
    /// It doubles as the plural spelling, which is what lets `mind list rules`
    /// work without a second table to keep in step.
    pub const fn folder(self) -> &'static str {
        match self {
            Kind::Skill => "skills",
            Kind::Rule => "rules",
        }
    }

    /// What this kind looks like on disk.
    pub const fn layout(self) -> Layout {
        match self {
            Kind::Skill => Layout::Directory {
                manifest: "SKILL.md",
            },
            Kind::Rule => Layout::Files { extension: "md" },
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// By its slug, the word a command line takes (`skill`), rather than
/// derived: a derive spells the variant, and what a JSON report calls a kind
/// is a contract that renaming a variant must not break, and the one
/// spelling a script and a command line share.
impl Serialize for Kind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(feature = "ts")]
crate::typescript::words! {
    /// A kind of artifact, by the word a command line takes for it.
    Kind by slug { Skill, Rule }
}

/// From the word a command line takes, through [`FromStr`], so what a front
/// end sends back (a task the panel asks to run) is read as a command line
/// reads it, and a word one refuses the other refuses too.
impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl FromStr for Kind {
    type Err = UnknownKind;

    /// Singular or plural. `mind list rules` and `mind show rule:x` are one
    /// word in two grammatical positions, and refusing either would be a rule
    /// with nothing behind it.
    fn from_str(word: &str) -> Result<Self, Self::Err> {
        Kind::ALL
            .into_iter()
            .find(|kind| word == kind.slug() || word == kind.folder())
            .ok_or_else(|| UnknownKind(word.to_owned()))
    }
}

/// A word that names no kind.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct UnknownKind(pub String);

codes!(UnknownKind => "unknown_kind");

impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected: Vec<&str> = Kind::ALL.iter().map(|kind| kind.folder()).collect();
        write!(
            f,
            "unknown kind `{}`; expected {}",
            self.0,
            expected.join(" or ")
        )
    }
}

/// How a kind is stored, which is all discovery needs to know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// One directory per artifact, holding a manifest with a fixed name. The
    /// whole directory belongs to the artifact (its scripts, references and
    /// assets included), which is why it is never walked recursively.
    Directory { manifest: &'static str },
    /// One file per artifact, at any depth. Folders under the kind's own
    /// folder group and mean nothing else.
    Files { extension: &'static str },
}

#[cfg(test)]
#[path = "kind_test.rs"]
mod tests;
