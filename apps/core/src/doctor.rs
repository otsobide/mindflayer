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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::agents_md::{self, CODEX_BUDGET};
use crate::code::{codes, Said};
use crate::codex_mcp::{self, CodexMcpError, Trust, UserConfig};
use crate::paths;
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
}

codes!(Symptom {
    Self::CodexUntrusted { .. } => "doctor_codex_untrusted",
    Self::CodexDistrusted { .. } => "doctor_codex_distrusted",
    Self::CodexOverridden { .. } => "doctor_codex_overridden",
    Self::CodexNoGitRoot { .. } => "doctor_codex_no_git_root",
    Self::CodexNearBudget { .. } => "doctor_codex_near_budget",
    Self::CodexOverBudget { .. } => "doctor_codex_over_budget",
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
}

codes!(DoctorFailure {} carried {
    Self::CodexConfig(error) => error,
    Self::Unmeasured(warning) => warning,
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
        }
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
}

#[cfg(test)]
#[path = "doctor_test.rs"]
mod tests;
