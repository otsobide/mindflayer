//! What keeps an agent from what a project gives it, which the agent never
//! says: `mindweaver doctor` (#96).
//!
//! Some failures are silent. An untrusted Codex project ignores its
//! `.codex/config.toml`, an `AGENTS.override.md` hides `AGENTS.md`, a
//! `CLAUDE.local.md` stops Claude reading it, an unset secret variable
//! breaks a server and a skill at user scope can shadow a project's. Each
//! is a fact about what an agent reads, recorded in docs/agents.md, so each
//! check here rests on one of them and on the function that already reads
//! it for an install or a render, never a second reading of its own: Codex's
//! config through `codex_mcp`, the chain Codex reads through `agents_md`,
//! what keeps Claude from the block through `claude_md`.
//!
//! - **File reads alone.** Nothing is written, no lock is taken, and the
//!   ledger is opened only to read ([`WeaverWorkspace::ledger_for_reading`]).
//!   No agent is run: what an agent would do is what docs/agents.md says it
//!   does, and running one is the probe's, by hand.
//! - **Never the environment.** The front end says where each agent keeps
//!   its own folders and which variables are set, by name alone
//!   ([`Machine`]), as it says every agent home: core reads no environment,
//!   and a value read here could reach a report. Nothing a finding says
//!   holds a value.
//! - **A finding is no failure.** Each is a [`Symptom`], with a code of its
//!   own, the path it is about and what fixes it ([`Finding`]). What doctor
//!   could not read to tell is a failure, collected beside the findings of
//!   the project it is about ([`Examined::failures`]), as every report
//!   collects one, and never a reason to leave the rest unsaid.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::agents_md::{self, CODEX_BUDGET};
use crate::catalog::DiscoveryFailure;
use crate::claude_md::{self, ClaudeMdError};
use crate::claude_rules;
use crate::code::{codes, Said};
use crate::codex_mcp::{self, CodexMcpError, Trust, UserConfig};
use crate::hash;
use crate::install;
use crate::kind::Kind;
use crate::names::Names;
use crate::paths;
use crate::value::ContentHash;
use crate::workspace::{same_directory, Member, MindProject, WeaverWorkspace};

/// What a front end tells doctor of the machine a project is on: where each
/// agent keeps what it reads for every project, and the names of the
/// environment variables set where it runs, never their values.
///
/// Given, never found, as [`AgentHomes`] is: core reads no environment, so
/// a test hands over what it chose, and the CLI what its command line and
/// its environment say, once per command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    pub homes: AgentHomes,
    /// The names of the variables set, and nothing of what they hold: a
    /// value doctor never had is one no report can say.
    set: BTreeSet<String>,
}

impl Machine {
    /// `homes`, with `set` the names of the environment variables set.
    pub fn new(homes: AgentHomes, set: impl IntoIterator<Item = String>) -> Machine {
        Machine {
            homes,
            set: set.into_iter().collect(),
        }
    }
}

/// What doctor found in every project a report covers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// In the order the workspace's marker lists them, each directory once
    /// however many entries point at it, as `status` reports them.
    pub projects: Vec<Examined>,
}

impl Report {
    /// Every project `workspace` links.
    pub fn of_workspace(workspace: &WeaverWorkspace, machine: &Machine) -> Report {
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut projects = Vec::new();
        for member in workspace.members() {
            if seen.iter().any(|root| same_directory(root, &member.root)) {
                continue;
            }
            seen.push(member.root.clone());
            projects.push(Examined::of_member(&member, workspace, machine));
        }
        Report { projects }
    }

    /// One project, against what `workspace` records of it when that one
    /// links it: a project no workspace links is still examined, for what
    /// is in it and around it.
    pub fn of_project(
        project: &MindProject,
        workspace: Option<&WeaverWorkspace>,
        machine: &Machine,
    ) -> Report {
        let entry = workspace
            .and_then(|workspace| workspace.linked_as(project.root()))
            .map(paths::as_route);
        Report {
            projects: vec![Examined::of(project, entry, workspace, machine)],
        }
    }

    /// How many findings it holds, every project's.
    pub fn found(&self) -> usize {
        self.projects
            .iter()
            .map(|examined| examined.findings.len())
            .sum()
    }
}

/// One project doctor examined, and what it found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Examined {
    /// The workspace's entry for it, as the marker spells it; none for a
    /// project no workspace links.
    pub entry: Option<String>,
    /// What its marker calls it; none for an entry that opens no project.
    pub name: Option<String>,
    /// Where it is.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What keeps one of its agents from what it is given, in the order
    /// the checks are made.
    pub findings: Vec<Finding>,
    /// What could not be read to tell, by its code: why its entry opens no
    /// project, or a file a check needed.
    pub failures: Vec<Said>,
}

impl Examined {
    /// The project `member` opens, or why it opens none.
    pub(crate) fn of_member(
        member: &Member,
        workspace: &WeaverWorkspace,
        machine: &Machine,
    ) -> Examined {
        let entry = Some(paths::as_route(&member.entry));
        match &member.project {
            Ok(project) => Examined::of(project, entry, Some(workspace), machine),
            Err(error) => Examined {
                entry,
                name: None,
                path: member.root.clone(),
                findings: Vec::new(),
                failures: vec![Said::of(error.code(), error)],
            },
        }
    }

    /// `project`, linked as `entry`.
    fn of(
        project: &MindProject,
        entry: Option<String>,
        _workspace: Option<&WeaverWorkspace>,
        machine: &Machine,
    ) -> Examined {
        let mut exam = Exam::new(project, machine);
        exam.codex_trust();
        exam.codex_reads();
        exam.claude_block();
        exam.skill_copies();
        exam.user_skills();
        Examined {
            entry,
            name: Some(project.name().to_owned()),
            path: project.root().to_path_buf(),
            findings: exam.symptoms.iter().map(Symptom::finding).collect(),
            failures: exam
                .failures
                .iter()
                .map(|failure| Said::of(failure.code(), failure))
                .collect(),
        }
    }
}

/// One thing doctor found, as a report says it: what it is, by its code,
/// the agents it keeps something from, the path it is about and what fixes
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Finding {
    /// The [`Symptom`]'s code, unique across core.
    pub code: &'static str,
    /// The agents it keeps something from, in report order: never empty
    /// but for what keeps nothing from an agent yet, a leftover of an
    /// install.
    pub agents: Vec<Agent>,
    /// The file or folder it is about, absolute: the one the fix acts on.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What is wrong, naming that path.
    pub message: String,
    /// What to do about it.
    pub fix: String,
}

/// What keeps one of a project's agents from what the project gives it,
/// each check's own, with a code of its own.
///
/// Each names the path it is about first, as every warning does, and says
/// what fixes it ([`Symptom::fix`]). A symptom rather than an error: the
/// project is read whole, and what doctor says of it is true.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Symptom {
    /// The project holds a `.codex/config.toml`, and nothing marks it
    /// trusted, so Codex reads none of it. `config` is the user's config
    /// looked in, none when no Codex home was given.
    #[error("{file}: Codex reads it only in a project it trusts, and {}", codex_mcp::unmarked(.project, .config.as_deref()))]
    CodexUntrusted {
        file: PathBuf,
        project: PathBuf,
        config: Option<PathBuf>,
    },
    /// The user's config marks the project `untrusted`, so Codex reads
    /// neither its `AGENTS.md` nor its `.codex/config.toml`.
    #[error("{config}: marks {project} `untrusted`, so Codex reads neither its AGENTS.md nor its .codex/config.toml")]
    CodexDistrusted { config: PathBuf, project: PathBuf },
    /// An `AGENTS.override.md` beside the project's `AGENTS.md`, which
    /// Codex reads in its place.
    #[error("{file}: Codex reads it in place of the {hidden} beside it, so nothing in that AGENTS.md reaches Codex, the rules Mindweaver gives it included")]
    CodexOverridden { file: PathBuf, hidden: PathBuf },
    /// No git work tree holds the project, so Codex, finding no root to
    /// read down from, reads only the folder it starts in.
    #[error("{project}: in no git work tree, so Codex reads its AGENTS.md only when started in that folder itself")]
    CodexNoGitRoot { project: PathBuf },
    /// What Codex reads from the git root down comes close to its budget.
    /// `file` is the last it reads, the project's own when there is one.
    #[error("{file}: the AGENTS.md files Codex reads down to it come to {bytes} bytes, close to the {CODEX_BUDGET} it reads before cutting off the rest")]
    CodexNearBudget { file: PathBuf, bytes: u64 },
    /// What Codex reads from the git root down comes to more than its
    /// budget, so it cuts off the end, where `file` is.
    #[error("{file}: the AGENTS.md files Codex reads down to it come to {bytes} bytes, past the {CODEX_BUDGET} it reads, so it cuts off the end, where this one is")]
    CodexOverBudget { file: PathBuf, bytes: u64 },
    /// Something keeps Claude Code from reading the project's `AGENTS.md`,
    /// and no import brings it in, so the rules its block holds reach
    /// Claude through nothing, none of them in Claude's own rules folder
    /// either. `file` is the one nearest the project that keeps it from
    /// reading `AGENTS.md`.
    #[error("{file}: Claude Code reads it in place of the AGENTS.md of {project}, and nothing imports that file, so these rules in its block reach Claude Code nowhere: {}", .routes.join(", "))]
    ClaudeMissesRules {
        file: PathBuf,
        project: PathBuf,
        /// Each rule the block holds that Claude's rules folder does not,
        /// in the block's order. Never empty.
        routes: Vec<String>,
    },
    /// Copies of one skill, in folders `agent` reads alike in the project,
    /// that do not hold the same, so which it loads is not told: OpenCode,
    /// which reads three, keeps one of them (docs/agents.md).
    #[error("{}: `{name}` is also at {}, which {} reads too, and the copies differ, so it loads one of them", .copies[0].display(), spelled(&.copies[1..]), .agent.title())]
    CopiesDiffer {
        agent: Agent,
        name: String,
        /// Every copy, in the order the agent's folders are listed. At
        /// least two.
        copies: Vec<PathBuf>,
    },
    /// A skill of the project whose name a skill at user scope holds too,
    /// in a folder the same agents read, holding something else: one name
    /// at both scopes resolves differently in each agent (D-22).
    #[error("{copy}: {} also read{} a skill `{name}` at user scope, at {user}, which holds something else, and one name at both scopes resolves differently in each agent", titled(.agents), if .agents.len() == 1 { "s" } else { "" })]
    UserSkillDiffers {
        name: String,
        copy: PathBuf,
        user: PathBuf,
        /// The project's agents that read both, in report order.
        agents: Vec<Agent>,
    },
}

/// `paths`, each as it is written, in a list a sentence holds.
fn spelled(paths: &[PathBuf]) -> String {
    let paths: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    listed(&paths)
}

/// `agents`, each by its title, in a list a sentence holds.
fn titled(agents: &[Agent]) -> String {
    let titles: Vec<String> = agents
        .iter()
        .map(|agent| agent.title().to_owned())
        .collect();
    listed(&titles)
}

/// `words`, with `and` before the last.
fn listed(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

codes!(Symptom {
    Self::CodexUntrusted { .. } => "doctor_codex_untrusted",
    Self::CodexDistrusted { .. } => "doctor_codex_distrusted",
    Self::CodexOverridden { .. } => "doctor_codex_overridden",
    Self::CodexNoGitRoot { .. } => "doctor_codex_no_git_root",
    Self::CodexNearBudget { .. } => "doctor_codex_near_budget",
    Self::CodexOverBudget { .. } => "doctor_codex_over_budget",
    Self::ClaudeMissesRules { .. } => "doctor_claude_misses_rules",
    Self::CopiesDiffer { .. } => "doctor_copies_differ",
    Self::UserSkillDiffers { .. } => "doctor_user_skill_differs",
});

impl Symptom {
    /// The agents it keeps something from, in report order.
    pub fn agents(&self) -> Vec<Agent> {
        match self {
            Symptom::CodexUntrusted { .. }
            | Symptom::CodexDistrusted { .. }
            | Symptom::CodexOverridden { .. }
            | Symptom::CodexNoGitRoot { .. }
            | Symptom::CodexNearBudget { .. }
            | Symptom::CodexOverBudget { .. } => vec![Agent::Codex],
            Symptom::ClaudeMissesRules { .. } => vec![Agent::Claude],
            Symptom::CopiesDiffer { agent, .. } => vec![*agent],
            Symptom::UserSkillDiffers { agents, .. } => agents.clone(),
        }
    }

    /// The path it is about: the one its fix acts on.
    pub fn path(&self) -> &Path {
        match self {
            Symptom::CodexUntrusted {
                file, config: None, ..
            } => file,
            Symptom::CodexUntrusted {
                config: Some(config),
                ..
            } => config,
            Symptom::CodexDistrusted { config, .. } => config,
            Symptom::CodexOverridden { file, .. }
            | Symptom::CodexNearBudget { file, .. }
            | Symptom::CodexOverBudget { file, .. } => file,
            Symptom::CodexNoGitRoot { project } => project,
            Symptom::ClaudeMissesRules { file, .. } => file,
            Symptom::CopiesDiffer { copies, .. } => &copies[0],
            Symptom::UserSkillDiffers { copy, .. } => copy,
        }
    }

    /// What fixes it, in a sentence a person acts on.
    pub fn fix(&self) -> String {
        match self {
            Symptom::CodexUntrusted {
                project,
                config: Some(config),
                ..
            } => format!(
                "trust the project when Codex asks, or add `trust_level = \"trusted\"` under \
                 `[projects.\"{}\"]` in {}",
                project.display(),
                config.display()
            ),
            Symptom::CodexUntrusted { config: None, .. } => String::from(
                "name Codex's home with --codex-home, for doctor to read whether it trusts the \
                 project",
            ),
            Symptom::CodexDistrusted { project, .. } => format!(
                "set `trust_level = \"trusted\"` under `[projects.\"{}\"]` there, if Codex is to \
                 work in the project",
                project.display()
            ),
            Symptom::CodexOverridden { hidden, .. } => format!(
                "move what Codex is to read into {} and remove this one",
                hidden.display()
            ),
            Symptom::CodexNoGitRoot { .. } => String::from(
                "make the project a git repository (`git init`), or start Codex in that folder",
            ),
            Symptom::CodexNearBudget { .. } | Symptom::CodexOverBudget { .. } => format!(
                "keep what those files say under {CODEX_BUDGET} bytes, or raise \
                 `project_doc_max_bytes` in Codex's config.toml"
            ),
            Symptom::ClaudeMissesRules { routes, .. } => format!(
                "install one of them again (`mind install rule:{}`), which renders every rule \
                 for Claude Code as the project stands, through an import of AGENTS.md or in \
                 {}",
                routes[0],
                claude_rules::folder().display()
            ),
            Symptom::CopiesDiffer { .. } => {
                String::from("make the copies hold the same, or keep one and remove the others")
            }
            Symptom::UserSkillDiffers { .. } => String::from(
                "rename the project's or the one at user scope, or make them hold the same",
            ),
        }
    }

    /// As a report says it.
    pub fn finding(&self) -> Finding {
        Finding {
            code: self.code(),
            agents: self.agents(),
            path: self.path().to_path_buf(),
            message: self.to_string(),
            fix: self.fix(),
        }
    }
}

/// What doctor could not read to tell, collected beside what it found.
#[derive(Debug, Error)]
pub enum DoctorFailure {
    /// The user's own config in Codex's home, which says whether Codex
    /// trusts the project.
    #[error(transparent)]
    CodexConfig(CodexMcpError),
    /// The repository, or a file Codex would read, which says how much of
    /// the `AGENTS.md` chain Codex reads: always the render's own
    /// [`agents_md::Warning::Unmeasured`].
    #[error(transparent)]
    Unmeasured(agents_md::Warning),
    /// A `CLAUDE.md` in or above the project, read to say whether Claude
    /// reads its `AGENTS.md`.
    #[error(transparent)]
    ClaudeMd(ClaudeMdError),
    /// A folder of skills, the project's or one at user scope, that cannot
    /// be listed, or a copy that cannot be read through to be compared,
    /// as discovery says each.
    #[error(transparent)]
    Discovery(DiscoveryFailure),
}

codes!(DoctorFailure {} carried {
    Self::CodexConfig(error) => error,
    Self::Unmeasured(warning) => warning,
    Self::ClaudeMd(error) => error,
    Self::Discovery(failure) => failure,
});

/// One project's examination as it goes: what it has found and what it
/// could not read so far.
struct Exam<'a> {
    project: &'a MindProject,
    machine: &'a Machine,
    /// The agents its settings name, each once, in report order: none for a
    /// project a `mind.toml` marks, which names none, so no check of an
    /// agent's is made there.
    agents: Vec<Agent>,
    symptoms: Vec<Symptom>,
    failures: Vec<DoctorFailure>,
    /// The skills each folder holds, listed once however many checks ask.
    listed: BTreeMap<PathBuf, Vec<(String, PathBuf)>>,
    /// Each copy's tree hash, read once however many checks compare it:
    /// none for one that cannot be read, which is said once.
    hashed: BTreeMap<PathBuf, Option<ContentHash>>,
}

impl<'a> Exam<'a> {
    fn new(project: &'a MindProject, machine: &'a Machine) -> Exam<'a> {
        let mut agents = project
            .settings()
            .map(|settings| settings.agents.clone())
            .unwrap_or_default();
        agents.sort();
        agents.dedup();
        Exam {
            project,
            machine,
            agents,
            symptoms: Vec::new(),
            failures: Vec::new(),
            listed: BTreeMap::new(),
            hashed: BTreeMap::new(),
        }
    }

    /// Every skill directly in `folder`, by name, in name order: each
    /// folder in it holding a `SKILL.md`, through a link as the agents read
    /// one (docs/agents.md), and none of a hidden name, which is what an
    /// install leaves beside a copy, said apart. None when `folder` is not
    /// there, which is no failure; one that cannot be listed is.
    fn skills_in(&mut self, folder: &Path) -> Vec<(String, PathBuf)> {
        if let Some(listed) = self.listed.get(folder) {
            return listed.clone();
        }
        let mut skills = Vec::new();
        match fs::read_dir(folder) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let Ok(name) = entry.file_name().into_string() else {
                        continue;
                    };
                    let path = entry.path();
                    if !name.starts_with('.') && path.is_dir() && install::holds(&path, Kind::Skill)
                    {
                        skills.push((name, path));
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) => {}
            Err(source) => {
                self.failures
                    .push(DoctorFailure::Discovery(DiscoveryFailure::Directory {
                        path: folder.to_path_buf(),
                        source,
                    }))
            }
        }
        skills.sort();
        self.listed.insert(folder.to_path_buf(), skills.clone());
        skills
    }

    /// What `copy` of the skill `name` holds, as [`hash::tree`] hashes an
    /// installed copy: none for one that cannot be read through, said once.
    fn hash(&mut self, name: &str, copy: &Path) -> Option<ContentHash> {
        if let Some(hashed) = self.hashed.get(copy) {
            return hashed.clone();
        }
        let hashed = match hash::tree(copy) {
            Ok(hash) => Some(hash),
            Err(source) => {
                self.failures
                    .push(DoctorFailure::Discovery(DiscoveryFailure::Uncompared {
                        name: name.to_owned(),
                        path: copy.to_path_buf(),
                        source,
                    }));
                None
            }
        };
        self.hashed.insert(copy.to_path_buf(), hashed.clone());
        hashed
    }

    /// The folders in the project `agent` reads skills from, every one of
    /// them ([`Agent::reads`]), not only those an install writes in: a copy
    /// somebody keeps in one is read all the same.
    fn read_by(&self, agent: Agent) -> Vec<PathBuf> {
        agent
            .reads(Kind::Skill)
            .iter()
            .map(|folder| self.root().join(folder))
            .collect()
    }

    fn root(&self) -> &Path {
        self.project.root()
    }

    fn names(&self, agent: Agent) -> bool {
        self.agents.contains(&agent)
    }

    /// Whether Codex trusts the project, as the user's config in its home
    /// says, read as an install reads it ([`UserConfig`]): marked
    /// `untrusted`, it reads neither the project's `AGENTS.md` nor its
    /// `.codex/config.toml`; marked neither way, it reads the first and not
    /// the second, which is said only where there is one to read.
    fn codex_trust(&mut self) {
        if !self.names(Agent::Codex) {
            return;
        }
        let user = match UserConfig::read(&self.machine.homes) {
            Ok(user) => user,
            Err(error) => {
                self.failures.push(DoctorFailure::CodexConfig(error));
                return;
            }
        };
        let trust = match &user {
            Some(user) => match user.trust(self.root()) {
                Ok(trust) => trust,
                Err(error) => {
                    self.failures.push(DoctorFailure::CodexConfig(error));
                    return;
                }
            },
            None => Trust::Unmarked,
        };
        let project = self.root().to_path_buf();
        let config = self
            .machine
            .homes
            .codex_home
            .as_ref()
            .map(|home| home.join(codex_mcp::USER_FILE));
        match (trust, user) {
            (Trust::Trusted, _) => {}
            (Trust::Untrusted, Some(user)) => self.symptoms.push(Symptom::CodexDistrusted {
                config: user.path,
                project,
            }),
            // Only a config read marks a project untrusted.
            (Trust::Untrusted, None) => {}
            (Trust::Unmarked, _) => {
                let file = self.root().join(codex_mcp::FILE);
                if std::fs::symlink_metadata(&file).is_ok() {
                    self.symptoms.push(Symptom::CodexUntrusted {
                        file,
                        project,
                        config,
                    });
                }
            }
        }
    }

    /// What keeps the project's `AGENTS.md` from Codex, as a render reads
    /// it ([`agents_md`]): an `AGENTS.override.md` beside it, no git root to
    /// read down from, and a chain near or past Codex's budget, each
    /// worked out for Codex started at the project's root. The override
    /// only while there is an `AGENTS.md` for it to hide.
    fn codex_reads(&mut self) {
        if !self.names(Agent::Codex) {
            return;
        }
        let hidden = self.root().join(agents_md::FILE);
        for warning in agents_md::codex_warnings(self.root()) {
            // Matched with no wildcard, so a warning added says here
            // whether doctor finds it.
            let symptom = match warning {
                agents_md::Warning::Overridden { file } => {
                    if !hidden.is_file() {
                        continue;
                    }
                    Symptom::CodexOverridden {
                        file,
                        hidden: hidden.clone(),
                    }
                }
                agents_md::Warning::NoGitRoot { project } => Symptom::CodexNoGitRoot { project },
                agents_md::Warning::NearBudget { bytes, files, .. } => Symptom::CodexNearBudget {
                    file: files.last().cloned().unwrap_or_else(|| hidden.clone()),
                    bytes,
                },
                agents_md::Warning::OverBudget { bytes, files, .. } => Symptom::CodexOverBudget {
                    file: files.last().cloned().unwrap_or_else(|| hidden.clone()),
                    bytes,
                },
                unmeasured @ agents_md::Warning::Unmeasured { .. } => {
                    self.failures.push(DoctorFailure::Unmeasured(unmeasured));
                    continue;
                }
            };
            self.symptoms.push(symptom);
        }
    }

    /// Whether Claude Code reads the rules the block of the project's
    /// `AGENTS.md` holds: through the file itself, an import of it, or each
    /// rule's own file in Claude's rules folder. A render chose one when the
    /// rules were installed ([`claude_md::reach`]), and a `CLAUDE.md` or a
    /// `CLAUDE.local.md` made since, in the project or above it, keeps
    /// Claude from the file with nothing said.
    fn claude_block(&mut self) {
        if !self.names(Agent::Claude) {
            return;
        }
        let routes = install::rules::block_routes(self.root());
        if routes.is_empty() {
            return;
        }
        let file = match claude_md::unread_by(self.project, &self.machine.homes) {
            Ok(Some(file)) => file,
            Ok(None) => return,
            Err(error) => {
                self.failures.push(DoctorFailure::ClaudeMd(error));
                return;
            }
        };
        // In the folder of either names, where a Mindflayer build filed
        // them too.
        let given = |route: &String| {
            Names::ALL.into_iter().any(|names| {
                self.root()
                    .join(claude_rules::file_under(names, route))
                    .is_file()
            })
        };
        let routes: Vec<String> = routes.into_iter().filter(|route| !given(route)).collect();
        if !routes.is_empty() {
            self.symptoms.push(Symptom::ClaudeMissesRules {
                file,
                project: self.root().to_path_buf(),
                routes,
            });
        }
    }

    /// Copies of one skill in folders an agent reads alike in the project,
    /// that differ: OpenCode reads `.agents/skills`, `.claude/skills` and
    /// `.opencode/skills`, and given one name twice keeps whichever it
    /// takes last (docs/agents.md, docs-only), where discovery compares
    /// only the copies in the folders an install writes in.
    fn skill_copies(&mut self) {
        for agent in self.agents.clone() {
            let mut copies: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
            for folder in self.read_by(agent) {
                for (name, copy) in self.skills_in(&folder) {
                    copies.entry(name).or_default().push(copy);
                }
            }
            for (name, copies) in copies {
                if copies.len() < 2 {
                    continue;
                }
                let hashes: Vec<Option<ContentHash>> =
                    copies.iter().map(|copy| self.hash(&name, copy)).collect();
                if hashes.iter().any(Option::is_none) {
                    continue;
                }
                if hashes.iter().any(|hash| *hash != hashes[0]) {
                    self.symptoms.push(Symptom::CopiesDiffer {
                        agent,
                        name,
                        copies,
                    });
                }
            }
        }
    }

    /// Each skill of the project whose name a skill at user scope holds
    /// too, in a folder the same agent reads ([`AgentHomes::skills_read_by`]),
    /// with something else in it (D-22): Claude prefers the personal one,
    /// OpenCode V2 lets `~/.agents` beat a project's `.claude`, and Codex
    /// shows both. One finding for each pair, with every agent reading both.
    fn user_skills(&mut self) {
        let mut met: BTreeMap<(PathBuf, PathBuf), (String, Vec<Agent>)> = BTreeMap::new();
        for agent in self.agents.clone() {
            let mut users: Vec<(String, PathBuf)> = Vec::new();
            for folder in self.machine.homes.skills_read_by(agent) {
                users.extend(self.skills_in(&folder));
            }
            for folder in self.read_by(agent) {
                for (name, copy) in self.skills_in(&folder) {
                    for (_, user) in users.iter().filter(|(user, _)| *user == name) {
                        // A project at the user's home, whose folder is the
                        // one at user scope.
                        if same_directory(&copy, user) {
                            continue;
                        }
                        let (Some(ours), Some(theirs)) =
                            (self.hash(&name, &copy), self.hash(&name, user))
                        else {
                            continue;
                        };
                        if ours != theirs {
                            let (_, agents) = met
                                .entry((copy.clone(), user.clone()))
                                .or_insert_with(|| (name.clone(), Vec::new()));
                            if !agents.contains(&agent) {
                                agents.push(agent);
                            }
                        }
                    }
                }
            }
        }
        for ((copy, user), (name, agents)) in met {
            self.symptoms.push(Symptom::UserSkillDiffers {
                name,
                copy,
                user,
                agents,
            });
        }
    }
}

#[cfg(test)]
#[path = "doctor_test.rs"]
mod tests;
