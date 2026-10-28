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

use thiserror::Error;

use crate::exclude::{self, ExcludeError, Followed};
use crate::gather::Scrub;
use crate::install;
use crate::ledger::LedgerError;
use crate::settings::{ProjectId, Share};
use crate::workspace::{FlayerWorkspace, MindProject, WorkspaceError};

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

/// Why a project could not be given a new id, or followed to where it moved.
#[derive(Debug, Error)]
pub enum RelinkError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Exclude(#[from] ExcludeError),
}
