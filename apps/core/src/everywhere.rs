//! Removing one of the workspace's own artifacts from the workspace and from
//! every project it links that has it: `flayer remove --everywhere` (#50).
//!
//! Removing the library copy alone leaves every copy installed from it
//! behind, with a record that nothing offers any more: orphaned, as `status`
//! says, never updated again, and taken out only by an uninstall somebody
//! remembers to run in each project. Removing it everywhere takes those
//! copies out first, as an uninstall from each project does
//! ([`install::uninstall_with`]), and then the library copy, as `remove`
//! does ([`lifecycle::remove`]): the copies first, since a record from
//! before hashes were kept is told from an edit only against its offer,
//! which the library copy is.
//!
//! D-04 says which copies go. One nobody has changed since it was installed
//! is Mindflayer's to take out, whether it is current, outdated, missing a
//! copy or orphaned already. One changed since is somebody's work: left,
//! every copy of it, as an uninstall leaves it unforced, and orphaned from
//! then on, for `uninstall --orphaned --force` to take out once somebody has
//! looked. Somebody's own of that name, which nothing records, is never
//! Mindflayer's to take out, and is said so it is not mistaken for a copy.
//!
//! Only the projects the workspace links: a project linked by another
//! workspace, or by none, keeps what was installed there, as the ledger
//! that could say so is not this one.
//!
//! [`plan`] says what removing it everywhere would do, writing nothing, the
//! ledger read without being made, and [`apply`] does it. Each project's
//! uninstall takes the workspace's lock for itself, as `flayer uninstall
//! --all-projects` takes it ([`crate::assign`]): nothing ties one project's
//! copies to another's, and the uninstall looks again under the lock at what
//! the plan saw, so a copy edited between the two is still left.

use std::path::PathBuf;

use thiserror::Error;

use crate::agent::AgentHomes;
use crate::catalog::Reference;
use crate::code::codes;
use crate::gather::Scrub;
use crate::install::{self, Edits, Found, InstallError, Offer, Removed, Standing};
use crate::ledger::{InstallRecord, Ledger, LedgerError};
use crate::lifecycle::{self, LifecycleError, Target};
use crate::workspace::{same_directory, FlayerWorkspace, MindProject, WorkspaceError};

/// What removing one of the workspace's own everywhere would do: [`plan`],
/// or what [`apply`] did.
#[derive(Debug)]
pub struct Plan {
    /// The artifact, among the workspace's own.
    pub target: Target,
    /// Each project the workspace links that holds something of the
    /// artifact's name that removing it everywhere has to say, in the order
    /// the workspace links them, each once: a copy installed from it, or
    /// somebody's own of that name. A project with neither, or with one
    /// installed from somewhere else, which removing this one leaves
    /// offering it, is not among them.
    pub copies: Vec<ProjectCopy>,
    /// Each project the workspace links that cannot be opened: whether it
    /// holds a copy cannot be told, so it is said, beside the rest.
    pub unread: Vec<WorkspaceError>,
}

impl Plan {
    /// The copies that removing the artifact from the workspace alone, and
    /// not everywhere, would leave orphaned: every one the ledger records
    /// as installed from it, changed since or not.
    pub fn orphaned(&self) -> Vec<&ProjectCopy> {
        self.copies
            .iter()
            .filter(|copy| !matches!(copy.step, Step::Somebodys(_)))
            .collect()
    }
}

/// One project's copy of the artifact, how it stands, and what removing the
/// artifact everywhere does with it.
#[derive(Debug)]
pub struct ProjectCopy {
    pub project: MindProject,
    /// How it stands against what was installed and what the library
    /// offers ([`Standing`]): `Foreign` for somebody's own.
    pub standing: Standing,
    pub step: Step,
    /// What uninstalling it came to, once applied: none until then, and for
    /// a copy left.
    pub done: Option<Result<Removed, InstallError>>,
}

/// What removing an artifact everywhere does with one project's copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Mindflayer's, and unchanged since it was installed: current,
    /// outdated, missing a copy, whose others go and whose record goes with
    /// them, or orphaned already. Uninstalled, every copy the ledger records.
    Uninstall,
    /// Edited: these copies were changed since it was installed, so it is
    /// left, every copy of it, as an uninstall leaves it unforced, and
    /// orphaned once the library copy is gone.
    Leave(Vec<PathBuf>),
    /// Somebody's own of that name, at this path, which nothing records, so
    /// nothing of Mindflayer's: left, and said.
    Somebodys(PathBuf),
}

/// What [`apply`] did.
#[derive(Debug)]
pub struct Applied {
    /// What was planned, each copy uninstalled with what came of it
    /// ([`ProjectCopy::done`]): one that failed is said there, beside the
    /// rest, rather than stopping them.
    pub plan: Plan,
    /// What removing the library copy came to, after every project's.
    pub removed: Result<lifecycle::Removed, LifecycleError>,
    /// What opening the ledger to write took out of it, for the front end
    /// to say.
    pub scrub: Scrub,
}

/// Why removing an artifact everywhere could not be planned or begun, before
/// anything was written.
#[derive(Debug, Error)]
pub enum EverywhereError {
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Install(#[from] InstallError),
}

codes!(EverywhereError {} carried {
    Self::Lifecycle(error) => error,
    Self::Ledger(error) => error,
    Self::Install(error) => error,
});

/// What removing what `reference` names, one of `workspace`'s own, from the
/// workspace and from every project it links would do: nothing written, the
/// ledger read without being made.
///
/// The artifact is found among the workspace's own as `remove` finds it
/// ([`lifecycle::find_to_change`]), and refused as it would be there.
pub fn plan(workspace: &FlayerWorkspace, reference: &Reference) -> Result<Plan, EverywhereError> {
    let target = lifecycle::find_to_change(Some(workspace), &workspace.own(), reference)?;
    let ledger = workspace.ledger_for_reading()?;
    plan_with(workspace, &ledger, target)
}

/// Remove what `reference` names, one of `workspace`'s own, from every
/// project the workspace links and then from the workspace: [`plan`], done.
///
/// Each copy planned to go is uninstalled ([`install::uninstall_with`],
/// never forced), and one that fails says so in its own
/// [`ProjectCopy::done`] while the rest go ahead. The library copy goes
/// after them all, whatever each came to: what is left in a project is
/// orphaned then, as a copy left because it was edited is, and `uninstall
/// --orphaned` takes it out, where keeping the library copy back would ask
/// for the whole removal again.
///
/// The ledger is opened to write, scrubbing it, only when there is one: a
/// workspace that never installed anything has no copy to take out, and
/// no reason to be given a ledger to say so. Only finding the artifact and
/// opening the ledger refuse it as a whole, before anything is written.
///
/// `homes` are the agents' own folders, which the front end decides, as for
/// an uninstall.
pub fn apply(
    workspace: &FlayerWorkspace,
    reference: &Reference,
    homes: &AgentHomes,
) -> Result<Applied, EverywhereError> {
    let target = lifecycle::find_to_change(Some(workspace), &workspace.own(), reference)?;
    let (ledger, scrub) = if workspace.ledger_path().is_file() {
        workspace.ledger_to_write()?
    } else {
        (workspace.ledger_for_reading()?, Scrub::default())
    };
    let mut plan = plan_with(workspace, &ledger, target)?;
    let (kind, name) = (plan.target.kind, plan.target.name.clone());
    for copy in &mut plan.copies {
        if copy.step == Step::Uninstall {
            copy.done = Some(install::uninstall_with(
                workspace,
                &ledger,
                &copy.project,
                kind,
                &name,
                homes,
                Edits::Kept,
            ));
        }
    }
    let removed = lifecycle::remove(Some(workspace), &workspace.own(), &plan.target);
    Ok(Applied {
        plan,
        removed,
        scrub,
    })
}

/// [`plan`], for `target`, against `ledger`.
fn plan_with(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    target: Target,
) -> Result<Plan, EverywhereError> {
    let (kind, name) = (target.kind, target.name.clone());
    let offers = install::offers(workspace, ledger, kind)?;
    // Built from the target rather than found among the offers: one whose
    // front matter no longer reads is offered by nothing, and what was
    // installed from it is its copy all the same.
    let library = Offer::Workspace {
        kind,
        name: name.clone(),
        summary: None,
        path: target.path.clone(),
    };
    let elsewhere: Vec<Offer> = offers
        .iter()
        .filter(|offer| !matches!(offer, Offer::Workspace { .. }))
        .cloned()
        .collect();
    let (projects, unread) = workspace.projects();
    let mut copies: Vec<ProjectCopy> = Vec::new();
    for project in projects {
        if copies
            .iter()
            .any(|seen| same_directory(seen.project.root(), project.root()))
        {
            continue;
        }
        let surveyed = install::surveyed_one(workspace, ledger, &project, kind, &name, &offers)?;
        let (standing, step) = match surveyed {
            Some(surveyed) if from_the_library(&surveyed.record, &library, &elsewhere) => {
                let step = match surveyed.standing {
                    Standing::Edited => Step::Leave(surveyed.found(Found::Edited)),
                    _ => Step::Uninstall,
                };
                (surveyed.standing, step)
            }
            // From somewhere that offers it still, which removing the
            // library copy changes nothing for.
            Some(_) => continue,
            None => match install::somebodys(workspace, ledger, &project, kind, &name)? {
                Some(path) => (Standing::Foreign, Step::Somebodys(path)),
                None => continue,
            },
        };
        copies.push(ProjectCopy {
            project,
            standing,
            step,
            done: None,
        });
    }
    Ok(Plan {
        target,
        copies,
        unread,
    })
}

/// Whether the installation `record` came from the library copy, `library`,
/// as far as the ledger tells: what it records of where it came from
/// matches that ([`install::offered_for`]), and nothing offered from
/// `elsewhere`.
///
/// A record names its origin, and one naming the workspace's own came from
/// the library. One from before origins were kept may have come from the
/// workspace's own or from any workspace it loads, and is taken for the
/// library's only while no loaded workspace offers its name: while one
/// does, removing the library copy leaves it offered, and taking a copy out
/// that may be that one's would be a guess.
fn from_the_library(record: &InstallRecord, library: &Offer, elsewhere: &[Offer]) -> bool {
    !install::offered_for(record, std::slice::from_ref(library)).is_empty()
        && install::offered_for(record, elsewhere).is_empty()
}
