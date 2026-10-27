//! Putting an artifact from the workspace shelf into a mind project, and
//! taking it back out.
//!
//! Three places offer what can be installed: the workspace's own artifacts,
//! the own artifacts of every workspace it loads (read where they are), and
//! the shelf gathering fills. This is the other half. It writes into a mind
//! project, the only thing in Mindflayer that does, and the rule it works by
//! is that **it only manages what it installed**. An artifact somebody wrote by
//! hand is neither overwritten nor deleted, whatever a caller asks for, and it
//! is told so rather than being obeyed quietly. Which of the two a file is, is
//! what [`crate::ledger`] remembers.
//!
//! Where in the project is its agents' to say: a copy in each folder one of
//! them reads ([`targets`]), written together and recorded as one
//! installation, so that every agent the project names runs the same skill.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, DiscoveryFailure};
use crate::copy;
use crate::exclude::{self, ExcludeError, Excluded};
use crate::gather::Scrub;
use crate::hash;
use crate::kind::{Kind, Layout};
use crate::ledger::{
    Action, Forgotten, Gathered, InstallRecord, Ledger, LedgerError, Outcome, Target,
};
use crate::paths;
use crate::settings::Share;
use crate::url::redacted;
use crate::workspace::{same_directory, FlayerWorkspace, MindProject, WorkspaceError};

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
    /// [`LOADED_PREFIX`] (`load:../team`), so it can be neither the word the
    /// workspace's own go by nor the address of something gathered, even from
    /// that same directory.
    ///
    /// One string for every front end: what a listing prints is what
    /// `--from` accepts. A source's address carries no credential, because
    /// the ledger hands none back.
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
    /// origin, and only when nothing matches it as typed, so a path never takes
    /// a shelf source's address or the workspace's own away. An address typed
    /// with credentials names the source they were taken out of.
    pub fn comes_from(&self, from: &str) -> bool {
        let from = redacted(from);
        let from = from.as_ref();
        self.origin() == from
            || match self {
                Offer::Shelf(gathered) => gathered.source.url == from,
                Offer::Loaded { workspace, .. } => from != WORKSPACE_ORIGIN && workspace == from,
                Offer::Workspace { .. } => false,
            }
    }

    /// The directory it is copied from.
    pub(crate) fn source(&self, workspace: &FlayerWorkspace) -> PathBuf {
        match self {
            Offer::Shelf(gathered) => workspace.root().join(&gathered.path),
            Offer::Workspace { path, .. } | Offer::Loaded { path, .. } => path.clone(),
        }
    }
}

/// What a loaded workspace's origin starts with.
pub const LOADED_PREFIX: &str = "load:";

/// `from` as `--from` should match it: when it is a path, typed relative to
/// `base` (bare or after `load:`) to a workspace `workspace` loads, that
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
/// absolute path or `a/b` would put the copy (and the deletion that
/// replacing it starts with) somewhere else entirely.
pub fn is_folder_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    ) && !name.contains(['/', '\\'])
}

/// What an install leaves out of every copy it writes, from the top of the
/// artifact, and why: a `.claude-plugin` folder, by which Claude would load
/// the skill as a plugin, with whatever hooks and MCP servers it declares
/// (docs-only, in `docs/agents.md`, so it is left out whatever Claude does
/// with it).
///
/// A skill is instructions an agent reads when it chooses to; a plugin's
/// hooks run commands on their own and its servers are started for it, which
/// installing a skill must not turn on behind anybody's back. Left out of
/// every install, whichever agents the project names: an agent folder an
/// override names (#40) may be Claude's too. Only at the top, where Claude
/// looks for one: a skill that teaches plugin writing can ship one further
/// down as an example, and it is the skill's like any other file.
const LEAVE_OUT: [(&str, &str); 1] = [(
    ".claude-plugin",
    "Claude would load the skill as a plugin, with the hooks and MCP servers it declares",
)];

/// The names [`LEAVE_OUT`] leaves out, as the copy takes them.
pub(crate) fn left_out_names() -> [&'static str; LEAVE_OUT.len()] {
    LEAVE_OUT.map(|(name, _)| name)
}

/// Something an offer holds that an install left out of every copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// Where it is in the offer.
    pub path: PathBuf,
    /// Why no copy holds it, as a clause a front end can say after its name.
    pub reason: &'static str,
}

/// One folder of a project's that an install of a kind puts a copy in, under
/// the artifact's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    /// The folder, under the project's root, whether or not it is there yet.
    pub folder: PathBuf,
    /// The agents the copy is for, in report order: none for the folder a
    /// `mind.toml` names, which is no one agent's.
    pub agents: Vec<Agent>,
}

impl InstallTarget {
    /// The agent the ledger says a copy here is for ([`agent_of`]).
    fn agent(&self) -> Option<String> {
        agent_of(&self.agents)
    }
}

/// The agent the ledger says a copy for `agents` is for: the one agent it is
/// for, or none for a copy two agents share, or no agent owns.
pub(crate) fn agent_of(agents: &[Agent]) -> Option<String> {
    match agents {
        [agent] => Some(agent.name().to_owned()),
        _ => None,
    }
}

/// Where an install of `kind` into `project` puts a copy: each folder the
/// target table sends one of the agents its settings name to
/// ([`MindProject::targets`], D-07), or, for a project a `mind.toml` marks,
/// which names no agents, the one folder that marker names for it.
///
/// Not a project's own folder from its settings' `directories`, which is
/// what it authors in (D-06): what the workspace offers goes where the agents
/// read, and what the project writes itself stays where it writes it.
///
/// Two targets that are one folder on disk, `.claude/skills` linked to
/// `.agents/skills` so that Claude and Codex share it, are one target, under
/// the first spelling and for the agents of both, as discovery reads them
/// once: two copies put in place in one folder would find each other there.
///
/// Refused when there is nowhere to put one: a settings file naming no agent
/// that is sent `kind` (`NoTarget`), or a `mind.toml` naming no folder for it
/// (`NoDirectory`).
pub fn targets(project: &MindProject, kind: Kind) -> Result<Vec<InstallTarget>, InstallError> {
    if project.settings().is_none() {
        return Ok(vec![InstallTarget {
            folder: project.required_directory(kind)?,
            agents: Vec::new(),
        }]);
    }
    let mut targets: Vec<InstallTarget> = Vec::new();
    for target in project.targets(kind) {
        let folder = project.root().join(&target.path);
        match targets
            .iter_mut()
            .find(|seen| same_directory(&seen.folder, &folder))
        {
            Some(seen) => {
                seen.agents.extend(target.agents);
                seen.agents.sort();
                seen.agents.dedup();
            }
            None => targets.push(InstallTarget {
                folder,
                agents: target.agents,
            }),
        }
    }
    if targets.is_empty() {
        return Err(InstallError::NoTarget {
            project: project.name().to_owned(),
            kind,
        });
    }
    Ok(targets)
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
///
/// A project with nowhere to put `kind` ([`targets`]) is offered nothing,
/// rather than refused: nothing can be put there, and the screen surveys
/// every project a workspace links, where a refusal for one would hide the
/// rest. Asking for one by name ([`offered`]) is refused.
pub fn survey(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<Candidate>, InstallError> {
    let Ok(targets) = targets(project, kind) else {
        return Ok(Vec::new());
    };
    let entry = project_entry(workspace, project);
    let recorded = ledger.copies(&entry, kind)?;

    // The workspace's own come first: what it keeps for its projects before
    // what it gathered from elsewhere. Only kinds that are a directory each,
    // because installing copies a directory: the same line the shelf draws.
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
            candidate.standing =
                standing(project.root(), &targets, kind, candidate.name(), &recorded);
            candidate
        })
        .collect();
    Ok(candidates)
}

/// The one candidate called `name` that `from` names, among what `project`
/// can be offered.
///
/// `from` is needed only when two places offer the same name: a project has
/// one directory of that name in each folder it is installed in, and which
/// of the two goes there is not something to guess.
///
/// A project with nowhere to put `kind` ([`targets`]) is refused here, where
/// [`survey`] offers it nothing: "not offered" would send somebody to gather
/// what could never be installed there anyway.
pub fn offered(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    from: Option<&str>,
) -> Result<Candidate, InstallError> {
    targets(project, kind)?;
    let mut matching: Vec<Candidate> = survey(workspace, ledger, project, kind)?
        .into_iter()
        .filter(|candidate| candidate.name() == name)
        .filter(|candidate| from.is_none_or(|from| candidate.comes_from(from)))
        .collect();
    match matching.len() {
        0 => Err(InstallError::NotOffered {
            name: name.to_owned(),
            from: from.map(|from| redacted(from).into_owned()),
        }),
        1 => Ok(matching.remove(0)),
        _ => Err(InstallError::OfferedTwice {
            name: name.to_owned(),
            origins: matching.iter().map(Candidate::origin).collect(),
        }),
    }
}

/// Copy one shelf entry into a project: a copy in each of its [`targets`],
/// recorded as one installation with a target for each.
///
/// The copies are built in the project's scratch folder and put in place
/// together, all of them or none (`copy::Stage::place`), so an agent that
/// reads two of them never finds two versions of one skill. What an
/// interrupted install left there, or beside a copy, is cleared first and
/// said ([`Installation::swept`]).
///
/// `homes` are the folders the agents read for every project, which the
/// front end decides: a copy that would land inside one of them is refused
/// ([`InstallError::UserScope`]).
pub fn install(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
) -> Result<Installation, InstallError> {
    let kind = candidate.kind();
    let name = candidate.name().to_owned();
    let entry = project_entry(workspace, project);

    if !is_folder_name(&name) {
        return Err(InstallError::UnsafeName { name });
    }
    // The folder is named after the artifact, not after the folder it had on
    // the shelf: inside a project a skill's directory has to match its
    // declared name, which is what `validate` checks and what an agent uses to
    // find it.
    let targets = targets(project, kind)?;
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, &name))
        .collect();
    // A project at the user's home has its agents' own folders for targets:
    // `.claude/skills` there is Claude's skills for every project, and
    // `.agents/skills` is the one Codex and OpenCode read everywhere. That is
    // the user-scope install D-22 leaves for later, behind a `--user` of its
    // own, so it is refused here rather than done by accident. The targets
    // cannot tell, being relative to the project, and core has no home to
    // compare them with: the front end hands it the agents' folders.
    if let Some(error) = user_scope(homes, &copies) {
        return Err(error);
    }
    // Somebody's work in any of them refuses the whole install: one copy
    // written beside it would leave the agents reading both folders two
    // different skills of one name. Looked at before the lock, so a refusal
    // waits for nobody, and again under it below.
    if candidate.standing == Standing::Foreign {
        let recorded = ledger.copies(&entry, kind)?;
        if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
            return Ok(Installed::Foreign { name, path }.into());
        }
    }
    // Held until the record is written, so that what the ledger says was
    // installed is the copy that is there: two installs of one name at once
    // would otherwise each record theirs, and the copies left would be
    // whichever moved into place last.
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;

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

    // A loaded workspace can be the project itself, keeping its own in a
    // folder the install writes in: then what would be copied is already
    // there. Nothing is written, and above all nothing is recorded: a record
    // would let `uninstall` delete the loaded workspace's own files.
    if let Some(to) = copies.iter().find(|to| same_place(&from, to)) {
        return Ok(Installed::Unchanged {
            name,
            paths: vec![to.clone()],
            unhashed: None,
        }
        .into());
    }
    for to in &copies {
        // One inside the other is worse: replacing `to` starts by deleting
        // it, and with it `from`, or copies `from` into itself.
        if overlaps(&from, to) {
            return Err(InstallError::Overlaps {
                from,
                to: to.clone(),
            });
        }
        // And never over what a loaded workspace keeps as its own: a project
        // can be the workspace it loads, sharing a folder, and a record from
        // before the load would otherwise let this overwrite it.
        if let Some(loaded) = kept_by_a_load(workspace, kind, to) {
            return Err(InstallError::KeptByALoad {
                path: to.clone(),
                loaded,
            });
        }
    }

    // Strict, where gathering was lenient: the copy is what the project's
    // agents load, so one they would skip is not put there.
    let refused = unfit(kind, &from, &name)?;
    if !refused.is_empty() {
        return Err(InstallError::Unfit {
            name,
            issues: refused,
        });
    }

    // Looked at again under the lock, before anything is written. The
    // candidate's standing is from a survey made before it was held, minutes
    // before on the install screen, and since then an edit may have made a
    // copy its author's and dropped its record, or somebody may have written
    // a folder of that name by hand. Only a turn to Foreign changes what
    // happens here, and that one must: it is somebody's work, never written
    // over.
    let recorded = ledger.copies(&entry, kind)?;
    if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
        return Ok(Installed::Foreign { name, path }.into());
    }

    // Links are followed only inside where the artifact comes from: the
    // workspace that keeps it, or the shelf folder it was gathered into.
    let within = match &candidate.offer {
        Offer::Loaded { entry, .. } => paths::normalize(&workspace.root().join(entry)),
        Offer::Workspace { .. } => workspace.root().to_path_buf(),
        Offer::Shelf(_) => from.clone(),
    };
    // Out of git before anything is written, so a copy in a local project
    // never shows in `git status`, not even for a moment, as its settings
    // never do; and a block this cannot edit refuses the install with
    // nothing written, rather than putting copies one machine has in every
    // teammate's way. Taken back out if the copies then fail, from where
    // nothing is left: a line naming an empty place would keep whatever
    // somebody writes there later out of git without a word.
    let excluded = match local(project) {
        Some(key) => Some(exclude::add(
            project.root(),
            key,
            &entries(project, &copies),
        )?),
        None => None,
    };
    let undo = || {
        if let Some(key) = local(project) {
            let gone: Vec<PathBuf> = copies
                .iter()
                .filter(|copy| !holds(copy, kind))
                .cloned()
                .collect();
            let _ = exclude::remove(project.root(), key, &entries(project, &gone));
        }
    };
    // Built in the project's scratch folder rather than beside a copy, in a
    // folder agents read: neither a copy half made nor one an interrupted
    // run left behind sits where an agent would load it.
    let scratch = project.scratch_dir();
    let mut stage = match copy::Stage::open(&scratch) {
        Ok(stage) => stage,
        Err(source) => {
            undo();
            return Err(InstallError::Write {
                path: scratch,
                source,
                swept: Vec::new(),
            });
        }
    };
    // Said, since what lands is then not all of what was offered.
    let left_out: Vec<LeftOut> = LEAVE_OUT
        .iter()
        .map(|(name, reason)| LeftOut {
            path: from.join(name),
            reason,
        })
        .filter(|left| fs::symlink_metadata(&left.path).is_ok())
        .collect();
    let changes = match stage.place(&from, &copies, &within, &left_out_names()) {
        Ok(changes) => changes,
        // What was cleared on the way goes out with the error rather than
        // with the stage: it was cleared whether or not this copy lands.
        Err(copy::PlaceError { target, source }) => {
            undo();
            return Err(InstallError::Write {
                // The target it failed at, since the error may not say: or
                // the first, as with one target, when it failed building the
                // copy, which is no target's doing.
                path: target.unwrap_or_else(|| copies[0].clone()),
                source,
                swept: stage.swept().to_vec(),
            });
        }
    };
    let swept = stage.swept().to_vec();
    // Cleared before the ledger is written rather than after, so the scratch
    // folder is held for no longer than the copy takes.
    drop(stage);
    // Of the copies as they now stand, not of where they came from: what was
    // written is what a later look at a copy is compared with, and the
    // source may hold links the copy resolved. A copy that cannot be read
    // back is recorded all the same, with no hash, since without a record it
    // would be Foreign for good, and D-04 reads a record with no hash as it
    // reads one from before hashes were kept. That is said, in what this
    // returns and in the log: a copy left as it was can hold a link somebody
    // put there, which `copy::same` follows and the hash refuses.
    let (content_hash, unhashed) = match one_hash(&copies) {
        Ok(hash) => (Some(hash), None),
        Err(reason) => (None, Some(reason)),
    };

    // A source to point at only for what was gathered: the workspace's own
    // came from nowhere but the workspace. The origin says where every offer
    // came from, those two included, in the one spelling `--from` takes.
    let source_id = match &candidate.offer {
        Offer::Shelf(gathered) => Some(gathered.source.id),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    };
    ledger.installed(
        &entry,
        &InstallRecord {
            kind,
            name: name.clone(),
            source_id,
            origin: Some(candidate.origin()),
            content_hash,
            targets: targets
                .iter()
                .zip(&copies)
                .map(|(target, to)| Target {
                    agent: target.agent(),
                    path: route(project.root(), to),
                })
                .collect(),
        },
    )?;
    let mut detail = format!("{name} into {}", project.name());
    for left in &left_out {
        if let Some(name) = left.path.file_name() {
            detail.push_str(&format!(", without {}", name.to_string_lossy()));
        }
    }
    if let Some(reason) = &unhashed {
        detail.push_str(&format!(", recorded without a hash: {reason}"));
    }
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    // One change for the installation, as it is recorded once: added when
    // every copy is new, unchanged when none is, and updated otherwise, a
    // copy put back in one folder beside one that was current included.
    let paths = copies;
    let installed = if changes
        .iter()
        .all(|change| *change == copy::Change::Unchanged)
    {
        Installed::Unchanged {
            name,
            paths,
            unhashed,
        }
    } else if changes.iter().all(|change| *change == copy::Change::Added) {
        Installed::Added {
            name,
            paths,
            unhashed,
        }
    } else {
        Installed::Updated {
            name,
            paths,
            unhashed,
        }
    };
    Ok(Installation {
        installed,
        swept,
        left_out,
        excluded,
    })
}

/// The refusal for the first of `copies` inside one of the agents' own
/// folders, if one is.
///
/// Resolved as far as each path exists, so a home reached through a link,
/// or a project reached through one, is still the folder it is.
pub(crate) fn user_scope(homes: &AgentHomes, copies: &[PathBuf]) -> Option<InstallError> {
    let folders: Vec<(&str, &Path, PathBuf)> = homes
        .folders()
        .into_iter()
        .filter_map(|(reader, folder)| Some((reader, folder, resolved(folder)?)))
        .collect();
    copies.iter().find_map(|copy| {
        let at = resolved(copy)?;
        folders
            .iter()
            .find(|(_, _, home)| at.starts_with(home))
            .map(|(reader, folder, _)| InstallError::UserScope {
                path: copy.clone(),
                home: folder.to_path_buf(),
                reader,
            })
    })
}

/// The key of a project's group in the exclude block, when what is installed
/// into it stays out of git: a local project's (D-02), named by its id, which
/// stays when the project is renamed or moved. None for a shared one, whose
/// copies are the team's to commit, and for one a `mind.toml` marks, since a
/// project from before settings was committed by design.
fn local(project: &MindProject) -> Option<&str> {
    project
        .settings()
        .filter(|settings| settings.share == Share::Local)
        .map(|settings| settings.id.as_str())
}

/// One exclude entry for each of `copies`, a folder each, as the project
/// spells it.
pub(crate) fn entries(project: &MindProject, copies: &[PathBuf]) -> Vec<exclude::Entry> {
    copies
        .iter()
        .filter_map(|copy| paths::relative_to(copy, project.root()))
        .map(exclude::Entry::directory)
        .collect()
}

/// The one hash every copy holds ([`hash::tree`]), or why there is none:
/// a copy that cannot be hashed, or, should one change while the others are
/// read, copies that do not hold the same.
pub(crate) fn one_hash(copies: &[PathBuf]) -> Result<String, String> {
    let mut found: Option<(String, &Path)> = None;
    for copy in copies {
        let hash = hash::tree(copy).map_err(|error| error.to_string())?;
        match &found {
            Some((first, at)) if *first != hash => {
                return Err(format!(
                    "{} does not hold what {} does",
                    copy.display(),
                    at.display()
                ))
            }
            Some(_) => {}
            None => found = Some((hash, copy)),
        }
    }
    found
        .map(|(hash, _)| hash)
        .ok_or_else(|| String::from("no copy was written"))
}

/// What stops the artifact at `from` from being installed as `name`:
/// whichever strict rules it breaks once filed under that name
/// ([`artifact::strict_issues`]).
///
/// Read from the files about to be copied rather than from what the ledger
/// noted when it was gathered, because the shelf is a folder anybody can
/// edit, and the workspace's own are edited as a matter of course. And held
/// to `name` rather than to the folder they sit in, because `name` is the
/// folder the copy will be: a shelf folder keeps the source's spelling, and
/// `name` is what the ledger recorded, which the files stop declaring once
/// they are edited, or once the source renames the skill and the record of
/// the old name stays behind.
fn unfit(kind: Kind, from: &Path, name: &str) -> Result<Vec<ValidationIssue>, InstallError> {
    // Matched on the kind, so a third one has to say what it is held to.
    let artifact = match kind {
        Kind::Skill => Artifact::skill(from, from)?,
        // A rule is one file, and installing copies a directory, so none is
        // ever offered.
        Kind::Rule => return Ok(Vec::new()),
    };
    Ok(artifact
        .manifest()
        .map_or_else(Vec::new, |manifest| artifact::strict_issues(manifest, name)))
}

/// Take one artifact back out of a project: every copy its installation
/// recorded, wherever the project would install it now.
///
/// Only if the ledger says Mindflayer put it there, and only the copies it
/// says Mindflayer wrote: a folder of that name in a target the record does
/// not name is somebody's, and stays. Anything else is reported and left
/// where it is.
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
    // Held from before the record is read until it is dropped, for the
    // reason `install` holds it: an install of the same name at the same
    // moment comes wholly before this or wholly after it. Between the copies
    // going and their record going, it would leave a copy with no record,
    // which Mindflayer would never touch again.
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, kind, name)? else {
        // Nowhere to look is nothing there: a project with no targets now
        // holds nothing an install would have written.
        let held = targets(project, kind)
            .unwrap_or_default()
            .into_iter()
            .map(|target| copy_at(&target.folder, kind, name))
            .find(|copy| holds(copy, kind));
        return Ok(match held {
            Some(path) => Removed::Foreign {
                name: name.to_owned(),
                path,
            },
            None => Removed::Absent {
                name: name.to_owned(),
            },
        });
    };

    let mut paths = Vec::with_capacity(record.targets.len());
    for target in &record.targets {
        let path = recorded_copy(project, name, &target.path)?;
        if let Some(loaded) = kept_by_a_load(workspace, kind, &path) {
            return Err(InstallError::KeptByALoad { path, loaded });
        }
        paths.push(path);
    }
    for path in &paths {
        if path.exists() {
            std::fs::remove_dir_all(path).map_err(|source| InstallError::Write {
                path: path.clone(),
                source,
                swept: Vec::new(),
            })?;
        }
    }
    ledger.uninstalled(&entry, kind, name)?;
    let detail = format!("{name} from {}", project.name());
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;

    // After the copies, so none shows in `git status` on its way out; and
    // said rather than raised when the block cannot be edited, since they
    // are gone and so is their record.
    let still_excluded = local(project)
        .and_then(|key| exclude::remove(project.root(), key, &entries(project, &paths)).err())
        .map(|error| error.to_string());
    Ok(Removed::Removed {
        name: name.to_owned(),
        paths,
        still_excluded,
    })
}

/// Where in `project` a target of `name`'s installation recorded at `route`
/// is, as long as that is a folder of the project named after it.
///
/// The ledger is the workspace's own, but a removal deletes whatever this
/// says, so a route that climbs out of the project, starts over at a root, or
/// ends in another name is refused rather than followed: before records had
/// targets, a removal was only ever `name` joined onto a folder of the
/// project's.
pub(crate) fn recorded_copy(
    project: &MindProject,
    name: &str,
    route: &str,
) -> Result<PathBuf, InstallError> {
    let path = Path::new(route);
    let inside = path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !inside || path.file_name() != Some(std::ffi::OsStr::new(name)) {
        return Err(InstallError::Recorded {
            name: name.to_owned(),
            route: route.to_owned(),
        });
    }
    Ok(project.root().join(path))
}

/// Drop the record that Mindflayer installed `name` into `project`, when it
/// names a copy at `at`, saying whether there was one.
///
/// For when what is in the project stops being what was installed: it was
/// removed, renamed or edited by hand, or something new was written under a
/// name an old record still claims. The record is what gives the install
/// screen leave to overwrite and delete, so it must not outlive the copy it
/// was about: whatever somebody writes there next would inherit it.
///
/// Only a record of a copy at `at`, the place the change reached, and only
/// that copy's part of it ([`Ledger::uninstalled_copy`]): a project marked by
/// its settings authors in a folder of its own, beside the copies installed
/// for its agents, and a change there leaves those copies what was installed.
/// Dropping their record by name would make them Foreign, never updated or
/// removed again, over a change to something else. So would dropping the
/// whole installation when the folder it authors in is also a target, which
/// its settings may make it (`"skills": ".claude/skills"`), and the change
/// reached the copy there: the one in `.agents/skills` is still what was
/// installed. The installation goes with its last copy. A record from before
/// installations had targets names none, and is dropped as it was.
///
/// In a local project, `at` stops being kept out of git with its record:
/// what is there now, or is written there next, is its author's, and a line
/// left in the exclude block would hide it from `git status` without a word.
///
/// Only the changes in [`crate::lifecycle`] call it, each when its change
/// calls for it. Public, it would let a front end drop a record on a rule of
/// its own. Keeping it here cannot make a front end drop one; changing things
/// through [`crate::lifecycle`] does.
///
/// `workspace` is the one the front end resolved for the command, given
/// rather than found by walking up from the project: a walk misses every
/// workspace that links the project from somewhere that is not above it, and
/// each of those would keep its stale record. Another workspace that links
/// the same project still keeps its own, since nothing in the project says
/// which workspaces link it.
///
/// The ledger is opened to write, as by every command that writes to it, so
/// what that scrubbed comes back too, for the change to hand on: the changes
/// in [`crate::lifecycle`] reach the ledger only through here.
pub(crate) fn disown(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
) -> Result<Disowned, InstallError> {
    // Only a workspace that manages the project keeps records about it, and
    // one that has never installed anything has no ledger: this must not
    // create one just to look.
    if !workspace.is_linked(project.root()) || !workspace.ledger_path().is_file() {
        return Ok(Disowned::default());
    }
    // However long the scrub waits for the workspace, which it does only
    // when an older build left a credential in a clone: the change this
    // follows is made, so giving up would leave its record behind, and
    // trying the change again would find nothing to change.
    let (ledger, scrub) = workspace.ledger_to_write_waiting(Duration::MAX)?;
    let entry = project_entry(workspace, project);
    let reached = route(project.root(), at);
    let detail = match ledger.uninstalled_copy(&entry, kind, name, &reached)? {
        Forgotten::Nothing => {
            return Ok(Disowned {
                scrub,
                ..Disowned::default()
            })
        }
        Forgotten::Copy => format!(
            "{name}'s copy at {reached} from {}, which no longer holds what was installed there",
            project.name()
        ),
        Forgotten::Installation => format!(
            "{name} from {}, which no longer holds the copy that was installed",
            project.name()
        ),
    };
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;
    // After the record, as an uninstall takes its entries out after the
    // copies; and handed back rather than raised, since the record is gone.
    let still_excluded = local(project).and_then(|key| {
        exclude::remove(project.root(), key, &entries(project, &[at.to_path_buf()]))
            .err()
            .map(|source| InstallError::StillExcluded {
                path: at.to_path_buf(),
                source,
            })
    });
    Ok(Disowned {
        dropped: true,
        scrub,
        still_excluded,
    })
}

/// What [`disown`] did.
#[derive(Debug, Default)]
pub(crate) struct Disowned {
    /// Whether there was a record of the copy, which is now gone.
    pub dropped: bool,
    /// What opening the ledger to write took out of it, for the front end to
    /// say; nothing when there was no ledger to open.
    pub scrub: Scrub,
    /// Why the exclude block of a local project still keeps the copy's place
    /// out of git, when it could not be edited.
    pub still_excluded: Option<InstallError>,
}

/// What installing one artifact did.
///
/// Each that leaves copies names every one of them (`paths`), one for each
/// target, in the order [`targets`] gives them, and says whether they could
/// be hashed (`unhashed`): copies that could not are recorded all the same,
/// and said rather than recorded quietly, since a record with no hash is
/// read the way one from before hashes were kept is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    Added {
        name: String,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    Updated {
        name: String,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    Unchanged {
        name: String,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    /// Something of that name is already in one of the targets, at `path`,
    /// and is not ours to replace, so nothing was written in any of them.
    Foreign { name: String, path: PathBuf },
}

impl Installed {
    /// Why the copy could not be hashed, when it could not, so the ledger
    /// records it with no hash.
    pub fn unhashed(&self) -> Option<&str> {
        match self {
            Installed::Added { unhashed, .. }
            | Installed::Updated { unhashed, .. }
            | Installed::Unchanged { unhashed, .. } => unhashed.as_deref(),
            Installed::Foreign { .. } => None,
        }
    }
}

/// What [`install`] did: to the artifact, and to the project's scratch folder
/// on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub installed: Installed,
    /// What installs that were interrupted had left, cleared before this one
    /// built its copy: in the project's scratch folder, and beside the copy,
    /// where earlier versions staged. Said rather than cleared quietly,
    /// because each is an install that did not finish.
    pub swept: Vec<PathBuf>,
    /// What the offer holds that no copy does (`LEAVE_OUT`): said, because
    /// the copies are then not all that was offered.
    pub left_out: Vec<LeftOut>,
    /// What keeping the copies out of git came to, for a local project; none
    /// for a project whose copies are committed. [`Excluded::warning`] says
    /// when nothing keeps them out, outside a git work tree.
    pub excluded: Option<Excluded>,
}

impl From<Installed> for Installation {
    /// An installation that never reached the scratch folder, and so cleared
    /// nothing in it and copied nothing.
    fn from(installed: Installed) -> Self {
        Installation {
            installed,
            swept: Vec::new(),
            left_out: Vec::new(),
            excluded: None,
        }
    }
}

/// What uninstalling one artifact did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Every copy its installation recorded is gone, and so is the record.
    Removed {
        name: String,
        paths: Vec<PathBuf>,
        /// Why the exclude block of a local project still names them, when it
        /// could not be edited: a line left there keeps whatever somebody
        /// writes in that place later out of git, which is worth saying.
        still_excluded: Option<String>,
    },
    /// Present, at `path`, but nobody here put it there.
    Foreign { name: String, path: PathBuf },
    /// Nothing of that name to remove.
    Absent { name: String },
}

/// How `name` stands in a project whose root is `root` and which installs
/// into `targets`, given every copy the ledger records Mindflayer writing
/// there (`recorded`, as [`Ledger::copies`] spells them).
///
/// Foreign when any target holds something of that name the ledger does not
/// record, whatever the others hold: installing would write beside it, and
/// uninstalling would take away the copies the agents reading both folders
/// share with it. Installed when every copy there is recorded, even if one a
/// record names has gone since, which the next install puts back.
fn standing(
    root: &Path,
    targets: &[InstallTarget],
    kind: Kind,
    name: &str,
    recorded: &[String],
) -> Standing {
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, name))
        .collect();
    if unrecorded(root, kind, &copies, recorded).is_some() {
        Standing::Foreign
    } else if copies.iter().any(|copy| holds(copy, kind)) {
        Standing::Installed
    } else {
        Standing::Absent
    }
}

/// The first of `copies` that is there and that `recorded` does not name,
/// if one is: somebody's work.
///
/// By the copy's path, as a listing marks installed copies
/// (`Catalog::with_records`), not by its name: an installation of a name
/// into one folder does not make what somebody keeps under that name in
/// another folder Mindflayer's to write over.
fn unrecorded(root: &Path, kind: Kind, copies: &[PathBuf], recorded: &[String]) -> Option<PathBuf> {
    copies
        .iter()
        .find(|copy| holds(copy, kind) && !recorded.contains(&route(root, copy)))
        .cloned()
}

/// Where a copy of `name` goes in `folder`: a folder of that name for a kind
/// that is one, a file for a kind that is files.
pub(crate) fn copy_at(folder: &Path, kind: Kind, name: &str) -> PathBuf {
    match kind.layout() {
        Layout::Directory { .. } => folder.join(name),
        Layout::Files { extension } => folder.join(format!("{name}.{extension}")),
    }
}

/// Whether something is at `copy`, where an install would write.
pub(crate) fn holds(copy: &Path, kind: Kind) -> bool {
    match kind.layout() {
        // Anything by that name, not only a finished artifact: a folder of
        // notes somebody started by hand is theirs, and installing over it
        // would delete them.
        Layout::Directory { .. } => copy.exists(),
        Layout::Files { .. } => copy.is_file(),
    }
}

/// How the ledger names a project: relative to the workspace, the way a
/// registered project is stored, so moving the two together keeps it true.
///
/// The route to where the workspace's own entry leads
/// ([`FlayerWorkspace::spelling_of`]), not to however the caller spelled the
/// project: `mind` run inside it and `flayer -p` must read and write one
/// record, whichever symlinks either went through.
fn project_entry(workspace: &FlayerWorkspace, project: &MindProject) -> String {
    entry_for_root(workspace, project.root())
}

/// [`project_entry`], for a project known only by its root, as an artifact
/// knows the one it was found in.
pub(crate) fn entry_for_root(workspace: &FlayerWorkspace, root: &Path) -> String {
    route(workspace.root(), &workspace.spelling_of(root))
}

/// A path below another, `/` separated, or the path itself when there is no
/// route between them.
pub(crate) fn route(base: &Path, target: &Path) -> String {
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
/// in, or around, if one does (spelled as its root, which `flayer unload`
/// accepts from anywhere).
///
/// By the folder, not by what is in it yet: a project that is also a loaded
/// workspace keeping both in one folder has nothing in it Mindflayer may
/// write or remove, from the first install on; otherwise an install today
/// is the loaded workspace's own tomorrow, and can never be updated again.
pub(crate) fn kept_by_a_load(
    workspace: &FlayerWorkspace,
    kind: Kind,
    path: &Path,
) -> Option<PathBuf> {
    let at = resolved(path)?;
    workspace.loads().into_iter().find_map(|loaded| {
        let source = loaded.workspace.ok()?;
        let kept = fs::canonicalize(source.own().directory_for(kind)?).ok()?;
        (at.starts_with(&kept) || kept.starts_with(&at)).then_some(loaded.root)
    })
}

/// Whether `candidate` is the copy `project` holds: the same names and bytes
/// as the folder there, but for what an install leaves out (`LEAVE_OUT`).
/// For telling which of two offers of one name is the one installed, which
/// a record from before schema 3 does not say for what came from a workspace
/// rather than the shelf, and which an edit since may have made neither.
///
/// Compared with the first copy the project holds of it: an install writes
/// every copy from one tree, so any of them is what was installed, and one
/// edited since is no longer any offer's.
pub fn is_current(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    candidate: &Candidate,
) -> bool {
    let Ok(targets) = targets(project, candidate.kind()) else {
        return false;
    };
    targets
        .iter()
        .map(|target| target.folder.join(candidate.name()))
        .find(|at| at.is_dir())
        .is_some_and(|at| {
            copy::same_leaving(&candidate.source(workspace), &at, &left_out_names())
                .unwrap_or(false)
        })
}

/// Why an install or an uninstall could not happen at all.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("{name}: the ledger has it at {path}, but nothing is there; gather {workspace} again")]
    Missing {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("{name}: {path} is not there any more, because the workspace loaded from {workspace} has changed; `flayer unload {workspace}` if it has gone")]
    Gone {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("`{name}` is not one folder name, so it cannot be installed or removed")]
    UnsafeName { name: String },
    #[error("`{name}` cannot be installed: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("{from} and {to} are one inside the other, so installing one onto the other would delete it")]
    Overlaps { from: PathBuf, to: PathBuf },
    #[error("{path} is where the workspace loaded from {loaded} keeps its own, so Mindflayer neither installs nor removes there; `flayer unload {loaded}` first to manage it as a project")]
    KeptByALoad { path: PathBuf, loaded: PathBuf },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
        /// What interrupted installs had left that this one cleared before
        /// the write failed ([`InstallError::swept`]).
        swept: Vec<PathBuf>,
    },
    #[error("nothing called `{name}` can be installed{}: gather it, load a workspace that has it with `flayer load`, or add it to the workspace with `flayer add`", from.as_deref().map(|from| format!(" from {from}")).unwrap_or_default())]
    NotOffered { name: String, from: Option<String> },
    #[error("`{name}` is offered by more than one place ({}), so say which with --from", .origins.join(", "))]
    OfferedTwice { name: String, origins: Vec<String> },
    #[error("{project} names no agent in its settings that {kind}s are installed for, so none can be installed into it")]
    NoTarget { project: String, kind: Kind },
    #[error("{name}: the ledger records a copy at {route}, which is not a folder of the project named after it, so nothing was removed")]
    Recorded { name: String, route: String },
    #[error("{0}, and nothing was installed, since its copies could not be kept out of git")]
    Exclude(#[from] ExcludeError),
    #[error("{path}: its record of being installed was dropped, but the exclude file still keeps it out of git: {source}")]
    StillExcluded {
        path: PathBuf,
        #[source]
        source: ExcludeError,
    },
    #[error("{path} is inside {home}, which {reader} reads for every project, so nothing was installed: Mindflayer installs into one project at a time")]
    UserScope {
        path: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
}

impl InstallError {
    /// What an install that failed had cleared first, as
    /// [`Installation::swept`] says for one that did not: for a front end to
    /// say with the error, because each is an install that did not finish,
    /// whether or not this one did.
    ///
    /// Only a copy that fails carries any. A ledger that fails after the copy
    /// stops everything, as it does wherever it fails, and what was swept
    /// goes unsaid with the rest.
    pub fn swept(&self) -> &[PathBuf] {
        match self {
            InstallError::Write { swept, .. } => swept,
            _ => &[],
        }
    }
}
