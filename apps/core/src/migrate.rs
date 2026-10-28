//! Bringing a workspace made before its library into the layout a new one has.
//!
//! A workspace keeps its own artifacts in the folders its marker names, and
//! one made before the library names `skills` and `rules` beside its
//! projects: `~/skills` and `~/rules`, for the workspace in a home directory.
//! It is read exactly as it was written, so nothing moves them but being
//! asked to (D-05). [`library_plan`] says what asking would do, and
//! [`library`] does it: each of the workspace's own artifacts moved into
//! `.mindflayer/library/<kind>`, and the marker's `[directories]` pointed
//! there, with a copy of the marker as it was kept beside it.
//!
//! The ledger is not opened. An installation of one of the workspace's own
//! records its origin as `workspace`, which names no folder, and its copies
//! by where they are in the project, so nothing in it says where the
//! workspace's own were, and every record reads the same once they have
//! moved: the copy is still compared with the offer, found wherever the
//! marker now says. A workspace that loads this one names its root, not its
//! folders, and reads them through its marker as well.
//!
//! A project an older Mindflayer made is the other thing to bring up to
//! date: [`project_plan`] and [`project`] move it from its `mind.toml` to a
//! `settings.toml`, and what is installed into it to where its agents read,
//! or from its `settings.json` to a `settings.toml` saying the same.

mod project;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::catalog::{Catalog, DiscoveryFailure};
use crate::code::codes;
use crate::exclude::ExcludeError;
use crate::install::InstallError;
use crate::kind::Kind;
use crate::ledger::LedgerError;
use crate::paths;
use crate::workspace::{
    misplaced_own, Directories, FlayerWorkspace, MindProject, WorkspaceError, FLAYER_CONFIG,
    MIND_CONFIG,
};

pub use project::{
    carries_installs, project, project_plan, ProjectFailure, ProjectMigrated, ProjectPlan, Stranded,
};

/// What moving the workspace's own into its library would do, and what would
/// stop it.
#[derive(Debug, Default)]
pub struct Plan {
    /// Every kind kept outside the library, whose line in the marker is
    /// pointed at it. Empty when there is nothing to do.
    pub kinds: Vec<Repoint>,
    /// Every artifact moved, in report order.
    pub moves: Vec<Move>,
    /// What sits in a folder the workspace keeps a kind in but belongs to a
    /// project, and so stays where it is.
    pub left: Vec<Left>,
    /// Why it cannot go ahead. Nothing is moved while there is any.
    pub refusals: Vec<Refusal>,
    /// Where the marker is kept as it was: `flayer.toml.v<version>`, beside it.
    pub backup: PathBuf,
}

impl Plan {
    /// Whether every kind is in the library already, so that migrating
    /// changes nothing at all.
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

/// A kind the marker keeps outside the library, and the library folder its
/// line is pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repoint {
    pub kind: Kind,
    /// The folder the marker names now.
    pub from: PathBuf,
    /// Its folder in the library.
    pub to: PathBuf,
}

/// One artifact moved into the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub kind: Kind,
    /// Where it is: a skill's folder, or a rule's file.
    pub from: PathBuf,
    /// Where it goes, at the same route below the library's folder.
    pub to: PathBuf,
}

/// Something in one of the workspace's folders that a project holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Left {
    pub kind: Kind,
    pub path: PathBuf,
    /// The root of the project holding it.
    pub project: PathBuf,
    /// Whether the workspace counts it among its own as things stand, listing
    /// it and offering it to install ([`FlayerWorkspace::own_kinds`]), and so
    /// stops once the marker's line points at the library. It does for one a
    /// project at its root shares with it when it does not manage that
    /// project, which the migration leaves to the project all the same: an
    /// install of it stays recorded, but nothing offers it any more.
    pub counted: bool,
}

/// Why the workspace's own cannot be moved as they are.
#[derive(Debug, Error)]
pub enum Refusal {
    #[error("{to}: already there, so {from} cannot be moved onto it")]
    Taken { from: PathBuf, to: PathBuf },
    #[error("{path}: already there, kept by an earlier migration; move it aside to migrate again")]
    Backup { path: PathBuf },
    #[error("{path}: a link to {target}, which is relative to where the link is, so moved it would point elsewhere; make it absolute or move it yourself")]
    Link { path: PathBuf, target: PathBuf },
    /// The folder a kind is kept in, or one above it inside the workspace, is
    /// a link: what is in it lives wherever that leads, a dotfiles repository
    /// say, and a rename would take it out of there.
    #[error("{folder}: a link to {target}, so moving the workspace's {kind}s would take them out of wherever it leads; move them yourself, or point flayer.toml at the library")]
    Linked {
        kind: Kind,
        folder: PathBuf,
        target: PathBuf,
    },
    #[error("{folder}: the workspace keeps its {kind}s there, and it overlaps {with}, so which files are the workspace's is not clear; move them yourself")]
    Tangled {
        kind: Kind,
        folder: PathBuf,
        with: PathBuf,
    },
    /// A folder the marker names that `flayer init` would have refused:
    /// outside the workspace, or on the shelf or the cache.
    #[error("{0}, so what is in it is not the workspace's to move")]
    Misplaced(WorkspaceError),
    #[error("{0}, so what is in it cannot be moved")]
    Unlisted(DiscoveryFailure),
    #[error("{0}, so which of the workspace's own are that project's cannot be told")]
    Unreadable(WorkspaceError),
    /// A folder a project's `mind.toml` keeps a kind in that its settings
    /// cannot name ([`project_plan`]): inside `.mind`, where a version 1
    /// project kept every kind it named no folder for, or outside the
    /// project.
    #[error("{0}, so the project's settings cannot name it; move what is there and name the folder in {MIND_CONFIG} first")]
    Authored(WorkspaceError),
    /// A project to be moved to settings naming no agent, which would have
    /// nowhere to put what is installed into it.
    #[error("{project} would name no agent, and what is installed into it would reach none")]
    NoAgent { project: String },
    /// A copy a project's move would put where an install refuses to put
    /// one, or take from there.
    #[error("{0}")]
    Install(InstallError),
}

codes!(Refusal {
    Self::Taken { .. } => "migrate_target_taken",
    Self::Backup { .. } => "migrate_backup_present",
    Self::Link { .. } => "migrate_relative_link",
    Self::Linked { .. } => "migrate_folder_linked",
    Self::Tangled { .. } => "migrate_folder_tangled",
    Self::NoAgent { .. } => "migrate_no_agent",
} carried {
    Self::Misplaced(error) => error,
    Self::Unlisted(error) => error,
    Self::Unreadable(error) => error,
    Self::Authored(error) => error,
    Self::Install(error) => error,
});

/// What [`library`] did.
#[derive(Debug)]
pub struct Migrated {
    /// What was planned under the lock, and so done.
    pub plan: Plan,
    /// Where the marker was kept as it was; `None` when there was nothing to
    /// do, and so nothing was written.
    pub backup: Option<PathBuf>,
}

/// Why [`library`] did nothing, or stopped and put back what it had moved.
#[derive(Debug, Error)]
pub enum MigrateError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error(transparent)]
    Exclude(#[from] ExcludeError),
    #[error("nothing moved: {}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Refused(Vec<Refusal>),
    #[error("{path}: the marker cannot be kept as it was: {source}")]
    Backup {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{from}: cannot be moved to {to}: {source}")]
    Move {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
        /// What had moved before and could not be put back
        /// ([`MigrateError::stranded`]).
        stranded: Vec<Move>,
    },
    #[error("{source}")]
    Marker {
        /// Boxed, as every error here would otherwise be the size of the
        /// largest a workspace can fail with and a list beside it.
        #[source]
        source: Box<WorkspaceError>,
        stranded: Vec<Move>,
    },
    /// A step of moving a project to its settings failed, and what it had
    /// written was put back, but for what `stranded` names
    /// ([`MigrateError::left_behind`]).
    #[error("{failure}; nothing was migrated")]
    Project {
        /// Boxed, for the reason `Marker`'s source is.
        #[source]
        failure: Box<ProjectFailure>,
        stranded: Vec<Stranded>,
    },
}

codes!(MigrateError {
    Self::Refused(_) => "migrate_refused",
    Self::Backup { .. } => "migrate_marker_backup_failed",
    Self::Move { .. } => "migrate_move_failed",
} carried {
    Self::Workspace(error) => error,
    Self::Ledger(error) => error,
    Self::Install(error) => error,
    Self::Exclude(error) => error,
    Self::Marker { source, .. } => source,
    Self::Project { failure, .. } => failure,
});

impl MigrateError {
    /// What had moved into the library when the migration failed and could
    /// not be put back where it was, for a front end to say: the marker still
    /// names the old folders, so it is nowhere the workspace looks until it
    /// is moved back by hand.
    pub fn stranded(&self) -> &[Move] {
        match self {
            MigrateError::Move { stranded, .. } | MigrateError::Marker { stranded, .. } => stranded,
            _ => &[],
        }
    }

    /// What a project's move that failed could not put back as it was, for
    /// a front end to say: the `mind.toml` marks the project again but for
    /// what is named here.
    pub fn left_behind(&self) -> &[Stranded] {
        match self {
            MigrateError::Project { stranded, .. } => stranded,
            _ => &[],
        }
    }
}

/// What moving `workspace`'s own into its library would do, read from what is
/// on disk and changing none of it.
///
/// A kind already kept in the library is left out, so a workspace made with
/// it, or migrated once, plans nothing. For any other, every artifact its
/// folder holds is moved to the same route below the library's, found the
/// way discovery finds it but without reading it, since one whose front
/// matter is broken is no less the workspace's ([`Catalog::locate`]).
///
/// What a project holds is never moved: a project the workspace manages, or
/// one at its root whether or not it manages it, can keep a kind in a folder
/// the workspace names too, as a home workspace whose home is a project does
/// when both say `skills`. That is left where it is, and the marker's line is
/// still pointed at the library, where the workspace's own of that kind then
/// starts empty. One at the root that the workspace does not manage is a
/// holder here, though [`FlayerWorkspace::own_kinds`] counts what it shares
/// as the workspace's own: what a project holds is what its agents read, and
/// they do not ask whether the workspace was told about the project. What
/// the workspace counted is marked ([`Left::counted`]), since it stops being
/// listed and offered.
///
/// A folder the marker names outside the workspace, or on the shelf or the
/// cache, is refused rather than emptied: `flayer init` refuses to name one,
/// but `open` reads a marker edited by hand to, and what is there is somebody
/// else's, or gathering's. So is one reached through a link, which keeps
/// what is in it somewhere else, on purpose.
pub fn library_plan(workspace: &FlayerWorkspace) -> Plan {
    let mut plan = Plan {
        backup: backup_path(workspace),
        ..Plan::default()
    };
    let own = workspace.own();
    plan.kinds = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let from = paths::normalize(&own.directory_for(kind)?);
            let to = paths::normalize(&workspace.root().join(Directories::library_for(kind)));
            (from != to).then_some(Repoint { kind, from, to })
        })
        .collect();
    if plan.is_empty() {
        return plan;
    }

    let (projects, unreadable) = holders(workspace);
    // Only one whose root holds a folder being moved from: its folders are
    // inside its root, so no other can keep anything there.
    plan.refusals.extend(
        unreadable
            .into_iter()
            .filter(|(root, _)| {
                plan.kinds
                    .iter()
                    .any(|repoint| overlap(root, &repoint.from))
            })
            .map(|(_, error)| Refusal::Unreadable(error)),
    );
    let theirs = held_by(&projects, &plan.kinds, &mut plan.refusals);
    let counted = workspace.own_kinds();

    for repoint in &plan.kinds {
        let route = workspace
            .config()
            .directories
            .get(repoint.kind)
            .map_or_else(|| Directories::library_for(repoint.kind), Path::to_path_buf);
        if let Some(error) = misplaced_own(repoint.kind, &route) {
            plan.refusals.push(Refusal::Misplaced(error));
            continue;
        }
        if let Some(with) = tangle(repoint, &plan.kinds, &projects) {
            plan.refusals.push(Refusal::Tangled {
                kind: repoint.kind,
                folder: repoint.from.clone(),
                with,
            });
            continue;
        }
        if let Some((folder, target)) = linked_folder(workspace.root(), &repoint.from) {
            plan.refusals.push(Refusal::Linked {
                kind: repoint.kind,
                folder,
                target,
            });
            continue;
        }
        let (found, failures) = Catalog::locate(&repoint.from, repoint.kind);
        plan.refusals
            .extend(failures.into_iter().map(Refusal::Unlisted));
        for path in found {
            if let Some((_, project)) = theirs.iter().find(|(held, _)| overlap(held, &path)) {
                plan.left.push(Left {
                    kind: repoint.kind,
                    path,
                    project: project.clone(),
                    counted: counted.contains(&repoint.kind),
                });
                continue;
            }
            let Ok(route) = path.strip_prefix(&repoint.from) else {
                continue;
            };
            let to = repoint.to.join(route);
            if let Some(target) = relative_link(&path) {
                plan.refusals.push(Refusal::Link { path, target });
            } else if is_there(&to) {
                plan.refusals.push(Refusal::Taken { from: path, to });
            } else {
                plan.moves.push(Move {
                    kind: repoint.kind,
                    from: path,
                    to,
                });
            }
        }
    }

    if is_there(&plan.backup) {
        plan.refusals.push(Refusal::Backup {
            path: plan.backup.clone(),
        });
    }
    plan
}

/// Move `workspace`'s own into its library, as [`library_plan`] says, and
/// point its marker there, keeping the marker as it was beside it.
///
/// Planned again once the workspace's lock is held, from the marker on disk
/// rather than `workspace`, so what is moved is what is there: a plan shown
/// a minute before can be out of date, and a second writer must not change
/// the marker between the plan and the rewrite. Refused as a whole when the
/// plan has any [`Refusal`], and a no-op, writing nothing, when it plans
/// nothing, which is what makes a second run harmless.
///
/// All or nothing as far as a rename allows: the marker is kept first, then
/// each artifact is renamed into place, then the marker is rewritten, and a
/// step that fails puts back every artifact already moved and takes the kept
/// copy away, so the workspace reads as it did before. A rename rather than
/// a copy, because the library is inside the workspace and so, but for a
/// folder mounted in between, on the same filesystem: a move across two is
/// not attempted, and fails naming both paths, as an install's does. The
/// folders a move emptied are removed, the kind's own included, so a home
/// workspace leaves no empty `~/skills` behind; anything else in them stays.
///
/// An [`unmade`](FlayerWorkspace::unmade) workspace keeps its own in the
/// library it would be made with, so there is nothing to move and nothing
/// is made.
pub fn library(workspace: &mut FlayerWorkspace) -> Result<Migrated, MigrateError> {
    if !workspace.is_made() {
        return Ok(Migrated {
            plan: library_plan(workspace),
            backup: None,
        });
    }
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let mut current = FlayerWorkspace::open(workspace.root())?;
    let plan = library_plan(&current);
    if !plan.refusals.is_empty() {
        return Err(MigrateError::Refused(plan.refusals));
    }
    if plan.is_empty() {
        *workspace = current;
        return Ok(Migrated { plan, backup: None });
    }

    keep(&current.config_path(), &plan.backup).map_err(|source| MigrateError::Backup {
        path: plan.backup.clone(),
        source,
    })?;
    let mut moved: Vec<Move> = Vec::new();
    for one in &plan.moves {
        if let Err(source) = move_one(one) {
            let stranded = put_back(&moved);
            let _ = fs::remove_file(&plan.backup);
            return Err(MigrateError::Move {
                from: one.from.clone(),
                to: one.to.clone(),
                source,
                stranded,
            });
        }
        moved.push(one.clone());
    }
    if let Err(source) =
        current.edit_marker_held(|document| point_at_library(document, &plan.kinds))
    {
        let stranded = put_back(&moved);
        let _ = fs::remove_file(&plan.backup);
        return Err(MigrateError::Marker {
            source: Box::new(source),
            stranded,
        });
    }
    clear_emptied(&plan);

    *workspace = current;
    let backup = Some(plan.backup.clone());
    Ok(Migrated { plan, backup })
}

/// Where `workspace`'s marker is kept before a migration rewrites it: beside
/// it, named after the version of the marker it holds, `flayer.toml.v2`.
///
/// After its version, as the ledger's copy is, so which build reads it is in
/// its name. Put back in place of the rewritten marker, it names the old
/// folders again, though what was moved stays in the library until it is
/// moved back.
pub fn backup_path(workspace: &FlayerWorkspace) -> PathBuf {
    workspace
        .flayer_dir()
        .join(format!("{FLAYER_CONFIG}.v{}", workspace.config().version))
}

/// Every project whose artifacts can sit among the workspace's own, and the
/// root of each that could not be read, with why.
///
/// The ones the workspace manages, and one at its root even when it does
/// not: a home directory made a project and never linked still keeps its
/// skills in `~/skills`, and moving them would take them from the agents
/// that read them there. An entry that leads to no project holds nothing;
/// one that leads to a project whose marker cannot be read could hold
/// anything.
fn holders(workspace: &FlayerWorkspace) -> (Vec<MindProject>, Vec<(PathBuf, WorkspaceError)>) {
    let root = workspace.root();
    let mut opened: Vec<(PathBuf, Result<MindProject, WorkspaceError>)> = workspace
        .members()
        .into_iter()
        .map(|member| (member.root, member.project))
        .collect();
    if MindProject::is_marked(root) {
        opened.push((root.to_path_buf(), MindProject::open(root)));
    }
    // One directory linked twice, or linked and found at the root, is one
    // project, and one failure.
    opened.sort_by(|a, b| a.0.cmp(&b.0));
    opened.dedup_by(|a, b| a.0 == b.0);

    let mut projects = Vec::new();
    let mut unreadable = Vec::new();
    for (root, project) in opened {
        match project {
            Ok(project) => projects.push(project),
            Err(WorkspaceError::NotAProject { .. }) => {}
            Err(error) => unreadable.push((root, error)),
        }
    }
    (projects, unreadable)
}

/// Every artifact a project holds in a folder that overlaps one the
/// workspace keeps a kind in, with that project's root, and a refusal for
/// each such folder that could not be listed.
///
/// Every kind of the project's, not only the one the folder holds for the
/// workspace: a workspace keeping its rules in `docs` finds a project's
/// `docs/skills/x/SKILL.md` among them. Only overlapping folders are walked,
/// since an artifact of a project's inside one of the workspace's folders
/// has both folders above it, and so they overlap.
fn held_by(
    projects: &[MindProject],
    kinds: &[Repoint],
    refusals: &mut Vec<Refusal>,
) -> Vec<(PathBuf, PathBuf)> {
    let mut held = Vec::new();
    for project in projects {
        for kind in Kind::ALL {
            let Some(folder) = project.directory_for(kind) else {
                continue;
            };
            let folder = paths::normalize(&folder);
            if !kinds.iter().any(|repoint| overlap(&repoint.from, &folder)) {
                continue;
            }
            let (found, failures) = Catalog::locate(&folder, kind);
            // The workspace's own walk says so of a folder they share.
            if !kinds.iter().any(|repoint| repoint.from == folder) {
                refusals.extend(failures.into_iter().map(Refusal::Unlisted));
            }
            held.extend(
                found
                    .into_iter()
                    .map(|path| (path, project.root().to_path_buf())),
            );
        }
    }
    held
}

/// What a kind's folder overlaps that makes moving what is in it a guess:
/// its own folder in the library, another kind's folder, or a project's
/// root. Each would have moving one artifact carry another, or a project's
/// files, with it, or read the library as its own source.
fn tangle(repoint: &Repoint, kinds: &[Repoint], projects: &[MindProject]) -> Option<PathBuf> {
    if overlap(&repoint.from, &repoint.to) {
        return Some(repoint.to.clone());
    }
    if let Some(other) = kinds
        .iter()
        .find(|other| other.kind != repoint.kind && overlap(&other.from, &repoint.from))
    {
        return Some(other.from.clone());
    }
    projects
        .iter()
        .map(|project| paths::normalize(project.root()))
        .find(|root| root.starts_with(&repoint.from))
}

/// Whether one path is the other or inside it, either way round, compared
/// lexically, as the workspace compares a project's folders with its own.
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Whether anything at all is at `path`, a link that leads nowhere included.
fn is_there(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// The first folder below `root` on the way down to `folder`, `folder`
/// included, that is a link, and where it points.
///
/// Every step rather than `folder` alone, as `config -> dotfiles/config`
/// keeps `config/skills` elsewhere as surely as `skills -> dotfiles/skills`
/// does. `root` itself is not asked: a workspace reached through a link is
/// still one, and the library is inside it with everything else. `folder`
/// is inside `root` here, the plan having refused one that is not.
fn linked_folder(root: &Path, folder: &Path) -> Option<(PathBuf, PathBuf)> {
    let route = folder.strip_prefix(root).ok()?;
    let mut current = root.to_path_buf();
    for step in route.components() {
        current.push(step);
        let metadata = fs::symlink_metadata(&current).ok()?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&current).unwrap_or_default();
            return Some((current, target));
        }
    }
    None
}

/// Where `path` points when it is a link whose target is relative: read
/// against wherever the link is, so a rename to a folder at another depth
/// would aim it somewhere else.
fn relative_link(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_symlink() {
        return None;
    }
    fs::read_link(path)
        .ok()
        .filter(|target| target.is_relative())
}

/// Write a copy of the marker at `from` to `to`, which must not exist.
///
/// `create_new`, so a copy an earlier migration kept is never written over,
/// though the plan refuses one already: something made there since is
/// somebody's. A copy cut short is taken away rather than left looking
/// whole. With the marker's permissions, as a rewrite of it keeps them.
fn keep(from: &Path, to: &Path) -> io::Result<()> {
    use std::io::Write as _;

    let contents = fs::read(from)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    if let Err(error) = file.write_all(&contents).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(to);
        return Err(error);
    }
    if let Ok(metadata) = fs::metadata(from) {
        let _ = fs::set_permissions(to, metadata.permissions());
    }
    Ok(())
}

/// Rename one artifact into the library, making the folders above it.
///
/// Looked for again just before: a rename onto a file replaces it, and
/// something made there since the plan is somebody's.
fn move_one(one: &Move) -> io::Result<()> {
    if let Some(parent) = one.to.parent() {
        fs::create_dir_all(parent)?;
    }
    if is_there(&one.to) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    fs::rename(&one.from, &one.to)
}

/// Put every artifact in `moved` back where it was, the last one first, and
/// say which could not be.
fn put_back(moved: &[Move]) -> Vec<Move> {
    moved
        .iter()
        .rev()
        .filter(|one| fs::rename(&one.to, &one.from).is_err())
        .cloned()
        .collect()
}

/// Point every kind in `kinds` at its folder in the library, changing only
/// the value of its line, so the comment beside it and every other byte of
/// the marker stay as they were.
fn point_at_library(
    document: &mut toml_edit::DocumentMut,
    kinds: &[Repoint],
) -> Result<(), String> {
    let table = document
        .entry("directories")
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| String::from("`directories` is not a table"))?;
    for repoint in kinds {
        let library = Directories::library_for(repoint.kind);
        let library = paths::to_config_string(&library).unwrap_or_default();
        match table
            .get_mut(repoint.kind.folder())
            .and_then(toml_edit::Item::as_value_mut)
        {
            Some(value) => {
                let decor = value.decor().clone();
                *value = toml_edit::Value::from(library);
                *value.decor_mut() = decor;
            }
            None => {
                table.insert(repoint.kind.folder(), toml_edit::value(library));
            }
        }
    }
    Ok(())
}

/// Remove every folder a move left empty, from the one each artifact was in
/// up to its kind's folder, which goes too once nothing is left in it.
///
/// `remove_dir`, which refuses a folder with anything in it, so what is not
/// an artifact stays, and with it the folders above it: a `README.md` beside
/// the skills keeps `~/skills`. Best effort: a folder left behind is litter,
/// not a reason to call a migration that moved everything a failure.
fn clear_emptied(plan: &Plan) {
    for one in &plan.moves {
        let Some(repoint) = plan.kinds.iter().find(|repoint| repoint.kind == one.kind) else {
            continue;
        };
        let mut folder = one.from.parent();
        while let Some(current) = folder {
            if !current.starts_with(&repoint.from) || fs::remove_dir(current).is_err() {
                break;
            }
            folder = current.parent();
        }
    }
}
