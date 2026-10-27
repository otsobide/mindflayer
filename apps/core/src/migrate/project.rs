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
use std::path::{Path, PathBuf};

use super::{MigrateError, Refusal};
use crate::agent::{Agent, AgentHomes};
use crate::catalog::Catalog;
use crate::exclude;
use crate::install::{self, Candidate, InstallError, Offer};
use crate::kind::{Kind, Layout};
use crate::ledger::{InstallRecord, Ledger};
use crate::reapply::{self, Retarget, Why};
use crate::settings::{Settings, Share, SETTINGS_FILE};
use crate::workspace::{
    misplaced_authored, same_directory, Directories, FlayerWorkspace, MindConfig, MindProject,
    ProjectConfig, DIRECTORIES_VERSION, MIND_CONFIG, MIND_DIR,
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
/// every kind, or outside the project; no agent; a copy of the marker an
/// earlier migration kept; and a copy that would land inside one of
/// `homes`, or where a loaded workspace keeps its own, as an install
/// refuses one.
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
/// version 1 marker kept every kind inside `.mind`, and is described so, for
/// the plan to refuse: settings cannot name a folder there.
fn authored(config: &MindConfig) -> Directories {
    if config.version >= DIRECTORIES_VERSION {
        return config.directories.clone().with_defaults();
    }
    Kind::ALL
        .into_iter()
        .fold(config.directories.clone(), |directories, kind| {
            directories.with(kind, Path::new(MIND_DIR).join(kind.folder()))
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
