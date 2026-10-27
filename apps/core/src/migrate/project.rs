//! Bringing a project an older Mindflayer made, marked by a `mind.toml`, to
//! the settings a new one has, and what is installed into it to where its
//! agents read.
//!
//! A `mind.toml` names the folders a project authors in and nothing else:
//! no agents, so everything installed into it went into the one folder it
//! names for each kind, and no share mode, because a project from before
//! settings was committed by design. It is read the way it was written, so
//! nothing moves it but being asked to. [`project_plan`] says what asking
//! would do, and [`project`] does it: `.mind/settings.json` written from the
//! `mind.toml`, its name and folders kept, with the agents and share mode
//! asked for; every installation a workspace records given a copy where each
//! agent the project names is sent one (D-07), and its untouched copy taken
//! out of the folder the project authors in; and the `mind.toml` kept as it
//! was in `.mind/tmp`, whose own `.gitignore` keeps it out of git even in a
//! shared project.
//!
//! It is a change of agents, from none to the ones asked for, and decides
//! what moves the way [`crate::reapply`] does, through the same
//! [`reapply::retarget`]: only what the ledger records is moved, only a copy
//! that still holds what was installed is taken out, and what somebody else
//! put anywhere is left where it is and said. A record from before hashes
//! were kept, which is what most of a v2 project's are, is held to its offer
//! (D-04).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{MigrateError, Refusal};
use crate::agent::{Agent, AgentHomes};
use crate::catalog::Catalog;
use crate::copy;
use crate::exclude::{self, ExcludeError};
use crate::gather::Scrub;
use crate::install::{self, Candidate, InstallError, Offer};
use crate::kind::{Kind, Layout};
use crate::ledger::{Action, InstallRecord, Ledger, LedgerError, Moved, Target};
use crate::reapply::{self, Retarget, Why};
use crate::settings::{Settings, Share, SETTINGS_FILE};
use crate::workspace::{
    ignore_itself, misplaced_authored, same_directory, write_new, Directories, FlayerWorkspace,
    MindConfig, MindProject, ProjectConfig, WorkspaceError, DIRECTORIES_VERSION, MIND_CONFIG,
    MIND_DIR, MIND_IGNORE,
};

/// What moving a project from its `mind.toml` to its settings would do:
/// [`project_plan`], or what [`project`] did.
#[derive(Debug)]
pub struct ProjectPlan {
    /// The settings written: the `mind.toml`'s name and folders, the agents
    /// and share mode asked for, and an id of the project's own.
    pub settings: Settings,
    /// Where they go: `.mind/settings.json`.
    pub settings_file: PathBuf,
    /// The `mind.toml` that marks the project now.
    pub marker: PathBuf,
    /// Where it is kept as it was, `.mind/tmp/mind.toml.v<version>`.
    pub backup: PathBuf,
    /// Whether the installations a workspace records were read: false when
    /// no workspace given links the project, whose copies are then left
    /// where they are, since nothing says which of them are Mindflayer's.
    pub recorded: bool,
    /// The `.gitignore` that keeps a local project's `.mind` out of git
    /// (D-01), written when the project is to be local and has none.
    pub ignore: Option<PathBuf>,
    /// Every installation whose copies move, by kind and then by name.
    pub installations: Vec<Retarget>,
    /// What the project authors itself, in the folders its settings keep
    /// naming: every artifact there that no installation records, left
    /// where it is, as an edit makes a copy its author's.
    pub authored: Vec<PathBuf>,
    /// What sits in a folder the project's agents are sent copies to that
    /// Mindflayer did not put there, but for one an installation's copy
    /// would have gone to, which its installation says: left, and said,
    /// since the project is listed from those folders from now on.
    pub foreign: Vec<PathBuf>,
    /// Copies already in place that a local project keeps out of git from
    /// now on. Those an installation is given are kept out as they land,
    /// and are not here.
    pub excluded: Vec<PathBuf>,
    /// The exclude file the project's entries go in, when it is to be local
    /// and a git work tree holds it.
    pub exclude_file: Option<PathBuf>,
    /// Whether it would keep copies out of git in a project no git work tree
    /// holds, where nothing does: for a front end to say.
    pub outside: bool,
    /// Whether git tracks something in `.mind` of a project that is to be
    /// local, whose `.gitignore` then keeps none of it out
    /// ([`exclude::tracks`]): for a front end to say how to stop that, which
    /// is not Mindflayer's to do.
    pub tracked: bool,
    /// Why it cannot go ahead. Nothing is written while there is any.
    pub refusals: Vec<Refusal>,
}

/// What moving `project` from its `mind.toml` to settings naming `agents`
/// and `share` would do, against what `workspace` records of it when that
/// one links it: nothing written, the ledger read without being made.
/// `None` for a project its settings mark already, which has nothing to
/// move.
///
/// Refused as [`project`] would refuse it, every reason at once, so a dry
/// run says what the real one will: a folder the `mind.toml` keeps a kind in
/// that settings cannot name, inside `.mind` as a version 1 project kept
/// every kind it named no folder for, or outside the project; no agent; a
/// copy of the marker an earlier migration kept; and a copy that would land
/// inside one of `homes`, or where a loaded workspace keeps its own, as an
/// install refuses one.
pub fn project_plan(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    agents: &[Agent],
    share: Share,
    homes: &AgentHomes,
) -> Result<Option<ProjectPlan>, MigrateError> {
    let linked = linking(workspace, project);
    let ledger = match linked {
        Some(workspace) => Some(workspace.ledger_for_reading()?),
        None => None,
    };
    plan_with(linked.zip(ledger.as_ref()), project, agents, share, homes)
}

/// What [`project`] did.
#[derive(Debug)]
pub struct ProjectMigrated {
    /// What was planned under the lock, and so done.
    pub plan: ProjectPlan,
    /// What opening the ledger to write took out of it, for the front end to
    /// say.
    pub scrub: Scrub,
    /// What interrupted installs had left in the project's scratch folder,
    /// cleared before anything was built there.
    pub swept: Vec<PathBuf>,
}

/// A step of moving a project to its settings that failed once something
/// was written, which is then put back ([`MigrateError::Project`]).
#[derive(Debug, Error)]
pub enum ProjectFailure {
    #[error("{0}, since what was to be installed into it could not be kept out of git")]
    Exclude(ExcludeError),
    #[error(transparent)]
    Workspace(WorkspaceError),
    #[error(transparent)]
    Ledger(LedgerError),
    #[error("{path}: cannot be made, to build copies and keep the marker in: {source}")]
    Scratch {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written, so {name} is not copied where the project's agents read it: {source}")]
    Copy {
        name: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be moved out of the way: {source}")]
    Retire {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be kept as {backup}: {source}")]
    Keep {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// What a project's move that failed could not put back as it was, for a
/// front end to say beside the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stranded {
    /// A copy put where an agent reads, which nothing records.
    Copy(PathBuf),
    /// An installed copy that was moved out of the folder the project
    /// authors in and could not be moved back to `path`: it goes with the
    /// scratch folder, and installing it again puts it back.
    Retired { name: String, path: PathBuf },
    /// The settings, written beside the `mind.toml` they are read before.
    Settings(PathBuf),
    /// The `mind.toml`, kept at `backup` and not back at `marker`, which
    /// leaves the project marked by nothing but settings, if by anything.
    Marker { marker: PathBuf, backup: PathBuf },
    /// The `.gitignore` written for a project that was to be local, which
    /// keeps the committed `.mind` of a project from before settings out of
    /// git.
    Ignore(PathBuf),
}

/// Move `project` from its `mind.toml` to settings naming `agents` and
/// `share`, and carry what `workspace` records installing into it along,
/// when that one links it: [`project_plan`], done. `None` for a project its
/// settings mark already, which is left as it is, and nothing written.
///
/// Holds that workspace's lock from before the project is read again until
/// every copy, record and marker is written, as a change of agents does, so
/// an install in that workspace comes wholly before it or wholly after it.
/// The ledger is opened to write before the lock, since its scrub takes the
/// lock when it has clones to rewrite, and not at all when there is none.
///
/// All or nothing, for the project. A refusal writes nothing. Otherwise, in
/// an order that shows nothing in `git status` for a moment: a local
/// project's exclude entries for the copies about to land and its
/// `.gitignore`; the new copies, each installation's built in one stage and
/// put in place together (`copy::Stage::place`); the untouched copies taken
/// out, moved into that stage rather than deleted; the settings, with
/// `create_new`; the `mind.toml` moved into `.mind/tmp`; and last the ledger,
/// in one transaction ([`Ledger::migrated`]). A step that fails puts back
/// every one before it, the last first, so the `mind.toml` marks the project
/// again and the ledger names what it named; what cannot be put back is
/// said ([`MigrateError::left_behind`]). Only once the ledger holds the move
/// are the copies taken out deleted, with the stage.
pub fn project(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    agents: &[Agent],
    share: Share,
    homes: &AgentHomes,
) -> Result<Option<ProjectMigrated>, MigrateError> {
    if project.settings().is_some() {
        return Ok(None);
    }
    let linked = linking(workspace, project);
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
    // and another run may have moved it since.
    let project = MindProject::open(project.root())?;
    let records = linked.zip(ledger.as_ref());
    let Some(mut plan) = plan_with(records, &project, agents, share, homes)? else {
        return Ok(None);
    };
    if !plan.refusals.is_empty() {
        return Err(MigrateError::Refused(std::mem::take(&mut plan.refusals)));
    }

    let scratch = project.scratch_dir();
    let mut stage = copy::Stage::open(&scratch).map_err(|source| MigrateError::Project {
        failure: Box::new(ProjectFailure::Scratch {
            path: scratch.clone(),
            source,
        }),
        stranded: Vec::new(),
    })?;
    let mut undo = Undo::new(&project, plan.settings.id.as_str());
    if let Err(failure) = carry_out(&project, records, &plan, &mut stage, &mut undo) {
        // Before the stage goes, since what was taken out is in it.
        let stranded = undo.run();
        return Err(MigrateError::Project {
            failure: Box::new(failure),
            stranded,
        });
    }
    // Once every copy is placed, since placing one clears what an earlier
    // version left beside it too.
    let swept = stage.swept().to_vec();
    // What was taken out goes with it, now that the ledger holds the move.
    drop(stage);
    Ok(Some(ProjectMigrated { plan, scrub, swept }))
}

/// Every step of [`project`] after the plan, each one written down in
/// `undo` as it is made, so a failure can put it back.
fn carry_out(
    project: &MindProject,
    records: Option<(&FlayerWorkspace, &Ledger)>,
    plan: &ProjectPlan,
    stage: &mut copy::Stage,
    undo: &mut Undo,
) -> Result<(), ProjectFailure> {
    let root = project.root();
    let key = plan.settings.id.as_str();
    if plan.settings.share == Share::Local {
        let landing: Vec<PathBuf> = plan
            .installations
            .iter()
            .flat_map(|retarget| retarget.added.iter().map(|added| added.path.clone()))
            .chain(plan.excluded.iter().cloned())
            .collect();
        if !landing.is_empty() {
            let entries = install::entries(project, &landing);
            // Only the lines this adds are taken back out on a failure: the
            // group is new with the id, so any it holds are somebody's.
            let listed = exclude::listed(root, key)
                .map_err(ProjectFailure::Exclude)?
                .map(|listing| (listing.route, listing.lines));
            undo.excluded = entries
                .iter()
                .filter(|entry| {
                    listed.as_ref().is_some_and(|(route, lines)| {
                        entry
                            .pattern(route)
                            .is_ok_and(|line| !lines.contains(&line))
                    })
                })
                .cloned()
                .collect();
            exclude::add(root, key, &entries).map_err(ProjectFailure::Exclude)?;
        }
    }
    if let Some(path) = &plan.ignore {
        // Looked at again, so one made since is somebody's, and left.
        if fs::symlink_metadata(path).is_err() {
            ignore_itself(&project.mind_dir()).map_err(ProjectFailure::Workspace)?;
            undo.ignore = Some(path.clone());
        }
    }

    for retarget in &plan.installations {
        let (Some(source), false) = (&retarget.source, retarget.added.is_empty()) else {
            continue;
        };
        let copies: Vec<PathBuf> = retarget
            .added
            .iter()
            .map(|added| added.path.clone())
            .collect();
        // From a copy that was installed, so nothing an install leaves out
        // is in it: links followed only inside it all the same.
        stage
            .place(source, &copies, source, &install::left_out_names())
            .map_err(|copy::PlaceError { target, source }| ProjectFailure::Copy {
                name: retarget.name.clone(),
                path: target.unwrap_or_else(|| copies[0].clone()),
                source,
            })?;
        undo.placed.extend(copies);
    }
    // Moved aside rather than deleted, so a failure later puts them back:
    // into the stage, on the project's filesystem, which deletes them once
    // the ledger holds the move, and whose leftovers the next install sweeps.
    for (index, (retarget, path)) in plan
        .installations
        .iter()
        .flat_map(|retarget| retarget.removed.iter().map(move |path| (retarget, path)))
        .enumerate()
    {
        let aside = stage.path().join(format!("retired-{index}"));
        fs::rename(path, &aside).map_err(|source| ProjectFailure::Retire {
            path: path.clone(),
            source,
        })?;
        undo.retired
            .push((retarget.name.clone(), path.clone(), aside));
    }

    let text = plan.settings.text().map_err(ProjectFailure::Workspace)?;
    write_new(&plan.settings_file, &text).map_err(ProjectFailure::Workspace)?;
    undo.settings = Some(plan.settings_file.clone());
    // Looked for again just before: a rename onto a file replaces it, and a
    // copy made there since the plan is somebody's.
    let kept = if fs::symlink_metadata(&plan.backup).is_ok() {
        Err(io::Error::from(io::ErrorKind::AlreadyExists))
    } else {
        fs::rename(&plan.marker, &plan.backup)
    };
    kept.map_err(|source| ProjectFailure::Keep {
        path: plan.marker.clone(),
        backup: plan.backup.clone(),
        source,
    })?;
    undo.marker = Some((plan.marker.clone(), plan.backup.clone()));

    if let Some((workspace, ledger)) = records {
        let entry = install::entry_for_root(workspace, root);
        let (moved, log) = recorded(project, plan);
        ledger
            .migrated(&entry, &moved, &log)
            .map_err(ProjectFailure::Ledger)?;
    }
    Ok(())
}

/// What the ledger is to record of each installation that moved, and the
/// log of it.
///
/// Each keeps the copies that stayed and gains those put where its agents
/// read. A record with no hash whose copies were found current by their
/// offer gets the one hash they hold now, so the next change to the
/// project's agents compares them as it compares any other; one already
/// holding a hash keeps it, being what was installed.
fn recorded(project: &MindProject, plan: &ProjectPlan) -> (Vec<Moved>, Vec<(Action, String)>) {
    let root = project.root();
    let into = project.name();
    let mut moved = Vec::new();
    let mut log = Vec::new();
    for retarget in &plan.installations {
        let mut targets: Vec<Target> = retarget.kept.clone();
        targets.extend(retarget.added.iter().map(|added| added.target(root)));
        targets.sort_by(|a, b| a.path.cmp(&b.path));
        let content_hash = retarget.source.as_ref().and_then(|_| {
            let copies: Vec<PathBuf> = targets
                .iter()
                .map(|target| root.join(&target.path))
                .collect();
            install::one_hash(&copies).ok()
        });
        let name = &retarget.name;
        for added in &retarget.added {
            log.push((
                Action::Install,
                format!(
                    "{name}'s copy at {} into {into}, moved from its {MIND_CONFIG} to settings",
                    install::route(root, &added.path)
                ),
            ));
        }
        let gone = retarget.removed.iter().chain(&retarget.forgotten).chain(
            retarget
                .left
                .iter()
                .filter(|left| left.why.disowns())
                .map(|left| &left.path),
        );
        for path in gone {
            log.push((
                Action::Uninstall,
                format!(
                    "{name}'s copy at {} from {into}, moved from its {MIND_CONFIG} to settings",
                    install::route(root, path)
                ),
            ));
        }
        moved.push(Moved {
            kind: retarget.kind,
            name: name.clone(),
            targets,
            content_hash,
        });
    }
    (moved, log)
}

/// Everything [`carry_out`] has written so far, to put back when a later
/// step fails.
struct Undo<'a> {
    project: &'a MindProject,
    key: String,
    excluded: Vec<exclude::Entry>,
    ignore: Option<PathBuf>,
    placed: Vec<PathBuf>,
    /// Each copy taken out: its installation's name, where it was, and
    /// where in the stage it is.
    retired: Vec<(String, PathBuf, PathBuf)>,
    settings: Option<PathBuf>,
    marker: Option<(PathBuf, PathBuf)>,
}

impl<'a> Undo<'a> {
    fn new(project: &'a MindProject, key: &str) -> Self {
        Undo {
            project,
            key: key.to_owned(),
            excluded: Vec::new(),
            ignore: None,
            placed: Vec::new(),
            retired: Vec::new(),
            settings: None,
            marker: None,
        }
    }

    /// Put back everything written, the last first, and say what could not
    /// be.
    ///
    /// Exclude lines that will not come out are passed over, as an install
    /// passes over them when its copies fail: what is left is a line naming
    /// an empty place.
    fn run(self) -> Vec<Stranded> {
        let mut stranded = Vec::new();
        if let Some((marker, backup)) = self.marker {
            if fs::rename(&backup, &marker).is_err() {
                stranded.push(Stranded::Marker { marker, backup });
            }
        }
        if let Some(settings) = self.settings {
            if fs::remove_file(&settings).is_err() {
                stranded.push(Stranded::Settings(settings));
            }
        }
        for (name, path, aside) in self.retired.into_iter().rev() {
            if fs::rename(&aside, &path).is_err() {
                stranded.push(Stranded::Retired { name, path });
            }
        }
        for path in self.placed.into_iter().rev() {
            if fs::remove_dir_all(&path).is_err() {
                stranded.push(Stranded::Copy(path));
            }
        }
        if let Some(ignore) = self.ignore {
            // Only while it holds what was written.
            let ours = fs::read(&ignore).is_ok_and(|bytes| bytes == MIND_IGNORE.as_bytes());
            if ours && fs::remove_file(&ignore).is_err() {
                stranded.push(Stranded::Ignore(ignore));
            }
        }
        if !self.excluded.is_empty() {
            let _ = exclude::remove(self.project.root(), &self.key, &self.excluded);
        }
        stranded
    }
}

/// `workspace`, when it is made and links `project`: the one whose records
/// say what in the project is Mindflayer's.
pub(super) fn linking<'a>(
    workspace: Option<&'a FlayerWorkspace>,
    project: &MindProject,
) -> Option<&'a FlayerWorkspace> {
    workspace.filter(|workspace| workspace.is_made() && workspace.is_linked(project.root()))
}

/// [`project_plan`], against `records`: the workspace that links the
/// project and its ledger, or none.
pub(super) fn plan_with(
    records: Option<(&FlayerWorkspace, &Ledger)>,
    project: &MindProject,
    agents: &[Agent],
    share: Share,
    homes: &AgentHomes,
) -> Result<Option<ProjectPlan>, MigrateError> {
    let ProjectConfig::Toml(config) = project.config() else {
        return Ok(None);
    };
    let root = project.root();
    let mut refusals = Vec::new();

    let directories = authored(config);
    refusals.extend(
        Kind::ALL
            .into_iter()
            .filter_map(|kind| misplaced_authored(kind, directories.get(kind)?))
            .map(Refusal::Authored),
    );
    let settings = Settings::new(project.name(), agents.to_vec(), share, directories);
    if let Err(error) = settings.text() {
        refusals.push(Refusal::Authored(error));
    }
    if agents.is_empty() {
        refusals.push(Refusal::NoAgent {
            project: project.name().to_owned(),
        });
    }
    let backup = project
        .scratch_dir()
        .join(format!("{MIND_CONFIG}.v{}", config.version));
    if fs::symlink_metadata(&backup).is_ok() {
        refusals.push(Refusal::Backup {
            path: backup.clone(),
        });
    }

    let after = project.with_settings(settings.clone());
    let mut installations = Vec::new();
    let mut foreign = Vec::new();
    let mut authored_found = Vec::new();
    // Every recorded copy that stays where it is and is there: what a
    // project that is to be local keeps out of git.
    let mut in_place = Vec::new();
    for kind in Kind::ALL {
        let recorded = match records {
            Some((workspace, ledger)) => {
                ledger.copies(&install::entry_for_root(workspace, root), kind)?
            }
            None => Vec::new(),
        };
        // What the project authors, wherever its settings keep naming it:
        // the folder its `mind.toml` named, where its installs went too.
        if let Some(folder) = after.directory_for(kind) {
            let (found, _) = Catalog::locate(&folder, kind);
            authored_found.extend(
                found
                    .into_iter()
                    .filter(|path| !recorded.contains(&install::route(root, path))),
            );
        }
        // Only kinds whose artifacts are a folder each are installed.
        let Some((workspace, ledger)) = records else {
            continue;
        };
        if !matches!(kind.layout(), Layout::Directory { .. }) {
            continue;
        }
        let entry = install::entry_for_root(workspace, root);
        let wanted = install::targets(&after, kind).unwrap_or_default();
        let installed = ledger.records(&entry, kind)?;
        // Looked for only when a record has no hash to hold its copies to.
        let offers = if installed.iter().any(|record| record.content_hash.is_none()) {
            install::survey(workspace, ledger, project, kind)?
        } else {
            Vec::new()
        };
        let mut said = Vec::new();
        for record in &installed {
            let held = offered(workspace, &offers, record);
            let retarget = reapply::retarget(project, kind, record, &wanted, Some(&held))?;
            in_place.extend(
                retarget
                    .kept
                    .iter()
                    .map(|target| root.join(&target.path))
                    .filter(|path| fs::symlink_metadata(path).is_ok()),
            );
            said.extend(
                retarget
                    .left
                    .iter()
                    .filter(|left| left.why == Why::Foreign)
                    .map(|left| left.path.clone()),
            );
            if retarget.moves() || retarget.kept != retarget.recorded {
                installations.push(retarget);
            }
        }

        // What somebody keeps in a folder the project's agents are now sent
        // copies to, said since the project is read from there from now on.
        // Not the folder it authors in, whose skills are its own.
        let authoring = after.directory_for(kind);
        for target in wanted.iter().filter(|target| {
            !authoring
                .as_deref()
                .is_some_and(|authoring| same_directory(authoring, &target.folder))
        }) {
            let Ok(read) = fs::read_dir(&target.folder) else {
                continue;
            };
            let mut found: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    !recorded.contains(&install::route(root, path)) && !said.contains(path)
                })
                .collect();
            found.sort();
            foreign.extend(found);
        }
    }

    // A copy is put nowhere an install would refuse to put one, nor taken
    // from where a loaded workspace keeps its own.
    let landing: Vec<PathBuf> = installations
        .iter()
        .flat_map(|retarget: &Retarget| retarget.added.iter().map(|added| added.path.clone()))
        .collect();
    if let Some(error) = install::user_scope(homes, &landing) {
        refusals.push(Refusal::Install(error));
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
                    refusals.push(Refusal::Install(InstallError::KeptByALoad {
                        path: path.clone(),
                        loaded,
                    }));
                }
            }
        }
    }

    let local = share == Share::Local;
    let ignore = project.mind_dir().join(".gitignore");
    let ignore = (local && fs::symlink_metadata(&ignore).is_err()).then_some(ignore);
    let mut plan = ProjectPlan {
        settings_file: project.mind_dir().join(SETTINGS_FILE),
        marker: project.mind_dir().join(MIND_CONFIG),
        backup,
        recorded: records.is_some(),
        ignore,
        installations,
        authored: authored_found,
        foreign,
        excluded: Vec::new(),
        exclude_file: None,
        outside: false,
        tracked: false,
        refusals,
        settings,
    };
    if local {
        exclusions(project, &in_place, &mut plan)?;
        // A hint and nothing more, so a repository that cannot be read
        // says nothing rather than refusing the migration.
        plan.tracked = exclude::tracks(root, Path::new(MIND_DIR)).unwrap_or(false);
    }
    Ok(Some(plan))
}

/// The folders a project's `mind.toml` keeps each kind in, written out, as
/// its settings have to name them: a kind they leave out has no folder at
/// all, where the `mind.toml` meant the default beside the code.
///
/// Every kind's default filled in (`Directories::with_defaults`, as `mind
/// init` fills them), and any key this build does not know carried along. A
/// version 1 marker kept every kind it names no folder for inside `.mind`,
/// and is described so, for the plan to refuse: settings cannot name a
/// folder there. A folder it does name is kept, as every other command reads
/// it ([`MindProject::directory_for`]), so naming one outside `.mind` is how
/// such a project gets past the refusal.
fn authored(config: &MindConfig) -> Directories {
    if config.version >= DIRECTORIES_VERSION {
        return config.directories.clone().with_defaults();
    }
    Kind::ALL
        .into_iter()
        .fold(config.directories.clone(), |directories, kind| {
            if directories.get(kind).is_some() {
                directories
            } else {
                directories.with(kind, Path::new(MIND_DIR).join(kind.folder()))
            }
        })
}

/// Where what `record` installed is offered now, among `offers`: the
/// folders its copies are held to when it has no hash (D-04).
///
/// By the origin it records; a record from before origins were kept names a
/// shelf source by its id, or else came from the workspace's own or a
/// loaded workspace's, any of which may be the one, since the record does
/// not say.
fn offered(
    workspace: &FlayerWorkspace,
    offers: &[Candidate],
    record: &InstallRecord,
) -> Vec<PathBuf> {
    offers
        .iter()
        .filter(|candidate| candidate.name() == record.name)
        .filter(|candidate| match (&record.origin, record.source_id) {
            (Some(origin), _) => candidate.origin() == *origin,
            (None, Some(id)) => {
                matches!(&candidate.offer, Offer::Shelf(gathered) if gathered.source.id == id)
            }
            (None, None) => !matches!(candidate.offer, Offer::Shelf(_)),
        })
        .map(|candidate| candidate.offer.source(workspace))
        .collect()
}

/// What a project that is to be local keeps out of git: an entry for each
/// recorded copy in place (`in_place`) that the group does not hold, the
/// group being new with the id. Read from the file, so a dry run says what
/// will be written; not opened at all when there is nothing to keep out.
fn exclusions(
    project: &MindProject,
    in_place: &[PathBuf],
    plan: &mut ProjectPlan,
) -> Result<(), MigrateError> {
    let landing = plan
        .installations
        .iter()
        .any(|retarget| !retarget.added.is_empty());
    if !landing && in_place.is_empty() {
        return Ok(());
    }
    let Some(listing) = exclude::listed(project.root(), plan.settings.id.as_str())? else {
        plan.outside = true;
        return Ok(());
    };
    plan.exclude_file = Some(listing.file.clone());
    for path in in_place {
        let Some(relative) = crate::paths::relative_to(path, project.root()) else {
            continue;
        };
        let line = exclude::Entry::directory(relative).pattern(&listing.route)?;
        if !listing.lines.contains(&line) {
            plan.excluded.push(path.clone());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "project_test.rs"]
mod tests;
