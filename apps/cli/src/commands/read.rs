//! The commands that read artifacts: `list`, `show` and `validate`, at either
//! level.
//!
//! Each is handed a catalog of what is in scope, so the two levels differ only
//! in what that catalog holds and in how much a row has to name to be told
//! apart from the others.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::PathBuf;

use mindflayer_core::reading::{Shown, Validation};
use mindflayer_core::view::Listing;
use mindflayer_core::{Artifact, Catalog, FlayerWorkspace, Kind, MindProject, Reference};

use super::consulting;
use crate::render::{as_json, plural, printable, table, verdict};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

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
    warnings: Vec<Warning>,
    projects: &[MindProject],
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    workspace_wide(workspace, warnings, projects, kinds, false, command)
}

/// `everywhere`, and what the workspace has gathered onto its shelf too.
///
/// For `validate`, which is where a gathered skill's problems are reported:
/// gathering takes one that breaks the rules rather than hiding it, so
/// something has to be able to say so again later. Listing the shelf is
/// `gather list`'s, so `list` and `show` keep to what is held.
pub(crate) fn everywhere_and_the_shelf<F>(
    workspace: &FlayerWorkspace,
    warnings: Vec<Warning>,
    projects: &[MindProject],
    kinds: Vec<Kind>,
    command: F,
) -> Result<Outcome, Failure>
where
    F: FnOnce(&Catalog, &[MindProject], Level) -> Result<(String, bool), CliError>,
{
    workspace_wide(workspace, warnings, projects, kinds, true, command)
}

/// The two above.
fn workspace_wide<F>(
    workspace: &FlayerWorkspace,
    warnings: Vec<Warning>,
    projects: &[MindProject],
    kinds: Vec<Kind>,
    shelf: bool,
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
    // Every kind asked for, when the shelf is read at all: which of them are
    // ever on it is discovery's to know.
    let shelved = if shelf { kinds.clone() } else { Vec::new() };
    let catalog = Catalog::discover_kinds(projects, &kinds)
        .merge(Catalog::discover_kinds(&[workspace.own()], &own))
        .merge(Catalog::discover_gathered(workspace, &shelved));
    let holders = Holders::of(workspace, &own, &shelved);
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
    mut stderr: Vec<Warning>,
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
            .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}"))),
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
        looked_in(&mut text, projects);
        return (text, true);
    }

    let artifacts: Vec<&Artifact> = catalog.artifacts().iter().collect();
    let qualify = Qualify::over(&artifacts, level);
    let rows: Vec<Vec<String>> = artifacts
        .iter()
        .map(|artifact| {
            let mut row = Vec::new();
            if qualify.project {
                row.push(printable(&holder_of(artifact, level)));
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

/// `mind list --json`: the project here as a snapshot holds it, read for
/// `kinds`, with the warnings and the exit status of `mind list`.
///
/// Against the workspace `mind status` reads it against, the one `mind`
/// consults, when that one links it: how each stands is that workspace's
/// to say, from its ledger, and read alone a copy it installed would stand
/// as somebody's. Consulted as `mind status` consults it, so one found that
/// cannot be read is a warning beside the listing, the project read alone,
/// and one `-W` or the activation names that is no workspace refuses it:
/// the one place the JSON reads what the text does not, and says so.
pub(crate) fn listed_here(
    project: MindProject,
    kinds: Vec<Kind>,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let mut listing = None;
    let text = consulting(&project, lookup, |workspace| {
        listing = Some(Listing::of_project(&project, workspace, &kinds));
        one_project(
            project.clone(),
            kinds.clone(),
            |catalog, projects, level| Ok(list(catalog, projects, level)),
        )
    })?;
    let listing = listing.expect("a listing consulted reads the project");
    Ok(as_json(&listing, text))
}

/// Where a reading command looked in each of `projects`, a line a folder,
/// for a report that found nothing to say so.
///
/// Every folder discovery read, as core says: a project marked by its
/// settings is read in its agents' folders too. A kind the project keeps no
/// folder for was looked for nowhere.
pub(crate) fn looked_in(text: &mut String, projects: &[MindProject]) {
    for project in projects {
        for directory in Kind::ALL
            .into_iter()
            .flat_map(|kind| Catalog::folders(project, kind))
        {
            let _ = writeln!(text, "  {}: {}", project.name(), directory.display());
        }
    }
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

/// What a report calls the shelf, before the source an artifact came from.
pub const SHELF: &str = "shelf";

/// Who holds what, once a report is above any one project.
pub(crate) struct Holders {
    /// Where the workspace keeps its own, for the kinds it keeps apart.
    own: Vec<(Kind, PathBuf)>,
    /// Where the shelf keeps each kind, for the kinds a report reads there.
    shelf: Vec<(Kind, PathBuf)>,
}

impl Holders {
    fn of(workspace: &FlayerWorkspace, own_kinds: &[Kind], shelf_kinds: &[Kind]) -> Self {
        let own = workspace.own();
        Self {
            own: own_kinds
                .iter()
                .filter_map(|kind| Some((*kind, own.directory_for(*kind)?)))
                .collect(),
            shelf: shelf_kinds
                .iter()
                .map(|kind| (*kind, workspace.gathered_dir(*kind)))
                .collect(),
        }
    }

    /// The folder of the source an artifact on the shelf came from, when it
    /// is on the shelf.
    ///
    /// The folder rather than the URL: it is what tells two sources offering
    /// one name apart on disk, and it needs no ledger to read.
    fn source_of<'a>(&self, artifact: &'a Artifact) -> Option<&'a str> {
        let (_, shelf) = self
            .shelf
            .iter()
            .find(|(kind, _)| *kind == artifact.kind())?;
        let mut below = artifact.path().strip_prefix(shelf).ok()?.components();
        below.next()?.as_os_str().to_str()
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

/// What holds an artifact, for display: the project it is in, the workspace
/// for the workspace's own, or `shelf/<source>` for what was gathered.
fn holder_of<'a>(artifact: &'a Artifact, level: Level<'_>) -> Cow<'a, str> {
    if let Level::Workspace(holders) = level {
        if let Some(source) = holders.source_of(artifact) {
            return Cow::Owned(format!("{SHELF}/{source}"));
        }
        if holders.is_own(artifact) {
            return Cow::Borrowed(WORKSPACE);
        }
    }
    Cow::Borrowed(artifact.project_name().unwrap_or("?"))
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
///
/// Made printable, as a row is: a declared name is from a file somebody wrote,
/// gathered ones from anywhere, and a carriage return in one would let the
/// label pass for another artifact's line.
fn label(artifact: &Artifact, qualify: Qualify, level: Level) -> String {
    let name = if qualify.kind {
        artifact.qualified_name()
    } else {
        artifact.name().to_owned()
    };
    let label = if qualify.project {
        format!("{name} ({})", holder_of(artifact, level))
    } else {
        name
    };
    printable(&label)
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

/// One artifact in full: where it is, what it declares, and its contents;
/// with each artifact shown kept in `found`, for `--json` to print in place
/// of the text.
///
/// Laid out from what core read of each ([`Shown`]), the file read once,
/// so the text and the JSON cannot show different things.
pub(crate) fn show(
    catalog: &Catalog,
    reference: &Reference,
    level: Level,
    found: &mut Option<Vec<Shown>>,
) -> Result<(String, bool), CliError> {
    let matches = catalog.find(reference);
    if matches.is_empty() {
        return Err(CliError::UnknownArtifact(reference.typed().to_owned()));
    }
    let qualify = Qualify::over(&matches, level);
    let shown = matches
        .iter()
        .map(|artifact| Shown::of(artifact))
        .collect::<Result<Vec<Shown>, _>>()?;

    let mut text = String::new();
    for (index, (artifact, shown)) in matches.iter().zip(&shown).enumerate() {
        // One name can belong to two kinds, or to two projects. Both are
        // legal, so both are shown and the separator makes it obvious there
        // was more than one.
        if index > 0 {
            text.push_str("\n---\n\n");
        }
        let _ = writeln!(text, "{}", label(artifact, qualify, level));
        let _ = writeln!(text, "{}", shown.file.display());
        let _ = writeln!(text);

        // Only what the artifact actually declares. A rule that declares
        // nothing, as most do, gets no metadata block rather than an empty one.
        match shown.kind {
            Kind::Skill => {
                let _ = writeln!(text, "{}", shown.description.as_deref().unwrap_or(""));
                // One line per key, each made printable: a value is from a
                // file somebody wrote, and a carriage return in one could pass
                // for a key it does not declare.
                let mut keys = String::new();
                if let Some(tools) = &shown.allowed_tools {
                    let _ = writeln!(keys, "allowed-tools: {}", printable(&tools.join(", ")));
                }
                if let Some(license) = &shown.license {
                    let _ = writeln!(keys, "license: {}", printable(license));
                }
                if let Some(compatibility) = &shown.compatibility {
                    let _ = writeln!(keys, "compatibility: {}", printable(compatibility));
                }
                // Only the entries that are strings: the rest are
                // `validate`'s to report, and are still in the file for
                // whoever reads it.
                if let Some(metadata) = shown.metadata.as_ref().filter(|meta| !meta.is_empty()) {
                    keys.push_str("metadata:\n");
                    for (key, value) in metadata {
                        let _ = writeln!(keys, "  {}: {}", printable(key), printable(value));
                    }
                }
                if !keys.is_empty() {
                    let _ = write!(text, "\n{keys}");
                }
                let _ = writeln!(text);
            }
            // Laid out as a skill's is, a part at a time, since either part
            // may be missing.
            Kind::Rule => {
                let mut parts = Vec::new();
                if let Some(description) = &shown.description {
                    parts.push(format!("{description}\n"));
                }
                // One pattern a line rather than joined like `allowed-tools`:
                // a brace expansion such as `*.{ts,tsx}` has commas of its own.
                if let Some(paths) = &shown.paths {
                    let mut keys = String::from("paths:\n");
                    for pattern in paths {
                        let _ = writeln!(keys, "  - {}", printable(pattern));
                    }
                    parts.push(keys);
                }
                if !parts.is_empty() {
                    let _ = writeln!(text, "{}", parts.join("\n"));
                }
            }
        }
        // Blank lines trimmed at both ends, not indentation: the body usually
        // opens with a blank line after the fence, and the line above is
        // already one.
        let body = shown.body.trim_start_matches(['\n', '\r']).trim_end();
        let _ = writeln!(text, "{body}");
    }
    *found = Some(shown);
    Ok((text, true))
}

/// Every artifact's problems, or a line saying it has none, and whether
/// all passes; with what was found kept in `found`, for `--json` to print
/// in place of the text.
///
/// What it found is core's ([`Validation`]), the counts the verdict follows
/// among it, so the text and the JSON cannot come to different verdicts.
pub(crate) fn validate(
    catalog: &Catalog,
    selector: &Selector,
    level: Level,
    found: &mut Option<Validation>,
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
    let validation = Validation::of(&artifacts, catalog.failures());
    let reported = verdicts(&artifacts, &validation, level);
    *found = Some(validation);
    Ok(reported)
}

/// What [`validate`] says of `artifacts`, as `validation` found them.
fn verdicts(artifacts: &[&Artifact], validation: &Validation, level: Level) -> (String, bool) {
    if artifacts.is_empty() {
        return (String::from("nothing to check\n"), true);
    }
    let qualify = Qualify::over(artifacts, level);

    let mut text = String::new();
    for (artifact, checked) in artifacts.iter().zip(&validation.artifacts) {
        verdict(&mut text, &label(artifact, qualify, level), &checked.issues);
    }

    // Counting only what loaded would report "0 invalid" for a project whose
    // files could not be read at all, with the reason on stderr where a CI log
    // will not put it next to the verdict. Copies of one name that differ
    // were read, so they are counted apart, for the same reason.
    let mut tally = String::new();
    if validation.unreadable > 0 {
        let _ = write!(
            tally,
            ", {} unreadable",
            plural(validation.unreadable, "file")
        );
    }
    if validation.differing > 0 {
        let _ = write!(tally, ", {} with copies that differ", validation.differing);
    }
    let _ = writeln!(
        text,
        "\n{} checked, {} invalid{tally}",
        counted(artifacts),
        validation.invalid
    );
    (text, validation.passes())
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
