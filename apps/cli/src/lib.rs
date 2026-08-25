//! Command line interface for Mindflayer: create mind projects and flayer
//! workspaces, and list, inspect and check the skills they hold.
//!
//! The parser, the work and the rendering all live here rather than in
//! `main.rs` so the tests drive the real command surface instead of shelling
//! out to a binary, and so a later front end can reuse the same entry points.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use mindflayer_core::{
    Catalog, FlayerWorkspace, Initialization, MindProject, Skill, SkillError, WorkspaceError,
    FLAYER_DIR, MIND_DIR,
};
use thiserror::Error;

/// Manage agent skills across mind projects.
#[derive(Debug, Parser)]
#[command(name = "mind", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Directory to work in [default: the current directory].
    #[arg(short = 'C', long = "path", value_name = "DIR", global = true)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a mind project here, or a flayer workspace over several.
    Init {
        /// What to create.
        #[arg(value_enum, default_value_t = Kind::Mind)]
        kind: Kind,
    },

    /// List the skills in scope.
    #[command(alias = "ls")]
    List,

    /// Show one skill in full, front matter and instructions.
    Show {
        /// The skill's `name`, as declared in its front matter.
        name: String,
    },

    /// Check skills against what an agent requires of them.
    Validate {
        /// Check only this skill [default: all of them].
        name: Option<String>,
    },
}

/// The two things `init` can create.
///
/// `Mind` is first because it is the default: holding skills is what most
/// directories are for, and a workspace is the rarer, deliberate step above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// A `.mind` project, holding skills of its own.
    Mind,
    /// A `.mindflayer` workspace, orchestrating several mind projects.
    Flayer,
}

impl Cli {
    /// The directory this invocation works in.
    pub fn directory(&self) -> Result<PathBuf, CliError> {
        match &self.path {
            Some(dir) => Ok(dir.clone()),
            None => std::env::current_dir().map_err(CliError::CurrentDirectory),
        }
    }
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

/// Run a parsed command line.
pub fn run(cli: &Cli) -> Result<Outcome, CliError> {
    let directory = cli.directory()?;

    match &cli.command {
        Command::Init { kind } => init(*kind, &directory),
        Command::List => with_catalog(&directory, |catalog, projects| Ok(list(catalog, projects))),
        Command::Show { name } => with_catalog(&directory, |catalog, _| show(catalog, name)),
        Command::Validate { name } => {
            let name = name.as_deref();
            with_catalog(&directory, move |catalog, _| validate(catalog, name))
        }
    }
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

/// Create a workspace or a project in `directory`.
fn init(kind: Kind, directory: &Path) -> Result<Outcome, CliError> {
    let (kind_name, marker, name, outcome) = match kind {
        Kind::Flayer => {
            let (workspace, outcome) = FlayerWorkspace::init(directory)?;
            (
                "flayer workspace",
                workspace.flayer_dir(),
                workspace.name().to_owned(),
                outcome,
            )
        }
        Kind::Mind => {
            let (project, outcome) = MindProject::init(directory)?;
            (
                "mind project",
                project.mind_dir(),
                project.name().to_owned(),
                outcome,
            )
        }
    };

    let stdout = match outcome {
        Initialization::Created => {
            format!("initialized {kind_name} `{name}` in {}\n", marker.display())
        }
        Initialization::AlreadyInitialized => format!(
            "{kind_name} `{name}` already initialized in {}\n",
            marker.display()
        ),
    };

    Ok(Outcome {
        stdout,
        stderr: Vec::new(),
        ok: true,
    })
}

// ---------------------------------------------------------------------------
// Commands that read skills
// ---------------------------------------------------------------------------

/// The mind projects a command works over, and what could not be reached.
struct WorkingSet {
    projects: Vec<MindProject>,
    warnings: Vec<String>,
}

/// Resolve the working set, build its catalog and hand both to `command`.
///
/// Every reading command shares this preamble, and sharing it is what keeps
/// them agreeing on which projects are in scope.
fn with_catalog<F>(directory: &Path, command: F) -> Result<Outcome, CliError>
where
    F: FnOnce(&Catalog, &[MindProject]) -> Result<(String, bool), CliError>,
{
    let working = working_set(directory)?;
    let catalog = Catalog::discover(&working.projects);

    // Unreadable skills are reported alongside whatever was found: a broken
    // file is a thing the user wants to know about, not a reason to be told
    // nothing about the forty next to it.
    let mut stderr = working.warnings;
    stderr.extend(
        catalog
            .failures()
            .iter()
            .map(|failure| format!("warning: {failure}")),
    );

    let (stdout, ok) = command(&catalog, &working.projects)?;
    let clean = stderr.is_empty();
    Ok(Outcome {
        stdout,
        stderr,
        ok: ok && clean,
    })
}

/// Which mind projects are in scope from `directory`.
///
/// A flayer workspace above contributes every project it references; the mind
/// project the caller is standing in contributes itself, whether or not the
/// workspace knows about it yet.
fn working_set(directory: &Path) -> Result<WorkingSet, CliError> {
    let workspace = FlayerWorkspace::locate(directory)?;
    let here = MindProject::locate(directory)?;

    let mut projects = Vec::new();
    let mut warnings = Vec::new();

    if let Some(workspace) = &workspace {
        let (registered, failures) = workspace.projects();
        projects.extend(registered);
        warnings.extend(failures.iter().map(|failure| format!("warning: {failure}")));
    }
    if let Some(here) = here {
        if !projects.iter().any(|other| other.root() == here.root()) {
            projects.push(here);
        }
    } else if workspace.is_none() {
        return Err(CliError::Nowhere(directory.to_path_buf()));
    }

    projects.sort_by(|a, b| a.root().cmp(b.root()));
    Ok(WorkingSet { projects, warnings })
}

/// One line per skill: project, name, and the first line of the description.
fn list(catalog: &Catalog, projects: &[MindProject]) -> (String, bool) {
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
        if projects.is_empty() {
            text.push_str("  no mind projects in scope\n");
        }
        return (text, true);
    }

    let project_width = catalog
        .skills()
        .iter()
        .map(|skill| skill.project_name().unwrap_or("?").chars().count())
        .max()
        .unwrap_or(0);
    let name_width = catalog
        .skills()
        .iter()
        .map(|skill| skill.name().chars().count())
        .max()
        .unwrap_or(0);

    let mut text = String::new();
    for skill in catalog.skills() {
        let _ = writeln!(
            text,
            "{:project_width$}  {:name_width$}  {}",
            skill.project_name().unwrap_or("?"),
            skill.name(),
            summary(skill),
        );
    }
    (text, true)
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
fn show(catalog: &Catalog, name: &str) -> Result<(String, bool), CliError> {
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
        let _ = writeln!(
            text,
            "{} ({})",
            skill.name(),
            skill.project_name().unwrap_or("?")
        );
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
fn validate(catalog: &Catalog, name: Option<&str>) -> Result<(String, bool), CliError> {
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
        let project = skill.project_name().unwrap_or("?");
        if issues.is_empty() {
            let _ = writeln!(text, "{} ({project}): ok", skill.name());
            continue;
        }
        invalid += 1;
        let _ = writeln!(
            text,
            "{} ({project}): {}",
            skill.name(),
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
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(
        "{} is not inside a mind project or a flayer workspace \
         (run `mind init` to create a {MIND_DIR} here, or `mind init flayer` for a {FLAYER_DIR})",
        .0.display()
    )]
    Nowhere(PathBuf),
    #[error("no skill named `{0}`")]
    UnknownSkill(String),
    #[error(transparent)]
    Skill(#[from] SkillError),
}
