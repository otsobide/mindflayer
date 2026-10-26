//! The commands that read artifacts: `list`, `show` and `validate`, at either
//! level.
//!
//! Each is handed a catalog of what is in scope, so the two levels differ only
//! in what that catalog holds and in how much a row has to name to be told
//! apart from the others.

use std::fmt::Write as _;
use std::path::PathBuf;

use mindflayer_core::{Artifact, Catalog, Declared, FlayerWorkspace, Kind, MindProject, Reference};

use crate::render::{plural, printable, table};
use crate::{CliError, Failure, Outcome};

/// Which level a command reports at.
///
/// It decides one thing: whether naming what holds each artifact tells the
/// reader anything. Inside a single project it does not; above them it does,
/// and the workspace's own are named as the workspace's.
#[derive(Clone, Copy)]
pub(crate) enum Level<'a> {
    Project,
    Workspace(&'a Holders),
}

/// The kinds a command should discover, given the one it was pointed at.
pub(crate) fn wanted(kind: Option<Kind>) -> Vec<Kind> {
    kind.map_or_else(|| Kind::ALL.to_vec(), |kind| vec![kind])
}

/// What a `validate` positional named.
///
/// One positional serving both is a grammar the user learns once: a bare kind
/// word selects a kind, anything else names an artifact, and `kind:name`
/// always names an artifact. An artifact literally called `rules` is reachable
/// as `rule:rules`.
pub(crate) enum Selector {
    /// Everything in scope.
    Everything,
    /// One kind of thing.
    OneKind(Kind),
    /// One artifact.
    One(Reference),
}

impl Selector {
    pub(crate) fn parse(word: Option<&str>) -> Self {
        match word {
            None => Selector::Everything,
            Some(word) => match word.parse::<Kind>() {
                Ok(kind) => Selector::OneKind(kind),
                Err(_) => Selector::One(Reference::parse(word)),
            },
        }
    }

    /// The kinds worth discovering to answer it.
    pub(crate) fn kinds(&self) -> Vec<Kind> {
        match self {
            Selector::Everything => Kind::ALL.to_vec(),
            Selector::OneKind(kind) => vec![*kind],
            Selector::One(reference) => wanted(reference.kind()),
        }
    }
}

/// Run a reading command over one project: `mind`'s level, or `flayer -p`.
pub(crate) fn one_project<F>(
    project: MindProject,
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    let projects = [project];
    let catalog = Catalog::discover_kinds(&projects, &kinds);
    reading(Vec::new(), &catalog, &projects, Level::Project, command)
}

/// Run a reading command over the whole workspace: its own artifacts and
/// every managed project's, in one catalog.
pub(crate) fn everywhere<F>(
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
pub(crate) fn list(catalog: &Catalog, projects: &[MindProject], level: Level) -> (String, bool) {
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

/// `list` at the workspace level, where an empty result has two very
/// different causes and only one of them is answered by linking something.
pub(crate) fn list_workspace(
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
pub(crate) struct Holders {
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
pub(crate) fn show(
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
pub(crate) fn validate(
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
