//! What core knows about the agents that read what Mindflayer manages.
//!
//! So far, only where each keeps what it reads for every project. The checks
//! that will need those folders (Claude's user memory, Codex's config, the
//! skills in `~/.agents`) are handed them rather than looking them up.

use std::path::{Path, PathBuf};

/// Where each agent keeps what it reads for every project, beside whatever a
/// project holds.
///
/// Given, never found: core reads no environment, so the front end decides
/// each one (an option, then the agent's own variable, then the agent's folder
/// in the user's home) and passes the answer in. Looking them up here would
/// let a test that pins every option still read the developer's real
/// `~/.claude`.
///
/// One `Option` for each rather than one for the whole: each says on its own
/// that there is no such folder to look at, so a caller can leave out one
/// agent and keep the others. The default is none of them, which is what a
/// test hands over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHomes {
    /// Claude Code's configuration folder: `$CLAUDE_CONFIG_DIR`, or
    /// `~/.claude`.
    pub claude_config_dir: Option<PathBuf>,
    /// Codex's home: `$CODEX_HOME`, or `~/.codex`.
    pub codex_home: Option<PathBuf>,
    /// OpenCode's configuration folder: `$OPENCODE_CONFIG_DIR`, or
    /// `~/.config/opencode`.
    pub opencode_config_dir: Option<PathBuf>,
    /// The user's `~/.agents`, which belongs to no one agent: several read
    /// skills from it, whatever their own folders say.
    pub agents_dir: Option<PathBuf>,
}

impl AgentHomes {
    /// Where each agent keeps its folder when nothing moves it: the fallback a
    /// front end takes once neither its option nor the agent's own variable
    /// answers.
    ///
    /// Named here, as [`FLAYER_DIR`](crate::FLAYER_DIR) is, rather than in the
    /// front end: which folder an agent reads is a fact about the agent, and a
    /// second front end must not spell it differently. Finding the user's home
    /// is still the front end's, so this reads no environment.
    pub fn defaults_under(user_home: &Path) -> AgentHomes {
        AgentHomes {
            claude_config_dir: Some(user_home.join(".claude")),
            codex_home: Some(user_home.join(".codex")),
            // Joined a folder at a time rather than as `.config/opencode`, so
            // the path has one kind of separator on Windows too.
            opencode_config_dir: Some(user_home.join(".config").join("opencode")),
            agents_dir: Some(user_home.join(".agents")),
        }
    }
}

#[cfg(test)]
#[path = "agent_test.rs"]
mod tests;
