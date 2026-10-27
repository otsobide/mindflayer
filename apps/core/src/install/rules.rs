//! Installing a rule into a project, and taking it back out.
//!
//! A project a `mind.toml` marks is given a rule as a copy of its file, in
//! the folder its marker names, filed by its route as the library files it,
//! folders and all: such a project names no agents, and was given its rules
//! as files before rules had any other way in. What is Mindflayer's is what
//! the ledger records, as for a skill: one installation a rule, with a target
//! for its copy and the hash of what it holds, so an unrecorded file at a
//! rule's place is somebody's, never written over or taken away.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{
    copy_at, kept_by_a_load, project_entry, route, same_place, targets, unrecorded, user_scope,
    Candidate, InstallError, Installation, Installed, Offer, Standing,
};
use crate::agent::AgentHomes;
use crate::artifact::{Artifact, ArtifactError};
use crate::claude_rules;
use crate::copy;
use crate::hash;
use crate::kind::Kind;
use crate::ledger::{Action, InstallRecord, Ledger, Outcome, Target};
use crate::paths;
use crate::workspace::{replace_bytes_unresolved, FlayerWorkspace, MindProject, WorkspaceError};

/// Install `candidate`, a rule, into `project`: copied into the folder its
/// marker names. A project marked by its settings has no such folder among
/// its targets, and is refused ([`InstallError::NoTarget`]).
pub(crate) fn install(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
) -> Result<Installation, InstallError> {
    copy_in(workspace, ledger, project, candidate, homes)
}

/// One rule an install gives the project: what it is read from.
struct Wanted {
    /// The rule's file as written, which a copy holds whole.
    bytes: Vec<u8>,
    origin: String,
}

impl Wanted {
    /// The rule `offer` stands for, read where it is offered from.
    fn of(workspace: &FlayerWorkspace, offer: &Offer) -> Result<Wanted, InstallError> {
        let path = offer.source(workspace);
        let within = within(workspace, offer, &path);
        inside(&path, &within)?;
        let artifact = Artifact::rule(&path, offer.name().to_owned(), &within)?;
        let bytes = fs::read(artifact.file()).map_err(|source| {
            InstallError::Artifact(ArtifactError::Read {
                path: artifact.file(),
                source,
            })
        })?;
        Ok(Wanted {
            bytes,
            origin: offer.origin(),
        })
    }
}

/// Where links in the rule `offer` offers, at `path`, may lead: the
/// workspace that keeps it, as for a skill's copy.
fn within(workspace: &FlayerWorkspace, offer: &Offer, path: &Path) -> PathBuf {
    match offer {
        Offer::Loaded { entry, .. } => paths::normalize(&workspace.root().join(entry)),
        Offer::Workspace { .. } => workspace.root().to_path_buf(),
        Offer::Shelf(_) => path.to_path_buf(),
    }
}

/// Refuse the rule at `path` when it is a link, or reached through one, that
/// leads out of `within`, where it is offered from: what it leads to would
/// go into the project's instructions, and a credential is how such a file
/// ends up there, as a skill's copy refuses one.
fn inside(path: &Path, within: &Path) -> Result<(), InstallError> {
    let (Ok(real), Ok(root)) = (fs::canonicalize(path), fs::canonicalize(within)) else {
        // Nothing to follow: reading it says what is wrong.
        return Ok(());
    };
    if real.starts_with(&root) {
        Ok(())
    } else {
        Err(InstallError::Outside {
            path: path.to_path_buf(),
            target: real,
            within: within.to_path_buf(),
        })
    }
}

/// Copy `candidate`, a rule, into `project`, which a `mind.toml` marks: the
/// file as written, filed by its route in the folder the marker names, its
/// folders made on the way, and recorded as a skill's copy is.
///
/// Such a project names no agents and has no block: it was given its rules
/// as files before rules had one, and a copy is what it reads them as. An
/// unrecorded file at that place is somebody's, and refuses the install.
fn copy_in(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
) -> Result<Installation, InstallError> {
    let kind = Kind::Rule;
    let name = candidate.name().to_owned();
    let entry = project_entry(workspace, project);
    let targets = targets(project, kind)?;
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, &name))
        .collect();
    if let Some(error) = user_scope(homes, &copies) {
        return Err(error);
    }
    if candidate.standing == Standing::Foreign {
        let recorded = ledger.copies(&entry, kind)?;
        if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
            return Ok(Installed::Foreign { name, path }.into());
        }
    }
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;

    let from = candidate.source(workspace);
    if !from.is_file() {
        return Err(match &candidate.offer {
            Offer::Loaded { entry, .. } => InstallError::Gone {
                name,
                path: from,
                workspace: paths::normalize(&workspace.root().join(entry)),
            },
            _ => InstallError::Missing {
                name,
                path: from,
                workspace: workspace.root().to_path_buf(),
            },
        });
    }
    // A loaded workspace can be the project itself, keeping its own rules
    // where the copy would go: nothing is written, and nothing recorded,
    // which would let an uninstall take its own file away.
    if let Some(to) = copies.iter().find(|to| same_place(&from, to)) {
        return Ok(Installed::Unchanged {
            name,
            paths: vec![to.clone()],
            unhashed: None,
        }
        .into());
    }
    for to in &copies {
        if let Some(loaded) = kept_by_a_load(workspace, kind, to) {
            return Err(InstallError::KeptByALoad {
                path: to.clone(),
                loaded,
            });
        }
    }
    let recorded = ledger.copies(&entry, kind)?;
    if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
        return Ok(Installed::Foreign { name, path }.into());
    }

    let rule = Wanted::of(workspace, &candidate.offer)?;
    let mut changes = Vec::with_capacity(copies.len());
    for to in &copies {
        changes.push(put(to, &rule.bytes)?);
    }
    let held = hash::bytes(&rule.bytes);
    ledger.installed(
        &entry,
        &InstallRecord {
            kind,
            name: name.clone(),
            source_id: None,
            origin: Some(rule.origin.clone()),
            content_hash: Some(held.clone()),
            targets: targets
                .iter()
                .zip(&copies)
                .map(|(target, to)| Target {
                    agent: target.agent(),
                    path: route(project.root(), to),
                    content_hash: Some(held.clone()),
                })
                .collect(),
        },
    )?;
    let detail = format!("{name} into {}", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    let paths = copies;
    Ok(if changes
        .iter()
        .all(|change| *change == copy::Change::Unchanged)
    {
        Installed::Unchanged {
            name,
            paths,
            unhashed: None,
        }
    } else if changes.iter().all(|change| *change == copy::Change::Added) {
        Installed::Added {
            name,
            paths,
            unhashed: None,
        }
    } else {
        Installed::Updated {
            name,
            paths,
            unhashed: None,
        }
    }
    .into())
}

/// Put `bytes` at `to`, a rule's copy: made with its folders where nothing
/// is there, so a file somebody made meanwhile is never written over;
/// replaced through a temporary and a rename where a recorded copy is, never
/// through a link; left alone where it already holds them.
fn put(to: &Path, bytes: &[u8]) -> Result<copy::Change, InstallError> {
    let failed = |source| InstallError::Write {
        path: to.to_path_buf(),
        source,
        swept: Vec::new(),
    };
    match fs::symlink_metadata(to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            claude_rules::create(to, bytes).map_err(failed)?;
            Ok(copy::Change::Added)
        }
        Err(error) => Err(failed(error)),
        Ok(metadata) if metadata.is_file() && fs::read(to).map_err(failed)? == bytes => {
            Ok(copy::Change::Unchanged)
        }
        Ok(_) => {
            replace_bytes_unresolved(to, bytes).map_err(|(_, source)| failed(source))?;
            Ok(copy::Change::Updated)
        }
    }
}
