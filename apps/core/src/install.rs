//! Putting an artifact from the workspace shelf into a mind project, and
//! taking it back out.
//!
//! Gathering fills the shelf; this is the other half. It writes into a mind
//! project — the only thing in Mindflayer that does — and the rule it works by
//! is that **it only manages what it installed**. An artifact somebody wrote by
//! hand is neither overwritten nor deleted, whatever a caller asks for, and it
//! is told so rather than being obeyed quietly. Which of the two a file is, is
//! what [`crate::ledger`] remembers.

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::copy;
use crate::kind::Kind;
use crate::ledger::{Action, Gathered, Ledger, LedgerError, Outcome};
use crate::paths;
use crate::workspace::{FlayerWorkspace, MindProject};

/// How one shelf entry stands with respect to one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The project does not hold anything by that name.
    Absent,
    /// The project holds it, and the ledger says Mindflayer put it there.
    Installed,
    /// The project holds something by that name that Mindflayer did not put
    /// there. It is somebody's work, so it is left alone in both directions.
    Foreign,
}

impl Standing {
    /// Whether the project holds it at all, which is what a checkbox shows.
    pub fn present(self) -> bool {
        matches!(self, Standing::Installed | Standing::Foreign)
    }

    /// Whether Mindflayer may write over it or remove it.
    pub fn ours(self) -> bool {
        matches!(self, Standing::Installed | Standing::Absent)
    }
}

/// One shelf entry, seen from one project.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The shelf entry itself, origin and all.
    pub gathered: Gathered,
    pub standing: Standing,
}

impl Candidate {
    pub fn name(&self) -> &str {
        &self.gathered.name
    }

    pub fn kind(&self) -> Kind {
        self.gathered.kind
    }

    /// Where it came from, as one string: the branch is part of the address
    /// when one was asked for.
    pub fn origin(&self) -> String {
        match &self.gathered.source.reference {
            Some(reference) => format!("{}#{reference}", self.gathered.source.url),
            None => self.gathered.source.url.clone(),
        }
    }
}

/// Every shelf entry of `kind`, and how each stands with `project`.
///
/// The shelf is the list of what can be installed, so an artifact a project
/// holds that is on no shelf does not appear: it is not a thing this command
/// can offer to do anything about.
pub fn survey(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<Candidate>, InstallError> {
    let entry = project_entry(workspace, project);
    let ours = ledger.installations(&entry, kind)?;
    let directory = project.directory_for(kind);

    let mut candidates = Vec::new();
    for gathered in ledger.gathered()? {
        if gathered.kind != kind {
            continue;
        }
        let standing = if !holds(&directory, kind, &gathered.name) {
            Standing::Absent
        } else if ours.iter().any(|name| name == &gathered.name) {
            Standing::Installed
        } else {
            Standing::Foreign
        };
        candidates.push(Candidate { gathered, standing });
    }
    Ok(candidates)
}

/// Copy one shelf entry into a project.
pub fn install(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
) -> Result<Installed, InstallError> {
    let kind = candidate.kind();
    let name = candidate.name().to_owned();
    let entry = project_entry(workspace, project);

    if candidate.standing == Standing::Foreign {
        return Ok(Installed::Foreign { name });
    }

    let from = workspace.root().join(&candidate.gathered.path);
    if !from.is_dir() {
        return Err(InstallError::Missing {
            name,
            path: from,
            workspace: workspace.root().to_path_buf(),
        });
    }
    // The folder is named after the artifact, not after the folder it had on
    // the shelf: inside a project a skill's directory has to match its
    // declared name, which is what `validate` checks and what an agent uses to
    // find it.
    let to = project.directory_for(kind).join(&name);

    let change = copy::replace(&from, &to).map_err(|source| InstallError::Write {
        path: to.clone(),
        source,
    })?;

    ledger.installed(
        &entry,
        kind,
        &name,
        Some(candidate.gathered.source.id),
        &route(project.root(), &to),
    )?;
    let detail = format!("{name} into {}", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    Ok(match change {
        copy::Change::Added => Installed::Added { name, path: to },
        copy::Change::Updated => Installed::Updated { name, path: to },
        copy::Change::Unchanged => Installed::Unchanged { name, path: to },
    })
}

/// Take one artifact back out of a project.
///
/// Only if the ledger says Mindflayer put it there. Anything else is reported
/// and left where it is.
pub fn uninstall(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
) -> Result<Removed, InstallError> {
    let entry = project_entry(workspace, project);
    let ours = ledger.installations(&entry, kind)?;
    if !ours.iter().any(|installed| installed == name) {
        let directory = project.directory_for(kind);
        return Ok(if holds(&directory, kind, name) {
            Removed::Foreign {
                name: name.to_owned(),
            }
        } else {
            Removed::Absent {
                name: name.to_owned(),
            }
        });
    }

    let path = project.directory_for(kind).join(name);
    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|source| InstallError::Write {
            path: path.clone(),
            source,
        })?;
    }
    ledger.uninstalled(&entry, kind, name)?;
    let detail = format!("{name} from {}", project.name());
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;

    Ok(Removed::Removed {
        name: name.to_owned(),
        path,
    })
}

/// What installing one artifact did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    Added {
        name: String,
        path: PathBuf,
    },
    Updated {
        name: String,
        path: PathBuf,
    },
    Unchanged {
        name: String,
        path: PathBuf,
    },
    /// Something of that name is already there and is not ours to replace.
    Foreign {
        name: String,
    },
}

/// What uninstalling one artifact did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    Removed {
        name: String,
        path: PathBuf,
    },
    /// Present, but nobody here put it there.
    Foreign {
        name: String,
    },
    /// Nothing of that name to remove.
    Absent {
        name: String,
    },
}

/// Whether a project's directory already holds an artifact of that name.
fn holds(directory: &Path, kind: Kind, name: &str) -> bool {
    match kind.layout() {
        crate::kind::Layout::Directory { manifest } => {
            directory.join(name).join(manifest).is_file()
        }
        crate::kind::Layout::Files { extension } => {
            directory.join(format!("{name}.{extension}")).is_file()
        }
    }
}

/// How the ledger names a project: relative to the workspace, the way a
/// registered project is stored, so moving the two together keeps it true.
fn project_entry(workspace: &FlayerWorkspace, project: &MindProject) -> String {
    route(workspace.root(), project.root())
}

/// A path below another, `/` separated, or the path itself when there is no
/// route between them.
fn route(base: &Path, target: &Path) -> String {
    paths::relative_to(target, base)
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| target.display().to_string())
}

/// Why an install or an uninstall could not happen at all.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(
        "{name}: the ledger has it at {path}, but nothing is there — gather {workspace} again"
    )]
    Missing {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
