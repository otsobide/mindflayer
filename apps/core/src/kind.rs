//! What kinds of artifact a mind project holds, and what each looks like on
//! disk.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

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
    /// A markdown file that gives an agent context, and declares nothing.
    Rule,
}

impl Kind {
    /// Every kind, in report order.
    pub const ALL: [Kind; 2] = [Kind::Skill, Kind::Rule];

    /// How a kind is written when it qualifies a name: `skill/commit-style`.
    pub const fn slug(self) -> &'static str {
        match self {
            Kind::Skill => "skill",
            Kind::Rule => "rule",
        }
    }

    /// The folder under `.mind` holding this kind.
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

impl FromStr for Kind {
    type Err = UnknownKind;

    /// Singular or plural. `mind list rules` and `mind show rule/x` are one
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
    /// whole directory belongs to the artifact — its scripts, references and
    /// assets included — which is why it is never walked recursively.
    Directory { manifest: &'static str },
    /// One file per artifact, at any depth. Folders under the kind's own
    /// folder group and mean nothing else.
    Files { extension: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_named_by_its_slug_or_its_folder() {
        for kind in Kind::ALL {
            assert_eq!(kind.slug().parse::<Kind>(), Ok(kind));
            assert_eq!(kind.folder().parse::<Kind>(), Ok(kind));
        }
    }

    #[test]
    fn a_word_that_is_not_a_kind_says_what_was_expected() {
        let error = "prompts".parse::<Kind>().unwrap_err();
        assert!(error.to_string().contains("skills"));
        assert!(error.to_string().contains("rules"));
    }

    #[test]
    fn every_kind_has_its_own_folder_and_slug() {
        // Two kinds sharing either would make discovery and `show` ambiguous.
        let folders: Vec<&str> = Kind::ALL.iter().map(|k| k.folder()).collect();
        let slugs: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
        for (index, kind) in Kind::ALL.iter().enumerate() {
            assert_eq!(folders.iter().filter(|f| **f == kind.folder()).count(), 1);
            assert_eq!(slugs.iter().filter(|s| **s == kind.slug()).count(), 1);
            let _ = index;
        }
    }
}
