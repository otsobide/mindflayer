//! The command lines `mind` and `flayer` take, as clap types.
//!
//! Only the grammar, apart from the work, so the whole surface reads in one
//! place. The binaries parse with these types and so does every line the TUI
//! runs, which is why a feature cannot reach the TUI without a command.

use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Args, Parser, Subcommand};
use mindflayer_core::{Kind, Shell, DEFAULT_SUBDIRECTORY};

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

    // A string rather than a path for the reason `home` is one: empty is an
    // answer, and a path cannot spell it.
    /// The workspace to use, by its directory or its `.mindflayer`, wherever
    /// you are [default: $MINDFLAYER_WORKSPACE, then the one at or above
    /// here, then the default one; empty to name none and ignore
    /// $MINDFLAYER_WORKSPACE].
    #[arg(short = 'W', long = "workspace", value_name = "DIR", global = true)]
    pub workspace: Option<String>,

    #[command(flatten)]
    pub agent_homes: AgentHomeOptions,
}

/// Where each agent keeps what it reads for every project: global options on
/// both binaries, which `agent_homes` decides beside `default_home`.
///
/// Options, rather than variables read where a check needs them, so a test
/// can pin all four to none the way it pins `--home=`. Strings for the same
/// reason `--home` is one: empty means none, and a path cannot spell it.
//
// Each field is named after its flag rather than after its agent: a global
// argument shares a namespace with every subcommand's own arguments, and
// `agents` is what an option naming a project's agents would be called.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
#[command(next_help_heading = "Agent homes")]
pub struct AgentHomeOptions {
    /// Claude Code's configuration folder [default: $CLAUDE_CONFIG_DIR, then
    /// ~/.claude; empty for none].
    #[arg(long = "claude-config-dir", value_name = "DIR", global = true)]
    pub claude_config_dir: Option<String>,

    /// Codex's home [default: $CODEX_HOME, then ~/.codex; empty for none].
    #[arg(long = "codex-home", value_name = "DIR", global = true)]
    pub codex_home: Option<String>,

    /// OpenCode's configuration folder [default: $OPENCODE_CONFIG_DIR, then
    /// ~/.config/opencode; empty for none].
    #[arg(long = "opencode-config-dir", value_name = "DIR", global = true)]
    pub opencode_config_dir: Option<String>,

    /// The folder several agents share for your own skills [default:
    /// ~/.agents; empty for none].
    #[arg(long = "agents-dir", value_name = "DIR", global = true)]
    pub agents_dir: Option<String>,
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
        /// Its name, or `kind:name` when one name belongs to two kinds.
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

    // A string rather than a path for the reason `home` is one: empty is an
    // answer, and a path cannot spell it.
    /// The workspace to use, by its directory or its `.mindflayer`, wherever
    /// you are [default: $MINDFLAYER_WORKSPACE, then the one at or above
    /// here, then the default one; empty to name none and ignore
    /// $MINDFLAYER_WORKSPACE].
    #[arg(short = 'W', long = "workspace", value_name = "DIR", global = true)]
    pub workspace: Option<String>,

    #[command(flatten)]
    pub agent_homes: AgentHomeOptions,
}

#[derive(Debug, Subcommand)]
pub enum FlayerCommand {
    /// Create a flayer workspace here, or in the directory given.
    Init {
        /// The directory to make it in, made as well when it is not there
        /// [default: the directory the command works in].
        ///
        /// So a workspace kept for one purpose can be made from anywhere, the
        /// way `python -m venv DIR` makes a virtualenv, and then activated.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

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

    /// Print the shell code that activates a workspace for every command the
    /// shell runs, as a Python virtualenv's `activate` does:
    /// `eval "$(flayer activate DIR)"`. `flayer-deactivate` undoes it.
    ///
    /// It writes nothing, and prints no code when there is nothing to
    /// activate, so a failed `eval` changes nothing either.
    Activate {
        /// The workspace, by its directory or its `.mindflayer` [default: the
        /// one a command run here would use].
        #[arg(value_name = "DIR")]
        path: Option<PathBuf>,

        /// The shell to write it for [default: the one $SHELL names].
        #[arg(long, value_name = "SHELL", value_parser = shells())]
        shell: Option<Shell>,
    },

    /// Say which workspace a command run here would use, and why: named with
    /// -W, activated, found walking up, or the default one.
    Which,

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

    /// Check the workspace's own artifacts, every managed project's, and what
    /// is on the shelf.
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

/// What `--shell` takes, from core's list rather than spelled again here, as
/// possible values so `--help` lists them and a typo is refused with them.
fn shells() -> impl TypedValueParser<Value = Shell> {
    PossibleValuesParser::new(Shell::ALL.map(Shell::name))
        .map(|name| name.parse::<Shell>().expect("every listed shell parses"))
}
