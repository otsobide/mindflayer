//! Where things live outside the directory a command works in: the default
//! workspace's home, and each agent's own folder.
//!
//! Resolved here, in the front end, because core never reads the environment.
//! It takes what this module answers as parameters, which is what lets the
//! tests hand it empty values and know nothing real is reached.

use std::path::{Path, PathBuf};

use mindflayer_core::AgentHomes;

use crate::AgentHomeOptions;

/// Where the default workspace lives: `--home`, then `$MINDFLAYER_HOME`, then
/// the user's home directory. Empty, from either, means there is none.
///
/// A string rather than a path because empty is a meaningful answer here,
/// and one a path cannot spell.
pub fn default_home(given: Option<&str>) -> Option<PathBuf> {
    resolve(given, variable("MINDFLAYER_HOME"), std::env::home_dir())
}

/// Where each agent keeps what it reads for every project: the option, then
/// the variable the agent itself reads, then its folder in the user's home.
/// Empty, from either, means there is none.
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
    let under = |folders: &[&str]| {
        user.map(|user| {
            folders
                .iter()
                .fold(user.to_path_buf(), |path, folder| path.join(folder))
        })
    };
    AgentHomes {
        claude_config_dir: resolve(
            given.claude_config_dir.as_deref(),
            variable("CLAUDE_CONFIG_DIR"),
            under(&[".claude"]),
        ),
        codex_home: resolve(
            given.codex_home.as_deref(),
            variable("CODEX_HOME"),
            under(&[".codex"]),
        ),
        // Joined a folder at a time rather than as `.config/opencode`, so the
        // path has one separator on Windows too.
        opencode_config_dir: resolve(
            given.opencode_config_dir.as_deref(),
            variable("OPENCODE_CONFIG_DIR"),
            under(&[".config", "opencode"]),
        ),
        // No variable: the folder belongs to no one agent, so none of their
        // variables moves it.
        agents_dir: resolve(given.agents_dir.as_deref(), None, under(&[".agents"])),
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
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
#[path = "homes_test.rs"]
mod tests;
