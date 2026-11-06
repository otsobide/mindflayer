//! What core knows about the agents that read what Mindweaver manages.
//!
//! Which agents there are ([`Agent`]), where each kind goes in a project for
//! each of them to read it ([`TargetTable`], and for an MCP server's entry
//! the config file [`Agent::mcp_file`] names), every folder in a project each
//! reads it from, whoever put it there ([`Agent::reads`]), and where each
//! keeps what it reads for every project ([`AgentHomes`]). The checks that will need those
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

use serde::de::{self, Deserialize, Deserializer};
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::agents_md;
use crate::claude_mcp;
use crate::claude_md::{CLAUDE_LOCAL_MD, CLAUDE_MD};
use crate::code::codes;
use crate::codex_mcp;
use crate::config_entry::FileError;
use crate::kind::Kind;
use crate::opencode_mcp;

/// A coding agent Mindweaver puts skills and rules where it reads them.
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
    ///
    /// An array, which the compiler does not hold to the enum: a new agent
    /// is added here by hand, or it parses as no agent and the standard
    /// table, built from this list, gives it no entry, whatever its arm in
    /// `standard` says.
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

    /// How a sentence names it: the name of the product, as its makers
    /// write it, where [`name`](Self::name) is the word a command line
    /// takes.
    pub const fn title(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::OpenCode => "OpenCode",
        }
    }

    /// Every folder in a project this agent reads `kind` from, whoever put
    /// what is there, each relative to the project's root.
    ///
    /// Not the [`TargetTable`], which says where Mindweaver puts a copy for
    /// the agent: one folder for each, where this is every folder it reads,
    /// which can be more. OpenCode reads `.claude/skills` as well as
    /// `.agents/skills`, and is sent a copy only in the second. A listing
    /// asks this which agents see a skill wherever it sits; an install asks
    /// the table where to write. Folded into one, either OpenCode would be
    /// sent a second copy it does not need, or a listing would say it misses
    /// the skills in Claude's folder, which it reads.
    ///
    /// Pure, as the table is, and resting on the same facts in
    /// `docs/agents.md`: a change here that disagrees with them starts
    /// there. Nothing at user scope, which is no project's.
    pub fn reads(self, kind: Kind) -> Vec<PathBuf> {
        read(kind, self)
    }

    /// Every name at a project's root this agent reads something from: the
    /// folder each place it reads a kind from sits in ([`reads`](Agent::reads),
    /// so `.claude` for `.claude/skills`), its instruction files and the
    /// folder its project configuration sits in, if any
    /// ([`reads_files`](Agent::reads_files), so `.codex` for
    /// `.codex/config.toml`), in name order.
    ///
    /// What tells a repository somebody works in with this agent from one
    /// nobody does, which is what a scan ranks a repository by when it
    /// suggests what to link. Names rather than
    /// what the files hold: an empty `CLAUDE.md` is still somebody's choice
    /// of agent, and a scan that read every file it found would be a crawl
    /// of the disk. The names come from `reads` and `reads_files` rather
    /// than being spelled again, so a folder or a file an agent is found to
    /// read moves here with it.
    ///
    /// Pure, as `reads` is, and resting on the same facts in
    /// `docs/agents.md`.
    pub fn reads_at_root(self) -> Vec<PathBuf> {
        let mut names: Vec<PathBuf> = Kind::ALL
            .into_iter()
            .flat_map(|kind| self.reads(kind))
            .chain(self.reads_files())
            .filter_map(|path| {
                path.components()
                    .next()
                    .map(|first| PathBuf::from(first.as_os_str()))
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Every file in a project this agent reads beside the folders it reads
    /// a kind from: its instruction files and its project configuration,
    /// each relative to the project's root, in name order.
    ///
    /// Each file rather than the name at the root it sits under, which is
    /// what [`reads_at_root`](Agent::reads_at_root) gives: `.codex` says
    /// Codex is used in a repository, but what changes when its
    /// configuration does is `.codex/config.toml`, and a front end that
    /// watches a project for changes watches that file, not the whole
    /// folder (#74). Pure, as `reads` is, and resting on the same facts in
    /// `docs/agents.md`.
    pub fn reads_files(self) -> Vec<PathBuf> {
        let mut files = files(self);
        files.sort();
        files
    }

    /// The config file this agent is given an MCP server's entry in, in the
    /// project at `root`, as a route from it, `/` separated: where D-10
    /// writes a server's one entry for it, and what the ledger records the
    /// entry under.
    ///
    /// Asked of the renderer that writes the file, never spelled again here,
    /// so the file an entry is recorded in is the file it is written into:
    /// Claude's `.mcp.json` ([`claude_mcp::FILE`]) and Codex's
    /// `.codex/config.toml` ([`codex_mcp::FILE`]) wherever the project is,
    /// and OpenCode's the first of its four config files there is, or the
    /// `opencode.json` made when none is ([`opencode_mcp::FILES`]). Not a
    /// row of the [`TargetTable`], which is pure: which of OpenCode's files
    /// it is depends on what the project holds, so that one is looked for on
    /// disk, and only that one can fail.
    pub fn mcp_file(self, root: &Path) -> Result<&'static str, FileError> {
        match self {
            Agent::Claude => Ok(claude_mcp::FILE),
            Agent::Codex => Ok(codex_mcp::FILE),
            Agent::OpenCode => opencode_mcp::chosen(root),
        }
    }

    /// What this agent asks before it starts an MCP server a project's
    /// config file gives it: what an install says once it has written one,
    /// as `docs/agents.md` ("MCP servers") records it, never decided again
    /// by a front end. Matched with no wildcard, so an agent added says it
    /// before it compiles.
    pub const fn starts_a_server(self) -> Starts {
        match self {
            // In a session somebody works in, once they approve it
            // (docs-only); a `-p` run starts every server the file names,
            // approved or not (verified on 2.1.318, `claude-mcp`).
            Agent::Claude => Starts::OnceApproved,
            // Only in a project its user's config marks trusted
            // (docs-only, `codex-mcp-trusted` and `codex-mcp-untrusted`
            // pending), which an install says by `codex_mcp_untrusted`.
            Agent::Codex => Starts::InATrustedProject,
            // Unasked: `opencode mcp list` connected every server the
            // project's file named, nothing approved (verified on 2.2.5,
            // 2.1.3 and 1.19.4, the OpenCode runs).
            Agent::OpenCode => Starts::Unasked,
        }
    }
}

/// What an agent asks before it starts an MCP server its project's config
/// file gives it ([`Agent::starts_a_server`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Starts {
    /// Once somebody approves the project's servers when it asks; a run
    /// with no one to ask starts them all the same.
    OnceApproved,
    /// Only in a project its user's config marks trusted.
    InATrustedProject,
    /// With nothing asked.
    Unasked,
}

/// The files `agent` reads in a project, beside the folders [`read`] names,
/// each relative to the project's root.
///
/// One arm per agent with no wildcard, as `read`'s, so a new agent does not
/// compile until somebody has said what it reads there. Each configuration
/// file is one D-10 puts an MCP server's entry in, named by the renderer
/// that writes it, as [`Agent::mcp_file`] names it.
fn files(agent: Agent) -> Vec<PathBuf> {
    // Joined a folder at a time, as `standard` joins them, so a route with a
    // folder in it has one kind of separator on Windows too.
    let at_root = |routes: &[&str]| {
        routes
            .iter()
            .map(|route| route.split('/').collect::<PathBuf>())
            .collect::<Vec<_>>()
    };
    match agent {
        // Verified on Claude Code 2.1.309: `CLAUDE.md`, `CLAUDE.local.md`,
        // and `AGENTS.md` natively when neither is there. `.mcp.json`, its
        // project's MCP servers, verified on 2.1.318 (`claude-mcp`).
        Agent::Claude => at_root(&[
            CLAUDE_MD,
            CLAUDE_LOCAL_MD,
            agents_md::FILE,
            claude_mcp::FILE,
        ]),
        // Docs-only: `AGENTS.md`, or `AGENTS.override.md` in its place, and
        // a trusted project's `.codex/config.toml`.
        Agent::Codex => at_root(&[agents_md::FILE, agents_md::OVERRIDE, codex_mcp::FILE]),
        // Docs-only: `AGENTS.md`, the `CLAUDE.md` V1 reads when there is no
        // `AGENTS.md`, and each of the four files its project's
        // configuration may be in, the first of them there is the one read,
        // so a snapshot reading the one there is reads one named here.
        Agent::OpenCode => {
            let mut files = at_root(&[agents_md::FILE, CLAUDE_MD]);
            files.extend(at_root(&opencode_mcp::FILES));
            files
        }
    }
}

/// Where `agent` reads `kind` in a project, in path order.
///
/// One arm per pair with no wildcard, as [`standard`]'s, so a new kind or a
/// new agent does not compile until somebody has said what it reads.
fn read(kind: Kind, agent: Agent) -> Vec<PathBuf> {
    // Joined a folder at a time, as `standard` joins them, so a folder here
    // and the same folder in the table are equal paths on Windows too.
    let skills = |folder: &str| Path::new(folder).join("skills");
    match (kind, agent) {
        // Verified on Claude Code 2.1.309, which does not read
        // `.agents/skills`.
        (Kind::Skill, Agent::Claude) => vec![skills(".claude")],
        // Docs-only. Not `.codex/skills`, read only in a trusted project, so
        // it says nothing of what Codex sees in any other.
        (Kind::Skill, Agent::Codex) => vec![skills(".agents")],
        // Docs-only, for V1 and V2 alike. V1 can switch `.claude/skills` off
        // (`OPENCODE_DISABLE_CLAUDE_CODE`), which nothing in a project shows,
        // so it is read here as the default reads it.
        (Kind::Skill, Agent::OpenCode) => {
            vec![skills(".agents"), skills(".claude"), skills(".opencode")]
        }
        // A rule reaches an agent through `AGENTS.md`, or Claude's rule files
        // (D-03), rendered outside the target table (`install::rules`).
        // Claude does read `.claude/rules`, but what Mindweaver writes there
        // is listed from the ledger as its rule's target, so no folder of
        // rules is said to be read, and no listing takes those files for a
        // rule somebody keeps there.
        (Kind::Rule, Agent::Claude | Agent::Codex | Agent::OpenCode) => Vec::new(),
        // An MCP server reaches an agent as an entry in its own config file
        // (D-10), one of the files `files` names, never as a folder of
        // definitions: no agent reads one.
        (Kind::Mcp, Agent::Claude | Agent::Codex | Agent::OpenCode) => Vec::new(),
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// By its name, the one `--agents` and a project's settings take, rather
/// than derived: a derive would spell `OpenCode` as `open_code`, a name no
/// command line or settings file holds, and what a JSON report calls an
/// agent is a contract from the first report on.
impl Serialize for Agent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// A coding agent, by the name `--agents` and a project's settings take.
    Agent by name { Claude, Codex, OpenCode }
}

/// From its name, through [`FromStr`], for the reason a [`Kind`] is read
/// through it.
impl<'de> Deserialize<'de> for Agent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
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

codes!(UnknownAgent => "unknown_agent");

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

/// Where each kind goes in a project for each agent.
///
/// One table in core, so no front end ever spells an agent's folder: an
/// install asks it where to put a copy for each agent a project names
/// ([`install::targets`](crate::install::targets)), and a check asks it where
/// to look.
///
/// A value rather than a function a caller reaches for, because the
/// standard layout is only the default: a project's settings may put a kind
/// somewhere else for every agent ([`overriding`](TargetTable::overriding)),
/// and a caller handed the table then follows that without changing. The project hands it on, in core: which
/// table applies is its settings' to say, so a caller asks the project
/// ([`MindProject::targets`](crate::MindProject::targets)), as an install
/// does, and no front end calls
/// [`standard`](TargetTable::standard) and picks one those settings would
/// contradict.
///
/// Where a kind goes for an agent is where its copy is put for that agent,
/// not proof the agent reads it there: the standard layout is built from
/// what each one reads, but an override's folder reaches an agent only once
/// the agent is pointed at it, which is for a project's status to say.
///
/// A list of locations per kind and agent, kept private: the standard
/// layout needs one, and an override names as many folders as it likes,
/// each for every agent ([`overriding`](TargetTable::overriding)). A pair
/// with none is a kind that reaches that agent some other way, or not at
/// all, which is every rule: D-03 puts rules in a block of `AGENTS.md`, one
/// file every agent shares rather than a folder a copy goes in, rendered
/// outside the table ([`crate::install::rules`]), and every MCP server, an
/// entry in each agent's own config file (D-10), which [`Agent::mcp_file`]
/// names, since OpenCode's is looked for on disk.
///
/// Pure: no entry is looked for on disk, and every location is relative to
/// a project's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetTable {
    locations: BTreeMap<(Kind, Agent), Vec<PathBuf>>,
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
            .filter_map(|(kind, agent)| Some(((kind, agent), vec![standard(kind, agent)?])))
            .collect();
        TargetTable { locations }
    }

    /// This table with `kind` sent to `folders`, for every agent, in place of
    /// wherever it sent it: what a project's settings make of it when they
    /// say where a kind is installed (the owner's amendment to D-07).
    ///
    /// For every agent alike, rather than a folder per agent: the override
    /// is one place every agent the project names shares (one `.skills` for
    /// all three, say), and two agents sent to one folder get one copy, as
    /// Codex and OpenCode share `.agents/skills` in the standard layout. A
    /// folder per agent would be a second table to write by hand, which the
    /// standard one already is. No folder at all sends the kind nowhere.
    ///
    /// Pure, and unchecked: whether each folder is one an install may use
    /// (inside the project, outside `.mind`) is the project's to say, which
    /// hands over only those
    /// ([`MindProject::targets`](crate::MindProject::targets)).
    pub fn overriding(mut self, kind: Kind, folders: &[PathBuf]) -> TargetTable {
        for agent in Agent::ALL {
            self.locations.insert((kind, agent), folders.to_vec());
        }
        self
    }

    /// Where `kind` goes for `agents`: one target per location, each with
    /// the agents it is for, so two agents sent to one folder get one copy.
    ///
    /// Sorted here, by location and then by agent in report order, so every
    /// front end lists them alike; an agent named twice is served once, one
    /// location named twice for an agent is one target, and an agent the
    /// kind does not reach adds nothing.
    ///
    /// Pure, as the table is: nothing is read on disk, and every location
    /// comes from the table it is called on, which for
    /// [`standard`](TargetTable::standard) rests on the facts in
    /// `docs/agents.md`.
    pub fn targets(&self, kind: Kind, agents: &[Agent]) -> Vec<AgentTarget> {
        let mut served: BTreeMap<&Path, BTreeSet<Agent>> = BTreeMap::new();
        for &agent in agents {
            for location in self.locations.get(&(kind, agent)).into_iter().flatten() {
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
        // An entry in each agent's own config file, rendered outside the
        // table as a rule's section is (D-10), in the file `Agent::mcp_file`
        // names, which no release writes yet.
        (Kind::Mcp, Agent::Claude | Agent::Codex | Agent::OpenCode) => None,
    }
}

/// One place in a project where a kind's copies go, and the agents they go
/// there for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTarget {
    /// Relative to the project's root. For a skill, the folder its copy goes
    /// in, under the skill's own name.
    pub path: PathBuf,
    /// The agents asked about that the table sends here, each once, in report
    /// order. Never empty: a location none of them is sent to is not one of
    /// their targets. Under the standard table these are agents that read the
    /// location; under an override they need not be.
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
    /// `~/.claude`, while `$CLAUDE_CONFIG_DIR` moves Claude Code's folder
    /// elsewhere: OpenCode reads Claude's skills in `~/.claude/skills`
    /// whatever that variable says (docs/agents.md, docs-only). None while
    /// `claude_config_dir` is that folder already, or an option names it
    /// outright, which every agent reading it is taken to read.
    pub claude_dir_in_home: Option<PathBuf>,
    /// `~/.config/opencode`, while `$OPENCODE_CONFIG_DIR` names another
    /// folder for `opencode_config_dir`: OpenCode loads that one on top of
    /// this one, never in its place (docs/agents.md, docs-only), so both
    /// are read. None while `opencode_config_dir` is that folder already,
    /// or an option names OpenCode's outright.
    pub opencode_dir_in_home: Option<PathBuf>,
}

impl AgentHomes {
    /// Each folder there is, with who reads it, in report order: the folders
    /// a copy put inside would reach every project, not one.
    pub fn folders(&self) -> Vec<(&'static str, &Path)> {
        [
            ("Claude Code", &self.claude_config_dir),
            ("Codex", &self.codex_home),
            ("OpenCode", &self.opencode_config_dir),
            ("more than one agent", &self.agents_dir),
            ("OpenCode", &self.claude_dir_in_home),
            ("OpenCode", &self.opencode_dir_in_home),
        ]
        .into_iter()
        .filter_map(|(reader, folder)| Some((reader, folder.as_deref()?)))
        .collect()
    }

    /// Every folder these homes name that `agent` reads skills from at user
    /// scope, in the order docs/agents.md lists them: a skill there is read
    /// beside a project's in every project, which is what doctor compares a
    /// project's skills with (D-22). Mindweaver writes in none of them.
    ///
    /// One arm per agent with no wildcard, as `read`'s, so a new agent does
    /// not compile until somebody has said where it reads at user scope.
    pub fn skills_read_by(&self, agent: Agent) -> Vec<PathBuf> {
        let skills = |home: &Option<PathBuf>| home.as_ref().map(|home| home.join("skills"));
        let folders = match agent {
            // Verified in part on Claude Code 2.1.309: `<config dir>/skills`.
            Agent::Claude => vec![skills(&self.claude_config_dir)],
            // Docs-only: `$HOME/.agents/skills`, whatever `$CODEX_HOME` says.
            // Not `$CODEX_HOME/skills`, which is deprecated.
            Agent::Codex => vec![skills(&self.agents_dir)],
            // Docs-only, V1 and V2: its own folder's, `~/.config/opencode`'s
            // beneath the one `$OPENCODE_CONFIG_DIR` names, `~/.claude/skills`
            // whatever `$CLAUDE_CONFIG_DIR` says, and `~/.agents/skills`.
            Agent::OpenCode => vec![
                skills(&self.opencode_dir_in_home),
                skills(&self.opencode_config_dir),
                skills(if self.claude_dir_in_home.is_some() {
                    &self.claude_dir_in_home
                } else {
                    &self.claude_config_dir
                }),
                skills(&self.agents_dir),
            ],
        };
        folders.into_iter().flatten().collect()
    }

    /// Where each agent keeps its folder when nothing moves it: the fallback a
    /// front end takes once neither its option nor the agent's own variable
    /// answers.
    ///
    /// Named here, as [`WEAVER_DIR`](crate::WEAVER_DIR) is, rather than in the
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
            // Each is one of the above, which no variable has moved.
            claude_dir_in_home: None,
            opencode_dir_in_home: None,
        }
    }

    /// OpenCode's own folders, each of them it reads, in the order it loads
    /// them: `~/.config/opencode`, then the one `$OPENCODE_CONFIG_DIR`
    /// names on top of it (docs/agents.md, docs-only).
    pub fn opencode_dirs(&self) -> Vec<&Path> {
        [&self.opencode_dir_in_home, &self.opencode_config_dir]
            .into_iter()
            .filter_map(Option::as_deref)
            .collect()
    }
}

#[cfg(test)]
#[path = "agent_test.rs"]
mod tests;
