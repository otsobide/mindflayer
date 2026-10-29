//! The panel's backend: what the local server hands each request from the
//! browser to (#67).
//!
//! The server is transport, generic over a [`Backend`] and depending on no
//! mindflayer crate (D-17); this is the CLI's, with its own [`Query`], the
//! [`Task`] the TUI runs, and an [`Outcome`] of its own. It is a front end
//! as the TUI is, and works as the TUI does (D-18): a read is a query core
//! answers, the snapshot or a file's contents, and a change is a task turned
//! into the command lines it is, each shown before it runs, then run through
//! the real parsers ([`run_line`]) on the server's one writer thread.
//!
//! What it adds is distrust, since every request comes from a browser:
//!
//! - **It acts on one workspace**, the one the panel was launched on,
//!   resolved once ([`PanelBackend::launch`]) and named to every line it
//!   runs, so a change made meanwhile elsewhere on the disk cannot send a
//!   line to another.
//! - **The browser supplies no `-C`, `--home`, `-W` or agent home.** A task
//!   has no field for them, no value it holds is read as an option, and
//!   every line runs with the launch's home and agent homes, from the
//!   workspace's root, as the TUI's do.
//! - **A task names only what the path policy allows** ([`Admission`]): a
//!   project by its entry, which becomes the folder the registry resolves,
//!   and a folder not linked yet by an absolute, normalized path outside
//!   every `.mindflayer`, shown verbatim.
//! - **A task no front end without a terminal runs is refused**
//!   ([`Task::refusal`]): a screen, an editor, a shell's code, a gather, and
//!   `show`, which prints an artifact whole through any link.
//! - **A file reaches the browser only as text inside JSON**, held to its
//!   holder once its links are followed, and never larger than core's cap
//!   ([`Contents`]): the one way a file's contents leave, since `show` is
//!   refused.
//!
//! Every refusal has a code, as every error does, and nothing of a refused
//! task runs.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use mindflayer_core::code::Said;
use mindflayer_core::contents::Contents;
use mindflayer_core::gather::Watch;
use mindflayer_core::policy;
use mindflayer_core::resolve::{resolve, Intent, Reason, Resolved};
use mindflayer_core::view::Snapshot;
use mindflayer_core::{FlayerWorkspace, FLAYER_CONFIG, FLAYER_DIR};
use mindflayer_server::{Backend, Refusal};
use serde::{Deserialize, Serialize};

use crate::admit::Admission;
use crate::commands::{run_line, Ran};
use crate::task::{shell, Holder, Task};
use crate::{CliError, Lookup, Warning};

/// A read the panel asks for.
///
/// Tagged as a task is, by its name in snake_case beside its fields:
/// `{"type": "snapshot"}`, `{"type": "contents", "holder": {"type":
/// "workspace"}, "reference": "skill:deploy", "file": "SKILL.md"}`. A field
/// a query does not have is refused, as a task's is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// Everything the workspace holds ([`Snapshot`]).
    ///
    /// A variant with no fields rather than a unit one, which serde would
    /// read from an object holding any field at all: with braces there is
    /// something for `deny_unknown_fields` to hold the object to.
    Snapshot {},
    /// One file of one artifact, as text ([`Contents`]): `reference` names
    /// the artifact in `holder`, the workspace's own or a project by its
    /// entry, and `file` is its route from the artifact's folder, or a
    /// rule's file name.
    Contents {
        holder: Holder,
        reference: String,
        file: String,
    },
}

/// What running a task's lines produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// Whether every line ran and found everything in order.
    pub ok: bool,
    /// Each line that ran, in order: up to the first that failed, for a task
    /// whose lines lean on each other ([`Task::chained`]).
    pub runs: Vec<Run>,
}

/// What one command line produced, as a terminal would show it, with the
/// code of everything that went wrong beside its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    /// The line, as it would be typed into a shell ([`shell`]).
    pub command: String,
    pub stdout: String,
    /// Each line it said on stderr, with its code; none for a hint or a
    /// note, which no code is given to.
    pub stderr: Vec<Warning>,
    /// Why it could not run, when it could not.
    pub error: Option<Said>,
    /// Whether it ran and found everything in order: what its exit status
    /// would have said.
    pub ok: bool,
}

/// The backend over the workspace a panel was launched on.
#[derive(Debug, Clone)]
pub struct PanelBackend {
    /// The workspace's root: where every line runs from, and what each is
    /// pinned to.
    root: PathBuf,
    /// Why it is the one, which the snapshot says.
    reason: Reason,
    /// Where to look for workspaces, as the launch decided it: its home and
    /// agent homes, which every line runs with, and what chose the
    /// workspace, which decides whether a line needs `-W .` to be read again
    /// from the root.
    lookup: Lookup,
}

impl PanelBackend {
    /// The backend for the workspace `lookup` resolves from `directory`,
    /// resolved as a read resolves it, here and once: the default one in
    /// home as it will be, when nothing has made it yet, which the first
    /// change makes, as from the TUI.
    pub fn launch(directory: &Path, lookup: &Lookup) -> Result<PanelBackend, CliError> {
        let resolved = resolve(&lookup.inputs(directory, Intent::Read))?
            .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))?;
        Ok(PanelBackend {
            root: resolved.workspace.root().to_path_buf(),
            reason: resolved.reason,
            lookup: lookup.clone(),
        })
    }

    /// The root of the workspace it acts on.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The workspace, as the disk has it now: the default one as it will
    /// be, while nothing has made it yet.
    ///
    /// Only the default one: any other was there at launch, and one gone
    /// since is refused rather than taken for one to make, which the first
    /// change would do to it.
    fn workspace(&self) -> Result<FlayerWorkspace, Refusal> {
        let made = self.root.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file();
        if made || self.reason != Reason::Home {
            FlayerWorkspace::open(&self.root)
        } else {
            FlayerWorkspace::unmade(&self.root)
        }
        .map_err(|error| refused(error.code(), error))
    }

    /// What every line runs with: the launch's home and agent homes, and the
    /// workspace named, so each finds it and nothing but it, once it is
    /// made. Before then the default is named by nothing, and found as the
    /// TUI's lines find it, walking up from its root; its first change makes
    /// it first ([`plan`](Self::plan)).
    fn pinned(&self, workspace: &FlayerWorkspace) -> Lookup {
        Lookup {
            flag: Some(if workspace.is_made() {
                self.root.clone()
            } else {
                PathBuf::new()
            }),
            activated: None,
            ..self.lookup.clone()
        }
    }

    /// The lines `task` is, as they would run against the disk as it is
    /// now, with what they run with and whether each leans on the one
    /// before; or why none of them runs.
    fn plan(&self, task: &Task) -> Result<Plan, Refusal> {
        if let Some((line, refusal)) = task.refused_line() {
            return Err(refused(
                refusal.code(),
                format!("`{}` {refusal}", shell(&line)),
            ));
        }
        let workspace = self.workspace()?;
        let admitted = Admission {
            workspace: &workspace,
            lookup: &self.lookup,
        }
        .task(task)
        .map_err(|error| refused(error.code(), error))?;
        let mut lines = admitted.commands();
        // As the TUI does: a change to the default before it is made makes
        // it first, by a line of its own, rather than leave the change to
        // find a workspace above home while it walks up from the root.
        if !workspace.is_made() && admitted.writes() && !lines.is_empty() {
            lines.splice(0..0, Task::InitWorkspace.commands());
        }
        Ok(Plan {
            lines,
            lookup: self.pinned(&workspace),
            chained: admitted.chained(),
        })
    }
}

/// What a task runs: its lines, what they run with, and whether one failing
/// stops the rest.
struct Plan {
    lines: Vec<Vec<String>>,
    lookup: Lookup,
    chained: bool,
}

impl Backend for PanelBackend {
    type Query = Query;
    type Task = Task;
    type Outcome = Outcome;

    fn query(&self, query: Query) -> Result<serde_json::Value, Refusal> {
        let workspace = self.workspace()?;
        // Each is strings, numbers and lists of them, and maps keyed by
        // field names, with every path written as text whatever its bytes:
        // nothing in either can fail to serialize.
        match query {
            Query::Snapshot {} => {
                let resolved = Resolved {
                    workspace,
                    reason: self.reason,
                };
                let snapshot = Snapshot::of(&resolved, self.lookup.home.as_deref());
                Ok(serde_json::to_value(snapshot).expect("a snapshot serializes"))
            }
            Query::Contents {
                holder,
                reference,
                file,
            } => {
                let contents = match holder {
                    Holder::Workspace => Contents::of_library(&workspace, &reference, &file),
                    Holder::Project(entry) | Holder::Elsewhere(entry) => {
                        let member = policy::linked(&workspace, &entry)
                            .map_err(|error| refused(error.code(), error))?;
                        let project = member
                            .project
                            .map_err(|error| refused(error.code(), error))?;
                        Contents::of_project(&project, &reference, &file)
                    }
                }
                .map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(contents).expect("contents serialize"))
            }
        }
    }

    fn preview(&self, task: &Task) -> Result<Vec<String>, Refusal> {
        Ok(self
            .plan(task)?
            .lines
            .iter()
            .map(|line| shell(line))
            .collect())
    }

    /// Planned again rather than trusted to the preview the server asked
    /// for first: the plan is of the disk as the run meets it, and the
    /// trait promises no preview came first.
    fn run(&self, task: Task) -> Result<Outcome, Refusal> {
        let plan = self.plan(&task)?;
        let watch = Watch::new();
        let mut runs: Vec<Run> = Vec::new();
        for words in &plan.lines {
            // Every refusal of the list, where the TUI refuses only a
            // screen: the plan refused them already, and a line that is one
            // all the same is not run here either.
            let run = ran(
                words,
                run_line(words, &self.root, &plan.lookup, &watch, |_| true),
            );
            let failed = run.error.is_some();
            runs.push(run);
            if failed && plan.chained {
                break;
            }
        }
        Ok(Outcome {
            ok: runs.iter().all(|run| run.ok),
            runs,
        })
    }

    /// The workspace's own folder, and every project and workspace it names:
    /// for live updates (#74), which nothing reads yet.
    fn watched_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.root.join(FLAYER_DIR)];
        if let Ok(workspace) = self.workspace() {
            paths.extend(workspace.members().into_iter().map(|member| member.root));
            paths.extend(workspace.loads().into_iter().map(|loaded| loaded.root));
        }
        paths
    }
}

/// What came of `words`, as a run says it.
fn ran(words: &[String], ran: Ran) -> Run {
    let command = shell(words);
    match ran {
        Ran::Refused(refusal) => Run {
            command,
            stdout: String::new(),
            stderr: Vec::new(),
            error: Some(Said {
                code: refusal.code(),
                message: refusal.to_string(),
            }),
            ok: false,
        },
        Ran::Done(Ok(outcome)) => Run {
            command,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: None,
            ok: outcome.ok,
        },
        Ran::Done(Err(failure)) => Run {
            command,
            stdout: String::new(),
            stderr: failure.warnings,
            error: Some(Said {
                code: failure.error.code(),
                message: failure.error.to_string(),
            }),
            ok: false,
        },
    }
}

/// A refusal of `code`, saying `message`.
fn refused(code: &'static str, message: impl Display) -> Refusal {
    Refusal {
        code,
        message: message.to_string(),
    }
}
