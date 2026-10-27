//! Changing which agents work in a project, or whether what is installed
//! into it is the team's, and carrying what was installed along.
//!
//! A project's agents decide where an install puts its copies (D-07), and
//! its share mode whether they are kept out of git (D-02), so a change to
//! either is a change to what is installed as much as to `.mind/settings.json`
//! (D-09). [`plan`] says what a [`Change`] would do, writing nothing, and
//! [`apply`] does it: the settings edited where they stand
//! ([`MindProject::change_settings`]), each installation given a copy in
//! every folder an agent the project now names is sent one, and Mindflayer's
//! untouched copies taken out of the folders no agent it names is sent copies
//! to any more. A change of share mode writes or removes `.mind/.gitignore`
//! (D-01) and the project's group of the exclude block too.
//!
//! The installations are brought to the target set the settings give, not
//! to the difference between the settings before and after: running the same
//! change again finds nothing to do once it is done, and finishes what one
//! stopped halfway, or what a hand edit of the settings left undone.
//!
//! What is moved is what the ledger records, and it only manages what it
//! installed. A copy that is not recorded is somebody's, wherever it is: one
//! where a new copy would go is left alone, so that agent reads it rather
//! than what was installed, and one in a folder the change drops is left too,
//! and said. A recorded copy that is no longer what was installed, by its
//! hash, is never deleted: it is left where it is, and stops being
//! Mindflayer's, as an edit has made a copy its author's until D-04's states
//! (#47) say more.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::copy;
use crate::exclude::{self, ExcludeError};
use crate::gather::Scrub;
use crate::hash;
use crate::install::{self, InstallError, InstallTarget};
use crate::kind::{Kind, Layout};
use crate::ledger::{Action, InstallRecord, Ledger, LedgerError, Outcome, Target};
use crate::settings::{Settings, Share};
use crate::workspace::{
    ignore_itself, same_directory, FlayerWorkspace, MindProject, WorkspaceError, MIND_IGNORE,
};

/// What a change asks of a project's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Name exactly these agents, in this order, each once.
    Agents(Vec<Agent>),
    /// Name these as well, after the ones it names already.
    AddAgents(Vec<Agent>),
    /// Stop naming these.
    RemoveAgents(Vec<Agent>),
    /// Keep what is installed to this checkout or leave it for the team to
    /// commit, and say whether a file git tracks may be edited to reach an
    /// agent: the two settings the share mode is, written together, so a
    /// command that sets one says what it leaves the other as.
    Share { share: Share, edit_tracked: bool },
}

impl Change {
    /// `before`, as the change leaves it.
    fn applied(&self, before: &Settings) -> Settings {
        // Each once, in the order given, after whatever is there.
        fn named(agents: &[Agent], into: &mut Vec<Agent>) {
            for agent in agents {
                if !into.contains(agent) {
                    into.push(*agent);
                }
            }
        }
        let mut after = before.clone();
        match self {
            Change::Agents(agents) => {
                after.agents.clear();
                named(agents, &mut after.agents);
            }
            Change::AddAgents(agents) => named(agents, &mut after.agents),
            Change::RemoveAgents(agents) => after.agents.retain(|agent| !agents.contains(agent)),
            Change::Share {
                share,
                edit_tracked,
            } => {
                after.share = *share;
                after.edit_tracked = *edit_tracked;
            }
        }
        after
    }

    /// Whether it changes the share mode, the one change that touches
    /// `.mind/.gitignore` and the project's whole group in the exclude
    /// block: an agent change carries the copies it moves out of git and
    /// back, and leaves the rest of the group as it finds it.
    fn shares(&self) -> bool {
        matches!(self, Change::Share { .. })
    }
}

/// What a change would do: [`plan`], or what [`apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The project's settings as they are.
    pub before: Settings,
    /// The same, as the change leaves them.
    pub after: Settings,
    /// Whether the installations a workspace records were read: false when
    /// no workspace given links the project, whose installs are then left
    /// where they are, since nothing says which copies are Mindflayer's.
    pub recorded: bool,
    /// What happens to `.mind/.gitignore`, for a change of share mode.
    pub ignore: Option<Ignore>,
    /// Every installation whose copies move, by kind and then by name.
    pub installations: Vec<Retarget>,
    /// What sits in a folder the change stops sending copies to that
    /// Mindflayer did not put there: left, and said, since no listing of the
    /// project reads that folder any more.
    pub foreign: Vec<PathBuf>,
    /// Copies already in place that a project turning local keeps out of
    /// git from now on. Those an installation is given are kept out as they
    /// land, and are not here.
    pub excluded: Vec<PathBuf>,
    /// The lines of the project's group in the exclude block that a project
    /// turning shared takes out: all of them, whichever workspace wrote them.
    pub unexcluded: Vec<String>,
    /// The exclude file the project's entries are in, when the change edits
    /// it and a git work tree holds the project.
    pub exclude_file: Option<PathBuf>,
    /// Whether the change would keep something out of git in a project no
    /// git work tree holds, where nothing does: for a front end to say.
    pub outside: bool,
}

impl Plan {
    /// Whether the change would change nothing at all: the settings as they
    /// are, and nothing to copy, remove, forget, leave, ignore or exclude.
    pub fn is_empty(&self) -> bool {
        self.before == self.after
            && matches!(self.ignore, None | Some(Ignore::Kept(_)))
            && self.installations.is_empty()
            && self.excluded.is_empty()
            && self.unexcluded.is_empty()
    }
}

/// What a change of share mode does to `.mind/.gitignore`, the file that
/// keeps a local project's `.mind` out of git (D-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignore {
    /// Written, holding [`MIND_IGNORE`]: a project turning local.
    Write(PathBuf),
    /// Removed, so `.mind` can be committed: a project turning shared.
    Remove(PathBuf),
    /// Left as it is, though the project turns shared: it is not the one
    /// Mindflayer writes, so somebody changed it, and it may still keep
    /// `.mind` out of git.
    Kept(PathBuf),
}

/// How one installation's copies move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retarget {
    pub kind: Kind,
    pub name: String,
    /// Copies put where an agent the project now names is sent one, each
    /// with the agents it is for, made from one the record names that still
    /// holds what was installed.
    pub added: Vec<Added>,
    /// Untouched copies taken out of folders no agent the project names is
    /// sent copies to any more.
    pub removed: Vec<PathBuf>,
    /// Copies the record names in such a folder that are gone already,
    /// whose record goes.
    pub forgotten: Vec<PathBuf>,
    /// Copies left where they are, and why.
    pub left: Vec<Left>,
    /// The copy the added ones are made from.
    source: Option<PathBuf>,
    /// Each copy the record names that stays where it is, with the agent
    /// the ledger says it is for now.
    kept: Vec<Target>,
    /// What the record names now, to tell whether it changes.
    recorded: Vec<Target>,
}

impl Retarget {
    /// Whether anything a front end would say happens to it.
    fn moves(&self) -> bool {
        !(self.added.is_empty()
            && self.removed.is_empty()
            && self.forgotten.is_empty()
            && self.left.is_empty())
    }
}

/// A copy an installation is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub path: PathBuf,
    /// The agents among the project's that are sent a copy there, in report
    /// order.
    pub agents: Vec<Agent>,
}

impl Added {
    /// What the ledger records of it.
    fn target(&self, root: &Path) -> Target {
        Target {
            agent: install::agent_of(&self.agents),
            path: install::route(root, &self.path),
        }
    }
}

/// A copy left where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Left {
    pub path: PathBuf,
    pub why: Why,
}

/// Why a copy is left where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// Somebody's, where a copy would go for an agent the project now names:
    /// never written over, so that agent reads it rather than what was
    /// installed.
    Foreign,
    /// No copy the record names still holds what was installed, so there is
    /// nothing to make one here from.
    Unsourced,
    /// Changed since it was installed, in a folder no agent the project
    /// names is sent copies to any more: never deleted, and no longer
    /// Mindflayer's.
    Edited,
    /// The same, for a copy that cannot be compared with what was installed:
    /// recorded without a hash, or not read whole, for the reason given.
    Uncompared(String),
}

impl Why {
    /// Whether the copy stops being Mindflayer's: its record goes, and in a
    /// local project its exclude entry, since what is there is its author's.
    fn disowns(&self) -> bool {
        matches!(self, Why::Edited | Why::Uncompared(_))
    }
}

/// What [`apply`] did.
#[derive(Debug)]
pub struct Applied {
    /// What was planned under the lock and done: an addition or a removal
    /// that failed is in `failures` instead.
    pub plan: Plan,
    /// What opening the ledger to write took out of it, for the front end to
    /// say.
    pub scrub: Scrub,
    /// What interrupted installs had left in the project's scratch folder,
    /// cleared before a copy was built there.
    pub swept: Vec<PathBuf>,
    /// What could not be done, collected rather than raised: the settings
    /// are changed, and each installation carried as far as it would go.
    pub failures: Vec<ReapplyFailure>,
}

/// Something a change could not carry out once its settings were written.
#[derive(Debug, Error)]
pub enum ReapplyFailure {
    #[error("{path}: cannot be written, so {name} is not copied where the project's agents now read it: {source}")]
    Copy {
        name: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: could not be removed, so it is still installed there: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the exclude file still keeps copies that are gone, or the team's, out of git: {0}")]
    StillExcluded(#[source] ExcludeError),
    #[error("{path}: could not be removed, so .mind is still kept out of git: {source}")]
    Ignore {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Why a change could not be made at all: refused before anything is
/// written, but for a ledger that fails while the copies' records are being
/// written, which stops everything, as it does wherever it fails.
#[derive(Debug, Error)]
pub enum ReapplyError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error(transparent)]
    Exclude(#[from] ExcludeError),
    /// A project naming no agent has nowhere to put what is installed, so
    /// carrying it along would take every copy out: an uninstall's to do,
    /// asked for by name, not a side of changing the agents.
    #[error("{project} would name no agent, and what is installed into it would reach none, so its last agent is not removed; `flayer uninstall` takes out what it has")]
    NoAgent { project: String },
}

/// What `change` would do to `project`, against what `workspace` records
/// of it when that one links it: nothing written, the ledger read without
/// being made.
///
/// Refused as [`apply`] would refuse it, so a dry run says what the real one
/// will: a project a `mind.toml` marks, which has no settings until it is
/// moved to them; a change that leaves it naming no agent; a copy that would
/// land inside one of `homes` or in a folder a loaded workspace keeps as its
/// own; and a record naming a copy outside the project.
pub fn plan(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    change: &Change,
    homes: &AgentHomes,
) -> Result<Plan, ReapplyError> {
    let linked = linking(workspace, project)?;
    let ledger = match linked {
        Some(workspace) => Some(workspace.ledger_for_reading()?),
        None => None,
    };
    plan_with(linked.zip(ledger.as_ref()), project, change, homes)
}

/// Make `change` to `project`, and carry what `workspace` records
/// installing into it along, when that one links it: [`plan`], done.
///
/// Holds that workspace's lock from before the settings are read again
/// until every copy, record and exclude entry is written, as an install
/// holds it, so an install or another change in that workspace comes wholly
/// before this or wholly after it, and never puts a copy where the settings
/// no longer send one. The ledger is opened to write before the lock, since
/// its scrub takes the lock when it has clones to rewrite, and not at all
/// when there is none, which a workspace that never installed anything has
/// no reason to be given.
///
/// In the order that keeps out of git whatever would show there for a
/// moment otherwise: a local project's exclude entries for the copies about
/// to land and its `.gitignore` before its settings, then the settings, the
/// copies and their records, and last what keeps out of git what has gone,
/// or what a project turning shared means to commit. A refusal before the
/// settings are written leaves everything as it was; what fails after is
/// collected in [`Applied::failures`], and the same change run again
/// finishes it.
pub fn apply(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    change: &Change,
    homes: &AgentHomes,
) -> Result<Applied, ReapplyError> {
    let linked = linking(workspace, project)?;
    let (ledger, scrub) = match linked {
        Some(workspace) if workspace.ledger_path().is_file() => {
            let (ledger, scrub) = workspace.ledger_to_write()?;
            (Some(ledger), scrub)
        }
        Some(workspace) => (Some(workspace.ledger_for_reading()?), Scrub::default()),
        None => (None, Scrub::default()),
    };
    let _lock = match linked {
        Some(workspace) => Some(workspace.lock().map_err(WorkspaceError::from)?),
        None => None,
    };
    // Read again under the lock: the project handed in may be minutes old,
    // and the settings are what the copies are brought to.
    let mut project = MindProject::open(project.root())?;
    let mut plan = plan_with(linked.zip(ledger.as_ref()), &project, change, homes)?;
    let mut applied = Applied {
        plan: plan.clone(),
        scrub,
        swept: Vec::new(),
        failures: Vec::new(),
    };
    if plan.is_empty() {
        return Ok(applied);
    }

    let root = project.root().to_path_buf();
    let key = plan.after.id.as_str().to_owned();
    let local = plan.after.share == Share::Local;
    let landing: Vec<PathBuf> = plan
        .installations
        .iter()
        .flat_map(|retarget| retarget.added.iter().map(|added| added.path.clone()))
        .chain(plan.excluded.iter().cloned())
        .collect();
    if local && !landing.is_empty() {
        exclude::add(&root, &key, &install::entries(&project, &landing))?;
    }
    // Taken back out of what a failure before the settings leaves empty.
    let undo = |project: &MindProject| {
        if local && !landing.is_empty() {
            let gone: Vec<PathBuf> = landing
                .iter()
                .filter(|path| fs::symlink_metadata(path).is_err())
                .cloned()
                .collect();
            let _ = exclude::remove(&root, &key, &install::entries(project, &gone));
        }
    };
    let written_ignore = match &plan.ignore {
        Some(Ignore::Write(_)) => match ignore_itself(&project.mind_dir()) {
            Ok(()) => true,
            Err(error) => {
                undo(&project);
                return Err(error.into());
            }
        },
        _ => false,
    };

    let after = plan.after.clone();
    if let Err(error) = project.change_settings(|settings| {
        settings.agents = after.agents.clone();
        settings.share = after.share;
        settings.edit_tracked = after.edit_tracked;
    }) {
        undo(&project);
        if written_ignore {
            let _ = fs::remove_file(project.mind_dir().join(".gitignore"));
        }
        return Err(error.into());
    }

    let mut gone: Vec<PathBuf> = Vec::new();
    for retarget in &mut plan.installations {
        let placed = place(&project, retarget, &mut applied);
        if local && !placed {
            let left: Vec<PathBuf> = retarget
                .added
                .iter()
                .map(|added| added.path.clone())
                .filter(|path| !install::holds(path, retarget.kind))
                .collect();
            let _ = exclude::remove(&root, &key, &install::entries(&project, &left));
        }
        if !placed {
            retarget.added.clear();
        }
        let mut removed = Vec::new();
        let mut kept = retarget.kept.clone();
        for path in std::mem::take(&mut retarget.removed) {
            match fs::remove_dir_all(&path) {
                Ok(()) => removed.push(path),
                Err(source) => {
                    kept.push(Target {
                        agent: recorded_agent(retarget, &root, &path),
                        path: install::route(&root, &path),
                    });
                    applied
                        .failures
                        .push(ReapplyFailure::Remove { path, source });
                }
            }
        }
        retarget.removed = removed;
        let mut targets = kept;
        targets.extend(retarget.added.iter().map(|added| added.target(&root)));
        targets.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some((workspace, ledger)) = linked.zip(ledger.as_ref()) {
            record(workspace, ledger, &project, retarget, &targets)?;
        }
        gone.extend(retarget.removed.iter().cloned());
        gone.extend(retarget.forgotten.iter().cloned());
        gone.extend(
            retarget
                .left
                .iter()
                .filter(|left| left.why.disowns())
                .map(|left| left.path.clone()),
        );
    }

    // Out of git after the copies go: every line of the group for a project
    // turning shared, and for a local one the entries of what has gone or
    // stopped being Mindflayer's.
    let unexcluded = if !plan.unexcluded.is_empty() {
        exclude::forget(&root, &key).err()
    } else if local && !gone.is_empty() {
        exclude::remove(&root, &key, &install::entries(&project, &gone)).err()
    } else {
        None
    };
    applied
        .failures
        .extend(unexcluded.map(ReapplyFailure::StillExcluded));
    if let Some(Ignore::Remove(path)) = &plan.ignore {
        // Looked at again, so one changed since the plan read it is left.
        if fs::read(path).is_ok_and(|bytes| bytes == MIND_IGNORE.as_bytes()) {
            if let Err(source) = fs::remove_file(path) {
                applied.failures.push(ReapplyFailure::Ignore {
                    path: path.clone(),
                    source,
                });
            }
        }
    }

    applied.plan = plan;
    Ok(applied)
}

/// `workspace`, when it is made and links `project`: the one whose records
/// say what in the project is Mindflayer's. The project is refused first
/// when a `mind.toml` marks it, before any ledger is opened.
fn linking<'a>(
    workspace: Option<&'a FlayerWorkspace>,
    project: &MindProject,
) -> Result<Option<&'a FlayerWorkspace>, ReapplyError> {
    if project.settings().is_none() {
        return Err(WorkspaceError::NoSettings {
            path: project.root().to_path_buf(),
        }
        .into());
    }
    Ok(workspace.filter(|workspace| workspace.is_made() && workspace.is_linked(project.root())))
}

/// [`plan`], against `records`: the workspace that links the project and
/// its ledger, or none.
fn plan_with(
    records: Option<(&FlayerWorkspace, &Ledger)>,
    project: &MindProject,
    change: &Change,
    homes: &AgentHomes,
) -> Result<Plan, ReapplyError> {
    let Some(before) = project.settings().cloned() else {
        return Err(WorkspaceError::NoSettings {
            path: project.root().to_path_buf(),
        }
        .into());
    };
    let after = change.applied(&before);
    if after.agents.is_empty() {
        return Err(ReapplyError::NoAgent {
            project: project.name().to_owned(),
        });
    }
    let changed = project.with_settings(after.clone());
    let root = project.root();

    // Only kinds whose artifacts are a folder each, as only those are
    // installed: a kind of files has no copy to move.
    let kinds: Vec<Kind> = Kind::ALL
        .into_iter()
        .filter(|kind| matches!(kind.layout(), Layout::Directory { .. }))
        .collect();
    let mut installations = Vec::new();
    let mut foreign = Vec::new();
    // Every recorded copy that stays where it is and is there, moved or not:
    // what a project turning local keeps out of git.
    let mut in_place = Vec::new();
    for &kind in &kinds {
        let Some((workspace, ledger)) = records else {
            break;
        };
        let wanted = install::targets(&changed, kind).unwrap_or_default();
        let entry = install::entry_for_root(workspace, root);
        for record in ledger.records(&entry, kind)? {
            let retarget = retarget(project, kind, &record, &wanted)?;
            in_place.extend(
                retarget
                    .kept
                    .iter()
                    .map(|target| root.join(&target.path))
                    .filter(|path| fs::symlink_metadata(path).is_ok()),
            );
            if retarget.moves() || retarget.kept != retarget.recorded {
                installations.push(retarget);
            }
        }

        // What somebody keeps in a folder the change drops, said since no
        // listing reads it once no agent the project names is sent copies
        // there. Only where the records tell an install from somebody's.
        // Not the folder the project authors in, which it is read from
        // whatever its agents, and whose skills are its own.
        let recorded = ledger.copies(&entry, kind)?;
        let had = install::targets(project, kind).unwrap_or_default();
        let authored = project.directory_for(kind);
        for target in had.iter().filter(|target| {
            !wanted
                .iter()
                .any(|kept| same_directory(&kept.folder, &target.folder))
                && !authored
                    .as_deref()
                    .is_some_and(|authored| same_directory(authored, &target.folder))
        }) {
            let Ok(read) = fs::read_dir(&target.folder) else {
                continue;
            };
            let mut found: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| !recorded.contains(&install::route(root, path)))
                .collect();
            found.sort();
            foreign.extend(found);
        }
    }

    // A copy is put nowhere an install would refuse to put one: inside the
    // folders the agents read for every project, or a folder a loaded
    // workspace keeps as its own; nor is one removed from there.
    let landing: Vec<PathBuf> = installations
        .iter()
        .flat_map(|retarget: &Retarget| retarget.added.iter().map(|added| added.path.clone()))
        .collect();
    if let Some(error) = install::user_scope(homes, &landing) {
        return Err(error.into());
    }
    if let Some((workspace, _)) = records {
        for retarget in &installations {
            let touched = retarget
                .added
                .iter()
                .map(|added| &added.path)
                .chain(&retarget.removed);
            for path in touched {
                if let Some(loaded) = install::kept_by_a_load(workspace, retarget.kind, path) {
                    return Err(InstallError::KeptByALoad {
                        path: path.clone(),
                        loaded,
                    }
                    .into());
                }
            }
        }
    }

    let ignore = if change.shares() {
        ignore(project, after.share)
    } else {
        None
    };
    let mut plan = Plan {
        before,
        after,
        recorded: records.is_some(),
        ignore,
        installations,
        foreign,
        excluded: Vec::new(),
        unexcluded: Vec::new(),
        exclude_file: None,
        outside: false,
    };
    exclusions(project, change, &in_place, &mut plan)?;
    Ok(plan)
}

/// How the installation `record` of `kind` moves in `project`, given a copy
/// in each of `wanted`.
fn retarget(
    project: &MindProject,
    kind: Kind,
    record: &InstallRecord,
    wanted: &[InstallTarget],
) -> Result<Retarget, ReapplyError> {
    let root = project.root();
    let name = &record.name;
    if !install::is_folder_name(name) {
        return Err(InstallError::UnsafeName { name: name.clone() }.into());
    }
    let mut retarget = Retarget {
        kind,
        name: name.clone(),
        added: Vec::new(),
        removed: Vec::new(),
        forgotten: Vec::new(),
        left: Vec::new(),
        source: None,
        kept: Vec::new(),
        recorded: record.targets.clone(),
    };
    let wanted: Vec<(PathBuf, &InstallTarget)> = wanted
        .iter()
        .map(|target| (install::copy_at(&target.folder, kind, name), target))
        .collect();

    // Each recorded copy compared once with what was installed, by the hash
    // every copy of it was written with: none for one that is not there.
    let mut compared: Vec<(PathBuf, &Target, Option<Compared>)> = Vec::new();
    for target in &record.targets {
        let path = install::recorded_copy(project, name, &target.path)?;
        let there = fs::symlink_metadata(&path).is_ok();
        let found = there.then(|| compare(&path, record.content_hash.as_deref()));
        compared.push((path, target, found));
    }
    retarget.source = compared
        .iter()
        .find(|(_, _, found)| matches!(found, Some(Compared::Untouched)))
        .map(|(path, _, _)| path.clone());

    for (path, target, found) in compared {
        if let Some((_, now)) = wanted
            .iter()
            .find(|(copy, _)| install::route(root, copy) == target.path)
        {
            retarget.kept.push(Target {
                agent: install::agent_of(&now.agents),
                path: target.path.clone(),
            });
            continue;
        }
        match found {
            None => retarget.forgotten.push(path),
            Some(Compared::Untouched) => retarget.removed.push(path),
            Some(Compared::Edited) => retarget.left.push(Left {
                path,
                why: Why::Edited,
            }),
            Some(Compared::Unknown(reason)) => retarget.left.push(Left {
                path,
                why: Why::Uncompared(reason),
            }),
        }
    }
    for (path, target) in &wanted {
        let route = install::route(root, path);
        if record.targets.iter().any(|recorded| recorded.path == route) {
            continue;
        }
        if install::holds(path, kind) {
            retarget.left.push(Left {
                path: path.clone(),
                why: Why::Foreign,
            });
        } else if retarget.source.is_some() {
            retarget.added.push(Added {
                path: path.clone(),
                agents: target.agents.clone(),
            });
        } else {
            retarget.left.push(Left {
                path: path.clone(),
                why: Why::Unsourced,
            });
        }
    }
    Ok(retarget)
}

/// How a copy stands against what was installed.
enum Compared {
    /// It holds what was installed.
    Untouched,
    /// It holds something else.
    Edited,
    /// Whether it does cannot be told, for the reason given.
    Unknown(String),
}

/// How the copy at `path` stands against what was installed, whose hash is
/// `recorded`.
///
/// By the hash and nothing else, as D-04 compares a copy: a record without
/// one, written before hashes were kept or for copies that could not be read
/// back, cannot say, and a copy that is not there whole (a link, which the
/// hash refuses, or a file that cannot be read) cannot be compared. Either is
/// left where it is rather than deleted on a guess.
fn compare(path: &Path, recorded: Option<&str>) -> Compared {
    let Some(recorded) = recorded else {
        return Compared::Unknown(String::from("it was recorded without a hash"));
    };
    match hash::tree(path) {
        Ok(hash) if hash == recorded => Compared::Untouched,
        Ok(_) => Compared::Edited,
        Err(error) => Compared::Unknown(error.to_string()),
    }
}

/// The agent the ledger says the copy at `path` is for, as the record of
/// `retarget` names it.
fn recorded_agent(retarget: &Retarget, root: &Path, path: &Path) -> Option<String> {
    let route = install::route(root, path);
    retarget
        .recorded
        .iter()
        .find(|target| target.path == route)
        .and_then(|target| target.agent.clone())
}

/// What a change of share mode does to `.mind/.gitignore`: written for a
/// local project that has none, removed from a shared one that has the one
/// Mindflayer writes, and left in a shared one whose `.gitignore` says
/// something else, which somebody wrote.
fn ignore(project: &MindProject, share: Share) -> Option<Ignore> {
    let path = project.mind_dir().join(".gitignore");
    match share {
        Share::Local if fs::symlink_metadata(&path).is_err() => Some(Ignore::Write(path)),
        Share::Local => None,
        Share::Shared => match fs::read(&path) {
            Ok(bytes) if bytes == MIND_IGNORE.as_bytes() => Some(Ignore::Remove(path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            _ => Some(Ignore::Kept(path)),
        },
    }
}

/// What the change does to the project's group of the exclude block, beyond
/// the copies it adds and removes: the whole group for a project turning
/// shared, and for one turning local an entry for each recorded copy in place
/// (`in_place`) that the group does not hold yet. Read from the file, so a
/// dry run says what will be written rather than what might be.
fn exclusions(
    project: &MindProject,
    change: &Change,
    in_place: &[PathBuf],
    plan: &mut Plan,
) -> Result<(), ReapplyError> {
    let landing = plan
        .installations
        .iter()
        .any(|retarget| !retarget.added.is_empty());
    let local = plan.after.share == Share::Local;
    // A shared project's copies are the team's, so an agent change leaves
    // its exclude file alone; so does one that lands nothing in a local one.
    if !(change.shares() || local && landing) {
        return Ok(());
    }
    let Some(listing) = exclude::listed(project.root(), plan.after.id.as_str())? else {
        // Nothing keeps a local project's copies out of git outside a work
        // tree, which is worth saying when there are copies to keep out.
        plan.outside = local && (landing || change.shares() && !in_place.is_empty());
        return Ok(());
    };
    plan.exclude_file = Some(listing.file.clone());
    if !local {
        plan.unexcluded = listing.lines;
    } else if change.shares() {
        for path in in_place {
            let Some(relative) = crate::paths::relative_to(path, project.root()) else {
                continue;
            };
            let line = exclude::Entry::directory(relative).pattern(&listing.route)?;
            if !listing.lines.contains(&line) {
                plan.excluded.push(path.clone());
            }
        }
    }
    Ok(())
}

/// Put the copies `retarget` adds in place, from its source, all of them or
/// none (`copy::Stage::place`), saying whether they landed.
fn place(project: &MindProject, retarget: &Retarget, applied: &mut Applied) -> bool {
    let (Some(source), false) = (&retarget.source, retarget.added.is_empty()) else {
        return true;
    };
    let copies: Vec<PathBuf> = retarget
        .added
        .iter()
        .map(|added| added.path.clone())
        .collect();
    let scratch = project.scratch_dir();
    let mut stage = match copy::Stage::open(&scratch) {
        Ok(stage) => stage,
        Err(source) => {
            applied.failures.push(ReapplyFailure::Copy {
                name: retarget.name.clone(),
                path: scratch,
                source,
            });
            return false;
        }
    };
    // From a copy that was installed, so nothing an install leaves out is
    // in it, and no link, which its hash would have refused: followed only
    // inside it all the same.
    let placed = stage.place(source, &copies, source, &install::left_out_names());
    applied.swept.extend(stage.swept().iter().cloned());
    match placed {
        Ok(_) => true,
        Err(copy::PlaceError { target, source }) => {
            applied.failures.push(ReapplyFailure::Copy {
                name: retarget.name.clone(),
                path: target.unwrap_or_else(|| copies[0].clone()),
                source,
            });
            false
        }
    }
}

/// Write down where `retarget`'s copies are now (`targets`), and log what
/// moved.
fn record(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    retarget: &Retarget,
    targets: &[Target],
) -> Result<(), ReapplyError> {
    let root = project.root();
    let entry = install::entry_for_root(workspace, root);
    if targets == retarget.recorded.as_slice() {
        return Ok(());
    }
    ledger.retargeted(&entry, retarget.kind, &retarget.name, targets)?;
    let name = &retarget.name;
    let into = project.name();
    for added in &retarget.added {
        let detail = format!(
            "{name}'s copy at {} into {into}, for an agent it names now",
            install::route(root, &added.path)
        );
        ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;
    }
    let gone = retarget.removed.iter().chain(&retarget.forgotten).chain(
        retarget
            .left
            .iter()
            .filter(|left| left.why.disowns())
            .map(|left| &left.path),
    );
    for path in gone {
        let detail = format!(
            "{name}'s copy at {} from {into}, which no agent it names is sent copies to",
            install::route(root, path)
        );
        ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "reapply_test.rs"]
mod tests;
