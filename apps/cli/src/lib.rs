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
use mindflayer_core::paths;
use mindflayer_core::{
    Catalog, FlayerWorkspace, Initialization, MindProject, Registration, Skill, SkillError,
    WorkspaceError,
};
use thiserror::Error;

/// Manage the agent skills in a mind project.
#[derive(Debug, Parser)]
#[command(name = "mind", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a mind project here.
    Init,

    /// List this project's skills.
    #[command(alias = "ls")]
    List,

    /// Show one skill in full, front matter and instructions.
    Show {
        /// The skill's `name`, as declared in its front matter.
        name: String,
    },

    /// Check this project's skills against what an agent requires.
    Validate {
        /// Check only this skill [default: all of them].
        name: Option<String>,
    },

    /// Act on the flayer workspace instead. Also reachable as `flayer <cmd>`.
    #[command(subcommand)]
    Flayer(FlayerCommand),
}

/// The `flayer` binary: a shortcut into the workspace half of `mind`.
#[derive(Debug, Parser)]
#[command(name = "flayer", version, about, long_about = None)]
pub struct FlayerCli {
    #[command(subcommand)]
    pub command: FlayerCommand,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum FlayerCommand {
    /// Create a flayer workspace here.
    Init,

    /// List the skills of every project this workspace manages.
    #[command(alias = "ls")]
    List,

    /// Show one skill in full, from any project this workspace manages.
    Show {
        /// The skill's `name`, as declared in its front matter.
        name: String,
    },

    /// Check every managed project's skills.
    Validate {
        /// Check only this skill [default: all of them].
        name: Option<String>,
    },

    /// Register a mind project with this workspace.
    Link {
        /// The project's directory, the one holding `.mind`.
        project: PathBuf,
    },

    /// Drop a registered mind project.
    Unlink {
        /// The project's directory, as registered. It need not still exist.
        project: PathBuf,
    },
}

/// Which level a command reports at.
///
/// It decides one thing: whether naming the project each skill came from tells
/// the reader anything. Inside a single project it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Project,
    Workspace,
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
    pub fn report(&self) -> ExitCode {
        print!("{}", self.stdout);
        for line in &self.stderr {
            eprintln!("{line}");
        }
        if self.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// Run `mind`.
pub fn run(cli: &Cli) -> Result<Outcome, CliError> {
    let directory = working_directory(cli.directory.as_deref())?;
    match &cli.command {
        Command::Flayer(command) => run_flayer(command, &directory),
        Command::Init => {
            let (project, outcome) = MindProject::init(&directory)?;
            Ok(Outcome::plain(initialized(
                "mind project",
                project.name(),
                &project.mind_dir(),
                outcome,
            )))
        }
        Command::List => {
            let project = project_here(&directory)?;
            with_catalog(&[project], |catalog, projects| {
                Ok(list(catalog, projects, Level::Project))
            })
        }
        Command::Show { name } => {
            let project = project_here(&directory)?;
            with_catalog(&[project], |catalog, _| show(catalog, name, Level::Project))
        }
        Command::Validate { name } => {
            let project = project_here(&directory)?;
            with_catalog(&[project], |catalog, _| {
                validate(catalog, name.as_deref(), Level::Project)
            })
        }
    }
}

/// Run `flayer`, which is the same as running `mind flayer`.
pub fn run_flayer_cli(cli: &FlayerCli) -> Result<Outcome, CliError> {
    let directory = working_directory(cli.directory.as_deref())?;
    run_flayer(&cli.command, &directory)
}

/// The workspace half, shared by `mind flayer <cmd>` and `flayer <cmd>`.
fn run_flayer(command: &FlayerCommand, directory: &Path) -> Result<Outcome, CliError> {
    match command {
        FlayerCommand::Init => {
            let (workspace, outcome) = FlayerWorkspace::init(directory)?;
            Ok(Outcome::plain(initialized(
                "flayer workspace",
                workspace.name(),
                &workspace.flayer_dir(),
                outcome,
            )))
        }

        FlayerCommand::Link { project: path } => {
            let mut workspace = workspace_here(directory)?;
            let target = resolve(directory, path)?;
            // Opening it is the check: a directory with no `.mind` is not a
            // project, and registering one would only fail later, further from
            // the command that caused it.
            let project = MindProject::open(&target)?;
            let name = project.name().to_owned();

            let (entry, outcome) = workspace.link(&project)?;
            let entry = as_stored(&entry);
            Ok(Outcome::plain(match outcome {
                Registration::Added => format!("linked {name} as {entry}\n"),
                Registration::AlreadyRegistered => {
                    format!("{name} is already linked as {entry}\n")
                }
            }))
        }

        FlayerCommand::Unlink { project: path } => {
            let mut workspace = workspace_here(directory)?;
            let target = resolve(directory, path)?;
            let removed = workspace.unlink(&target)?;
            Ok(Outcome::plain(format!(
                "unlinked {}\n",
                as_stored(&removed)
            )))
        }

        FlayerCommand::List => {
            let (workspace, projects, warnings) = managed(directory)?;
            with_warnings(warnings, projects, |catalog, projects| {
                Ok(list_workspace(catalog, projects, &workspace))
            })
        }
        FlayerCommand::Show { name } => {
            let (_, projects, warnings) = managed(directory)?;
            with_warnings(warnings, projects, |catalog, _| {
                show(catalog, name, Level::Workspace)
            })
        }
        FlayerCommand::Validate { name } => {
            let (_, projects, warnings) = managed(directory)?;
            with_warnings(warnings, projects, |catalog, _| {
                validate(catalog, name.as_deref(), Level::Workspace)
            })
        }
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

/// The flayer workspace the caller is standing in.
fn workspace_here(directory: &Path) -> Result<FlayerWorkspace, CliError> {
    FlayerWorkspace::locate(directory)?
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// The workspace here and the projects it manages.
///
/// Only the registered ones. A workspace manages what it was told to manage,
/// so a project that happens to sit inside it is not in scope until it is
/// linked — otherwise `flayer list` would answer a different question
/// depending on which directory it was run from.
fn managed(directory: &Path) -> Result<(FlayerWorkspace, Vec<MindProject>, Vec<String>), CliError> {
    let workspace = workspace_here(directory)?;
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| format!("warning: {failure}"))
        .collect();
    Ok((workspace, projects, warnings))
}

// ---------------------------------------------------------------------------
// Commands that read skills
// ---------------------------------------------------------------------------

/// Build a catalog over `projects` and hand it to `command`.
fn with_catalog<F>(projects: &[MindProject], command: F) -> Result<Outcome, CliError>
where
    F: FnOnce(&Catalog, &[MindProject]) -> Result<(String, bool), CliError>,
{
    with_warnings(Vec::new(), projects.to_vec(), command)
}

/// The same, starting from warnings the caller has already collected.
fn with_warnings<F>(
    mut stderr: Vec<String>,
    projects: Vec<MindProject>,
    command: F,
) -> Result<Outcome, CliError>
where
    F: FnOnce(&Catalog, &[MindProject]) -> Result<(String, bool), CliError>,
{
    let catalog = Catalog::discover(&projects);

    // Unreadable skills are reported alongside whatever was found: a broken
    // file is a thing the user wants to know about, not a reason to be told
    // nothing about the forty next to it.
    stderr.extend(
        catalog
            .failures()
            .iter()
            .map(|failure| format!("warning: {failure}")),
    );

    let (stdout, ok) = command(&catalog, &projects)?;
    let clean = stderr.is_empty();
    Ok(Outcome {
        stdout,
        stderr,
        ok: ok && clean,
    })
}

/// One line per skill.
fn list(catalog: &Catalog, projects: &[MindProject], level: Level) -> (String, bool) {
    if catalog.is_empty() {
        let mut text = String::from("no skills found\n");
        for project in projects {
            let _ = writeln!(
                text,
                "  {}: {}",
                project.name(),
                project.skills_dir().display()
            );
        }
        return (text, true);
    }

    let name_width = catalog
        .skills()
        .iter()
        .map(|skill| skill.name().chars().count())
        .max()
        .unwrap_or(0);

    let mut text = String::new();
    if level == Level::Project {
        for skill in catalog.skills() {
            let _ = writeln!(text, "{:name_width$}  {}", skill.name(), summary(skill));
        }
        return (text, true);
    }

    let project_width = catalog
        .skills()
        .iter()
        .map(|skill| project_of(skill).chars().count())
        .max()
        .unwrap_or(0);
    for skill in catalog.skills() {
        let _ = writeln!(
            text,
            "{:project_width$}  {:name_width$}  {}",
            project_of(skill),
            skill.name(),
            summary(skill),
        );
    }
    (text, true)
}

/// `list` at the workspace level, where an empty result is more often "you
/// have not linked anything yet" than "these projects have no skills".
fn list_workspace(
    catalog: &Catalog,
    projects: &[MindProject],
    workspace: &FlayerWorkspace,
) -> (String, bool) {
    if projects.is_empty() {
        return (
            format!(
                "{} manages no projects yet\n  link one with `flayer link <path>`\n",
                workspace.name()
            ),
            true,
        );
    }
    list(catalog, projects, Level::Workspace)
}

/// The project a skill came from, for display.
fn project_of(skill: &Skill) -> &str {
    skill.project_name().unwrap_or("?")
}

/// How a skill is labelled in a report: bare inside one project, qualified by
/// its project across several.
fn label(skill: &Skill, level: Level) -> String {
    match level {
        Level::Project => skill.name().to_owned(),
        Level::Workspace => format!("{} ({})", skill.name(), project_of(skill)),
    }
}

/// The first line of a description, short enough to sit in a column.
fn summary(skill: &Skill) -> String {
    const BUDGET: usize = 72;

    let line = skill.description().lines().next().unwrap_or("").trim();
    if line.chars().count() <= BUDGET {
        return line.to_owned();
    }
    let kept: String = line.chars().take(BUDGET - 1).collect();
    format!("{}…", kept.trim_end())
}

/// One skill in full: where it is, what it declares, and its instructions.
fn show(catalog: &Catalog, name: &str, level: Level) -> Result<(String, bool), CliError> {
    let matches = catalog.find(name);
    if matches.is_empty() {
        return Err(CliError::UnknownSkill(name.to_owned()));
    }

    let mut text = String::new();
    for (index, skill) in matches.iter().enumerate() {
        // The same name in two projects is legal, so both are shown and the
        // separator makes it obvious there were two rather than one.
        if index > 0 {
            text.push_str("\n---\n\n");
        }
        let _ = writeln!(text, "{}", label(skill, level));
        let _ = writeln!(text, "{}", skill.path().display());
        let _ = writeln!(text);
        let _ = writeln!(text, "{}", skill.description());
        if let Some(tools) = &skill.manifest.allowed_tools {
            let _ = writeln!(text, "\nallowed-tools: {}", tools.join(", "));
        }
        if let Some(license) = &skill.manifest.license {
            let _ = writeln!(text, "license: {license}");
        }
        let _ = writeln!(text, "\n{}", skill.instructions()?.trim_end());
    }
    Ok((text, true))
}

/// Every skill's problems, or a line saying it has none.
fn validate(
    catalog: &Catalog,
    name: Option<&str>,
    level: Level,
) -> Result<(String, bool), CliError> {
    let skills: Vec<&Skill> = match name {
        Some(name) => {
            let matches = catalog.find(name);
            if matches.is_empty() {
                return Err(CliError::UnknownSkill(name.to_owned()));
            }
            matches
        }
        None => catalog.skills().iter().collect(),
    };

    if skills.is_empty() {
        return Ok((String::from("no skills to check\n"), true));
    }

    let mut text = String::new();
    let mut invalid = 0usize;
    for skill in &skills {
        let issues = skill.validate();
        if issues.is_empty() {
            let _ = writeln!(text, "{}: ok", label(skill, level));
            continue;
        }
        invalid += 1;
        let _ = writeln!(
            text,
            "{}: {}",
            label(skill, level),
            plural(issues.len(), "problem")
        );
        for issue in issues {
            let _ = writeln!(text, "  - {issue}");
        }
    }

    let _ = writeln!(
        text,
        "\n{} checked, {invalid} invalid",
        plural(skills.len(), "skill")
    );
    Ok((text, invalid == 0))
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
    #[error("no skill named `{0}`")]
    UnknownSkill(String),
    #[error(transparent)]
    Skill(#[from] SkillError),
}
