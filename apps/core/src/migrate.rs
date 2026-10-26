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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::catalog::{Catalog, DiscoveryFailure};
use crate::kind::Kind;
use crate::paths;
use crate::workspace::{
    Directories, FlayerWorkspace, MindProject, WorkspaceError, FLAYER_CONFIG, MIND_CONFIG, MIND_DIR,
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
    #[error("{folder}: the workspace keeps its {kind}s there, and it overlaps {with}, so which files are the workspace's is not clear; move them yourself")]
    Tangled {
        kind: Kind,
        folder: PathBuf,
        with: PathBuf,
    },
    #[error("{0}, so what is in it cannot be moved")]
    Unlisted(DiscoveryFailure),
    #[error("{0}, so which of the workspace's own are that project's cannot be told")]
    Unreadable(WorkspaceError),
}

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
}

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
/// starts empty.
pub fn library_plan(workspace: &FlayerWorkspace) -> Plan {
    let mut plan = Plan {
        backup: backup_path(workspace),
        ..Plan::default()
    };
    let own = workspace.own();
    plan.kinds = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let from = paths::normalize(&own.directory_for(kind));
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

    for repoint in &plan.kinds {
        if let Some(with) = tangle(repoint, &plan.kinds, &projects) {
            plan.refusals.push(Refusal::Tangled {
                kind: repoint.kind,
                folder: repoint.from.clone(),
                with,
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
    if root.join(MIND_DIR).join(MIND_CONFIG).is_file() {
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
            let folder = paths::normalize(&project.directory_for(kind));
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
