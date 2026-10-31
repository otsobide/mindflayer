//! The panel's backend: what the local server hands each request from the
//! browser to (#67).
//!
//! The server is transport, generic over a [`Backend`] and depending on no
//! mindflayer crate (D-17); this is the CLI's, with its own [`Query`], the
//! [`Task`] the TUI runs, and an [`Outcome`] of its own. It is a front end
//! as the TUI is, and works as the TUI does (D-18): a read is a query core
//! answers, the snapshot or a file's contents, and a change is a task turned
//! into the command lines it is, each shown before it runs, with what of it
//! the panel has somebody confirm first ([`Confirm`]), then run through the
//! real parsers ([`run_line`]) on the server's one writer thread.
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
//! - **A scan walks only the folders the browser names**, each held to the
//!   path policy as a folder to link is, and is only read: it is asked for
//!   by hand ([`Query::Scan`]), never with the snapshot, which a live update
//!   reads again on every change.
//! - **A plan names only linked projects**, each box's by its entry, held
//!   to the path policy as a task's projects are, and is only read: what
//!   the boxes ticked amount to is core's to say ([`Query::Plan`]), and
//!   nothing runs until its tasks are sent back.
//! - **What an update would do is only read** ([`Query::Updates`]): the
//!   dry run of `flayer update --all`, core's, naming nothing a browser
//!   sent; an update runs only as the task the panel sends for it.
//! - **A draft is only judged** ([`Query::Validate`]): what core says of an
//!   artifact a form or the editor is about to send, by the checks the
//!   command makes and in its words, with what `validate` would say of it;
//!   nothing is written until the task that writes it is sent.
//! - **A file reaches the browser only as text inside JSON**, held to its
//!   holder once its links are followed, and never larger than core's cap
//!   ([`Contents`]): the one way a file's contents leave, since `show` is
//!   refused. What an artifact holds is listed the same way ([`Files`]),
//!   each file by the route a file is then asked for by.
//!
//! Every refusal has a code, as every error does, and nothing of a refused
//! task runs.

use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use mindflayer_core::code::Said;
use mindflayer_core::contents::{Contents, Files};
use mindflayer_core::draft;
use mindflayer_core::gather::Watch;
use mindflayer_core::install::{self, Cell, Conflict, Locked};
use mindflayer_core::resolve::{resolve, Intent, Reason, Resolved};
use mindflayer_core::scan::{self, Order, DEFAULT_DEPTH};
use mindflayer_core::update::Updates;
use mindflayer_core::view::Snapshot;
use mindflayer_core::{paths, policy, watch};
use mindflayer_core::{
    FlayerWorkspace, Found, Kind, MindProject, Reference, Scan, FLAYER_CONFIG, FLAYER_DIR,
};
use mindflayer_server::{Backend, Preview, Refusal, Watched};
use serde::{Deserialize, Serialize};

use crate::admit::Admission;
use crate::commands::project::{scan_warned, worth_linking};
use crate::commands::{run_line, Ran};
use crate::task::{self, cautioned, shell, Caution, Holder, MindsChange, Task};
use crate::tui::state::{transcript, Done};
use crate::{CliError, Lookup, Warning};

/// A read the panel asks for.
///
/// Tagged as a task is, by its name in snake_case beside its fields:
/// `{"type": "snapshot"}`, `{"type": "files", "holder": {"type":
/// "workspace"}, "reference": "skill:deploy"}`, `{"type": "contents",
/// "holder": {"type": "workspace"}, "reference": "skill:deploy", "file":
/// "SKILL.md"}`, `{"type": "scan", "roots": ["/home/you/Work"], "depth":
/// null}`, `{"type": "plan", "selection": [{"project": "alpha",
/// "reference": "skill:deploy", "origin": "workspace", "wanted": true}]}`,
/// `{"type": "updates"}`, `{"type": "validate", "holder": {"type":
/// "workspace"}, "draft": {"type": "edit", "reference": "skill:deploy",
/// "text": "..."}}`. A field a query does not have is refused, as a task's
/// is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// Everything the workspace holds ([`Snapshot`]).
    ///
    /// A variant with no fields rather than a unit one, which serde would
    /// read from an object holding any field at all: with braces there is
    /// something for `deny_unknown_fields` to hold the object to.
    Snapshot {},
    /// Every file and folder of one artifact ([`Files`]), named as for
    /// [`Contents`](Self::Contents), whose `file` each route it lists is.
    Files { holder: Holder, reference: String },
    /// One file of one artifact, as text ([`Contents`]): `reference` names
    /// the artifact in `holder`, the workspace's own or a project by its
    /// entry, and `file` is its route from the artifact's folder, or a
    /// rule's file name.
    Contents {
        holder: Holder,
        reference: String,
        file: String,
    },
    /// What could be linked under `roots`, `depth` levels down, as `flayer
    /// scan --suggest` finds it ([`Scanned`]): each root an absolute,
    /// normalized path outside every `.mindflayer` ([`policy::path`]), as a
    /// folder to link is, which is only read; none is the workspace's root,
    /// and no depth core's own (`scan::DEFAULT_DEPTH`), as for the command.
    ///
    /// A query of its own, asked for only when somebody asks, rather than a
    /// part of the snapshot: a scan walks the disk far past what a snapshot
    /// reads, and a live update reads the snapshot again on every change.
    Scan {
        roots: Vec<String>,
        depth: Option<usize>,
    },
    /// What `selection`, boxes of projects and offers ticked or not,
    /// amounts to ([`Planned`]): core's plan of it ([`install::plan`]), the
    /// one place the rules of a batch live, each change as the task that
    /// runs it, beside what clashes and what is locked.
    ///
    /// A read, as marking a box on the install screen is: nothing runs
    /// until the panel sends the tasks back, to be previewed and run as any
    /// task is.
    /// Each project a box names is an entry the workspace links, held to
    /// the path policy as a task's projects are
    /// ([`Admission::selection`]).
    Plan { selection: Vec<Cell> },
    /// What updating every project the workspace links would do ([`Updates`]):
    /// each installation an update says something of, how it stands, what
    /// an update does with it, and why it leaves one it leaves, every copy
    /// edited since left alone, as `flayer update --all` says without
    /// `--yes`.
    ///
    /// A query of its own, asked for by the page of updates, rather than a
    /// part of the snapshot: telling what an update does reads where each
    /// installation came from again, past what the snapshot reads, and a
    /// live update reads the snapshot again on every change. Nothing in it
    /// is sent by the browser, so nothing is held to the path policy.
    Updates {},
    /// What core says of `draft`, an artifact a command is about to write
    /// in `holder` ([`draft::Judged`]): why the command would write nothing, in its
    /// words, and what `validate` would say of it once written, in its own.
    ///
    /// How the panel's forms and editor judge what is typed as it is typed,
    /// by core's rules and never a copy of them in TypeScript: only a read,
    /// as a plan is, which writes nothing, and a draft judged fit is still
    /// held to the same rules by the line that writes it. The holder is held
    /// to the path policy as a read of an artifact's is.
    Validate { holder: Holder, draft: Draft },
}

/// An artifact a command is about to write, as the panel asks core to judge
/// it ([`Query::Validate`]): tagged as a query is, `{"type": "add", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Draft {
    /// What `add` would write, by the fields an `add` task carries: a
    /// template by its name, as `--template` takes it.
    Add {
        kind: Kind,
        name: String,
        description: String,
        template: Option<String>,
    },
    /// What `edit --contents` would write into the artifact `reference`
    /// names, `kind:name`, given `text` whole: its `SKILL.md`, or a rule's
    /// file.
    Edit { reference: String, text: String },
}

/// What a scan the panel asked for found ([`Query::Scan`]): what `flayer
/// scan --suggest --json` prints, each directory with what the panel needs
/// to offer it, and what the command warns of beside it.
///
/// A shape of the backend's rather than the envelope the command prints,
/// since what it adds to each directory is a front end's: the words the
/// command says why in, what marking it asks for, and the entry a relink
/// names. Each worked out here, in Rust, where the minds screen works out
/// the same, rather than by the panel from the scan's fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Scanned {
    /// Where it looked, each once, in the order given: the workspace's root
    /// when it was given none.
    pub roots: Vec<String>,
    /// How many levels below each root it looked.
    pub depth: usize,
    /// Every directory it found, in the order `flayer scan --suggest` ranks
    /// them: what is not linked first, most worth linking first, then what
    /// is.
    pub found: Vec<Suggestion>,
    /// Each entry where no project is any more whose id the ledger never
    /// learned, as the scan spells it: what `flayer relink <entry> <path>`
    /// follows, which only somebody who knows where it went can type.
    pub untraced: Vec<String>,
    /// Whether it stopped at core's bound before looking everywhere, which
    /// `warnings` says too.
    pub truncated: bool,
    /// What it could not read, and that it stopped early, as `flayer scan`
    /// warns of them, each with its code.
    pub warnings: Vec<Warning>,
}

/// One directory a scan found, and what the panel offers for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Suggestion {
    /// What `flayer scan --json` prints of it.
    pub found: Found,
    /// Why it is worth linking, in the words `flayer scan --suggest` and the
    /// minds screen say it in ([`worth_linking`]); none for one linked.
    pub why: Option<String>,
    /// What marking it asks for ([`MindsChange::of_found`]): linking it, or
    /// making it a project first. None for one linked, and for one found
    /// moved, which wants a relink rather than a second entry beside the one
    /// it left stale.
    pub offers: Option<MindsChange>,
    /// The entry it moved from, as the marker spells it, which a relink
    /// names: matched by core's `scan::moved_from`, as the minds screen
    /// matches it, since the scan spells `moved` as `flayer link` would
    /// take it. None when it did not move.
    pub moved_from: Option<String>,
}

impl Scanned {
    /// What `scan` found in `workspace`, as the panel is answered with it.
    fn of(workspace: &FlayerWorkspace, scan: &Scan) -> Scanned {
        let members = workspace.members();
        let left = |moved: &str| {
            members
                .iter()
                .find(|member| scan::moved_from(moved, workspace.root(), &member.root))
                .and_then(|member| paths::to_config_string(&member.entry))
        };
        let found = scan
            .found
            .iter()
            .map(|found| {
                let moved_from = found.moved.as_deref().and_then(left);
                Suggestion {
                    why: (!found.linked).then(|| {
                        worth_linking(
                            &found.route,
                            found.moved.as_deref(),
                            found.reason,
                            &found.agent_files,
                        )
                    }),
                    offers: (!found.linked && moved_from.is_none())
                        .then(|| MindsChange::of_found(found.mind)),
                    moved_from,
                    found: found.clone(),
                }
            })
            .collect();
        Scanned {
            roots: scan
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
            depth: scan.depth,
            found,
            untraced: scan.untraced.clone(),
            truncated: scan.truncated,
            warnings: scan_warned(scan),
        }
    }
}

/// What a selection the panel asked to plan amounts to ([`Query::Plan`]):
/// core's plan of it ([`install::Plan`]), each change in it the task that
/// runs it.
///
/// Tasks rather than core's changes, so the panel sends back what it was
/// handed, never a task it puts together itself: the `install` and
/// `uninstall` tasks, each a project by the entry its box named it by, which
/// the backend turns into the folder the registry resolves as it does any
/// task's. Sent as one `apply_install`, as the install screen applies its
/// marks, each line runs whatever came of the ones before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Planned {
    /// Each `install` and `uninstall` that carries the selection out, in
    /// the order to run them.
    pub tasks: Vec<Task>,
    /// Boxes of one name in one project that cannot all be as asked, none
    /// of which is planned.
    pub conflicts: Vec<Conflict>,
    /// Boxes whose change is held back, each by the code the command would
    /// refuse it with, or why the project could not be asked about it.
    pub locked: Vec<Locked>,
}

impl From<install::Plan> for Planned {
    fn from(plan: install::Plan) -> Self {
        Planned {
            tasks: plan.changes.into_iter().map(Task::from).collect(),
            conflicts: plan.conflicts,
            locked: plan.locked,
        }
    }
}

/// What running a task's lines produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

/// One thing a line of a task takes away, which the panel has somebody
/// confirm before it asks for the run: an item of a preview's `confirm`,
/// one for each [`Caution`] of each line ([`cautioned`]).
///
/// The words beside the word, so the panel says what a line takes in the
/// CLI's words rather than its own, and draws by the word alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Confirm {
    /// Which of the preview's lines, by its place among them, from 0.
    pub line: usize,
    /// What it takes away.
    pub caution: Caution,
    /// The same, said for whoever is asked.
    pub message: String,
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
    /// Every line it ran for a task that changes something, in the order
    /// they ran ([`transcript`](Self::transcript)). Shared by its clones.
    changed: Arc<Mutex<Vec<Run>>>,
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
            changed: Arc::default(),
        })
    }

    /// Every line run so far for a task that changes something, with what
    /// each said, as a shell session would show it: what `flayer ui` prints
    /// once it stops, as the TUI prints its own when it closes, so what the
    /// panel did outlives the server and can be done again without it.
    ///
    /// The TUI's transcript, laid out by the TUI's function, and kept by the
    /// TUI's rule ([`Task::writes`]): a read changed nothing there is to do
    /// again. Kept by every clone of the backend alike, so one kept back
    /// while the server holds another reads what the server's ran.
    pub fn transcript(&self) -> String {
        let changed = self.changed.lock().unwrap_or_else(PoisonError::into_inner);
        transcript(&changed.iter().map(Done::from).collect::<Vec<_>>())
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
        // find a workspace above home while it walks up from the root. Not
        // in front of a task that makes it already: the TUI offers that one
        // only with nothing made, but a browser may send it at any time.
        let init = Task::InitWorkspace.commands();
        if !workspace.is_made()
            && admitted.writes()
            && !lines.is_empty()
            && !lines.starts_with(&init)
        {
            lines.splice(0..0, init);
        }
        Ok(Plan {
            lines,
            lookup: self.pinned(&workspace),
            chained: admitted.chained(),
            writes: admitted.writes(),
        })
    }
}

/// What a task runs: its lines, what they run with, whether one failing
/// stops the rest, and whether they change something.
struct Plan {
    lines: Vec<Vec<String>>,
    lookup: Lookup,
    chained: bool,
    writes: bool,
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
            Query::Files { holder, reference } => {
                let files = match linked(&workspace, holder)? {
                    None => Files::of_library(&workspace, &reference),
                    Some(project) => Files::of_project(&project, &reference),
                }
                .map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(files).expect("a listing serializes"))
            }
            Query::Contents {
                holder,
                reference,
                file,
            } => {
                let contents = match linked(&workspace, holder)? {
                    None => Contents::of_library(&workspace, &reference, &file),
                    Some(project) => Contents::of_project(&project, &reference, &file),
                }
                .map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(contents).expect("contents serialize"))
            }
            Query::Scan { roots, depth } => {
                let roots = roots
                    .iter()
                    .map(|root| policy::path(root))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| refused(error.code(), error))?;
                let request = scan::Request {
                    roots,
                    depth: depth.unwrap_or(DEFAULT_DEPTH),
                    from: None,
                    order: Order::Suggested,
                };
                let scanned = Scanned::of(&workspace, &scan::scan(&workspace, &request));
                Ok(serde_json::to_value(scanned).expect("a scan serializes"))
            }
            Query::Plan { selection } => {
                Admission {
                    workspace: &workspace,
                    lookup: &self.lookup,
                }
                .selection(&selection)
                .map_err(|error| refused(error.code(), error))?;
                let plan = install::plan(&workspace, &selection)
                    .map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(Planned::from(plan)).expect("a plan serializes"))
            }
            Query::Updates {} => {
                let updates =
                    Updates::read(&workspace).map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(updates).expect("what an update would do serializes"))
            }
            // The holder a line would change, by the workspace on screen,
            // which a line of the panel's names as it runs, so the templates
            // offered and the records consulted are that one's.
            Query::Validate { holder, draft } => {
                let holder = linked(&workspace, holder)?.unwrap_or_else(|| workspace.own());
                let judged = match draft {
                    Draft::Add {
                        kind,
                        name,
                        description,
                        template,
                    } => draft::added(
                        Some(&workspace),
                        &holder,
                        kind,
                        &name,
                        &description,
                        template.as_deref(),
                    ),
                    Draft::Edit { reference, text } => draft::edited(
                        Some(&workspace),
                        &holder,
                        &Reference::parse(&reference),
                        &text,
                    ),
                };
                Ok(serde_json::to_value(judged).expect("a judgement serializes"))
            }
        }
    }

    /// Each line as a shell would read it, and what the panel has somebody
    /// confirm first, both of the one plan: so what the panel asks about is
    /// the lines it shows, a `flayer init` in front of them counted.
    fn preview(&self, task: &Task) -> Result<Preview, Refusal> {
        let plan = self.plan(task)?;
        let confirm = cautioned(&plan.lines)
            .into_iter()
            .map(|(line, caution)| Confirm {
                line,
                caution,
                message: caution.to_string(),
            })
            // A number, a word and its words: nothing to fail on.
            .map(|confirm| serde_json::to_value(confirm).expect("a confirm serializes"))
            .collect();
        Ok(Preview {
            lines: plan.lines.iter().map(|line| shell(line)).collect(),
            confirm,
        })
    }

    /// Planned again rather than trusted to the preview the server asked
    /// for first: the plan is of the disk as the run meets it, and the
    /// trait promises no preview came first.
    ///
    /// A save's text is put where its line reads it before the line runs,
    /// and taken away once the lines have run, whatever came of them
    /// ([`Scratch`]): the browser sends the text, never a path.
    fn run(&self, task: Task) -> Result<Outcome, Refusal> {
        let plan = self.plan(&task)?;
        let _scratch = match &task {
            Task::Save { text, .. } => Some(
                Scratch::put(&self.workspace()?, text)
                    .map_err(|error| refused(error.code(), error))?,
            ),
            _ => None,
        };
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
        if plan.writes {
            self.changed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(runs.iter().cloned());
        }
        Ok(Outcome {
            ok: runs.iter().all(|run| run.ok),
            runs,
        })
    }

    /// Every path core names for a snapshot of the workspace
    /// ([`watch::watched`]), with the home the snapshot is read with, each
    /// scope as the holder a query names: `{"type": "workspace"}`, or
    /// `{"type": "project", "entry": ...}` by the entry the snapshot lists.
    ///
    /// One that will not open now, gone or broken since launch, is watched
    /// where its marker and ledger would be, so its coming back is seen.
    fn watched_paths(&self) -> Vec<Watched> {
        let Ok(workspace) = self
            .workspace()
            .or_else(|_| FlayerWorkspace::unmade(&self.root))
        else {
            return Vec::new();
        };
        watch::watched(&workspace, self.lookup.home.as_deref())
            .into_iter()
            .map(|watched| Watched {
                path: watched.path,
                scope: scope(watched.scope),
            })
            .collect()
    }
}

/// A save's text, in the file its line reads it from
/// ([`task::scratch_route`]): the workspace's scratch folder as core has it
/// ([`FlayerWorkspace::scratch_dir`]), the file named by the text
/// ([`task::scratch_name`]). Held for as long as this is: taken away when
/// it is dropped, whatever came of the line.
///
/// Made with `create_new`, in a folder made with `create_dir`, never
/// `create_dir_all`, so no `.mindflayer` is made where there is none, which
/// a line of the panel's would then take for a workspace. The folder is
/// looked at once it is there, and a link in its place refuses the save:
/// `create_dir` says a link is there as it says a folder is, and
/// `create_new` refuses a link only as the file itself, following one in
/// the folder's place, which a cloned workspace can bring to an agent's own
/// folder. A file already there under the name is taken as it is when it
/// holds the text exactly, as one an interrupted run left would, the name
/// being the text's hash; anything else there refuses the save, and is left
/// as it is.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// `text`, put where the line saving it reads it from, in `workspace`.
    fn put(workspace: &FlayerWorkspace, text: &str) -> Result<Scratch, CliError> {
        let folder = workspace.scratch_dir();
        let path = folder.join(task::scratch_name(text));
        let unwritable = |source| CliError::Scratch {
            path: path.clone(),
            source,
        };
        match fs::create_dir(&folder) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                return Err(unwritable(error))
            }
            _ => {}
        }
        if !fs::symlink_metadata(&folder).is_ok_and(|found| found.is_dir()) {
            return Err(unwritable(io::Error::other(
                "its folder is a link or no folder, and nothing is written through one",
            )));
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                // Held from here, so a write that fails halfway takes away
                // what it wrote.
                let scratch = Scratch { path: path.clone() };
                file.write_all(text.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(unwritable)?;
                Ok(scratch)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let held = fs::symlink_metadata(&path).is_ok_and(|found| found.is_file())
                    && fs::read(&path).is_ok_and(|held| held == text.as_bytes());
                if held {
                    Ok(Scratch { path })
                } else {
                    Err(unwritable(error))
                }
            }
            Err(error) => Err(unwritable(error)),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// What a watched path is of, as a query names the holder it reads from.
fn scope(scope: watch::Scope) -> serde_json::Value {
    let holder = match scope {
        watch::Scope::Workspace => Holder::Workspace,
        watch::Scope::Project(entry) => Holder::Project(entry),
    };
    serde_json::to_value(holder).expect("a holder serializes")
}

/// The project a query's `holder` names by its entry in `workspace`, or none
/// for the workspace's own: a read of an artifact is held to one or the
/// other, as a task's holder is ([`policy::linked`]).
fn linked(workspace: &FlayerWorkspace, holder: Holder) -> Result<Option<MindProject>, Refusal> {
    match holder {
        Holder::Workspace => Ok(None),
        Holder::Project(entry) | Holder::Elsewhere(entry) => {
            let member =
                policy::linked(workspace, &entry).map_err(|error| refused(error.code(), error))?;
            member
                .project
                .map(Some)
                .map_err(|error| refused(error.code(), error))
        }
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

/// A line the panel ran, as the TUI keeps one it ran: its words for each
/// line of stderr and for why it could not run, without the codes, which a
/// transcript a person reads has no use for.
impl From<&Run> for Done {
    fn from(run: &Run) -> Self {
        Done {
            command: run.command.clone(),
            stdout: run.stdout.clone(),
            stderr: run.stderr.iter().map(|line| line.text.clone()).collect(),
            error: run.error.as_ref().map(|error| error.message.clone()),
            ok: run.ok,
        }
    }
}

/// A refusal of `code`, saying `message`.
fn refused(code: &'static str, message: impl Display) -> Refusal {
    Refusal {
        code,
        message: message.to_string(),
    }
}
