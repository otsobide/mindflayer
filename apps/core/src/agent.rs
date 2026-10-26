//! What core knows about the agents that read what Mindflayer manages.
//!
//! Which agents there are ([`Agent`]), where each kind goes in a project for
//! each of them to read it ([`TargetTable`]), and where each keeps what it
//! reads for every project ([`AgentHomes`]). The checks that will need those
//! last folders (Claude's user memory, Codex's config, the skills in
//! `~/.agents`) are handed them rather than looking them up.
//!
//! What each agent does with the files it reads is recorded in
//! `docs/agents.md`, each fact verified on the binary or marked docs-only
//! with a fallback; a change here that disagrees with it starts there.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;

use crate::kind::Kind;

/// A coding agent Mindflayer puts skills and rules where it reads them.
///
/// A closed enum, as [`Kind`] is: what an agent reads, and where, is a fact
/// about that agent, so a new one is a new arm in every `match` and the
/// compiler lists each place that has to learn it. A name taken at runtime
/// would reach a table with no entry for it and quietly reach nothing.
///
/// Declaration order is report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Agent {
    /// Claude Code.
    Claude,
    /// OpenAI's Codex.
    Codex,
    /// OpenCode, both of its maintained lines (V1 and V2), which read the
    /// same project folders for skills.
    OpenCode,
}

impl Agent {
    /// Every agent, in report order.
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::OpenCode];

    /// How it is named on a command line and in a project's settings: one
    /// lowercase word, the name of the agent's own binary.
    pub const fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::OpenCode => "opencode",
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Agent {
    type Err = UnknownAgent;

    /// The name exactly, as a [`Shell`](crate::Shell) is taken: `Claude` or
    /// `open-code` would be one more spelling for a settings file to hold and
    /// a second front end to agree with.
    fn from_str(word: &str) -> Result<Self, Self::Err> {
        Agent::ALL
            .into_iter()
            .find(|agent| agent.name() == word)
            .ok_or_else(|| UnknownAgent(word.to_owned()))
    }
}

/// A word that names no agent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct UnknownAgent(pub String);

impl fmt::Display for UnknownAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Agent::ALL.iter().map(|agent| agent.name()).collect();
        let (last, rest) = names.split_last().expect("there is an agent");
        write!(
            f,
            "unknown agent `{}`; expected {} or {last}",
            self.0,
            rest.join(", ")
        )
    }
}

/// Where each kind goes in a project for each agent to read it.
///
/// One table in core, so no front end ever spells an agent's folder: an
/// install (#34) asks it where to put a copy for each agent a project names,
/// and a check asks it where to look.
///
/// A value rather than a function a caller reaches for, because the
/// standard layout is only the default: a project's settings may put a kind
/// somewhere else for every agent (#40), and a caller handed the table then
/// follows that without changing. One entry per kind and agent. A pair with
/// none is a kind that reaches that agent some other way, or not at all,
/// which is every rule for now: D-03 puts rules in a block of `AGENTS.md`,
/// which M2 adds here, as M6 adds MCP servers.
///
/// Pure: no entry is looked for on disk, and every location is relative to
/// a project's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetTable {
    locations: BTreeMap<(Kind, Agent), PathBuf>,
}

impl TargetTable {
    /// D-07's layout: a skill goes in `.claude/skills` for Claude, and in
    /// `.agents/skills` for Codex and OpenCode, a copy in each.
    ///
    /// Every fact it rests on is in `docs/agents.md` ("Facts the roadmap
    /// cites", #28). Claude reads `.claude/skills` and not `.agents/skills`
    /// (verified on Claude Code 2.1.309), so Claude needs a copy of its own.
    /// Codex reads `.agents/skills`, and both OpenCode lines read it and
    /// `.claude/skills` too (docs-only). OpenCode is given `.agents/skills`
    /// rather than leaning on Claude's copy: `OPENCODE_DISABLE_CLAUDE_CODE`
    /// switches V1's `.claude/skills` off, and a project targeting OpenCode
    /// alone would otherwise need a Claude folder. `.codex/skills`, read only
    /// in trusted projects, and `$CODEX_HOME/skills`, deprecated, are never
    /// targeted; nor is any folder at user scope (D-22).
    pub fn standard() -> TargetTable {
        let locations = Kind::ALL
            .into_iter()
            .flat_map(|kind| Agent::ALL.into_iter().map(move |agent| (kind, agent)))
            .filter_map(|(kind, agent)| Some(((kind, agent), standard(kind, agent)?)))
            .collect();
        TargetTable { locations }
    }

    /// Where `kind` goes for `agents`: one target per location, each with
    /// the agents it serves, so two agents that read one folder get one copy.
    ///
    /// Sorted here, by location and then by agent in report order, so every
    /// front end lists them alike; an agent named twice is served once, and
    /// one the kind does not reach adds nothing.
    pub fn targets(&self, kind: Kind, agents: &[Agent]) -> Vec<AgentTarget> {
        let mut served: BTreeMap<&Path, BTreeSet<Agent>> = BTreeMap::new();
        for &agent in agents {
            if let Some(location) = self.locations.get(&(kind, agent)) {
                served.entry(location).or_default().insert(agent);
            }
        }
        served
            .into_iter()
            .map(|(location, agents)| AgentTarget {
                path: location.to_path_buf(),
                agents: agents.into_iter().collect(),
            })
            .collect()
    }
}

/// Where the standard layout puts `kind` for `agent`, if anywhere.
///
/// One arm per pair, with no wildcard, so a new kind or a new agent does not
/// compile until somebody has said where it goes, with the fact behind it in
/// `docs/agents.md`.
fn standard(kind: Kind, agent: Agent) -> Option<PathBuf> {
    // Joined a folder at a time, as `AgentHomes::defaults_under` is, so the
    // path has one kind of separator on Windows too.
    match (kind, agent) {
        (Kind::Skill, Agent::Claude) => Some(Path::new(".claude").join("skills")),
        (Kind::Skill, Agent::Codex | Agent::OpenCode) => Some(Path::new(".agents").join("skills")),
        (Kind::Rule, Agent::Claude | Agent::Codex | Agent::OpenCode) => None,
    }
}

/// One place in a project where a kind's copies go, and the agents that
/// read them there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTarget {
    /// Relative to the project's root. For a skill, the folder its copy goes
    /// in, under the skill's own name.
    pub path: PathBuf,
    /// Every agent this location serves, each once, in report order. Never
    /// empty: a location no agent reads is not a target.
    pub agents: Vec<Agent>,
}

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
