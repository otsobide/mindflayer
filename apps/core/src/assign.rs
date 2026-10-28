//! One skill or rule put into several projects at once, or taken back out of
//! them: what vision point 2 calls assigning, and what `flayer install` and
//! `flayer uninstall` do given `-p` more than once or `--all-projects` (#49).
//! And what nothing offers any more taken out of them, whatever its name:
//! `uninstall --orphaned` (#50).
//!
//! Each project's part is a single install's, through the very functions one
//! project's install goes through ([`install::offered_typed`] and
//! [`install::install_with`], or [`install::uninstall_typed`]), so nothing is
//! true of a project reached this way that is not true of it reached alone:
//! the same offer found, the same copies written, the same refusals. What the
//! batch adds is only what is true of many: each project once, in the order
//! asked, and a failure in one said of it beside the others, which go ahead,
//! as an update and a migration of every project go ahead past one.
//!
//! The workspace's lock is taken by each project's install, not held across
//! them all, as the install screen's batch takes it. Nothing ties one
//! project's install to another's, so a writer elsewhere in the workspace has
//! no reason to wait for every project, and holding the lock once would mean
//! a second body of each install, rules' included, that runs under a lock it
//! does not take, as an update has ([`install::install_held`]), to guard
//! nothing a project's own install does not: its copies and its record are
//! written together or not at all.

use std::path::Path;

use crate::agent::AgentHomes;
use crate::catalog::Reference;
use crate::gather::Scrub;
use crate::install::{self, Candidate, Edits, InstallError, Installation, Removed};
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError};
use crate::workspace::{same_directory, FlayerWorkspace, MindProject};

/// What a batch did: [`install`] or [`uninstall`].
#[derive(Debug)]
pub struct Assigned<T> {
    /// A part for each project asked, once each, in the order asked.
    pub parts: Vec<Part<T>>,
    /// What opening the ledger to write took out of it, for the front end
    /// to say: once for the batch, which opens it once.
    pub scrub: Scrub,
}

/// One project's part of a batch, and what came of it.
#[derive(Debug)]
pub struct Part<T> {
    pub project: MindProject,
    pub done: T,
}

/// One project's part of [`install`]: what it was offered and what
/// installing that came to, or why it was offered nothing of the name.
pub type Install = Result<Offered, InstallError>;

/// One project's part of [`uninstall`]: which kind the name was there and
/// what taking it out came to, or why it could not be asked.
pub type Uninstall = Result<(Kind, Removed), InstallError>;

/// What a project is offered under the name asked, and what installing it
/// there came to.
///
/// Apart from the refusal to find one ([`Install`]'s error), since what a
/// front end says of a failed install depends on what was being installed:
/// its kind, and so which of the project's folders were in play.
#[derive(Debug)]
pub struct Offered {
    pub candidate: Candidate,
    pub installed: Result<Installation, InstallError>,
}

/// Put what `reference` names into each of `projects`, every one a project
/// `workspace` links, as one install into each does: the one offer of that
/// name each is offered ([`install::offered_typed`], `from` and `base` as it
/// takes them), installed there ([`install::install_with`]), writing over an
/// edited copy only when `edits` says so, and into none of the agents' own
/// folders, `homes`.
///
/// A part for each project once, in the order asked: two spellings of one
/// directory are one project, installed into once. A project offered nothing
/// of that name, or whose install fails, says so in its own part, and the
/// rest go ahead; one whose copy is somebody's is told so by its install, as
/// [`install::Installed::Foreign`], and nothing is written there.
///
/// The ledger is opened to write once, for every part
/// ([`FlayerWorkspace::ledger_to_write`]), as an update opens it, and only
/// that refuses the whole batch, before anything is written. A batch of no
/// project opens nothing: there is nothing to record.
pub fn install(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    reference: &Reference,
    from: Option<&str>,
    base: &Path,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Assigned<Install>, LedgerError> {
    let Some((ledger, scrub)) = opened(workspace, projects)? else {
        return Ok(nothing());
    };
    let parts = each(projects)
        .into_iter()
        .map(|project| {
            let done = install::offered_typed(workspace, &ledger, project, reference, from, base)
                .map(|candidate| {
                    let installed = install::install_with(
                        workspace, &ledger, project, &candidate, homes, edits,
                    );
                    Offered {
                        candidate,
                        installed,
                    }
                });
            Part {
                project: project.clone(),
                done,
            }
        })
        .collect();
    Ok(Assigned { parts, scrub })
}

/// Take what `reference` names back out of each of `projects`, as one
/// uninstall from each does ([`install::uninstall_typed`]), and say which
/// kind it was there: a part for each project once, in the order asked, as
/// [`install`] gives them.
///
/// A project that has nothing of that name says so in its part
/// ([`Removed::Absent`]), and so does one where it is somebody's or was
/// changed since; whether an absence is worth a word is the front end's to
/// say, since one asked of every project the workspace links is what most
/// of them answer. The rest go ahead past a failure in one. The ledger is
/// opened as [`install`] opens it.
pub fn uninstall(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    reference: &Reference,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Assigned<Uninstall>, LedgerError> {
    let Some((ledger, scrub)) = opened(workspace, projects)? else {
        return Ok(nothing());
    };
    let parts = each(projects)
        .into_iter()
        .map(|project| Part {
            project: project.clone(),
            done: install::uninstall_typed(workspace, &ledger, project, reference, homes, edits),
        })
        .collect();
    Ok(Assigned { parts, scrub })
}

/// One project's part of [`uninstall_orphaned`]: each installation in it
/// that nothing offers any more where it came from, and what taking it out
/// came to; or why the project could not be asked.
pub type Orphans = Result<Vec<Orphan>, InstallError>;

/// One installation nothing offers any more, and what taking it out came
/// to.
#[derive(Debug)]
pub struct Orphan {
    pub kind: Kind,
    pub name: String,
    pub removed: Result<Removed, InstallError>,
}

/// Take every installation in each of `projects` that nothing offers any
/// more where it came from back out of it, as an uninstall of each from
/// each does ([`install::uninstall_with`]): what removing one of the
/// workspace's own from the workspace alone leaves behind, or unloading a
/// workspace that offered it. `uninstall --orphaned`.
///
/// Which those are is [`install::orphaned`]'s to say: one changed since it
/// was installed among them, which its uninstall leaves, every copy of it,
/// unless `edits` forces it, as an update leaves it with nothing to write it
/// from. A part for each project once, in the order asked, as [`uninstall`]
/// gives them, each installation of it by kind and then by name, one that
/// fails said beside the rest.
///
/// The ledger is opened to write only when there is one: with none,
/// nothing is installed anywhere to be orphaned, and each project's part
/// says so, empty, without a ledger made to say it.
pub fn uninstall_orphaned(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Assigned<Orphans>, LedgerError> {
    let (ledger, scrub) = if projects.is_empty() || !workspace.ledger_path().is_file() {
        (workspace.ledger_for_reading()?, Scrub::default())
    } else {
        workspace.ledger_to_write()?
    };
    let parts = each(projects)
        .into_iter()
        .map(|project| Part {
            project: project.clone(),
            done: orphans(workspace, &ledger, project, homes, edits),
        })
        .collect();
    Ok(Assigned { parts, scrub })
}

/// One project's part of [`uninstall_orphaned`].
fn orphans(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    homes: &AgentHomes,
    edits: Edits,
) -> Orphans {
    let mut orphans = Vec::new();
    for kind in Kind::ALL {
        for name in install::orphaned(workspace, ledger, project, kind)? {
            let removed =
                install::uninstall_with(workspace, ledger, project, kind, &name, homes, edits);
            orphans.push(Orphan {
                kind,
                name,
                removed,
            });
        }
    }
    Ok(orphans)
}

/// The ledger opened to write, and what its scrub took out, for a batch of
/// `projects`; none for a batch of none, which records nothing and so has
/// no reason to make a ledger, or to scrub one.
fn opened(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
) -> Result<Option<(Ledger, Scrub)>, LedgerError> {
    if projects.is_empty() {
        return Ok(None);
    }
    workspace.ledger_to_write().map(Some)
}

/// What a batch of no project did.
fn nothing<T>() -> Assigned<T> {
    Assigned {
        parts: Vec::new(),
        scrub: Scrub::default(),
    }
}

/// Each of `projects` once, in the order given: two spellings of one
/// directory, a name and a path, are one project, and installing into it
/// twice would say twice what was done once.
fn each(projects: &[MindProject]) -> Vec<&MindProject> {
    let mut once: Vec<&MindProject> = Vec::new();
    for project in projects {
        if !once
            .iter()
            .any(|seen| same_directory(seen.root(), project.root()))
        {
            once.push(project);
        }
    }
    once
}
