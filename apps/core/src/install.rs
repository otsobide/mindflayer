//! Putting an artifact from the workspace shelf into a mind project, and
//! taking it back out.
//!
//! Three places offer what can be installed: the workspace's own artifacts,
//! the own artifacts of every workspace it loads (read where they are), and
//! the shelf gathering fills. This is the other half. It writes into a mind
//! project — the only thing in Mindflayer that does — and the rule it works by
//! is that **it only manages what it installed**. An artifact somebody wrote by
//! hand is neither overwritten nor deleted, whatever a caller asks for, and it
//! is told so rather than being obeyed quietly. Which of the two a file is, is
//! what [`crate::ledger`] remembers.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::catalog::{Catalog, DiscoveryFailure};
use crate::copy;
use crate::kind::{Kind, Layout};
use crate::ledger::{Action, Gathered, Ledger, LedgerError, Outcome};
use crate::paths;
use crate::workspace::{FlayerWorkspace, MindProject, WorkspaceError};

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

/// What `origin` says for the workspace's own artifacts.
pub const WORKSPACE_ORIGIN: &str = "workspace";

/// Where something that can be installed would be copied from.
#[derive(Debug, Clone)]
pub enum Offer {
    /// Gathered from a source onto the shelf.
    Shelf(Gathered),
    /// One of the workspace's own artifacts, which it keeps to share with the
    /// projects it manages.
    Workspace {
        kind: Kind,
        name: String,
        summary: Option<String>,
        /// The directory it is, wherever the workspace keeps it.
        path: PathBuf,
    },
    /// One of the own artifacts of a workspace this one loads, read where
    /// that workspace keeps it rather than from a copy.
    Loaded {
        kind: Kind,
        name: String,
        summary: Option<String>,
        /// The directory it is, inside the loaded workspace.
        path: PathBuf,
        /// The `loaded` entry it came through, as the marker spells it.
        entry: String,
        /// The loaded workspace's declared name.
        workspace: String,
    },
}

impl Offer {
    pub fn name(&self) -> &str {
        match self {
            Offer::Shelf(gathered) => &gathered.name,
            Offer::Workspace { name, .. } | Offer::Loaded { name, .. } => name,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Offer::Shelf(gathered) => gathered.kind,
            Offer::Workspace { kind, .. } | Offer::Loaded { kind, .. } => *kind,
        }
    }

    /// The one line a listing shows.
    pub fn summary(&self) -> Option<&str> {
        match self {
            Offer::Shelf(gathered) => gathered.summary.as_deref(),
            Offer::Workspace { summary, .. } | Offer::Loaded { summary, .. } => summary.as_deref(),
        }
    }

    /// Where it came from, as one string: [`WORKSPACE_ORIGIN`] for the
    /// workspace's own; for a gathered one its source's address, with the
    /// branch when one was asked for; for a loaded one its entry after
    /// [`LOADED_PREFIX`] — `load:../team` — so it can be neither the word the
    /// workspace's own go by nor the address of something gathered, even from
    /// that same directory.
    ///
    /// One string for every front end: what a listing prints is what
    /// `--from` accepts.
    pub fn origin(&self) -> String {
        match self {
            Offer::Shelf(gathered) => match &gathered.source.reference {
                Some(reference) => format!("{}#{reference}", gathered.source.url),
                None => gathered.source.url.clone(),
            },
            Offer::Workspace { .. } => String::from(WORKSPACE_ORIGIN),
            Offer::Loaded { entry, .. } => format!("{LOADED_PREFIX}{entry}"),
        }
    }

    /// Whether `from`, as somebody typed it, names where this came from: the
    /// origin as printed; a source's address without its branch; a loaded
    /// workspace's name, unless that is the word the workspace's own go by.
    /// A path to a loaded workspace is [`resolve_from`]'s to turn into its
    /// origin — only when nothing matches it as typed, so a path never takes
    /// a shelf source's address or the workspace's own away.
    pub fn comes_from(&self, from: &str) -> bool {
        self.origin() == from
            || match self {
                Offer::Shelf(gathered) => gathered.source.url == from,
                Offer::Loaded { workspace, .. } => from != WORKSPACE_ORIGIN && workspace == from,
                Offer::Workspace { .. } => false,
            }
    }

    /// The directory it is copied from.
    fn source(&self, workspace: &FlayerWorkspace) -> PathBuf {
        match self {
            Offer::Shelf(gathered) => workspace.root().join(&gathered.path),
            Offer::Workspace { path, .. } | Offer::Loaded { path, .. } => path.clone(),
        }
    }
}

/// What a loaded workspace's origin starts with.
pub const LOADED_PREFIX: &str = "load:";

/// `from` as `--from` should match it: when it is a path, typed relative to
/// `base` — bare or after `load:` — to a workspace `workspace` loads, that
/// workspace's origin; otherwise `from` as it is, and always for the word
/// the workspace's own go by. So the path given to `flayer load` works as
/// `--from` from wherever it was typed, not only spelled as the marker
/// stores it.
pub fn resolve_from(workspace: &FlayerWorkspace, base: &Path, from: &str) -> String {
    if from == WORKSPACE_ORIGIN {
        return from.to_owned();
    }
    // `load:` with any spelling of the path after it means the same load.
    let typed = from.strip_prefix(LOADED_PREFIX).unwrap_or(from);
    let path = paths::normalize(&base.join(typed));
    workspace
        .loads()
        .into_iter()
        .find(|loaded| paths::normalize(&loaded.root) == path || same_place(&loaded.root, &path))
        .and_then(|loaded| paths::to_config_string(&loaded.entry))
        .map_or_else(
            || from.to_owned(),
            |entry| format!("{LOADED_PREFIX}{entry}"),
        )
}

/// Whether a declared name is one plain folder name, and so safe to join onto
/// a project's folder. A name is read from somebody's `SKILL.md`; `..`, an
/// absolute path or `a/b` would put the copy — and the deletion that
/// replacing it starts with — somewhere else entirely.
pub fn is_folder_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    ) && !name.contains(['/', '\\'])
}

/// One thing that can be installed, seen from one project.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub offer: Offer,
    pub standing: Standing,
}

impl Candidate {
    pub fn name(&self) -> &str {
        self.offer.name()
    }

    pub fn kind(&self) -> Kind {
        self.offer.kind()
    }

    /// The one line a listing shows.
    pub fn summary(&self) -> Option<&str> {
        self.offer.summary()
    }

    /// See [`Offer::origin`].
    pub fn origin(&self) -> String {
        self.offer.origin()
    }

    /// See [`Offer::comes_from`].
    pub fn comes_from(&self, from: &str) -> bool {
        self.offer.comes_from(from)
    }

    /// The directory it is copied from.
    fn source(&self, workspace: &FlayerWorkspace) -> PathBuf {
        self.offer.source(workspace)
    }
}

/// What the workspaces a workspace loads offer, and what stops them offering
/// more: [`offered_by_loads`].
#[derive(Debug, Default)]
pub struct Loads {
    pub offers: Vec<Offer>,
    pub failures: Vec<LoadFailure>,
}

/// The own artifacts of `kind` of every workspace `workspace` loads, in the
/// order its marker lists them. What could not be offered is dropped here;
/// [`offered_by_loads`] has it, for a front end to warn with.
pub fn loaded_offers(workspace: &FlayerWorkspace, kind: Kind) -> Vec<Offer> {
    from_loads(workspace, &[kind]).offers
}

/// Every installable artifact the workspaces `workspace` loads offer, and
/// every reason something was not offered: a loaded workspace that cannot be
/// opened, an artifact in one that cannot be read, a name that is not one
/// folder name. Failures collected, not raised, like every discovery.
pub fn offered_by_loads(workspace: &FlayerWorkspace) -> Loads {
    from_loads(workspace, &Kind::ALL)
}

/// The two above. No ledger: listing what a load offers is reading, and must
/// not create the database a workspace that never gathered does not have.
/// Only a loaded workspace's own, never what it loads in turn; all of its own,
/// even kinds it keeps in one folder with a project of its, since that project
/// is not what is being offered; and each loaded directory once, however many
/// spellings of it the marker has.
fn from_loads(workspace: &FlayerWorkspace, kinds: &[Kind]) -> Loads {
    let kinds: Vec<Kind> = kinds
        .iter()
        .copied()
        .filter(|kind| matches!(kind.layout(), Layout::Directory { .. }))
        .collect();
    let mut loads = Loads::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    for loaded in workspace.loads() {
        if seen
            .iter()
            .any(|root| *root == loaded.root || same_place(root, &loaded.root))
        {
            continue;
        }
        seen.push(loaded.root.clone());
        let source = match loaded.workspace {
            Ok(source) => source,
            Err(error) => {
                loads.failures.push(LoadFailure::Workspace(error));
                continue;
            }
        };
        if kinds.is_empty() {
            continue;
        }
        let entry = paths::to_config_string(&loaded.entry)
            .unwrap_or_else(|| loaded.entry.display().to_string());
        let own = source.own();
        let (artifacts, failures) =
            Catalog::discover_kinds(std::slice::from_ref(&own), &kinds).into_parts();
        loads
            .failures
            .extend(failures.into_iter().map(LoadFailure::Artifact));
        for artifact in artifacts {
            if !is_folder_name(artifact.name()) {
                loads.failures.push(LoadFailure::UnsafeName {
                    path: artifact.path().to_path_buf(),
                    name: artifact.name().to_owned(),
                });
                continue;
            }
            loads.offers.push(Offer::Loaded {
                kind: artifact.kind(),
                name: artifact.name().to_owned(),
                summary: artifact.summary().map(str::to_owned),
                path: artifact.path().to_path_buf(),
                entry: entry.clone(),
                workspace: source.name().to_owned(),
            });
        }
    }
    loads
}

/// Why something a loaded workspace has is not offered.
#[derive(Debug, Error)]
pub enum LoadFailure {
    #[error(transparent)]
    Workspace(WorkspaceError),
    #[error(transparent)]
    Artifact(DiscoveryFailure),
    #[error(
        "{path}: declares the name `{name}`, which is not one folder name, so it is not offered"
    )]
    UnsafeName { path: PathBuf, name: String },
}

/// Everything of `kind` that can be installed, and how each stands with
/// `project`: the workspace's own first, then what the workspaces it loads
/// offer, then the shelf. A loaded workspace that cannot be opened offers
/// nothing here; [`offered_by_loads`] says why, for a front end to warn with.
/// Nothing whose name is not one folder name is offered, from anywhere.
///
/// That is the list of what can be installed, so an artifact a project holds
/// that is in neither does not appear: it is not a thing this command can
/// offer to do anything about.
pub fn survey(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<Candidate>, InstallError> {
    let entry = project_entry(workspace, project);
    let ours = ledger.installations(&entry, kind)?;
    let directory = project.directory_for(kind);

    // The workspace's own come first: what it keeps for its projects before
    // what it gathered from elsewhere. Only kinds that are a directory each,
    // because installing copies a directory — the same line the shelf draws.
    let mut offers = Vec::new();
    let installable = matches!(kind.layout(), Layout::Directory { .. });
    if installable && workspace.own_kinds().contains(&kind) {
        let own = workspace.own();
        let catalog = Catalog::discover_kinds(std::slice::from_ref(&own), &[kind]);
        offers.extend(catalog.artifacts().iter().map(|artifact| Offer::Workspace {
            kind,
            name: artifact.name().to_owned(),
            summary: artifact.summary().map(str::to_owned),
            path: artifact.path().to_path_buf(),
        }));
    }
    offers.extend(loaded_offers(workspace, kind));
    offers.extend(
        ledger
            .gathered()?
            .into_iter()
            .filter(|gathered| gathered.kind == kind)
            .map(Offer::Shelf),
    );

    let candidates = offers
        .into_iter()
        .filter(|offer| is_folder_name(offer.name()))
        .map(|offer| {
            let mut candidate = Candidate {
                offer,
                standing: Standing::Absent,
            };
            let name = candidate.name();
            candidate.standing = if !holds(&directory, kind, name) {
                Standing::Absent
            } else if ours.iter().any(|installed| installed == name) {
                Standing::Installed
            } else {
                Standing::Foreign
            };
            candidate
        })
        .collect();
    Ok(candidates)
}

/// The one candidate called `name` that `from` names, among what `project`
/// can be offered.
///
/// `from` is needed only when two places offer the same name: a project has
/// one directory of that name, and which of the two goes in it is not
/// something to guess.
pub fn offered(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    from: Option<&str>,
) -> Result<Candidate, InstallError> {
    let mut matching: Vec<Candidate> = survey(workspace, ledger, project, kind)?
        .into_iter()
        .filter(|candidate| candidate.name() == name)
        .filter(|candidate| from.is_none_or(|from| candidate.comes_from(from)))
        .collect();
    match matching.len() {
        0 => Err(InstallError::NotOffered {
            name: name.to_owned(),
            from: from.map(str::to_owned),
        }),
        1 => Ok(matching.remove(0)),
        _ => Err(InstallError::OfferedTwice {
            name: name.to_owned(),
            origins: matching.iter().map(Candidate::origin).collect(),
        }),
    }
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

    if !is_folder_name(&name) {
        return Err(InstallError::UnsafeName { name });
    }
    if candidate.standing == Standing::Foreign {
        return Ok(Installed::Foreign { name });
    }

    let from = candidate.source(workspace);
    if !from.is_dir() {
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
    // The folder is named after the artifact, not after the folder it had on
    // the shelf: inside a project a skill's directory has to match its
    // declared name, which is what `validate` checks and what an agent uses to
    // find it.
    let to = project.directory_for(kind).join(&name);

    // A loaded workspace can be the project itself, keeping its own in the
    // project's folder: then what would be copied is what is already there.
    // Nothing is written, and above all nothing is recorded — a record would
    // let `uninstall` delete the loaded workspace's own files.
    if same_place(&from, &to) {
        return Ok(Installed::Unchanged { name, path: to });
    }
    // One inside the other is worse: replacing `to` starts by deleting it,
    // and with it `from` — or copies `from` into itself.
    if overlaps(&from, &to) {
        return Err(InstallError::Overlaps { from, to });
    }
    // And never over what a loaded workspace keeps as its own: a project can
    // be the workspace it loads, sharing a folder, and a record from before
    // the load would otherwise let this overwrite it.
    if let Some(loaded) = kept_by_a_load(workspace, kind, &to) {
        return Err(InstallError::KeptByALoad { path: to, loaded });
    }

    // Links are followed only inside where the artifact comes from: the
    // workspace that keeps it, or the shelf folder it was gathered into.
    let within = match &candidate.offer {
        Offer::Loaded { entry, .. } => paths::normalize(&workspace.root().join(entry)),
        Offer::Workspace { .. } => workspace.root().to_path_buf(),
        Offer::Shelf(_) => from.clone(),
    };
    let change = copy::replace(&from, &to, &within).map_err(|source| InstallError::Write {
        path: to.clone(),
        source,
    })?;

    // A source to point at only for what was gathered: the workspace's own
    // came from nowhere but the workspace.
    let source_id = match &candidate.offer {
        Offer::Shelf(gathered) => Some(gathered.source.id),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    };
    ledger.installed(&entry, kind, &name, source_id, &route(project.root(), &to))?;
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
    if !is_folder_name(name) {
        return Err(InstallError::UnsafeName {
            name: name.to_owned(),
        });
    }
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
    if let Some(loaded) = kept_by_a_load(workspace, kind, &path) {
        return Err(InstallError::KeptByALoad { path, loaded });
    }
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

/// Drop the record that Mindflayer installed `name` into `project`, returning
/// whether there was one.
///
/// For when what is in the project stops being what was installed: it was
/// removed, renamed or edited by hand, or something new was written under a
/// name an old record still claims. The record is what gives the install
/// screen leave to overwrite and delete, so it must not outlive the copy it
/// was about — whatever somebody writes there next would inherit it.
///
/// The workspace is found by walking up from the project, so one that links
/// the project from somewhere that is not above it is not consulted.
pub fn disown(project: &MindProject, kind: Kind, name: &str) -> Result<bool, InstallError> {
    let Some(workspace) = FlayerWorkspace::locate(project.root())? else {
        return Ok(false);
    };
    // Only a workspace that manages the project keeps records about it, and
    // one that has never installed anything has no ledger: this must not
    // create one just to look.
    if !workspace.is_linked(project.root()) || !workspace.ledger_path().is_file() {
        return Ok(false);
    }
    let ledger = workspace.ledger()?;
    let entry = project_entry(&workspace, project);
    let dropped = ledger.uninstalled(&entry, kind, name)?;
    if dropped {
        let detail = format!(
            "{name} from {}, which no longer holds the copy that was installed",
            project.name()
        );
        ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;
    }
    Ok(dropped)
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
        // Anything by that name, not only a finished artifact: a folder of
        // notes somebody started by hand is theirs, and installing over it
        // would delete them.
        crate::kind::Layout::Directory { .. } => directory.join(name).exists(),
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

/// Whether two paths are one directory on disk.
fn same_place(a: &Path, b: &Path) -> bool {
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Whether one of two paths is inside the other. `to` need not exist yet, so
/// it is resolved through the nearest ancestor that does.
fn overlaps(from: &Path, to: &Path) -> bool {
    match (fs::canonicalize(from), resolved(to)) {
        (Ok(from), Some(to)) => from.starts_with(&to) || to.starts_with(&from),
        _ => false,
    }
}

/// A path made canonical as far as it exists, with the rest appended.
fn resolved(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(real) = fs::canonicalize(existing) {
            return Some(
                missing
                    .iter()
                    .rev()
                    .fold(real, |path, part| path.join(part)),
            );
        }
        missing.push(existing.file_name()?.to_owned());
        existing = existing.parent()?;
    }
}

/// The loaded workspace that keeps its own of `kind` in a folder `path` is
/// in, or around, if one does — spelled as its root, which `flayer unload`
/// accepts from anywhere.
///
/// By the folder, not by what is in it yet: a project that is also a loaded
/// workspace keeping both in one folder has nothing in it Mindflayer may
/// write or remove, from the first install on — otherwise an install today
/// is the loaded workspace's own tomorrow, and can never be updated again.
fn kept_by_a_load(workspace: &FlayerWorkspace, kind: Kind, path: &Path) -> Option<PathBuf> {
    let at = resolved(path)?;
    workspace.loads().into_iter().find_map(|loaded| {
        let source = loaded.workspace.ok()?;
        let kept = fs::canonicalize(source.own().directory_for(kind)).ok()?;
        (at.starts_with(&kept) || kept.starts_with(&at)).then_some(loaded.root)
    })
}

/// Whether `candidate` is the copy `project` holds: the same names and bytes
/// as the folder there. For telling which of two offers of one name is the
/// one installed, which the ledger does not record for what came from a
/// workspace rather than the shelf.
pub fn is_current(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    candidate: &Candidate,
) -> bool {
    let at = project
        .directory_for(candidate.kind())
        .join(candidate.name());
    at.is_dir() && copy::same(&candidate.source(workspace), &at).unwrap_or(false)
}

/// Why an install or an uninstall could not happen at all.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(
        "{name}: the ledger has it at {path}, but nothing is there — gather {workspace} again"
    )]
    Missing {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("{name}: {path} is not there any more — the workspace loaded from {workspace} has changed; `flayer unload {workspace}` if it has gone")]
    Gone {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("`{name}` is not one folder name, so it cannot be installed or removed")]
    UnsafeName { name: String },
    #[error("{from} and {to} are one inside the other, so installing one onto the other would delete it")]
    Overlaps { from: PathBuf, to: PathBuf },
    #[error("{path} is where the workspace loaded from {loaded} keeps its own, so Mindflayer neither installs nor removes there — `flayer unload {loaded}` first to manage it as a project")]
    KeptByALoad { path: PathBuf, loaded: PathBuf },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("nothing called `{name}` can be installed{}: gather it, load a workspace that has it with `flayer load`, or add it to the workspace with `flayer add`", from.as_deref().map(|from| format!(" from {from}")).unwrap_or_default())]
    NotOffered { name: String, from: Option<String> },
    #[error("`{name}` is offered by more than one place — {} — so say which with --from", .origins.join(", "))]
    OfferedTwice { name: String, origins: Vec<String> },
}
