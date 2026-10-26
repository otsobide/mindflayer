//! Where things live outside the directory a command works in: the default
//! workspace's home, and each agent's own folder.
//!
//! Resolved here, in the front end, because core never reads the environment.
//! It takes what this module answers as parameters, which is what lets the
//! tests hand it empty values and know nothing real is reached.

use std::path::{Path, PathBuf};

use mindflayer_core::activate::WORKSPACE_VARIABLE;
use mindflayer_core::{AgentHomes, Inputs, Intent};

use crate::AgentHomeOptions;

/// What a command line says about which workspace to use, apart from where
/// the command starts: gathered here and handed to core's `resolve`, which
/// alone decides what it adds up to.
///
/// Decided once per command line. The TUI keeps its own and hands it to every
/// line it runs, so none of them looks again and finds something else.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lookup {
    /// The workspace `-W` names, as given: empty when given empty.
    pub flag: Option<PathBuf>,
    /// The workspace `$MINDFLAYER_WORKSPACE` activates, as set.
    pub activated: Option<PathBuf>,
    /// Where the default workspace lives ([`default_home`]).
    pub home: Option<PathBuf>,
}

impl Lookup {
    /// What a command line gave as `--home` and `-W`, with the environment
    /// for what it left out.
    pub fn given(home: Option<&str>, workspace: Option<&str>) -> Self {
        Self {
            home: default_home(home),
            ..Self::chosen(workspace, &variable)
        }
    }

    /// What `-W` and `$MINDFLAYER_WORKSPACE` say, with the environment passed
    /// in, so the order is tested on values.
    ///
    /// `-W` given at all, even empty, is the whole answer, and the variable is
    /// not read: the order `--home` and `$MINDFLAYER_HOME` are taken in.
    /// What an empty value means is core's to say, and it says none, as the
    /// variable's name is: it is what `flayer activate` sets.
    fn chosen(workspace: Option<&str>, variable: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            flag: workspace.map(PathBuf::from),
            activated: match workspace {
                Some(_) => None,
                None => variable(WORKSPACE_VARIABLE).map(PathBuf::from),
            },
            home: None,
        }
    }

    /// Nothing named and nothing activated, with the default in `home`.
    pub fn at_home(home: Option<&Path>) -> Self {
        Self {
            home: home.map(Path::to_path_buf),
            ..Self::default()
        }
    }

    /// What core is given to resolve a workspace from `start`.
    pub fn inputs<'a>(&'a self, start: &'a Path, intent: Intent) -> Inputs<'a> {
        Inputs {
            flag: self.flag.as_deref(),
            activated: self.activated.as_deref(),
            start,
            home: self.home.as_deref(),
            intent,
        }
    }
}

/// Where the default workspace lives: `--home`, then `$MINDFLAYER_HOME`, then
/// the user's home directory. Empty, from either, means there is none.
///
/// A string rather than a path because empty is a meaningful answer here,
/// and one a path cannot spell.
pub fn default_home(given: Option<&str>) -> Option<PathBuf> {
    resolve(given, variable("MINDFLAYER_HOME"), std::env::home_dir())
}

/// Where each agent keeps what it reads for every project: the option, then
/// the variable the agent itself reads, then its folder in the user's home,
/// which core names ([`AgentHomes::defaults_under`]). Empty, from either of
/// the first two, means there is none.
///
/// Decided once per command line, and handed to core as a value: core never
/// looks them up, so a test that pins every option cannot reach the real
/// ones through it.
pub fn agent_homes(given: &AgentHomeOptions) -> AgentHomes {
    agent_homes_from(given, &variable, std::env::home_dir().as_deref())
}

/// [`agent_homes`], with the environment and the user's home passed in.
fn agent_homes_from(
    given: &AgentHomeOptions,
    variable: &dyn Fn(&str) -> Option<String>,
    user: Option<&Path>,
) -> AgentHomes {
    // Only which home the folders sit in is decided here; the folders
    // themselves are core's.
    let fallback = user.map(AgentHomes::defaults_under).unwrap_or_default();
    AgentHomes {
        claude_config_dir: resolve(
            given.claude_config_dir.as_deref(),
            variable("CLAUDE_CONFIG_DIR"),
            fallback.claude_config_dir,
        ),
        codex_home: resolve(
            given.codex_home.as_deref(),
            variable("CODEX_HOME"),
            fallback.codex_home,
        ),
        opencode_config_dir: resolve(
            given.opencode_config_dir.as_deref(),
            variable("OPENCODE_CONFIG_DIR"),
            fallback.opencode_config_dir,
        ),
        // No variable: the folder belongs to no one agent, so none of their
        // variables moves it.
        agents_dir: resolve(given.agents_dir.as_deref(), None, fallback.agents_dir),
    }
}

/// The order every home is decided in: what the command line gave, then
/// what the environment says, then the fallback. Empty, from either of the
/// first two, means none, and the fallback is not consulted.
///
/// Given the environment's answer rather than reading it, so the order is
/// tested on values. Setting variables in a test would change them for every
/// other test running in the same process.
fn resolve(
    given: Option<&str>,
    from_environment: Option<String>,
    fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    let chosen = match (given, from_environment) {
        (Some(given), _) => given.to_owned(),
        (None, Some(from_environment)) => from_environment,
        (None, None) => return fallback,
    };
    (!chosen.is_empty()).then(|| PathBuf::from(chosen))
}

/// A variable's value. One that is not Unicode counts as unset, as it always
/// has for `$MINDFLAYER_HOME`: a path that cannot be spelled is no answer.
pub(crate) fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
#[path = "homes_test.rs"]
mod tests;
