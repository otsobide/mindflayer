//! Finding a project again by its id once it has moved, and telling a copy of
//! it from it.
//!
//! The ledger names every record by the project's route from the workspace,
//! so a repository moved somewhere else leaves its entry stale and its
//! records naming a place nothing is at. What survives the move is the id
//! its settings hold ([`ProjectId`]), random so that only a copy shares it,
//! which the ledger keeps with the route it last saw it at (schema 5). A
//! project found holding an id the ledger last saw at a stale entry is that
//! entry's project, moved, and `flayer relink` follows it there with every
//! record it had. A project copied with its `.mind` holds the same id as the
//! one it was copied from, which no record can tell apart, so it is never
//! taken for a move: `mind init --new-id` ([`new_id`]) gives it one of its
//! own.

use std::fmt;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::exclude::{self, ExcludeError, Followed};
use crate::gather::Scrub;
use crate::install;
use crate::ledger::LedgerError;
use crate::paths;
use crate::settings::{ProjectId, Share};
use crate::workspace::{same_directory, FlayerWorkspace, MindProject, WorkspaceError};

/// What [`relink`] did.
#[derive(Debug)]
pub struct Relinked {
    /// What the project is called.
    pub name: String,
    /// The entry that pointed where it was, as the marker spelled it.
    pub was: PathBuf,
    /// The entry that points where it is now, as the marker spells it.
    pub entry: PathBuf,
    /// The id it holds, which is how it was found.
    pub id: ProjectId,
    /// How many installations the ledger records of it, which moved with it.
    pub installations: usize,
    /// What became of its group in the exclude file, for a local project:
    /// carried to where it is ([`exclude::carry`]). `None` for a shared one.
    pub excluded: Option<Followed>,
    /// What opening the ledger took out that an older build stored with a
    /// credential, for the front end to say.
    pub scrub: Scrub,
}

/// Follow the project the workspace links at `old`, a directory that no
/// longer holds it, to `new`, which does: the entry is rewritten to point
/// there, in its place in the marker, and every record the ledger keeps of
/// it moves with it, so what was installed reads as it did before the move.
///
/// `new` is `old`'s project when it holds the id the ledger last saw at
/// `old`, which nothing but a copy shares. So it is refused when:
///
/// - `old` is no entry of the workspace's;
/// - `new` is no project, or one a `mind.toml` marks, which has no id to be
///   found by (moves of those are found by nobody), or one the workspace
///   links already, which would leave two entries for one project;
/// - `old` still holds a project, which has not moved: with `new`'s id, a
///   copy made with its `.mind` ([`Duplicate`]), and with any other, or
///   none, another project. So does one whose marker is there and would not
///   open ([`MindProject::is_vacant`]), whose id nobody can compare;
/// - another project the workspace links holds `new`'s id: a copy too,
///   refused until one of the two is given an id of its own ([`new_id`]). A
///   copy nobody linked is beyond what this reads; `flayer scan` names it,
///   and never offers a duplicate as a move;
/// - the ledger last saw another id at `old`, or `new`'s at an entry other
///   than `old`, which is where `new` moved from, if anywhere;
/// - the ledger records something at `old` but no id, as it records a
///   project's installs from before ids were kept: nothing says they were
///   `new`'s project's, and handing them to it would let Mindflayer write
///   over or delete what sits at the paths they name.
///
/// A workspace whose ledger knows nothing of `old`, or has none, has no
/// record to keep, and the entry alone moves.
///
/// Holds the workspace's lock from before the ledger is read for its checks
/// until the entry and the records are written, which go together or not
/// at all ([`Ledger::relinked`]): a relink at the same moment, or an install
/// into either, comes wholly before it or wholly after. A local project's
/// exclude lines are carried first ([`exclude::carry`]), where the project
/// is now, and back again if the entry cannot be written, anchored where it
/// was, though nothing is there. A project moved out of every work tree
/// had its lines taken out and written nowhere, so none come back: they
/// named copies where nothing is.
///
/// [`Ledger::relinked`]: crate::ledger::Ledger::relinked
pub fn relink(
    workspace: &mut FlayerWorkspace,
    old: &Path,
    new: &Path,
) -> Result<Relinked, RelinkError> {
    let old = paths::normalize(old);
    let new = paths::normalize(new);
    if !workspace.is_linked(&old) {
        return Err(WorkspaceError::NotRegistered {
            path: old,
            workspace: workspace.root().to_path_buf(),
        }
        .into());
    }
    let project = MindProject::open(&new)?;
    let Some(settings) = project.settings().cloned() else {
        return Err(RelinkError::NoId {
            path: new,
            marker: project.marker_file(),
        });
    };
    if let Some(entry) = workspace.linked_as(&new) {
        return Err(RelinkError::Linked {
            path: new,
            entry: entry.to_path_buf(),
        });
    }
    match MindProject::open(&old) {
        Ok(there) => {
            return Err(match there.settings() {
                Some(held) if held.id == settings.id => RelinkError::Copied(Duplicate {
                    id: settings.id,
                    paths: vec![old, new],
                }),
                _ => RelinkError::Present {
                    path: old,
                    name: there.name().to_owned(),
                },
            })
        }
        // A marker there that would not open still marks its project, whose
        // id nothing read: `new` holding the one the ledger last saw at `old`
        // says nothing, since a copy of that project holds it too.
        Err(source) if !MindProject::is_vacant(&old) => {
            return Err(RelinkError::Unread {
                path: old,
                source: Box::new(source),
            })
        }
        Err(_) => {}
    }
    // Another project the workspace links holding the id is a copy too, and
    // which of the two the records were made for is nobody's to guess.
    let copy = workspace.members().into_iter().find(|member| {
        member
            .project
            .as_ref()
            .ok()
            .and_then(MindProject::settings)
            .is_some_and(|held| held.id == settings.id)
    });
    if let Some(copy) = copy {
        return Err(RelinkError::Copied(Duplicate {
            id: settings.id,
            paths: vec![copy.root, new],
        }));
    }

    let (ledger, scrub) = if workspace.ledger_path().is_file() {
        let (ledger, scrub) = workspace.ledger_to_write()?;
        (Some(ledger), scrub)
    } else {
        (None, Scrub::default())
    };
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let from = install::entry_for_root(workspace, &old);
    let to = install::route(workspace.root(), &new);
    if let Some(ledger) = &ledger {
        match ledger.project_at(&from)? {
            Some(was) if was == settings.id => {}
            Some(was) => {
                return Err(RelinkError::Mismatch {
                    old,
                    was,
                    new,
                    id: settings.id,
                })
            }
            None => {
                let seen = ledger
                    .known_projects()?
                    .into_iter()
                    .find(|(id, _)| *id == settings.id)
                    .map(|(_, route)| route);
                let elsewhere = seen.filter(|route| {
                    workspace
                        .members()
                        .iter()
                        .any(|member| install::entry_for_root(workspace, &member.root) == *route)
                });
                if let Some(route) = elsewhere {
                    return Err(RelinkError::Elsewhere {
                        path: new,
                        id: settings.id,
                        at: workspace.root().join(route),
                    });
                }
                if ledger.holds(&from)? {
                    return Err(RelinkError::Unknown { path: old });
                }
            }
        }
    }

    let id = settings.id.clone();
    let excluded = match settings.share {
        Share::Local => Some(exclude::carry(&old, id.as_str(), &new, id.as_str())?),
        Share::Shared => None,
    };
    let written = match &ledger {
        Some(ledger) => ledger
            .relinked(&from, &to, &id, || workspace.relink_held(&old, &new))?
            .map(|((was, entry), moved)| (was, entry, moved)),
        None => workspace
            .relink_held(&old, &new)
            .map(|(was, entry)| (was, entry, 0)),
    };
    let (was, entry, installations) = match written {
        Ok(written) => written,
        Err(error) => {
            // Toward `old`, which is gone: `carry` reaches a place that is
            // not there, from the nearest folder above it that is.
            if excluded.is_some() {
                let _ = exclude::carry(&new, id.as_str(), &old, id.as_str());
            }
            return Err(error.into());
        }
    };
    Ok(Relinked {
        name: project.name().to_owned(),
        was,
        entry,
        id,
        installations,
        excluded,
        scrub,
    })
}

/// What [`new_id`] did.
#[derive(Debug)]
pub struct Renewed {
    /// What the project is called.
    pub name: String,
    /// The id it held.
    pub was: ProjectId,
    /// The id it holds now.
    pub id: ProjectId,
    /// What became of its group in the exclude file, for a local project:
    /// carried to the new id ([`exclude::carry`]). `None` for a shared one,
    /// whose copies are the team's.
    pub excluded: Option<Followed>,
    /// What opening the ledger of the workspace linking it took out that an
    /// older build stored with a credential, for the front end to say.
    pub scrub: Scrub,
}

/// Give `project` an id of its own in place of the one its settings hold,
/// keeping everything else: for a copy made with its `.mind`, which holds
/// the id of the project it was copied from until then.
///
/// Its exclude group goes with it, when it is local: every line of the old
/// id's group naming a path in the project is carried to the new id's
/// ([`exclude::carry`]), before the settings change, so no copy is offered
/// to git meanwhile, and taken back if they do not. A copy elsewhere in the
/// same work tree keeps its own lines under the old id. When `workspace`
/// links the project, its ledger knows the project by the new id from then
/// on, and so does every record of it, as it would at the next write
/// ([`FlayerWorkspace::ledger_to_write`]): what was installed stays the
/// project's.
///
/// Refused for a project an older marker marks, as a change to its settings
/// is ([`MindProject::changeable`]): a `mind.toml` has no id to change, and
/// a `settings.json` is changed by nothing but its move to a
/// `settings.toml`, which `mind migrate` makes.
///
/// Holds the lock of the workspace linking the project from before the
/// settings are read again until its ledger is written, as a change to the
/// project's agents does ([`crate::reapply::apply`]).
pub fn new_id(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
) -> Result<Renewed, RelinkError> {
    project.changeable()?;
    let linked =
        workspace.filter(|workspace| workspace.is_made() && workspace.is_linked(project.root()));
    let (ledger, scrub) = match linked {
        Some(workspace) if workspace.ledger_path().is_file() => {
            let (ledger, scrub) = workspace.ledger_to_write()?;
            (Some(ledger), scrub)
        }
        _ => (None, Scrub::default()),
    };
    let _lock = match linked {
        Some(workspace) => Some(workspace.lock().map_err(WorkspaceError::from)?),
        None => None,
    };
    // Read again under the lock, as the id carried is the one it holds now.
    let mut project = MindProject::open(project.root())?;
    let settings = project.changeable()?.clone();
    let id = ProjectId::generate();
    let root = project.root().to_path_buf();
    let excluded = match settings.share {
        Share::Local => Some(exclude::carry(
            &root,
            settings.id.as_str(),
            &root,
            id.as_str(),
        )?),
        Share::Shared => None,
    };
    if let Err(error) = project.change_id(&id) {
        if excluded.is_some() {
            let _ = exclude::carry(&root, id.as_str(), &root, settings.id.as_str());
        }
        return Err(error.into());
    }
    if let Some((workspace, ledger)) = linked.zip(ledger.as_ref()) {
        let route = install::entry_for_root(workspace, &root);
        ledger.knew(&[(id.clone(), route)], &[])?;
    }
    Ok(Renewed {
        name: project.name().to_owned(),
        was: settings.id,
        id,
        excluded,
        scrub,
    })
}

/// One id held by more than one directory: a project copied with its
/// `.mind`, which no record can tell from the project it was copied from.
///
/// A failure collected wherever the projects holding it are read together
/// (`flayer status`, `flayer scan`), naming every one, and never a move:
/// which of them is the project the records were made for is nobody's to
/// guess, so none is followed until all but one hold an id of their own
/// ([`new_id`]).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct Duplicate {
    pub id: ProjectId,
    /// Each directory holding it, in the order they were read.
    pub paths: Vec<PathBuf>,
}

codes!(Duplicate => "duplicate_id");

impl fmt::Display for Duplicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut named: Vec<String> = self
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let last = named.pop().unwrap_or_default();
        let all = if named.len() > 1 { "all" } else { "both" };
        write!(
            f,
            "{} and {last} are {all} project {}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own",
            named.join(", "),
            self.id
        )
    }
}

/// Every id `held` names for more than one directory, with each directory
/// holding it, in the order `held` lists them. Two spellings of one
/// directory are one directory, not a copy.
pub fn duplicates(held: &[(ProjectId, PathBuf)]) -> Vec<Duplicate> {
    let mut found: Vec<Duplicate> = Vec::new();
    for (id, path) in held {
        match found.iter_mut().find(|duplicate| duplicate.id == *id) {
            Some(duplicate) => {
                if !duplicate
                    .paths
                    .iter()
                    .any(|seen| same_directory(seen, path))
                {
                    duplicate.paths.push(path.clone());
                }
            }
            None => found.push(Duplicate {
                id: id.clone(),
                paths: vec![path.clone()],
            }),
        }
    }
    found.retain(|duplicate| duplicate.paths.len() > 1);
    found
}

/// Why a project could not be given a new id, or followed to where it moved.
#[derive(Debug, Error)]
pub enum RelinkError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Exclude(#[from] ExcludeError),
    /// A project with no id, which nothing can tell is the one another entry
    /// was: one a `mind.toml` marks.
    #[error(
        "{path}: a {marker} marks it, which names it by no id, so nothing says it is the project \
         another entry was; `flayer link` links it as one of its own"
    )]
    NoId {
        path: PathBuf,
        marker: crate::workspace::MarkerFile,
    },
    /// The directory a relink would point an entry at is linked already.
    #[error(
        "{path}: the workspace links it already, as {}; `flayer unlink` that entry first",
        crate::paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
    )]
    Linked { path: PathBuf, entry: PathBuf },
    /// The entry and the directory hold one id: a copy, not a move.
    #[error("{0}")]
    Copied(Duplicate),
    /// The entry still holds a project, so it has not moved.
    #[error(
        "{path}: still holds a project, {name}, so it has not moved; `flayer link` links the \
         other as a project of its own"
    )]
    Present { path: PathBuf, name: String },
    /// The entry still holds a project's marker, which would not open, so
    /// nothing says which project it is, or that it moved.
    #[error(
        "{path}: still holds a project, so it has not moved, though its marker would not open: \
         {source}"
    )]
    Unread {
        path: PathBuf,
        source: Box<WorkspaceError>,
    },
    /// The directory holds another project than the one the entry was.
    #[error(
        "{new}: holds project {id}, and {old} was project {was}, so it is not where that one \
         moved; nothing was changed"
    )]
    Mismatch {
        old: PathBuf,
        was: ProjectId,
        new: PathBuf,
        id: ProjectId,
    },
    /// The directory holds the project another entry was.
    #[error(
        "{path}: holds project {id}, which the workspace last saw at {at}; `flayer relink {at} \
         {path}` follows it from there"
    )]
    Elsewhere {
        path: PathBuf,
        id: ProjectId,
        at: PathBuf,
    },
    /// The entry has records no id says whose they were.
    #[error(
        "{path}: the ledger records what was installed there but no id it was installed for, so \
         nothing says which project it was; `flayer unlink` it and `flayer link` the other"
    )]
    Unknown { path: PathBuf },
}

codes!(RelinkError {
    Self::NoId { .. } => "relink_no_id",
    Self::Linked { .. } => "relink_already_linked",
    Self::Present { .. } => "relink_entry_present",
    Self::Unread { .. } => "relink_entry_unreadable",
    Self::Mismatch { .. } => "relink_mismatch",
    Self::Elsewhere { .. } => "relink_elsewhere",
    Self::Unknown { .. } => "relink_unknown",
} carried {
    Self::Workspace(error) => error,
    Self::Ledger(error) => error,
    Self::Exclude(error) => error,
    Self::Copied(duplicate) => duplicate,
});
