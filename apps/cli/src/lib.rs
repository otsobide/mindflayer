//! Command line interface for Mindflayer, split the way the model is.
//!
//! `mind <cmd>` acts on the mind project you are standing in. `mind flayer
//! <cmd>` acts on the flayer workspace above it, and the `flayer` binary is
//! the same tree reached directly, so `flayer link x` and `mind flayer link x`
//! are one command with two spellings.
//!
//! The parser, the work and the rendering all live here rather than in the
//! binaries so the tests drive the real command surface instead of shelling
//! out, and so both binaries cannot drift apart.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mindflayer_core::gather::{self, GatherError, Report, Request};
use mindflayer_core::ledger::LedgerError;
use mindflayer_core::paths;
use mindflayer_core::scan::MAX_LISTED;
use mindflayer_core::template::{self, Origin};
use mindflayer_core::{
    create, create_from, lifecycle, scan, Artifact, ArtifactError, Catalog, CreateError, Declared,
    Directories, FlayerWorkspace, Initialization, Kind, LifecycleError, MindProject, Reference,
    Registration, Scan, Target, TemplateError, WorkspaceError, DEFAULT_SUBDIRECTORY,
};
use thiserror::Error;

pub mod install;
pub mod tui;

/// Manage the agent skills in a mind project.
//
// `about` is spelled out rather than taken from the crate description, which
// is one line for a package that ships two binaries and so can only be right
// for one of them. `propagate_version` puts `--version` on the nested
// subcommands too, so `mind flayer --version` answers instead of erroring
// while `flayer --version` works.
#[derive(Debug, Parser)]
#[command(
    name = "mind",
    version,
    propagate_version = true,
    about = "Manage the agent skills in a mind project",
    long_about = None
)]
pub struct Cli {
    /// What to do [default: open the TUI on this project].
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,

    /// Where the default workspace lives, used when none is found above
    /// [default: $MINDFLAYER_HOME, then your home directory; empty for none].
    #[arg(long = "home", value_name = "DIR", global = true)]
    pub home: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a mind project here.
    Init {
        /// Where this project keeps its skills [default: `skills`].
        ///
        /// Relative to the project. Point it wherever the agents that read
        /// them already look, such as `.claude/skills`.
        #[arg(long, value_name = "DIR")]
        skills: Option<PathBuf>,

        /// Where this project keeps its rules [default: `rules`].
        #[arg(long, value_name = "DIR")]
        rules: Option<PathBuf>,
    },

    /// Create a skill or a rule in this project.
    Add {
        /// What to create: `skill` or `rule`.
        kind: Kind,

        /// Its name: `deploy`, or a route like `git/no-force-push` for a rule.
        name: String,

        /// One line saying what it is: a skill's description, a rule's
        /// opening line.
        description: String,

        /// Start from a template instead: `full`, or one of yours in
        /// `.mind/templates` or the workspace's `.mindflayer/templates`.
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
    },

    /// Open one artifact in your editor.
    Edit {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// The editor to open it in [default: $VISUAL, then $EDITOR, then vi].
        #[arg(long, value_name = "COMMAND")]
        editor: Option<String>,
    },

    /// Give one artifact a new name, on disk and in what it declares.
    #[command(alias = "mv")]
    Rename {
        /// Its name now, or `kind:name`.
        reference: String,

        /// The name it should have.
        to: String,
    },

    /// Delete one artifact from this project.
    #[command(alias = "rm")]
    Remove {
        /// Its name, or `kind:name`.
        reference: String,

        /// Delete it. Without this nothing is deleted, and the command says
        /// what would be.
        #[arg(short, long)]
        yes: bool,
    },

    /// List the templates `mind add --template` can start from here.
    Templates,

    /// List this project's artifacts.
    #[command(alias = "ls")]
    List {
        /// Only this kind: `skills` or `rules` [default: every kind].
        kind: Option<Kind>,
    },

    /// Show one artifact in full.
    Show {
        /// Its name, or `kind/name` when one name belongs to two kinds.
        reference: String,
    },

    /// Check this project's artifacts against what an agent requires.
    Validate {
        /// A kind (`rules`), or one artifact by name [default: everything].
        target: Option<String>,
    },

    /// Register this project with the flayer workspace above it.
    Link,

    /// Drop this project from the flayer workspace above it.
    Unlink,

    /// Offer other flayer workspaces' own skills to the workspace above this
    /// project, read live from where they are; on its own, list what it loads.
    Load {
        /// Each workspace's directory, the one holding `.mindflayer`
        /// [default: list the loaded ones].
        #[arg(value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Stop the workspace above this project offering a loaded workspace's.
    Unload {
        /// Each workspace's directory, as loaded. It need not still exist.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Put a skill into this project: one of the workspace's own, or one
    /// from its shelf.
    Install {
        /// The skill's name.
        name: String,

        /// Where it comes from, when two places offer that name: `workspace`,
        /// a shelf source's URL, or a loaded workspace's path or name.
        #[arg(long, value_name = "ORIGIN")]
        from: Option<String>,
    },

    /// Take a skill Mindflayer installed back out of this project.
    Uninstall {
        /// The skill's name.
        name: String,
    },

    /// Open the TUI on this project. What `mind` does on its own.
    Tui,

    /// Act on the flayer workspace instead. Also reachable as `flayer <cmd>`.
    ///
    /// On its own it opens the TUI, as `flayer` on its own does.
    Flayer {
        #[command(subcommand)]
        command: Option<FlayerCommand>,
    },
}

/// The `flayer` binary: a shortcut into the workspace half of `mind`.
#[derive(Debug, Parser)]
#[command(
    name = "flayer",
    version,
    propagate_version = true,
    about = "Manage the mind projects a flayer workspace orchestrates",
    long_about = None
)]
pub struct FlayerCli {
    /// What to do [default: open the TUI].
    #[command(subcommand)]
    pub command: Option<FlayerCommand>,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,

    /// Where the default workspace lives, used when none is found above
    /// [default: $MINDFLAYER_HOME, then your home directory; empty for none].
    #[arg(long = "home", value_name = "DIR", global = true)]
    pub home: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum FlayerCommand {
    /// Create a flayer workspace here.
    Init {
        /// Where the workspace keeps its own skills [default: `skills`].
        ///
        /// Relative to the workspace. They are the ones it shares with every
        /// project it manages.
        #[arg(long, value_name = "DIR")]
        skills: Option<PathBuf>,

        /// Where the workspace keeps its own rules [default: `rules`].
        #[arg(long, value_name = "DIR")]
        rules: Option<PathBuf>,
    },

    /// Create a skill or a rule: the workspace's own, or a project's.
    Add {
        /// A project this workspace manages, by name or path, instead of the
        /// workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// What to create: `skill` or `rule`.
        kind: Kind,

        /// Its name: `deploy`, or a route like `git/no-force-push` for a rule.
        name: String,

        /// One line saying what it is: a skill's description, a rule's
        /// opening line.
        description: String,

        /// Start from a template instead: `full`, or one in
        /// `.mindflayer/templates` or the project's `.mind/templates`.
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
    },

    /// Open one of the workspace's own artifacts, or a project's, in your
    /// editor.
    Edit {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// The editor to open it in [default: $VISUAL, then $EDITOR, then vi].
        #[arg(long, value_name = "COMMAND")]
        editor: Option<String>,
    },

    /// Give one artifact a new name, on disk and in what it declares.
    #[command(alias = "mv")]
    Rename {
        /// Its name now, or `kind:name`.
        reference: String,

        /// The name it should have.
        to: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// Delete one of the workspace's own artifacts, or a project's.
    #[command(alias = "rm")]
    Remove {
        /// Its name, or `kind:name`.
        reference: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Delete it. Without this nothing is deleted, and the command says
        /// what would be.
        #[arg(short, long)]
        yes: bool,
    },

    /// List the templates `flayer add --template` can start from.
    Templates {
        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// List the workspace's own artifacts and every managed project's.
    #[command(alias = "ls")]
    List {
        /// Only this kind: `skills` or `rules` [default: every kind].
        kind: Option<Kind>,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// Show one artifact in full, wherever in the workspace it is.
    Show {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// Check the workspace's own artifacts and every managed project's.
    Validate {
        /// A kind (`rules`), or one artifact by name [default: everything].
        target: Option<String>,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// Register one or more mind projects with this workspace.
    Link {
        /// Each project's directory, the one holding `.mind`.
        #[arg(required = true, value_name = "PROJECT")]
        projects: Vec<PathBuf>,
    },

    /// Drop one or more registered mind projects.
    Unlink {
        /// Each project's directory, as registered. It need not still exist.
        #[arg(required = true, value_name = "PROJECT")]
        projects: Vec<PathBuf>,
    },

    /// Offer other flayer workspaces' own skills to this workspace's projects,
    /// read live from where they are.
    ///
    /// Into the workspace at or above here that is not one being loaded, or
    /// else the default one in your home: so `flayer load .` from inside a
    /// workspace full of skills loads it into the one that manages you. On its
    /// own, it lists what this workspace loads and what each offers.
    Load {
        /// Each workspace's directory, the one holding `.mindflayer`
        /// [default: list the loaded ones].
        #[arg(value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Stop offering a loaded workspace's skills.
    Unload {
        /// Each workspace's directory, as loaded. It need not still exist.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Open the minds screen: every mind project and repository under this
    /// workspace, ticked where it is linked, to link and unlink in one pass.
    Minds,

    /// Look under this workspace for projects and repositories to link.
    Scan,

    /// Collect artifacts from elsewhere onto this workspace's shelf.
    #[command(subcommand)]
    Gather(GatherCommand),

    /// Put skills into the projects this workspace manages: the workspace's
    /// own, or its shelf's.
    ///
    /// On its own it opens a screen for all of them at once. Given a skill and
    /// a project, it installs that one without one.
    Install {
        /// The project to install into.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// The skill's name [default: open the screen].
        name: Option<String>,

        /// Where it comes from, when two places offer that name: `workspace`,
        /// a shelf source's URL, or a loaded workspace's path or name.
        #[arg(long, value_name = "ORIGIN")]
        from: Option<String>,
    },

    /// Take a skill Mindflayer installed back out of a project.
    Uninstall {
        /// The project to take it out of.
        #[arg(short, long, value_name = "PROJECT")]
        project: String,

        /// The skill's name.
        name: String,
    },

    /// Open the TUI: every managed project, and a key for each command.
    ///
    /// What `flayer` does on its own. Everything it changes, it changes by
    /// running one of these commands, and it prints them when it closes.
    Tui,
}

/// Where a gather takes artifacts from.
///
/// A subcommand per method rather than a `--from` flag: each one takes
/// different arguments, and the next one (a directory, an archive) should have
/// to say so rather than overloading a URL.
#[derive(Debug, Subcommand)]
pub enum GatherCommand {
    /// Take skills from a git repository.
    Git {
        /// The repository to clone.
        url: String,

        /// The folder inside the repository to take skills from.
        #[arg(long, value_name = "DIR", default_value = DEFAULT_SUBDIRECTORY)]
        path: String,

        /// A branch or tag to take, instead of the repository's default.
        #[arg(long = "ref", value_name = "REF")]
        reference: Option<String>,
    },

    /// List what can be installed from the shelf and from loaded workspaces,
    /// and where each artifact comes from.
    #[command(alias = "ls")]
    List,
}

/// Which level a command reports at.
///
/// It decides one thing: whether naming what holds each artifact tells the
/// reader anything. Inside a single project it does not; above them it does,
/// and the workspace's own are named as the workspace's.
#[derive(Clone, Copy)]
enum Level<'a> {
    Project,
    Workspace(&'a Holders),
}

/// What a command produced.
///
/// The text is built rather than printed as the command runs, which is what
/// lets a test assert on exactly what a user sees.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The report itself.
    pub stdout: String,
    /// Problems worth reporting that did not stop the command.
    pub stderr: Vec<String>,
    /// Whether everything the command looked at was in order.
    pub ok: bool,
}

impl Outcome {
    /// A clean report with nothing to warn about.
    fn plain(stdout: String) -> Self {
        Self {
            stdout,
            stderr: Vec::new(),
            ok: true,
        }
    }

    /// Print the outcome and return the code the process should exit with.
    ///
    /// Written rather than printed, because `print!` panics when the pipe is
    /// closed and `mind list | head` closes it on purpose. A reader that has
    /// seen enough is not an error, so that case exits as if all was well.
    pub fn report(&self) -> ExitCode {
        use std::io::Write as _;

        if let Err(error) = io::stdout().write_all(self.stdout.as_bytes()) {
            return if error.kind() == io::ErrorKind::BrokenPipe {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        for line in &self.stderr {
            let _ = writeln!(io::stderr(), "{line}");
        }
        if self.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// A command that could not run, and whatever it had already found out.
///
/// The warnings travel with the error rather than being dropped, because they
/// usually explain it: `no skill named \`broken\`` is baffling on its own and
/// obvious next to `broken/SKILL.md: the file does not start with a `---`
/// front matter fence`.
#[derive(Debug)]
pub struct Failure {
    /// Why the command stopped.
    ///
    /// Boxed to keep the error half of every `Result` small: a `CliError`
    /// carries paths and an io::Error, and the success path should not pay for
    /// that on every return.
    pub error: Box<CliError>,
    /// What it had already noticed before it stopped.
    pub warnings: Vec<String>,
}

impl Failure {
    /// Print the warnings and the error, and return the process exit code.
    pub fn report(&self) -> ExitCode {
        for line in &self.warnings {
            eprintln!("{line}");
        }
        eprintln!("error: {}", self.error);
        ExitCode::FAILURE
    }
}

impl From<CliError> for Failure {
    fn from(error: CliError) -> Self {
        Self {
            error: Box::new(error),
            warnings: Vec::new(),
        }
    }
}

impl From<WorkspaceError> for Failure {
    fn from(error: WorkspaceError) -> Self {
        CliError::from(error).into()
    }
}

impl From<ArtifactError> for Failure {
    fn from(error: ArtifactError) -> Self {
        CliError::from(error).into()
    }
}

/// Run `mind`.
pub fn run(cli: &Cli) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    let home = default_home(cli.home.as_deref());
    let home = home.as_deref();
    match cli.command.as_ref().unwrap_or(&Command::Tui) {
        Command::Flayer { command } => run_flayer(
            command.as_ref().unwrap_or(&FlayerCommand::Tui),
            &directory,
            home,
        ),
        Command::Init { skills, rules } => {
            let directories = asked_directories(skills.as_deref(), rules.as_deref());
            let (project, outcome) = MindProject::init_with(&directory, &directories)?;
            Ok(Outcome::plain(initialized(
                "mind project",
                project.name(),
                &project.mind_dir(),
                outcome,
            )))
        }
        Command::Add {
            kind,
            name,
            description,
            template,
        } => add_to(
            &project_here(&directory)?,
            *kind,
            name,
            description,
            template.as_deref(),
        ),
        Command::Edit { reference, editor } => {
            edit_in(&project_here(&directory)?, reference, editor.as_deref())
        }
        Command::Rename { reference, to } => rename_in(&project_here(&directory)?, reference, to),
        Command::Remove { reference, yes } => {
            remove_from(&project_here(&directory)?, reference, *yes)
        }
        Command::Templates => templates_of(&project_here(&directory)?),
        Command::List { kind } => {
            let project = project_here(&directory)?;
            one_project(project, wanted(*kind), |catalog, projects, level| {
                Ok(list(catalog, projects, level))
            })
        }
        Command::Show { reference } => {
            let project = project_here(&directory)?;
            let reference = Reference::parse(reference);
            one_project(project, wanted(reference.kind()), |catalog, _, level| {
                show(catalog, &reference, level)
            })
        }
        Command::Validate { target } => {
            let project = project_here(&directory)?;
            let selector = Selector::parse(target.as_deref());
            one_project(project, selector.kinds(), |catalog, _, level| {
                validate(catalog, &selector, level)
            })
        }
        Command::Link => {
            let project = project_here(&directory)?;
            let mut workspace = workspace_above(&project, home)?;
            let (entry, registration) = workspace.link(&project)?;
            Ok(Outcome::plain(linked(project.name(), &entry, registration)))
        }
        Command::Unlink => {
            let project = project_here(&directory)?;
            let mut workspace = workspace_above(&project, home)?;
            let removed = workspace.unlink(project.root())?;
            Ok(Outcome::plain(unlinked(&removed)))
        }
        Command::Load { workspaces } => {
            let project = project_here(&directory)?;
            load_command(&directory, project.root(), workspaces, home)
        }
        Command::Unload { workspaces } => {
            let project = project_here(&directory)?;
            unload_command(&directory, project.root(), workspaces, home)
        }
        Command::Install { name, from } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, home)?;
            install_named(&workspace, &project, name, from.as_deref(), &directory)
        }
        Command::Uninstall { name } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, home)?;
            uninstall_named(&workspace, &project, name)
        }
        Command::Tui => tui::run_project(&directory, home),
    }
}

/// Run `flayer`, which is the same as running `mind flayer`.
pub fn run_flayer_cli(cli: &FlayerCli) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    let home = default_home(cli.home.as_deref());
    run_flayer(
        cli.command.as_ref().unwrap_or(&FlayerCommand::Tui),
        &directory,
        home.as_deref(),
    )
}

/// Where the default workspace lives: `--home`, then `$MINDFLAYER_HOME`, then
/// the user's home directory. Empty, from either, means there is none.
///
/// A string rather than a path because empty is a meaningful answer here,
/// and one a path cannot spell.
pub fn default_home(given: Option<&str>) -> Option<PathBuf> {
    let chosen = match given {
        Some(given) => given.to_owned(),
        None => match std::env::var("MINDFLAYER_HOME") {
            Ok(from_environment) => from_environment,
            Err(_) => return std::env::home_dir(),
        },
    };
    (!chosen.is_empty()).then(|| PathBuf::from(chosen))
}

/// The workspace half, shared by `mind flayer <cmd>` and `flayer <cmd>`.
///
/// Every command that changes an artifact acts on the workspace's own unless
/// `-p` names one of its projects, and is the same function `mind` runs for
/// that project: the two levels differ in what they hold, never in the rules.
fn run_flayer(
    command: &FlayerCommand,
    directory: &Path,
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    match command {
        FlayerCommand::Init { skills, rules } => {
            let directories = asked_directories(skills.as_deref(), rules.as_deref());
            let (workspace, outcome) = FlayerWorkspace::init_with(directory, &directories)?;
            Ok(Outcome::plain(initialized(
                "flayer workspace",
                workspace.name(),
                &workspace.flayer_dir(),
                outcome,
            )))
        }

        FlayerCommand::Add {
            project,
            kind,
            name,
            description,
            template,
        } => {
            let workspace = workspace_here(directory, home)?;
            add_to(
                &holder(&workspace, project.as_deref())?,
                *kind,
                name,
                description,
                template.as_deref(),
            )
        }
        FlayerCommand::Edit {
            reference,
            project,
            editor,
        } => {
            let workspace = workspace_here(directory, home)?;
            edit_in(
                &holder(&workspace, project.as_deref())?,
                reference,
                editor.as_deref(),
            )
        }
        FlayerCommand::Rename {
            reference,
            to,
            project,
        } => {
            let workspace = workspace_here(directory, home)?;
            rename_in(&holder(&workspace, project.as_deref())?, reference, to)
        }
        FlayerCommand::Remove {
            reference,
            project,
            yes,
        } => {
            let workspace = workspace_here(directory, home)?;
            remove_from(&holder(&workspace, project.as_deref())?, reference, *yes)
        }
        FlayerCommand::Templates { project } => {
            let workspace = workspace_here(directory, home)?;
            templates_of(&holder(&workspace, project.as_deref())?)
        }

        FlayerCommand::Link { projects } => {
            let mut workspace = workspace_here(directory, home)?;
            // All or nothing: every path is opened before any is registered,
            // so a typo in the third does not leave the first two linked by a
            // command that reports failure. Opening is the check — a
            // directory with no `.mind` is not a project, and registering one
            // would only fail later, further from the command that caused it.
            let mut opened: Vec<MindProject> = Vec::new();
            let mut refused = Vec::new();
            for target in targets(directory, projects)? {
                match MindProject::open(&target) {
                    Ok(project) => opened.push(project),
                    Err(error) => refused.push(CliError::from(error)),
                }
            }
            if let Some(failure) = all_or_nothing(refused) {
                return Err(failure);
            }
            let mut text = String::new();
            for project in &opened {
                let (entry, registration) = workspace.link(project)?;
                text.push_str(&linked(project.name(), &entry, registration));
            }
            Ok(Outcome::plain(text))
        }

        FlayerCommand::Unlink { projects } => {
            let mut workspace = workspace_here(directory, home)?;
            let targets = targets(directory, projects)?;
            // All or nothing, for the reason `link` is: nothing is dropped
            // unless everything named is registered.
            let refused = targets
                .iter()
                .filter(|target| !workspace.is_linked(target))
                .map(|target| {
                    CliError::Workspace(WorkspaceError::NotRegistered {
                        path: target.clone(),
                        workspace: workspace.root().to_path_buf(),
                    })
                })
                .collect();
            if let Some(failure) = all_or_nothing(refused) {
                return Err(failure);
            }
            // One rewrite for all of them: two spellings of one project are
            // one project, not a second one to fail on halfway.
            Ok(Outcome::plain(unlinked(&workspace.unlink_all(&targets)?)))
        }

        FlayerCommand::Load { workspaces } => load_command(directory, directory, workspaces, home),
        FlayerCommand::Unload { workspaces } => {
            unload_command(directory, directory, workspaces, home)
        }

        FlayerCommand::Minds => {
            let workspace = workspace_here(directory, home)?;
            tui::run_minds(workspace.root(), home)
        }

        FlayerCommand::Scan => {
            let workspace = workspace_here(directory, home)?;
            Ok(scanned(&workspace, &scan(&workspace)))
        }

        FlayerCommand::Gather(command) => run_gather(command, directory, home),

        FlayerCommand::Install {
            project,
            name,
            from,
        } => {
            let workspace = workspace_here(directory, home)?;
            match (name, project) {
                (None, None) => {
                    let ledger = workspace.ledger().map_err(CliError::from)?;
                    install::run(&workspace, &ledger)
                }
                (Some(name), Some(project)) => install_named(
                    &workspace,
                    &pick(&workspace, project)?,
                    name,
                    from.as_deref(),
                    directory,
                ),
                // Half of one: a skill with no project to put it in, or a
                // project with nothing named to put there.
                (Some(_), None) => Err(CliError::InstallWhere.into()),
                (None, Some(_)) => Err(CliError::InstallWhat.into()),
            }
        }
        FlayerCommand::Uninstall { project, name } => {
            let workspace = workspace_here(directory, home)?;
            uninstall_named(&workspace, &pick(&workspace, project)?, name)
        }

        FlayerCommand::Tui => tui::run(directory, home),

        FlayerCommand::List { kind, project } => match project {
            Some(project) => {
                let workspace = workspace_here(directory, home)?;
                one_project(
                    pick(&workspace, project)?,
                    wanted(*kind),
                    |catalog, projects, level| Ok(list(catalog, projects, level)),
                )
            }
            None => {
                let (workspace, projects, warnings) = managed(directory, home)?;
                everywhere(
                    &workspace,
                    warnings,
                    &projects,
                    wanted(*kind),
                    |catalog, projects, level| {
                        Ok(list_workspace(catalog, projects, &workspace, level))
                    },
                )
            }
        },
        FlayerCommand::Show { reference, project } => {
            let reference = Reference::parse(reference);
            let kinds = wanted(reference.kind());
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                show(catalog, &reference, level)
            };
            match project {
                Some(project) => {
                    let workspace = workspace_here(directory, home)?;
                    one_project(pick(&workspace, project)?, kinds, read)
                }
                None => {
                    let (workspace, projects, warnings) = managed(directory, home)?;
                    everywhere(&workspace, warnings, &projects, kinds, read)
                }
            }
        }
        FlayerCommand::Validate { target, project } => {
            let selector = Selector::parse(target.as_deref());
            let kinds = selector.kinds();
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                validate(catalog, &selector, level)
            };
            match project {
                Some(project) => {
                    let workspace = workspace_here(directory, home)?;
                    one_project(pick(&workspace, project)?, kinds, read)
                }
                None => {
                    let (workspace, projects, warnings) = managed(directory, home)?;
                    everywhere(&workspace, warnings, &projects, kinds, read)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Changing artifacts, at either level
//
// Each takes the holder it acts on — a project, or the workspace's own — so
// `mind add` and `flayer add -p alpha` are one function given one project,
// and `flayer add` is the same function given the workspace.
// ---------------------------------------------------------------------------

/// What `init` was asked to put where, at either level.
///
/// Only what was asked for is set. A kind nobody mentioned is left to the
/// default rather than pinned to it here, so the default lives in one place.
fn asked_directories(skills: Option<&Path>, rules: Option<&Path>) -> Directories {
    let mut directories = Directories::default();
    if let Some(skills) = skills {
        directories = directories.with(Kind::Skill, skills);
    }
    if let Some(rules) = rules {
        directories = directories.with(Kind::Rule, rules);
    }
    directories
}

fn add_to(
    holder: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&str>,
) -> Result<Outcome, Failure> {
    let artifact = match template {
        None => create(holder, kind, name, description),
        Some(template) => {
            let template = template::find(holder, kind, template).map_err(CliError::from)?;
            create_from(holder, kind, name, description, &template)
        }
    }
    .map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "created {} {} at {}\n",
        artifact.kind(),
        artifact.name(),
        artifact.file().display()
    ));
    disowned(
        &mut outcome,
        holder,
        kind,
        name,
        "an old record said `flayer install` had put one of that name here; it is gone",
    );
    Ok(outcome)
}

fn edit_in(
    holder: &MindProject,
    reference: &str,
    editor: Option<&str>,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
    let file = target.file();
    let before = std::fs::read(&file).ok();
    let editor = editor.map_or_else(default_editor, str::to_owned);
    open_in_editor(&editor, &file)?;
    let changed = std::fs::read(&file).ok() != before;

    let mut outcome = edited(holder, &target, changed);
    if changed {
        disowned(
            &mut outcome,
            holder,
            target.kind,
            &target.name,
            "it had been installed from the shelf; edited, it is yours, and `flayer install` leaves it alone",
        );
    }
    Ok(outcome)
}

fn rename_in(holder: &MindProject, reference: &str, to: &str) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
    let renamed = lifecycle::rename(holder, &target, to).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "renamed {} {} to {} at {}\n",
        target.kind,
        target.name,
        renamed.name,
        renamed.path.display()
    ));
    disowned(
        &mut outcome,
        holder,
        target.kind,
        &target.name,
        "it had been installed from the shelf; renamed, it is yours, and `flayer install` leaves it alone",
    );
    // A stale claim on the new name would make what was just renamed look
    // like something Mindflayer put there.
    disowned(
        &mut outcome,
        holder,
        target.kind,
        &renamed.name,
        "an old record said `flayer install` had put one of that name here; it is gone",
    );
    Ok(outcome)
}

fn remove_from(holder: &MindProject, reference: &str, yes: bool) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
    // Deleting is the one thing here that cannot be taken back, so it is
    // asked for twice: once by naming it, once by saying yes.
    if !yes {
        return Err(CliError::Unconfirmed {
            kind: target.kind,
            path: target.path.clone(),
            files: lifecycle::files(&target),
        }
        .into());
    }
    lifecycle::remove(holder, &target).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "removed {} {} from {}\n",
        target.kind,
        target.name,
        target.path.display()
    ));
    disowned(
        &mut outcome,
        holder,
        target.kind,
        &target.name,
        "it had been installed from the shelf; `flayer install` can put it back",
    );
    Ok(outcome)
}

fn templates_of(holder: &MindProject) -> Result<Outcome, Failure> {
    let templates = template::templates(holder).map_err(CliError::from)?;
    let rows: Vec<Vec<String>> = templates
        .iter()
        .map(|template| {
            let origin = match &template.origin {
                Origin::BuiltIn => String::from("built in"),
                Origin::Path(path) => paths::relative_to(path, holder.root())
                    .as_deref()
                    .and_then(paths::to_config_string)
                    .unwrap_or_else(|| path.display().to_string()),
            };
            vec![
                printable(&template.name),
                template.kind.slug().to_owned(),
                printable(&origin),
            ]
        })
        .collect();
    Ok(Outcome::plain(table(&rows)))
}

// ---------------------------------------------------------------------------
// Registering, and installing into, one project
// ---------------------------------------------------------------------------

/// What `link` says it did, at either level.
fn linked(name: &str, entry: &Path, registration: Registration) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!("linked {name} as {entry}\n"),
        Registration::AlreadyRegistered => format!("{name} is already linked as {entry}\n"),
    }
}

/// What `unlink` says it did: every spelling that pointed at the project, not
/// just the first. Reporting "unlinked" while one entry still registers it is
/// the failure this reports its way out of.
fn unlinked(removed: &[PathBuf]) -> String {
    removed.iter().fold(String::new(), |mut text, entry| {
        let _ = writeln!(text, "unlinked {}", as_stored(entry));
        text
    })
}

/// `load`, at either level: the paths are typed from `directory`, and the
/// workspace they go into is found from `start` — the directory itself for
/// `flayer`, the project for `mind`. With no paths it lists what that
/// workspace loads, without making the default one to say it loads nothing.
fn load_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    if workspaces.is_empty() {
        let workspace = FlayerWorkspace::locate_except(start, &[], home)?.ok_or_else(|| {
            CliError::NowhereToLoad {
                from: start.to_path_buf(),
                verb: "list the loads of",
                named: false,
            }
        })?;
        return Ok(loaded_listing(&workspace, home));
    }
    let sources = targets(directory, workspaces)?;
    // Every source opened before anything is found or made: a typo must not
    // leave a default workspace behind in your home.
    let mut opened = Vec::new();
    let mut refused = Vec::new();
    for source in &sources {
        match FlayerWorkspace::open(source) {
            Ok(source) => opened.push(source),
            Err(error) => refused.push(CliError::from(error)),
        }
    }
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut workspace = FlayerWorkspace::locate_or_default_except(start, &sources, home)?
        .ok_or_else(|| CliError::NowhereToLoad {
            from: start.to_path_buf(),
            verb: "load into",
            named: true,
        })?;
    let mut text = String::new();
    for source in &opened {
        let (entry, registration) = workspace.load(source)?;
        text.push_str(&loaded(&workspace, source.name(), &entry, registration));
    }
    text.push_str(&acted_on("into", &workspace, home));
    let stderr = load_warnings(&workspace);
    Ok(Outcome {
        stdout: text,
        ok: stderr.is_empty(),
        stderr,
    })
}

/// `unload`, at either level: all or nothing, in one rewrite of the marker,
/// and never making the default workspace only to find it loads nothing.
fn unload_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let sources = targets(directory, workspaces)?;
    let mut workspace =
        FlayerWorkspace::locate_except(start, &sources, home)?.ok_or_else(|| {
            CliError::NowhereToLoad {
                from: start.to_path_buf(),
                verb: "unload from",
                named: true,
            }
        })?;
    let refused = sources
        .iter()
        .filter(|source| !workspace.is_loaded(source))
        .map(|source| {
            CliError::Workspace(WorkspaceError::NotLoaded {
                path: source.clone(),
                workspace: workspace.root().to_path_buf(),
            })
        })
        .collect();
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut text = String::new();
    for entry in workspace.unload_all(&sources)? {
        let _ = writeln!(text, "unloaded {}", as_stored(&entry));
    }
    text.push_str(&acted_on("from", &workspace, home));
    Ok(Outcome::plain(text))
}

/// The line naming the workspace a load or an unload changed, because that is
/// not always the one you are standing in — never when you are standing in
/// the one being loaded.
fn acted_on(preposition: &str, workspace: &FlayerWorkspace, home: Option<&Path>) -> String {
    let default = if workspace.is_default(home) {
        ", the default one"
    } else {
        ""
    };
    format!(
        "  {preposition} the workspace at {}{default}\n",
        workspace.root().display()
    )
}

/// `flayer load` on its own: each loaded workspace, what it offers or why it
/// cannot be read, and which workspace loads them.
fn loaded_listing(workspace: &FlayerWorkspace, home: Option<&Path>) -> Outcome {
    let loads = workspace.loads();
    if loads.is_empty() {
        return Outcome::plain(format!(
            "nothing loaded\n  offer another workspace's own with `flayer load <path>`\n{}",
            acted_on("in", workspace, home)
        ));
    }
    let rows: Vec<Vec<String>> = loads
        .iter()
        .enumerate()
        .map(|(index, load)| {
            let entry = as_stored(&load.entry);
            // Two spellings of one workspace are one source, offered once,
            // under the first.
            let earlier = loads[..index]
                .iter()
                .find(|earlier| same_directory(&earlier.root, &load.root));
            match (&load.workspace, earlier) {
                (Ok(source), Some(earlier)) => vec![
                    printable(&entry),
                    printable(source.name()),
                    format!("the same as {}", printable(&as_stored(&earlier.entry))),
                ],
                (Ok(source), None) => vec![
                    printable(&entry),
                    printable(source.name()),
                    offering(workspace, &entry),
                ],
                (Err(_), _) => vec![
                    printable(&entry),
                    String::from("-"),
                    String::from("cannot be opened"),
                ],
            }
        })
        .collect();
    let mut stdout = table(&rows);
    stdout.push_str(&acted_on("in", workspace, home));
    let stderr = load_warnings(workspace);
    Outcome {
        ok: stderr.is_empty(),
        stdout,
        stderr,
    }
}

/// Whether two paths name one directory: as written, or on disk.
pub(crate) fn same_directory(a: &Path, b: &Path) -> bool {
    paths::normalize(a) == paths::normalize(b)
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// What `load` says it did, with what the loaded workspace has to install.
fn loaded(
    workspace: &FlayerWorkspace,
    name: &str,
    entry: &Path,
    registration: Registration,
) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!(
            "loaded {name} as {entry}: {}\n",
            offering(workspace, &entry)
        ),
        Registration::AlreadyRegistered => format!("{name} is already loaded as {entry}\n"),
    }
}

/// "2 skills to install", or that there is nothing yet — counted from what
/// the install machinery itself offers through that entry, so the number is
/// what `flayer install` will show.
fn offering(workspace: &FlayerWorkspace, entry: &str) -> String {
    use mindflayer_core::install::{offered_by_loads, Offer};
    let offers = offered_by_loads(workspace).offers;
    let counts: Vec<String> = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let count = offers
                .iter()
                .filter(|offer| {
                    offer.kind() == kind
                        && matches!(offer, Offer::Loaded { entry: from, .. } if from == entry)
                })
                .count();
            (count > 0).then(|| plural(count, kind.slug()))
        })
        .collect();
    if counts.is_empty() {
        String::from("nothing to install yet")
    } else {
        format!("{} to install", counts.join(" and "))
    }
}

/// Put one skill into one project, from wherever it is offered.
///
/// `from` may be a path to a loaded workspace typed from `directory`, as it was
/// given to `load`. What the loaded workspaces could not offer is said as
/// warnings, whether or not the install then works: a loaded workspace that
/// has gone is why a skill is "not offered", and the error alone cannot say so.
fn install_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
    from: Option<&str>,
    directory: &Path,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Installed};

    let warnings = load_warnings(workspace);
    let with_warnings = |error: CliError| Failure {
        error: Box::new(error),
        warnings: warnings.clone(),
    };
    let ledger = workspace
        .ledger()
        .map_err(|error| with_warnings(error.into()))?;
    // As typed first, so a shelf source's address or `workspace` means what
    // it says; only when that names nothing, as a path to a loaded workspace.
    let offered = |from: Option<&str>| {
        installing::offered(workspace, &ledger, project, Kind::Skill, name, from)
    };
    let candidate = match (offered(from), from) {
        (Err(installing::InstallError::NotOffered { .. }), Some(typed)) => {
            offered(Some(&installing::resolve_from(workspace, directory, typed)))
        }
        (result, _) => result,
    }
    .map_err(|error| with_warnings(error.into()))?;
    let origin = candidate.origin();
    let into = project.name();
    let mut outcome = match installing::install(workspace, &ledger, project, &candidate)
        .map_err(|error| with_warnings(error.into()))?
    {
            Installed::Added { name, .. } => {
                Outcome::plain(format!("installed {name} into {into}, from {origin}\n"))
            }
            Installed::Updated { name, .. } => {
                Outcome::plain(format!("updated {name} in {into}, from {origin}\n"))
            }
            Installed::Unchanged { name, .. } => {
                Outcome::plain(format!("{name} in {into} was already current\n"))
            }
            // Refused rather than obeyed: somebody wrote that one.
            Installed::Foreign { name } => Outcome {
                stdout: String::new(),
                stderr: vec![format!(
                    "warning: {into} already has a {name} that Mindflayer did not put there, so it was left alone"
                )],
                ok: false,
            },
    };
    if !warnings.is_empty() {
        outcome.stderr.extend(warnings);
        outcome.ok = false;
    }
    Ok(outcome)
}

/// What the workspaces `workspace` loads could not offer, as `warning:` lines.
pub(crate) fn load_warnings(workspace: &FlayerWorkspace) -> Vec<String> {
    mindflayer_core::install::offered_by_loads(workspace)
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect()
}

/// Take one skill Mindflayer installed back out of one project.
fn uninstall_named(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    name: &str,
) -> Result<Outcome, Failure> {
    use mindflayer_core::install::{self as installing, Removed};

    let ledger = workspace.ledger().map_err(CliError::from)?;
    let from = project.name();
    match installing::uninstall(workspace, &ledger, project, Kind::Skill, name)
        .map_err(CliError::from)?
    {
        Removed::Removed { name, .. } => {
            Ok(Outcome::plain(format!("removed {name} from {from}\n")))
        }
        Removed::Foreign { name } => Ok(Outcome {
            stdout: String::new(),
            stderr: vec![format!(
                "warning: {name} in {from} was not installed by Mindflayer, so it was left alone"
            )],
            ok: false,
        }),
        // Taking out something that is not there is a typo far more often
        // than it is a no-op, the reason `unlink` says so too.
        Removed::Absent { name } => Err(CliError::NotInstalled {
            name,
            project: from.to_owned(),
        }
        .into()),
    }
}

// ---------------------------------------------------------------------------
// gather
// ---------------------------------------------------------------------------

/// Collect artifacts onto the workspace shelf, or say what is already on it.
fn run_gather(
    command: &GatherCommand,
    directory: &Path,
    home: Option<&Path>,
) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, home)?;
    let ledger = workspace.ledger().map_err(CliError::from)?;

    match command {
        GatherCommand::Git {
            url,
            path,
            reference,
        } => {
            let request = Request::git(url.clone())
                .from_subdirectory(path.clone())
                .at(reference.clone());
            let report = gather::gather(&workspace, &ledger, &request).map_err(CliError::from)?;
            Ok(gathered(url, &report))
        }
        GatherCommand::List => {
            use mindflayer_core::install::{offered_by_loads, Offer};
            // What the workspaces it loads offer first, as the install screen
            // orders them; what they cannot offer is a warning beside the
            // rest, not a reason to show nothing.
            let loads = offered_by_loads(&workspace);
            let stderr: Vec<String> = loads
                .failures
                .iter()
                .map(|failure| format!("warning: {failure}"))
                .collect();
            let mut offers: Vec<Offer> = loads.offers;
            let mut stderr = stderr;
            for gathered in ledger.gathered().map_err(CliError::from)? {
                // Listed only if it could be installed: a name that is not one
                // folder name never can be, wherever it came from.
                if mindflayer_core::install::is_folder_name(&gathered.name) {
                    offers.push(Offer::Shelf(gathered));
                } else {
                    stderr.push(format!(
                        "warning: {}: declares the name `{}`, which is not one folder name, so it is not offered",
                        gathered.path, gathered.name
                    ));
                }
            }
            let ok = stderr.is_empty();
            Ok(Outcome {
                stdout: shelf_listing(&offers, workspace.loads().is_empty()),
                stderr,
                ok,
            })
        }
    }
}

/// What one gather did, in the order it is worth reading: the source, then
/// every artifact and what happened to it, then the count.
fn gathered(url: &str, report: &Report) -> Outcome {
    let mut stdout = String::new();
    let at = report
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let _ = writeln!(stdout, "{url}{at}");

    // Added first, then updated, then unchanged: a second gather is run to
    // find out what moved, and what moved should not be at the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
        ("unchanged", &report.unchanged),
    ] {
        for artifact in harvested {
            rows.push(vec![
                label.to_owned(),
                printable(&artifact.name),
                printable(artifact.summary.as_deref().unwrap_or("")),
            ]);
        }
    }
    if rows.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        let _ = writeln!(
            stdout,
            "\n{}: {} added, {} updated, {} unchanged",
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest.
    let stderr: Vec<String> = report
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}

/// Everything that can be installed from elsewhere, with where each thing
/// comes from — spelled as `--from` takes it.
///
/// Empty, it says how to fill it — and suggests loading only when nothing is
/// loaded yet; `flayer load` on its own lists what is.
fn shelf_listing(shelf: &[mindflayer_core::install::Offer], nothing_loaded: bool) -> String {
    if shelf.is_empty() {
        return if nothing_loaded {
            String::from(
                "nothing gathered yet\n  take some with `flayer gather git <url>`, \
                 or offer a workspace's own with `flayer load <path>`\n",
            )
        } else {
            String::from(
                "nothing gathered yet, and the loaded workspaces offer nothing\n  \
                 take some with `flayer gather git <url>`; `flayer load` lists what is loaded\n",
            )
        };
    }

    // The same rule a catalog listing follows: a kind column earns its place
    // only once more than one kind is on the shelf.
    let mut kinds: Vec<Kind> = shelf.iter().map(|entry| entry.kind()).collect();
    kinds.sort();
    kinds.dedup();
    let name_kind = kinds.len() > 1;

    let rows: Vec<Vec<String>> = shelf
        .iter()
        .map(|entry| {
            let mut row = Vec::new();
            if name_kind {
                row.push(entry.kind().slug().to_owned());
            }
            row.push(printable(entry.name()));
            row.push(printable(&entry.origin()));
            row.push(printable(entry.summary().unwrap_or("")));
            row
        })
        .collect();
    table(&rows)
}

/// Two spaces in front of every line, so a block reads as belonging to the
/// line above it.
fn indent(text: &str) -> String {
    text.lines().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "  {line}");
        out
    })
}

/// A revision as a person refers to it.
fn short(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
}

// ---------------------------------------------------------------------------
// scan
// ---------------------------------------------------------------------------

/// Every repository under a workspace, and whether it is linked yet.
fn scanned(workspace: &FlayerWorkspace, scan: &Scan) -> Outcome {
    let mut stdout = String::new();
    if scan.found.is_empty() {
        let _ = writeln!(
            stdout,
            "no mind projects or git repositories under {}",
            workspace.root().display()
        );
    } else {
        let rows: Vec<Vec<String>> = scan
            .found
            .iter()
            .map(|found| {
                let what = match (found.linked, found.mind) {
                    (true, _) => "",
                    (false, true) => "mind project",
                    (false, false) => "git repository, not a mind project yet",
                };
                vec![
                    printable(&found.route),
                    String::from(if found.linked { "linked" } else { "not linked" }),
                    what.to_owned(),
                ]
            })
            .collect();
        stdout.push_str(&table(&rows));

        // Only the advice that applies: a hint about a case the listing does
        // not contain is one more line to read past.
        let mut hints = Vec::new();
        if scan.found.iter().any(|found| !found.linked && found.mind) {
            hints.push("link one with `flayer link <path>`");
        }
        if scan.found.iter().any(|found| !found.linked && !found.mind) {
            hints.push("a repository needs `mind -C <path> init` before it can be linked");
        }
        if !hints.is_empty() {
            stdout.push('\n');
            for hint in hints {
                let _ = writeln!(stdout, "  {hint}");
            }
        }
    }

    let mut stderr: Vec<String> = scan
        .failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    if scan.truncated {
        stderr.push(format!(
            "warning: stopped after listing {MAX_LISTED} folders; anything further down can still be linked by path"
        ));
    }
    let ok = stderr.is_empty();
    Outcome { stdout, stderr, ok }
}

// ---------------------------------------------------------------------------
// edit, rename, remove
// ---------------------------------------------------------------------------

/// The editor to use when none was named: `$VISUAL`, then `$EDITOR`, then the
/// one every system has — the order git looks in.
fn default_editor() -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from(if cfg!(windows) { "notepad" } else { "vi" }))
}

/// Open `file` in `editor` and wait for it to be closed.
///
/// On Unix the editor runs through `sh`, as git runs it, so `code --wait` and
/// an editor whose path has a space in it both work. The file is passed as an
/// argument rather than spliced into the command, so its name is never read
/// by the shell.
fn open_in_editor(editor: &str, file: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    let mut command = {
        let mut command = std::process::Command::new("sh");
        command
            .arg("-c")
            .arg(format!("{editor} \"$1\""))
            .arg(editor)
            .arg(file);
        command
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut words = editor.split_whitespace();
        let mut command = std::process::Command::new(words.next().unwrap_or("notepad"));
        command.args(words).arg(file);
        command
    };

    let status = command.status().map_err(|source| CliError::Editor {
        editor: editor.to_owned(),
        source,
    })?;
    if !status.success() {
        return Err(CliError::EditorFailed {
            editor: editor.to_owned(),
            status: status.to_string(),
        });
    }
    Ok(())
}

/// What editing came to: whether anything changed, and whether what is there
/// now still passes `validate` — the question worth answering the moment the
/// editor closes, not at the next CI run.
fn edited(project: &MindProject, target: &Target, changed: bool) -> Outcome {
    let what = format!("{} {}", target.kind, target.name);
    let head = if changed {
        format!("edited {what}")
    } else {
        format!("left {what} unchanged")
    };
    let artifact = match lifecycle::load(project, target.kind, &target.path, &target.name) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Outcome {
                stdout: format!("{head}, and it does not read\n"),
                stderr: vec![format!("warning: {error}")],
                ok: false,
            }
        }
    };

    let issues = artifact.validate();
    if issues.is_empty() {
        return Outcome::plain(format!("{head}: ok\n"));
    }
    let mut stdout = format!("{head}: {}\n", plural(issues.len(), "problem"));
    for issue in issues {
        let _ = writeln!(stdout, "  - {issue}");
    }
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok: false,
    }
}

/// Drop a record saying `flayer install` put `name` into `project`, and say so
/// when there was one.
///
/// Failing to is a warning rather than an error: the change the record was
/// about has already been made, and reporting it as not made would be false.
fn disowned(outcome: &mut Outcome, project: &MindProject, kind: Kind, name: &str, why: &str) {
    match mindflayer_core::install::disown(project, kind, name) {
        Ok(true) => {
            let _ = writeln!(outcome.stdout, "  {why}");
        }
        Ok(false) => {}
        Err(error) => {
            outcome.stderr.push(format!("warning: {error}"));
            outcome.ok = false;
        }
    }
}

/// "; it manages alpha, beta", or what to do when it manages nothing.
fn known_projects(known: &[String]) -> String {
    if known.is_empty() {
        String::from("; it manages none yet — `flayer link <path>` adds one")
    } else {
        format!("; it manages {}", known.join(", "))
    }
}

/// What `mind remove` would delete, as a sentence.
fn deleting(path: &Path, kind: &Kind, files: &usize) -> String {
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

// ---------------------------------------------------------------------------
// Locating what a command acts on
// ---------------------------------------------------------------------------

/// The directory this invocation works in.
fn working_directory(path: Option<&Path>) -> Result<PathBuf, CliError> {
    match path {
        Some(dir) => Ok(dir.to_path_buf()),
        None => std::env::current_dir().map_err(CliError::CurrentDirectory),
    }
}

/// Resolve a path a user typed, against the directory the command works in.
fn resolve(directory: &Path, path: &Path) -> Result<PathBuf, CliError> {
    let joined = directory.join(path);
    let absolute = std::path::absolute(&joined).map_err(|source| CliError::Resolve {
        path: joined.clone(),
        source,
    })?;
    Ok(paths::normalize(&absolute))
}

/// A registry entry spelled the way the marker file spells it.
///
/// Not `Path::display`, which uses the platform separator: entries are stored
/// with forward slashes so a workspace registered on one platform resolves on
/// the other, and on Windows `display` would report `..\collapse` for a line
/// that reads `../collapse`. What a command says it wrote has to be what
/// someone opening the file will find.
fn as_stored(entry: &Path) -> String {
    paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
}

/// The mind project the caller is standing in.
fn project_here(directory: &Path) -> Result<MindProject, CliError> {
    MindProject::locate(directory)?.ok_or_else(|| CliError::NotInProject(directory.to_path_buf()))
}

/// The flayer workspace the caller is standing in: the one at or above
/// `directory`, or else the default one in `home`.
fn workspace_here(directory: &Path, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_default(directory, home)?
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// Every path a command was given, resolved and each named once: the same
/// project typed twice is one project.
fn targets(directory: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut resolved: Vec<PathBuf> = Vec::new();
    for path in paths {
        let target = resolve(directory, path)?;
        if !resolved.contains(&target) {
            resolved.push(target);
        }
    }
    Ok(resolved)
}

/// A command given several things refuses them all when any is wrong, and
/// says why about each one rather than only the first.
fn all_or_nothing(mut refused: Vec<CliError>) -> Option<Failure> {
    if refused.is_empty() {
        return None;
    }
    let first = refused.remove(0);
    Some(Failure {
        error: Box::new(first),
        warnings: refused
            .iter()
            .map(|error| format!("error: {error}"))
            .collect(),
    })
}

/// The workspace here and the projects it manages.
///
/// Only the registered ones. A workspace manages what it was told to manage,
/// so a project that happens to sit inside it is not in scope until it is
/// linked — otherwise `flayer list` would answer a different question
/// depending on which directory it was run from.
fn managed(
    directory: &Path,
    home: Option<&Path>,
) -> Result<(FlayerWorkspace, Vec<MindProject>, Vec<String>), CliError> {
    let workspace = workspace_here(directory, home)?;
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    Ok((workspace, projects, warnings))
}

/// The workspace above a project, whether or not it manages it yet: where
/// `mind link` registers it — the default one in `home` when there is none
/// up the tree.
fn workspace_above(
    project: &MindProject,
    home: Option<&Path>,
) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate_or_default(project.root(), home)?
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// The workspace above a project that manages it: the one whose ledger
/// records what is installed there.
fn managing(project: &MindProject, home: Option<&Path>) -> Result<FlayerWorkspace, CliError> {
    let workspace = workspace_above(project, home)?;
    if !workspace.is_linked(project.root()) {
        return Err(CliError::NotManaged {
            project: project.root().to_path_buf(),
            workspace: workspace.root().to_path_buf(),
        });
    }
    Ok(workspace)
}

/// What a workspace command changes: the workspace's own artifacts, or the
/// one project `-p` names.
fn holder(workspace: &FlayerWorkspace, project: Option<&str>) -> Result<MindProject, CliError> {
    match project {
        None => Ok(workspace.own()),
        Some(wanted) => pick(workspace, wanted),
    }
}

/// The one managed project `wanted` names: by its name, by its entry as the
/// marker spells it, or by its path from the workspace.
fn pick(workspace: &FlayerWorkspace, wanted: &str) -> Result<MindProject, CliError> {
    let target = paths::normalize(&workspace.root().join(wanted));
    let mut found: Vec<MindProject> = Vec::new();
    let mut broken = None;
    for member in workspace.members() {
        let spelled = as_stored(&member.entry) == wanted || member.root == target;
        match member.project {
            // Two spellings of one directory are one project, not a choice.
            Ok(project) if spelled || project.name() == wanted => {
                if !found.iter().any(|other| other.root() == project.root()) {
                    found.push(project);
                }
            }
            Err(error) if spelled => broken = Some(error),
            _ => {}
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        // Named exactly, but it does not open: why is more use than "no
        // such project".
        0 => Err(match broken {
            Some(error) => CliError::Workspace(error),
            None => CliError::UnknownProject {
                wanted: wanted.to_owned(),
                known: workspace
                    .members()
                    .into_iter()
                    .filter_map(|member| member.project.ok())
                    .map(|project| project.name().to_owned())
                    .collect(),
            },
        }),
        _ => Err(CliError::AmbiguousProject {
            wanted: wanted.to_owned(),
            matches: found
                .iter()
                .map(|project| as_stored(&workspace.entry_for(project.root())))
                .collect(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Commands that read skills
// ---------------------------------------------------------------------------

/// The kinds a command should discover, given the one it was pointed at.
fn wanted(kind: Option<Kind>) -> Vec<Kind> {
    kind.map_or_else(|| Kind::ALL.to_vec(), |kind| vec![kind])
}

/// What a `validate` positional named.
///
/// One positional serving both is a grammar the user learns once: a bare kind
/// word selects a kind, anything else names an artifact, and `kind/name`
/// always names an artifact. An artifact literally called `rules` is reachable
/// as `rule/rules`.
enum Selector {
    /// Everything in scope.
    Everything,
    /// One kind of thing.
    OneKind(Kind),
    /// One artifact.
    One(Reference),
}

impl Selector {
    fn parse(word: Option<&str>) -> Self {
        match word {
            None => Selector::Everything,
            Some(word) => match word.parse::<Kind>() {
                Ok(kind) => Selector::OneKind(kind),
                Err(_) => Selector::One(Reference::parse(word)),
            },
        }
    }

    /// The kinds worth discovering to answer it.
    fn kinds(&self) -> Vec<Kind> {
        match self {
            Selector::Everything => Kind::ALL.to_vec(),
            Selector::OneKind(kind) => vec![*kind],
            Selector::One(reference) => wanted(reference.kind()),
        }
    }
}

/// Run a reading command over one project: `mind`'s level, or `flayer -p`.
fn one_project<F>(project: MindProject, kinds: Vec<Kind>, command: F) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    let projects = [project];
    let catalog = Catalog::discover_kinds(&projects, &kinds);
    reading(Vec::new(), &catalog, &projects, Level::Project, command)
}

/// Run a reading command over the whole workspace: its own artifacts and
/// every managed project's, in one catalog.
fn everywhere<F>(
    workspace: &FlayerWorkspace,
    warnings: Vec<String>,
    projects: &[MindProject],
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    // Only the kinds the workspace keeps apart from a project at its root:
    // the rest are that project's, and are found as its.
    let own: Vec<Kind> = workspace
        .own_kinds()
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .collect();
    let catalog = Catalog::discover_kinds(projects, &kinds)
        .merge(Catalog::discover_kinds(&[workspace.own()], &own));
    let holders = Holders::of(workspace, &own);
    reading(
        warnings,
        &catalog,
        projects,
        Level::Workspace(&holders),
        command,
    )
}

/// Hand a catalog to a reading command, with the warnings that came with it.
fn reading<F>(
    mut stderr: Vec<String>,
    catalog: &Catalog,
    projects: &[MindProject],
    level: Level,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    // Unreadable artifacts are reported alongside whatever was found: a broken
    // file is a thing the user wants to know about, not a reason to be told
    // nothing about the forty next to it.
    stderr.extend(
        catalog
            .failures()
            .iter()
            .map(|failure| format!("warning: {failure}")),
    );

    let (stdout, ok) = match command(catalog, projects, level) {
        Ok(reported) => reported,
        Err(error) => {
            return Err(Failure {
                error: Box::new(error),
                warnings: stderr,
            })
        }
    };
    let clean = stderr.is_empty();
    Ok(Outcome {
        stdout,
        stderr,
        ok: ok && clean,
    })
}

/// One line per artifact.
fn list(catalog: &Catalog, projects: &[MindProject], level: Level) -> (String, bool) {
    if catalog.is_empty() {
        // Where it looked, not where the marker is: a project that keeps its
        // skills somewhere else is exactly the one whose empty listing needs
        // explaining.
        let mut text = String::from("nothing found\n");
        if let Level::Workspace(holders) = level {
            for (_, directory) in &holders.own {
                let _ = writeln!(text, "  {WORKSPACE}: {}", directory.display());
            }
        }
        for project in projects {
            for kind in Kind::ALL {
                let _ = writeln!(
                    text,
                    "  {}: {}",
                    project.name(),
                    project.directory_for(kind).display()
                );
            }
        }
        return (text, true);
    }

    let artifacts: Vec<&Artifact> = catalog.artifacts().iter().collect();
    let qualify = Qualify::over(&artifacts, level);
    let rows: Vec<Vec<String>> = artifacts
        .iter()
        .map(|artifact| {
            let mut row = Vec::new();
            if qualify.project {
                row.push(printable(holder_of(artifact, level)));
            }
            if qualify.kind {
                row.push(artifact.kind().slug().to_owned());
            }
            row.push(printable(artifact.name()));
            row.push(summary(artifact));
            row
        })
        .collect();
    (table(&rows), true)
}

/// Rows padded into columns, the last one left ragged.
fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut text = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == columns {
                line.push_str(cell);
            } else {
                let padding = widths[column].saturating_sub(cell.chars().count());
                let _ = write!(line, "{cell}{}  ", " ".repeat(padding));
            }
        }
        // An artifact with no summary would otherwise leave the padding of the
        // column before it hanging off the end of the line.
        let _ = writeln!(text, "{}", line.trim_end());
    }
    text
}

/// `list` at the workspace level, where an empty result has two very
/// different causes and only one of them is answered by linking something.
fn list_workspace(
    catalog: &Catalog,
    projects: &[MindProject],
    workspace: &FlayerWorkspace,
    level: Level,
) -> (String, bool) {
    // The workspace's own artifacts are worth listing whether or not it
    // manages anything yet.
    if projects.is_empty() && catalog.is_empty() {
        let registered = workspace.config().projects.len();
        // Saying "manages no projects yet" when the registry is full of
        // entries that simply would not open sends the user to link something
        // that is already linked.
        return if registered == 0 {
            (
                format!(
                    "{} manages no projects yet\n  link one with `flayer link <path>`\n",
                    workspace.name()
                ),
                true,
            )
        } else {
            (
                format!(
                    "{} manages {}, none of which could be opened\n  \
                     see the warnings, or drop a stale entry with `flayer unlink <path>`\n",
                    workspace.name(),
                    plural(registered, "project"),
                ),
                false,
            )
        };
    }
    list(catalog, projects, level)
}

/// What a report calls the workspace, as the holder of its own artifacts.
pub const WORKSPACE: &str = "workspace";

/// Who holds what, once a report is above any one project.
struct Holders {
    /// Where the workspace keeps its own, for the kinds it keeps apart.
    own: Vec<(Kind, PathBuf)>,
}

impl Holders {
    fn of(workspace: &FlayerWorkspace, kinds: &[Kind]) -> Self {
        let own = workspace.own();
        Self {
            own: kinds
                .iter()
                .map(|kind| (*kind, own.directory_for(*kind)))
                .collect(),
        }
    }

    /// Whether an artifact is the workspace's own rather than a project's.
    ///
    /// Told by where it sits rather than by the root it was found from: a
    /// workspace and a project at its root share a root, not their folders.
    fn is_own(&self, artifact: &Artifact) -> bool {
        self.own.iter().any(|(kind, directory)| {
            *kind == artifact.kind() && artifact.path().starts_with(directory)
        })
    }
}

/// What holds an artifact, for display: the project it is in, or the
/// workspace for the workspace's own.
fn holder_of<'a>(artifact: &'a Artifact, level: Level<'_>) -> &'a str {
    match level {
        Level::Workspace(holders) if holders.is_own(artifact) => WORKSPACE,
        _ => artifact.project_name().unwrap_or("?"),
    }
}

/// What a report has to name to keep its rows apart.
///
/// One rule, asked twice. A column or a qualifier that could not have
/// disambiguated anything is a column the reader has to skip.
#[derive(Debug, Clone, Copy)]
struct Qualify {
    kind: bool,
    project: bool,
}

impl Qualify {
    /// What is worth naming about a set of artifacts.
    fn over(artifacts: &[&Artifact], level: Level) -> Self {
        let first = artifacts.first();
        Self {
            kind: first
                .is_some_and(|first| artifacts.iter().any(|other| other.kind() != first.kind())),
            // At the project level there is one project by definition, so the
            // question only arises above it.
            project: matches!(level, Level::Workspace(_))
                && first.is_some_and(|first| {
                    artifacts.iter().any(|other| {
                        other.project() != first.project()
                            || holder_of(other, level) != holder_of(first, level)
                    })
                }),
        }
    }
}

/// How an artifact is labelled in a report: as bare as the context allows.
fn label(artifact: &Artifact, qualify: Qualify, level: Level) -> String {
    let name = if qualify.kind {
        artifact.qualified_name()
    } else {
        artifact.name().to_owned()
    };
    if qualify.project {
        format!("{name} ({})", holder_of(artifact, level))
    } else {
        name
    }
}

/// Text safe to put in a row.
///
/// A name or an opening line comes from a file somebody wrote, and a carriage
/// return in one would let a row overwrite the row above it on the terminal —
/// hiding an entry, or fabricating one that looks real.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The first line of a summary, short enough to sit in a column.
fn summary(artifact: &Artifact) -> String {
    const BUDGET: usize = 72;

    let line = printable(artifact.summary().unwrap_or(""));
    if line.chars().count() <= BUDGET {
        return line;
    }
    let kept: String = line.chars().take(BUDGET - 1).collect();
    format!("{}…", kept.trim_end())
}

/// One artifact in full: where it is, what it declares, and its contents.
fn show(
    catalog: &Catalog,
    reference: &Reference,
    level: Level,
) -> Result<(String, bool), CliError> {
    let matches = catalog.find(reference);
    if matches.is_empty() {
        return Err(CliError::UnknownArtifact(reference.typed().to_owned()));
    }
    let qualify = Qualify::over(&matches, level);

    let mut text = String::new();
    for (index, artifact) in matches.iter().enumerate() {
        // One name can belong to two kinds, or to two projects. Both are
        // legal, so both are shown and the separator makes it obvious there
        // was more than one.
        if index > 0 {
            text.push_str("\n---\n\n");
        }
        let _ = writeln!(text, "{}", label(artifact, qualify, level));
        let _ = writeln!(text, "{}", artifact.file().display());
        let _ = writeln!(text);

        // Only what the artifact actually declares. A rule declares nothing,
        // so it gets no metadata block rather than an empty one.
        if let Declared::Skill(manifest) = artifact.declared() {
            let _ = writeln!(text, "{}", manifest.description);
            if let Some(tools) = &manifest.allowed_tools {
                let _ = writeln!(text, "\nallowed-tools: {}", tools.join(", "));
            }
            if let Some(license) = &manifest.license {
                let _ = writeln!(text, "license: {license}");
            }
            let _ = writeln!(text);
        }
        let _ = writeln!(text, "{}", artifact.contents()?.trim_end());
    }
    Ok((text, true))
}

/// Every artifact's problems, or a line saying it has none.
fn validate(
    catalog: &Catalog,
    selector: &Selector,
    level: Level,
) -> Result<(String, bool), CliError> {
    let artifacts: Vec<&Artifact> = match selector {
        Selector::One(reference) => {
            let matches = catalog.find(reference);
            if matches.is_empty() {
                return Err(CliError::UnknownArtifact(reference.typed().to_owned()));
            }
            matches
        }
        // The catalog was already built for the kinds the selector wanted, so
        // filtering again here would be filtering twice.
        Selector::Everything | Selector::OneKind(_) => catalog.artifacts().iter().collect(),
    };

    if artifacts.is_empty() {
        return Ok((String::from("nothing to check\n"), true));
    }
    let qualify = Qualify::over(&artifacts, level);

    let mut text = String::new();
    let mut invalid = 0usize;
    for artifact in &artifacts {
        let issues = artifact.validate();
        let label = label(artifact, qualify, level);
        if issues.is_empty() {
            let _ = writeln!(text, "{label}: ok");
            continue;
        }
        invalid += 1;
        let _ = writeln!(text, "{label}: {}", plural(issues.len(), "problem"));
        for issue in issues {
            let _ = writeln!(text, "  - {issue}");
        }
    }

    // Counting only what loaded would report "0 invalid" for a project whose
    // files could not be read at all, with the reason on stderr where a CI log
    // will not put it next to the verdict.
    let unreadable = catalog.failures().len();
    let unreadable = if unreadable == 0 {
        String::new()
    } else {
        format!(", {} unreadable", plural(unreadable, "file"))
    };
    let _ = writeln!(
        text,
        "\n{} checked, {invalid} invalid{unreadable}",
        counted(&artifacts)
    );
    Ok((text, invalid == 0 && unreadable.is_empty()))
}

/// "2 skills", "2 skills and 1 rule": what was looked at, by kind.
///
/// Naming the kinds only when they differ is the same rule the columns follow.
fn counted(artifacts: &[&Artifact]) -> String {
    let parts: Vec<String> = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let count = artifacts
                .iter()
                .filter(|artifact| artifact.kind() == kind)
                .count();
            (count > 0).then(|| {
                let noun = if count == 1 {
                    kind.slug()
                } else {
                    kind.folder()
                };
                format!("{count} {noun}")
            })
        })
        .collect();
    match parts.split_last() {
        None => String::from("nothing"),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// What `init` says it did, at either level.
fn initialized(kind: &str, name: &str, marker: &Path, outcome: Initialization) -> String {
    let marker = marker.display();
    match outcome {
        Initialization::Created => format!("initialized {kind} `{name}` in {marker}\n"),
        Initialization::AlreadyInitialized => {
            format!("{kind} `{name}` already initialized in {marker}\n")
        }
    }
}

/// "1 skill", "2 skills".
fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Why a command could not run at all, as opposed to running and finding
/// problems: those are in the [`Outcome`].
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
    Template(#[from] TemplateError),
    #[error("nothing removed: this deletes {}; add --yes to go ahead", deleting(.path, .kind, .files))]
    Unconfirmed {
        kind: Kind,
        path: PathBuf,
        files: usize,
    },
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
    #[error("{project}: not managed by the workspace at {workspace}; `mind link` adds it")]
    NotManaged {
        project: PathBuf,
        workspace: PathBuf,
    },
    #[error("no project called `{wanted}` in this workspace{}", known_projects(.known))]
    UnknownProject { wanted: String, known: Vec<String> },
    #[error("`{wanted}` names more than one project — {} — so name it by its path", .matches.join(", "))]
    AmbiguousProject {
        wanted: String,
        matches: Vec<String>,
    },
    #[error("say which project to install into — `flayer install -p <project> <skill>` — or run `flayer install` alone for the screen")]
    InstallWhere,
    #[error("say which skill to install — `flayer install -p <project> <skill>` — or run `flayer install` alone for the screen")]
    InstallWhat,
    #[error("{project} has no {name} installed, so there is nothing to take out")]
    NotInstalled { name: String, project: String },
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
