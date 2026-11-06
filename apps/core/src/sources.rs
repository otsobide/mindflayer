//! A gathered source's own changes, each a source named as `gather list`
//! shows it and `--from` takes it (#100): renaming one, gathering some
//! again, and removing one with what it put on the shelf. Listing them is
//! the ledger's
//! ([`LedgerStore::source_listing`](crate::port::LedgerStore::source_listing)).
//!
//! A source is found by the name it is called, word for word, among those
//! the ledger keeps, never by its URL or its folder: the name is what a
//! listing shows, and one word a person types for one source (D-13).

use std::fs;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::code::codes;
use crate::gather::{self, GatherError, Report, Request, Watch};
use crate::install;
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError, Source};
use crate::lock::LockError;
use crate::value::{ArtifactName, Origin, SourceName};
use crate::workspace::{same_directory, MindProject, WeaverWorkspace, WorkspaceError};

/// The source called `name`, as the ledger records it.
///
/// Compared with each recorded name as it is, rather than held to
/// [`SourceName::parse`] first: a name an older build recorded is the
/// source's still, and a word no source is called is no source either way.
pub fn named(ledger: &Ledger, name: &str) -> Result<Source, SourceError> {
    ledger
        .sources()?
        .into_iter()
        .find(|source| source.name.as_str() == name)
        .ok_or_else(|| SourceError::NotFound {
            name: name.to_owned(),
        })
}

/// What [`rename`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// What it was called.
    pub was: SourceName,
    /// The source, called what it is called now.
    pub source: Source,
}

impl Renamed {
    /// Whether it was called that already, and nothing changed.
    pub fn unchanged(&self) -> bool {
        self.was == self.source.name
    }
}

/// Call the source `from` names `to` from now on, held to the rules a name
/// is held to when `gather git --name` gives one ([`SourceName::parse`]),
/// and refused with the same codes: what `gather list` shows of what it
/// supplied, and what `--from` takes for it, change with it.
///
/// The origin every install from it recorded stays its URL, as a rename by
/// `--name` leaves it, so what was installed is still found where it came
/// from. Called what it is called already, it changes nothing. Held under
/// the workspace's lock, from finding the source to renaming it, so two
/// renames to one name cannot both find it free.
pub fn rename(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    from: &str,
    to: &str,
) -> Result<Renamed, SourceError> {
    let new = SourceName::parse(to).ok_or_else(|| SourceError::NameInvalid {
        name: to.to_owned(),
    })?;
    let _lock = workspace.lock()?;
    let source = named(ledger, from)?;
    let was = source.name.clone();
    if new == was {
        return Ok(Renamed { was, source });
    }
    if let Some(holder) = ledger.source_named(&new)? {
        return Err(SourceError::NameTaken {
            name: new,
            origin: holder.origin(),
        });
    }
    ledger.rename_source(source.id, &new)?;
    Ok(Renamed {
        was,
        source: Source {
            name: new,
            ..source
        },
    })
}

/// Which sources to gather again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which<'a> {
    /// Every source the ledger keeps, in the order it lists them.
    Every,
    /// The sources these name, in the order they are named, each once.
    Named(&'a [String]),
}

/// What gathering one source again came to.
#[derive(Debug)]
pub struct Regathered {
    /// The source, as the ledger recorded it before this gather.
    pub source: Source,
    /// What was added, updated, removed and left as it was, or why it could
    /// not be gathered again: one source that fails is no reason not to
    /// gather the rest.
    pub outcome: Result<Report, GatherError>,
}

/// Gather each source `which` names again ([`Request::again`]), through
/// [`gather::gather`] as `gather git` does, each told to `watch` and
/// stopped when it is cancelled.
///
/// Every name is found before anything is gathered, and one no source is
/// called refuses the whole, as a typo should cost nothing it did not name.
/// Each source is a gather of its own, the workspace's lock held for each
/// in turn as `gather::gather` holds it, never across them all: a gather
/// is the network, and a writer kept waiting that long would give up. One
/// that fails is said in its own [`Regathered::outcome`] and the rest go
/// ahead; once one is cancelled, the rest are not begun, and are said as
/// cancelled too, each with nothing put on the shelf.
pub fn update(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    which: Which<'_>,
    watch: &Watch,
) -> Result<Vec<Regathered>, SourceError> {
    let sources = match which {
        Which::Every => ledger.sources()?,
        Which::Named(names) => {
            let mut found: Vec<Source> = Vec::with_capacity(names.len());
            for name in names {
                let source = named(ledger, name)?;
                if !found.iter().any(|seen| seen.id == source.id) {
                    found.push(source);
                }
            }
            found
        }
    };
    let mut updated = Vec::with_capacity(sources.len());
    let mut cancelled = false;
    for source in sources {
        let outcome = if cancelled {
            Err(GatherError::Cancelled)
        } else {
            gather::gather(
                workspace,
                ledger,
                &Request::again(&source).watched_by(watch.clone()),
            )
        };
        cancelled = matches!(outcome, Err(GatherError::Cancelled));
        updated.push(Regathered { source, outcome });
    }
    Ok(updated)
}

/// One installation of what a source supplied, in a project the workspace
/// links: what removing the source leaves there, recorded and orphaned.
#[derive(Debug)]
pub struct FromIt {
    pub project: MindProject,
    pub kind: Kind,
    pub name: ArtifactName,
}

/// What removing a source would do ([`plan_removal`]), or did ([`remove`]).
#[derive(Debug)]
pub struct Removal {
    /// The source, as the ledger recorded it.
    pub source: Source,
    /// How many artifacts it holds on the shelf, whose records go with it.
    pub artifacts: usize,
    /// Its folders that are there, which removing it deletes: its clone
    /// under the cache, then its folder on the shelf. Nothing else is ever
    /// among them ([`SourceError::FolderNotItsOwn`]).
    pub folders: Vec<PathBuf>,
    /// What the projects the workspace links installed from it, in the
    /// order it links them: each keeps its record and its copies, which
    /// nothing offers once the source is gone, so each reads as orphaned.
    pub installed: Vec<FromIt>,
    /// Each project the workspace links that cannot be opened: what it
    /// installed from the source cannot be told, so it is said beside the
    /// rest.
    pub unread: Vec<WorkspaceError>,
}

/// What removing the source called `name` would do: the folders it would
/// delete and what the projects the workspace links installed from it,
/// nothing written.
pub fn plan_removal(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    name: &str,
) -> Result<Removal, SourceError> {
    let source = named(ledger, name)?;
    let folders = folders_of(workspace, &source)?;
    let artifacts = ledger
        .gathered()?
        .iter()
        .filter(|gathered| gathered.source.id == source.id)
        .count();
    let (projects, unread) = workspace.projects();
    let mut installed = Vec::new();
    let mut seen: Vec<MindProject> = Vec::new();
    for project in projects {
        // A project linked twice, by two spellings of its folder, is one
        // project's copies, said once.
        if seen
            .iter()
            .any(|other| same_directory(other.root(), project.root()))
        {
            continue;
        }
        let entry = install::entry_for_root(workspace, project.root());
        let mut from_it = Vec::new();
        for kind in Kind::ALL {
            for record in ledger.records(&entry, kind)? {
                if record.source_id == Some(source.id) {
                    from_it.push((kind, record.name));
                }
            }
        }
        installed.extend(from_it.into_iter().map(|(kind, name)| FromIt {
            project: project.clone(),
            kind,
            name,
        }));
        seen.push(project);
    }
    Ok(Removal {
        source,
        artifacts,
        folders,
        installed,
        unread,
    })
}

/// Remove the source called `name`: its clone, its folder on the shelf and
/// its rows, and with them the rows of what it supplied
/// ([`plan_removal`], done).
///
/// Planned again under the workspace's lock, which is held until its rows
/// are gone, so a gather of it cannot put on the shelf what this is taking
/// off. Only the folders the ledger records as the source's own are
/// deleted, each whole and never followed through a link; a source whose
/// recorded folder is not one folder name is refused, nothing deleted. The
/// clone goes first, which every gather makes again, then the shelf, and
/// the rows only once both are gone: a folder that will not go refuses the
/// rest, its rows kept, so the source is still listed and a removal tried
/// again finds what is left, where rows dropped first would leave a folder
/// on the shelf that `validate` reads and nothing knows to remove.
///
/// What the projects installed from it keeps its record, its origin
/// included, and no longer points at the source, as the ledger forgets a
/// source ([`forget_source`](crate::port::LedgerStore::forget_source)):
/// nothing offers it there any more, so `status` says it is orphaned, an
/// update leaves it, and `uninstall --orphaned` takes it out.
pub fn remove(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    name: &str,
) -> Result<Removal, SourceError> {
    let _lock = workspace.lock()?;
    let removal = plan_removal(workspace, ledger, name)?;
    for folder in &removal.folders {
        match fs::remove_dir_all(folder) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(SourceError::Unremoved {
                    name: removal.source.name.clone(),
                    path: folder.clone(),
                    source: error,
                })
            }
        }
    }
    ledger.forget_source(removal.source.id)?;
    Ok(removal)
}

/// The folders `source` has on the disk, each that is there: its clone
/// under the cache and its folder on the shelf, by the folder name the
/// ledger records for it, and nothing else.
///
/// Refused unless that name is one folder name: anything else would name a
/// folder that is not the source's to delete, `..` the cache itself. The
/// shelf's folder of each kind a gather fills, matched on the kind as the
/// refusal in a gather is, so a third kind gathered says whether a source
/// has a folder of it.
fn folders_of(workspace: &WeaverWorkspace, source: &Source) -> Result<Vec<PathBuf>, SourceError> {
    if !install::is_folder_name(&source.directory) {
        return Err(SourceError::FolderNotItsOwn {
            name: source.name.clone(),
            directory: source.directory.clone(),
        });
    }
    let mut folders = vec![workspace.cache_dir().join(&source.directory)];
    for kind in Kind::ALL {
        match kind {
            Kind::Skill => folders.push(workspace.gathered_dir(kind).join(&source.directory)),
            Kind::Rule | Kind::Mcp => {}
        }
    }
    folders.retain(|folder| folder.symlink_metadata().is_ok());
    Ok(folders)
}

/// Why a change to a source could not be made, with nothing changed.
#[derive(Debug, Error)]
pub enum SourceError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// No source is called that.
    #[error("no source is called `{name}`: `mindweaver gather sources` lists them by name")]
    NotFound { name: String },
    /// What no source can be called ([`SourceName::parse`]): the condition
    /// `gather git --name` meets, said for a rename.
    #[error(
        "`{name}` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    )]
    NameInvalid { name: String },
    /// Another source is called that already, at `origin`: the condition
    /// `gather git --name` meets, said for a rename.
    #[error("`{name}` is the name of the source at {origin} already: rename this one to another")]
    NameTaken { name: SourceName, origin: Origin },
    /// The folder the ledger records for the source is not one folder
    /// name, so what removing it would delete is no folder of its own.
    #[error(
        "the source `{name}` is recorded with the folder `{directory}`, which is no one folder of \
         the shelf or the cache, so nothing is removed"
    )]
    FolderNotItsOwn { name: SourceName, directory: String },
    /// One of its folders would not go: what was deleted before it is
    /// gone, and the source is kept, with its rows.
    #[error(
        "{path}: cannot be removed, so the source `{name}` is kept, with what is left of it: \
         {source}"
    )]
    Unremoved {
        name: SourceName,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(SourceError {
    Self::NotFound { .. } => "source_not_found",
    Self::FolderNotItsOwn { .. } => "source_folder_not_its_own",
    Self::Unremoved { .. } => "source_folder_unremoved",
} carried {
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
} echoed {
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
});

#[cfg(test)]
#[path = "sources_test.rs"]
mod tests;
