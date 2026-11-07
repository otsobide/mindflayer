//! The skill directories `gather search` asks (D-15, #108), each a
//! [`Finder`]: a service that helps find a skill and names where it is, a
//! git repository and a folder in it, and is never a source itself. What
//! is taken is gathered from that repository, as `gather git` gathers it,
//! so a directory whose API changes or goes away breaks nothing already on
//! the shelf, and its failure is a warning.
//!
//! Every finder is off until the workspace's marker lists it
//! ([`enabled`]), and one that is not is never asked: two of them are
//! services whose API is undocumented and whose terms a person reads
//! before using them (D-15), and none of them is asked on a person's
//! behalf without their say.

use std::fmt;
use std::path::PathBuf;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::code::codes;
use crate::text::printable;
use crate::WeaverWorkspace;

/// A skill directory a search can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Finder {
    /// GitHub's code search, for a `SKILL.md`: asked only with a token,
    /// which the API wants for a code search.
    Github,
    /// skills.sh's `/api/search`: unstable, since it is undocumented.
    SkillsSh,
    /// api.claude-plugins.dev's search of the skills it lists: unstable,
    /// since it is undocumented.
    ClaudePlugins,
}

impl Finder {
    /// Every finder, in the order a search lists what they found.
    pub const ALL: [Finder; 3] = [Finder::Github, Finder::SkillsSh, Finder::ClaudePlugins];

    /// The word the marker and a command line name it by: the service's
    /// own name, so what is listed in `weaver.toml` is what a person
    /// reading its terms looked up.
    pub fn name(self) -> &'static str {
        match self {
            Finder::Github => "github",
            Finder::SkillsSh => "skills.sh",
            Finder::ClaudePlugins => "claude-plugins.dev",
        }
    }

    /// The finder `name` names, if any.
    pub fn named(name: &str) -> Option<Finder> {
        Finder::ALL.into_iter().find(|finder| finder.name() == name)
    }

    /// Whether what it answers may change shape without a word: an API
    /// nobody documents, read as it answers today, best-effort.
    pub fn is_unstable(self) -> bool {
        match self {
            Finder::Github => false,
            Finder::SkillsSh | Finder::ClaudePlugins => true,
        }
    }

    /// Whether it is asked only with a token: GitHub's code search, which
    /// answers no request without one.
    pub fn needs_token(self) -> bool {
        match self {
            Finder::Github => true,
            Finder::SkillsSh | Finder::ClaudePlugins => false,
        }
    }

    /// Where it is asked unless a caller names another address that
    /// speaks the same API.
    pub fn address(self) -> &'static str {
        match self {
            Finder::Github => "https://api.github.com",
            Finder::SkillsSh => "https://skills.sh",
            Finder::ClaudePlugins => "https://api.claude-plugins.dev",
        }
    }
}

impl fmt::Display for Finder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// By its name, the word a command line takes, rather than derived from
/// the variant, as a kind and an agent go out.
impl Serialize for Finder {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// The finders a workspace's marker lists, and what it lists that is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enabled {
    /// Each finder listed, in [`Finder::ALL`]'s order, once.
    pub finders: Vec<Finder>,
    /// Each name listed that this build knows no finder by.
    pub unknown: Vec<UnknownFinder>,
    /// The marker they are listed in, which what is said of them names.
    pub config: PathBuf,
}

/// The finders `workspace`'s marker lists in `finders`.
///
/// A name this build does not know is passed over and said, rather than
/// refused: a later build may know a finder this one does not, and a
/// marker it wrote is still read. One that is not made yet, the default
/// before a write makes it, lists none.
pub fn enabled(workspace: &WeaverWorkspace) -> Enabled {
    let config = workspace.config_path();
    let listed = &workspace.config().finders;
    let finders = Finder::ALL
        .into_iter()
        .filter(|finder| listed.iter().any(|name| name == finder.name()))
        .collect();
    let mut unknown: Vec<UnknownFinder> = Vec::new();
    for name in listed {
        let said = printable(name);
        if Finder::named(name).is_none() && !unknown.iter().any(|known| known.name == said) {
            unknown.push(UnknownFinder {
                name: said,
                config: config.clone(),
            });
        }
    }
    Enabled {
        finders,
        unknown,
        config,
    }
}

impl Enabled {
    /// The finders a search asks: those `named`, each of which is to be
    /// enabled, or every one enabled when none is named.
    ///
    /// One named and not enabled is refused rather than asked or passed
    /// over: it was asked for by name, and asking it is what listing it
    /// would allow, which is the person's to do (D-15).
    pub fn asked(&self, named: &[Finder]) -> Result<Vec<Finder>, SearchError> {
        if let Some(finder) = named.iter().find(|finder| !self.finders.contains(finder)) {
            return Err(SearchError::NotEnabled {
                finder: *finder,
                config: self.config.clone(),
            });
        }
        let asked: Vec<Finder> = if named.is_empty() {
            self.finders.clone()
        } else {
            Finder::ALL
                .into_iter()
                .filter(|finder| named.contains(finder))
                .collect()
        };
        if asked.is_empty() {
            return Err(SearchError::NoneEnabled {
                config: self.config.clone(),
            });
        }
        Ok(asked)
    }
}

/// Why a search asked nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SearchError {
    /// The marker lists no finder, and none was named.
    #[error(
        "no finder is enabled, so nothing was searched: list one in `finders` in {} (github, \
         skills.sh, claude-plugins.dev; the last two are unstable and undocumented, so read their \
         terms first)",
        .config.display()
    )]
    NoneEnabled { config: PathBuf },
    /// A finder named that the marker does not list.
    #[error(
        "{finder} is not enabled, so it was not asked: list it in `finders` in {} to search it",
        .config.display()
    )]
    NotEnabled { finder: Finder, config: PathBuf },
}

codes!(SearchError {
    Self::NoneEnabled { .. } => "finder_none_enabled",
    Self::NotEnabled { .. } => "finder_not_enabled",
});

/// A name the marker lists among its finders that this build knows no
/// finder by: passed over, and said.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{}: `{name}` is among its finders, and this build knows none by that name, so it is passed over",
    .config.display()
)]
pub struct UnknownFinder {
    /// The name, printable.
    pub name: String,
    pub config: PathBuf,
}

codes!(UnknownFinder => "finder_unknown");

#[cfg(test)]
#[path = "finder_test.rs"]
mod tests;
