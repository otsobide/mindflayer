//! Why a command could not run at all.
//!
//! One enum for every command rather than one per family: a
//! [`Failure`](crate::Failure) carries whichever it is to the binary, and the
//! binary reports them all the same way.

use std::io;
use std::path::{Path, PathBuf};

use mindflayer_core::activate::{self, ActivateError, WORKSPACE_VARIABLE};
use mindflayer_core::everywhere::EverywhereError;
use mindflayer_core::gather::GatherError;
use mindflayer_core::ledger::LedgerError;
use mindflayer_core::{
    ArtifactError, CreateError, Kind, LifecycleError, Reason, ResolveError, TemplateError,
    WorkspaceError,
};
use thiserror::Error;

use crate::render::plural;

/// Why a command could not run at all, as opposed to running and finding
/// problems: those are in the [`Outcome`](crate::Outcome).
#[derive(Debug, Error)]
pub enum CliError {
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("{path}: cannot be resolved: {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("{} names a workspace that cannot be used: {source}", named_by(.reason))]
    Chosen {
        reason: Reason,
        source: WorkspaceError,
    },
    #[error("{0} is not inside a mind project (run `mind init` to create one here)")]
    NotInProject(PathBuf),
    #[error("{0} is not inside a flayer workspace (run `flayer init` to create one here)")]
    NotInWorkspace(PathBuf),
    #[error("no flayer workspace{} at or above {from} to {verb}, and no default one", if *named { " but the ones named" } else { "" })]
    NowhereToLoad {
        from: PathBuf,
        verb: &'static str,
        named: bool,
    },
    #[error("nothing named `{0}` here")]
    UnknownArtifact(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Gather(#[from] GatherError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Install(#[from] mindflayer_core::install::InstallError),
    #[error(transparent)]
    Create(#[from] CreateError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(transparent)]
    Everywhere(#[from] EverywhereError),
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Activate(#[from] ActivateError),
    #[error(
        "$SHELL is not set; say which shell to write for with --shell ({})",
        activate::expected()
    )]
    NoShell,
    #[error(
        "cannot tell which shell `{program}` is; say which to write for with --shell ({})",
        activate::expected()
    )]
    UnknownShell { program: String },
    #[error("nothing removed: this deletes {}; add --yes to go ahead", deleting(.path, .kind, .files))]
    Unconfirmed {
        kind: Kind,
        path: PathBuf,
        files: usize,
    },
    #[error("nothing moved: {0}")]
    Refused(mindflayer_core::migrate::Refusal),
    #[error(transparent)]
    Migrate(#[from] mindflayer_core::migrate::MigrateError),
    #[error("nothing moved: what the workspace records of the project decides which of its copies move, and it cannot be read: {0}")]
    MigrateUnrecorded(Box<CliError>),
    #[error("say what to migrate: --library moves the workspace's own into its library, -p <project> moves a project it links from its mind.toml or settings.json to a settings.toml, and --all-projects moves every one")]
    MigrateWhat,
    #[error(transparent)]
    Reapply(#[from] mindflayer_core::ReapplyError),
    #[error(transparent)]
    Update(#[from] mindflayer_core::update::UpdateError),
    #[error("say which projects to update: -p <project> for each, or --all for every one the workspace links")]
    UpdateWhere,
    #[error("say which project's agents to change (`flayer agents -p <project> <agent>...`); the workspace's own reach agents by being installed")]
    AgentsWhere,
    #[error("say which project's targets to change (`flayer targets -p <project> set <kind> <dir>...`); the workspace's own reach agents by being installed")]
    TargetsWhere,
    #[error("`{editor}` could not be started: {source}")]
    Editor {
        editor: String,
        #[source]
        source: io::Error,
    },
    #[error("`{editor}` failed ({status}), so the file is reported on no further")]
    EditorFailed { editor: String, status: String },
    #[error("no flayer workspace above {0}; make one with `flayer init` in the directory that holds your projects")]
    NoWorkspaceAbove(PathBuf),
    #[error("{0}: not known, since there is no default workspace to remember it (--home or $MINDFLAYER_HOME names none)")]
    NoDefault(PathBuf),
    #[error("{project}: not managed by the workspace at {workspace}; `mind link` adds it")]
    NotManaged {
        project: PathBuf,
        workspace: PathBuf,
    },
    #[error("no project called `{wanted}` in this workspace{}", known_projects(.known))]
    UnknownProject { wanted: String, known: Vec<String> },
    #[error("`{wanted}` names more than one project ({}), so name it by its path", .matches.join(", "))]
    AmbiguousProject {
        wanted: String,
        matches: Vec<String>,
    },
    #[error("say which projects to install into: -p <project> for each, or --all-projects for every one the workspace links; or run `flayer install` alone for the screen")]
    InstallWhere,
    #[error("say which skill or rule to install (`flayer install -p <project> <name>`), or run `flayer install` alone for the screen")]
    InstallWhat,
    #[error("say which projects to take it out of: -p <project> for each, or --all-projects for every one the workspace links")]
    UninstallWhere,
    #[error("{project} has no {name} installed, so there is nothing to take out")]
    NotInstalled { name: String, project: String },
    #[error("no project asked has {name} installed, so there is nothing to take out")]
    NotInstalledAnywhere { name: String },
    #[error("`{command}` needs a terminal to draw on, and this is not one: {source}")]
    NoTerminal {
        command: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("`flayer` on its own opens a screen, and this is not a terminal; `flayer --help` lists the commands")]
    NotATerminal,
    #[error("the screen could not be drawn: {source}")]
    Screen {
        #[source]
        source: io::Error,
    },
}

/// Core's answer to which workspace, or which of its projects, rendered as
/// the CLI says it: with the command that would put it right.
impl From<ResolveError> for CliError {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::Chosen { reason, source } => CliError::Chosen { reason, source },
            ResolveError::Workspace(source) => CliError::Workspace(source),
            ResolveError::NotManaged { project, workspace } => {
                CliError::NotManaged { project, workspace }
            }
            ResolveError::UnknownProject { wanted, known } => {
                CliError::UnknownProject { wanted, known }
            }
            ResolveError::AmbiguousProject { wanted, matches } => {
                CliError::AmbiguousProject { wanted, matches }
            }
        }
    }
}

/// What chose a workspace that cannot be used, as it was spelled.
fn named_by(reason: &Reason) -> String {
    match reason {
        Reason::Activated => format!("${WORKSPACE_VARIABLE}"),
        // Core refuses only what was chosen, so the other two never get here.
        Reason::Explicit | Reason::Above | Reason::Home => String::from("-W"),
    }
}

/// "; it manages alpha, beta", or what to do when it manages nothing.
fn known_projects(known: &[String]) -> String {
    if known.is_empty() {
        String::from("; it manages none yet: `flayer link <path>` adds one")
    } else {
        format!("; it manages {}", known.join(", "))
    }
}

/// What `mind remove` would delete, as a sentence.
pub(crate) fn deleting(path: &Path, kind: &Kind, files: &usize) -> String {
    match kind.layout() {
        mindflayer_core::Layout::Directory { .. } => {
            format!(
                "{} and the {} in it",
                path.display(),
                plural(*files, "file")
            )
        }
        mindflayer_core::Layout::Files { .. } => path.display().to_string(),
    }
}
