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
    Artifact, ArtifactError, Catalog, Declared, FlayerWorkspace, Initialization, Kind, MindProject,
    Reference, Registration, WorkspaceError,
};
use thiserror::Error;

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

    /// Act on the flayer workspace instead. Also reachable as `flayer <cmd>`.
    #[command(subcommand)]
    Flayer(FlayerCommand),
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

    /// List the artifacts of every project this workspace manages.
    #[command(alias = "ls")]
    List {
        /// Only this kind: `skills` or `rules` [default: every kind].
        kind: Option<Kind>,
    },

    /// Show one artifact in full, from any project this workspace manages.
    Show {
        /// Its name, or `kind/name` when one name belongs to two kinds.
        reference: String,
    },

    /// Check every managed project's artifacts.
    Validate {
        /// A kind (`rules`), or one artifact by name [default: everything].
        target: Option<String>,
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
        Command::List { kind } => {
            let project = project_here(&directory)?;
            with_catalog(&[project], wanted(*kind), |catalog, projects| {
                Ok(list(catalog, projects, Level::Project))
            })
        }
        Command::Show { reference } => {
            let project = project_here(&directory)?;
            let reference = Reference::parse(reference);
            with_catalog(&[project], wanted(reference.kind()), |catalog, _| {
                show(catalog, &reference, Level::Project)
            })
        }
        Command::Validate { target } => {
            let project = project_here(&directory)?;
            let selector = Selector::parse(target.as_deref());
            with_catalog(&[project], selector.kinds(), |catalog, _| {
                validate(catalog, &selector, Level::Project)
            })
        }
    }
}

/// Run `flayer`, which is the same as running `mind flayer`.
pub fn run_flayer_cli(cli: &FlayerCli) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    run_flayer(&cli.command, &directory)
}

/// The workspace half, shared by `mind flayer <cmd>` and `flayer <cmd>`.
fn run_flayer(command: &FlayerCommand, directory: &Path) -> Result<Outcome, Failure> {
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
            // Every spelling that pointed at the project, not just the first:
            // reporting "unlinked" while one entry still registers it is the
            // failure this reports its way out of.
            let mut text = String::new();
            for entry in &removed {
                let _ = writeln!(text, "unlinked {}", as_stored(entry));
            }
            Ok(Outcome::plain(text))
        }

        FlayerCommand::List { kind } => {
            let (workspace, projects, warnings) = managed(directory)?;
            with_warnings(warnings, projects, wanted(*kind), |catalog, projects| {
                Ok(list_workspace(catalog, projects, &workspace))
            })
        }
        FlayerCommand::Show { reference } => {
            let (_, projects, warnings) = managed(directory)?;
            let reference = Reference::parse(reference);
            with_warnings(
                warnings,
                projects,
                wanted(reference.kind()),
                |catalog, _| show(catalog, &reference, Level::Workspace),
            )
        }
        FlayerCommand::Validate { target } => {
            let (_, projects, warnings) = managed(directory)?;
            let selector = Selector::parse(target.as_deref());
            with_warnings(warnings, projects, selector.kinds(), |catalog, _| {
                validate(catalog, &selector, Level::Workspace)
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

/// Build a catalog over `projects` and hand it to `command`.
fn with_catalog<F>(
    projects: &[MindProject],
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject]) -> Result<(String, bool), CliError>,
{
    with_warnings(Vec::new(), projects.to_vec(), kinds, command)
}

/// The same, starting from warnings the caller has already collected.
fn with_warnings<F>(
    mut stderr: Vec<String>,
    projects: Vec<MindProject>,
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject]) -> Result<(String, bool), CliError>,
{
    let catalog = Catalog::discover_kinds(&projects, &kinds);

    // Unreadable artifacts are reported alongside whatever was found: a broken
    // file is a thing the user wants to know about, not a reason to be told
    // nothing about the forty next to it.
    stderr.extend(
        catalog
            .failures()
            .iter()
            .map(|failure| format!("warning: {failure}")),
    );

    let (stdout, ok) = match command(&catalog, &projects) {
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
        let mut text = String::from("nothing found\n");
        for project in projects {
            let _ = writeln!(
                text,
                "  {}: {}",
                project.name(),
                project.mind_dir().display()
            );
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
                row.push(printable(project_of(artifact)));
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
) -> (String, bool) {
    if projects.is_empty() {
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
    list(catalog, projects, Level::Workspace)
}

/// The project an artifact came from, for display.
fn project_of(artifact: &Artifact) -> &str {
    artifact.project_name().unwrap_or("?")
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
            project: level == Level::Workspace
                && first.is_some_and(|first| {
                    artifacts
                        .iter()
                        .any(|other| other.project() != first.project())
                }),
        }
    }
}

/// How an artifact is labelled in a report: as bare as the context allows.
fn label(artifact: &Artifact, qualify: Qualify) -> String {
    let name = if qualify.kind {
        artifact.qualified_name()
    } else {
        artifact.name().to_owned()
    };
    if qualify.project {
        format!("{name} ({})", project_of(artifact))
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
        let _ = writeln!(text, "{}", label(artifact, qualify));
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
        let label = label(artifact, qualify);
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
    #[error("nothing named `{0}` here")]
    UnknownArtifact(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}
