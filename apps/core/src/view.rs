//! Everything a workspace holds, read in one go for a front end to draw: the
//! snapshot every front end reads, the TUI first and the web panel next.
//!
//! One read rather than each front end putting its view together from the
//! reports, as the TUI did: which failure belongs to which artifact, which
//! install nothing of is found, how many of its own a loaded workspace
//! offers, are answers, and two front ends working them out would come to
//! two of them. Core builds and orders it, from the same types the CLI
//! prints, and a front end only chooses how to show it.
//!
//! - **A read, and nothing more.** No file is made or changed: the ledger is
//!   opened only through [`FlayerWorkspace::ledger_for_reading`], the
//!   default workspace is never made for it, and no scrub runs. A ledger
//!   from an older schema is migrated, as every open migrates it, which is
//!   upkeep of Mindflayer's own file rather than a change.
//! - **No scan.** What could be linked comes from [`crate::scan`], a query
//!   of its own run when somebody asks, since a walk of the disk is no price
//!   to pay for every live update of a screen.
//! - **Its names are a contract**, as a report's are: every field by its
//!   name, a kind, an agent and a state by the word a command line takes,
//!   and every URL through [`redacted`](crate::url::redacted), where it is
//!   written. [`FORMAT`] moves when a field goes or changes what it holds.
//! - **Failures are collected**, beside what they are about: a project's in
//!   the project, the library's in the library, and what belongs to neither
//!   in [`Snapshot::warnings`]. A ledger that cannot be read is about every
//!   project's records and the shelf alike, so each of them says it.

use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::slice;

use serde::Serialize;

use crate::agent::Agent;
use crate::artifact::ValidationIssue;
use crate::catalog::Catalog;
use crate::code::Said;
use crate::install::{self, LoadFailure, Offer};
use crate::kind::Kind;
use crate::known::{self, KnownWorkspace};
use crate::ledger::{Entry, Gathered, LedgerError, Source};
use crate::lifecycle;
use crate::paths;
use crate::relink;
use crate::resolve::{self, Inputs, Intent, Reason, ResolveError, Resolved};
use crate::settings::{ProjectId, Settings, Share};
use crate::status::{ArtifactStatus, Missed, ProjectStatus, State, Unreached};
use crate::template::{self, Template};
use crate::workspace::{
    same_directory, FlayerWorkspace, MarkerFile, Member, MindProject, WorkspaceError,
};

/// The version of what a snapshot holds, which it carries as `format`.
///
/// Its own rather than the CLI's envelope version: the snapshot is no
/// command's report, and a front end reading it holds it to this number.
/// It moves, as the envelope's does, when a field goes or changes what it
/// holds, and not when one is added.
pub const FORMAT: u32 = 1;

/// How many of the latest actions a snapshot carries, newest first: enough
/// to say what happened lately, and a bound on what every update carries.
pub const HISTORY: usize = 20;

/// Everything a workspace holds, as every front end shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// [`FORMAT`].
    pub format: u32,
    /// The workspace it is of, and why that one.
    pub workspace: Workspace,
    /// Every kind this build knows, in report order: what a front end lays
    /// its columns out by, so a kind added to core reaches it without a
    /// change of its own.
    pub kinds: Vec<Kind>,
    /// Every agent this build knows, in report order, for the same reason.
    pub agents: Vec<Agent>,
    /// Every state an artifact can stand in, in report order.
    pub states: Vec<State>,
    /// The workspace's own artifacts.
    pub library: Library,
    /// Every entry in its registry, in the order its marker lists them, a
    /// second spelling of one directory included: each is what `unlink` is
    /// given.
    pub projects: Vec<Project>,
    /// Every workspace it loads, in the order its marker lists them.
    pub loaded: Vec<LoadedWorkspace>,
    /// Everything gathered onto its shelf, by kind, then name, then source.
    pub shelf: Vec<Gathered>,
    /// Every source the shelf is filled from, by URL.
    pub sources: Vec<Source>,
    /// Every workspace this machine knows ([`crate::known`]), this one
    /// among them.
    pub known: Vec<KnownWorkspace>,
    /// The latest actions, newest first, at most [`HISTORY`].
    pub history: Vec<Entry>,
    /// What could not be read and belongs to no project or library: two
    /// projects holding one id, what a loaded workspace cannot offer, a
    /// ledger whose shelf or history would not read.
    pub warnings: Vec<Said>,
}

/// The workspace a snapshot is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Workspace {
    pub name: String,
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The rule that chose it ([`resolve`](crate::resolve)).
    pub reason: Reason,
    /// Whether it is on disk: only the default one is ever read otherwise,
    /// as it will be once the first command that writes makes it.
    pub made: bool,
}

/// The workspace's own artifacts, held as a project holds its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Library {
    /// Where it keeps each kind, as a route from the workspace's root.
    pub directories: Vec<Directory>,
    /// Each of its own, by kind and then name: no project's, so none
    /// stands in a state, and nothing is installed into them.
    pub artifacts: Vec<Held>,
    /// What `flayer add --template` can start from.
    pub templates: Vec<Template>,
    /// What could not be read in it and is no artifact's: a folder that
    /// cannot be listed, its templates.
    pub failures: Vec<Said>,
}

/// One entry in a workspace's registry, and the project it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    /// The entry as the marker spells it, which `-p` and `unlink` take; none
    /// for a project no workspace links.
    pub entry: Option<String>,
    /// Where it points.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What its marker calls it; none for an entry that opens no project.
    pub name: Option<String>,
    /// The file its marker is read from, which says an older one `mind
    /// migrate` moves.
    pub marker: Option<MarkerFile>,
    /// The id its settings name it by (D-09), which a move is followed by.
    pub id: Option<ProjectId>,
    /// The agents its settings name, in report order: a slot each for every
    /// artifact's own, so they line up.
    pub agents: Vec<Agent>,
    /// What its settings say, which a change of them starts from; none for
    /// a project a `mind.toml` marks, which has none to change.
    pub settings: Option<ProjectSettings>,
    /// Why its entry opens no project, when it does not: listed rather than
    /// left out, since it is the one somebody has to find to unlink it.
    pub stale: Option<Said>,
    /// Where it authors each kind, as a route from it.
    pub directories: Vec<Directory>,
    /// Everything it holds, authored, received or somebody's, by kind and
    /// then name, the rules its `AGENTS.md` block holds among them (D-23).
    pub artifacts: Vec<Held>,
    /// What `mind add --template` can start from in it.
    pub templates: Vec<Template>,
    /// Each kind it installs only where some of its agents do not read, as
    /// `status` says it.
    pub unreached: Vec<Unreached>,
    /// Each rule it received that some of its agents read nowhere.
    pub missed: Vec<Missed>,
    /// Each folder its settings name in `targets` that no install uses.
    pub misplaced: Vec<Said>,
    /// What could not be read in it and is no artifact's: a folder that
    /// cannot be listed, the ledger, its templates.
    pub failures: Vec<Said>,
}

/// What a project's settings say, as a change of them takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectSettings {
    /// Its agents, in the order its settings name them, which a change
    /// keeps for the ones it keeps: not report order, which
    /// [`Project::agents`] is in.
    pub agents: Vec<Agent>,
    pub share: Share,
    pub edit_tracked: bool,
    /// Each kind it names `targets` for, in report order: none when every
    /// kind goes where its agents read.
    pub targets: Vec<KindTargets>,
}

/// The folders a project's settings install one kind in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KindTargets {
    pub kind: Kind,
    /// Each folder as the settings spell it, `/` separated.
    pub folders: Vec<String>,
}

/// Where a holder keeps one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Directory {
    pub kind: Kind,
    /// A route from the holder, `/` separated, or the path itself when no
    /// route leads there.
    pub path: String,
}

/// One artifact a holder holds, and how it stands there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Held {
    pub kind: Kind,
    pub name: String,
    /// The one line a listing shows; none for one whose file does not read,
    /// and for an install nothing of which is found.
    pub summary: Option<String>,
    /// How it stands in its project, as `status` says; none for the
    /// library's own, which are no project's.
    pub state: Option<State>,
    /// The agents among its project's that read a copy of it, in report
    /// order.
    pub agents: Vec<Agent>,
    /// Where what was installed came from, as `--from` takes it.
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    pub origin: Option<String>,
    /// Where it is held, as routes from its holder, `/` separated, in path
    /// order: each copy discovery found, and each place the ledger records
    /// putting one, found or not.
    pub targets: Vec<String>,
    /// Everything `validate` says of it, warnings included, each saying
    /// whether it is one.
    pub issues: Vec<ValidationIssue>,
    /// Why its file does not read, when it does not: by its code and the
    /// reason alone, since where it is the artifact's place already says.
    /// Listed all the same, by the name its place gives it, because a file
    /// that does not read is the one most in need of opening.
    pub unread: Option<Said>,
}

/// A workspace another one loads, and how much of its own it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadedWorkspace {
    /// The entry as the marker spells it: what `flayer unload` is given.
    pub entry: String,
    /// Where it points: what `mind unload` is given, from a project.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// Its name; none for one that would not open.
    pub name: Option<String>,
    /// How many of its own it offers to install, counted from what the
    /// install machinery offers, so the number is what an install shows. A
    /// second spelling of one workspace offers what the first does.
    pub offers: usize,
    /// Why it would not open, when it would not.
    pub error: Option<Said>,
}

impl Snapshot {
    /// The workspace `inputs` resolve to, as a read resolves it whatever
    /// they intend, and everything it holds; none when no workspace
    /// resolves.
    ///
    /// A read: the default workspace is the one in home as it is, or as it
    /// would be, never made; a workspace named or activated that is not one
    /// is refused, as every command refuses it.
    pub fn read(inputs: &Inputs<'_>) -> Result<Option<Snapshot>, ResolveError> {
        let inputs = Inputs {
            intent: Intent::Read,
            ..*inputs
        };
        Ok(resolve::resolve(&inputs)?.map(|resolved| Snapshot::of(&resolved, inputs.home)))
    }

    /// Everything `resolved` holds, with the workspaces the default one in
    /// `home` knows: for a front end that resolved the workspace itself, to
    /// scan it too ([`crate::scan`]), which no snapshot does.
    pub fn of(resolved: &Resolved, home: Option<&Path>) -> Snapshot {
        let workspace = &resolved.workspace;
        let mut warnings: Vec<Said> = Vec::new();

        let members = workspace.members();
        let ids: Vec<(ProjectId, PathBuf)> = members
            .iter()
            .filter_map(|member| {
                let settings = member.project.as_ref().ok()?.settings()?;
                Some((settings.id.clone(), member.root.clone()))
            })
            .collect();
        let projects = members
            .into_iter()
            .map(|member| Project::of_member(member, workspace))
            .collect();
        warnings.extend(
            relink::duplicates(&ids)
                .iter()
                .map(|duplicate| Said::of(duplicate.code(), duplicate)),
        );

        let loads = install::offered_by_loads(workspace);
        let loaded = loaded(workspace, &loads.offers);
        // A loaded workspace that would not open says why as its own
        // `error`; the rest is no entry's.
        warnings.extend(
            loads
                .failures
                .iter()
                .filter(|failure| !matches!(failure, LoadFailure::Workspace(_)))
                .map(|failure| Said::of(failure.code(), failure)),
        );

        // A ledger that will not open is said once, with nothing from it;
        // one that opens says each thing it could not answer, and answers
        // the rest, so a shelf is counted whatever the log says.
        let (shelf, sources, history) = match workspace.ledger_for_reading() {
            Ok(ledger) => (
                answered(ledger.gathered(), &mut warnings),
                answered(ledger.sources(), &mut warnings),
                answered(ledger.history(HISTORY), &mut warnings),
            ),
            Err(error) => {
                warnings.push(Said::of(error.code(), &error));
                Default::default()
            }
        };

        // What could not be read of a workspace this machine knows is said
        // in its row, as `stale`.
        let (known, _) = known::known(home, Some(workspace));

        Snapshot {
            format: FORMAT,
            workspace: Workspace {
                name: workspace.name().to_owned(),
                path: workspace.root().to_path_buf(),
                reason: resolved.reason,
                made: workspace.is_made(),
            },
            kinds: Kind::ALL.to_vec(),
            agents: Agent::ALL.to_vec(),
            states: State::ALL.to_vec(),
            library: Library::of(workspace),
            projects,
            loaded,
            shelf,
            sources,
            known: known.workspaces,
            history,
            warnings,
        }
    }
}

impl Library {
    /// `workspace`'s own, of the kinds it keeps apart from a project at its
    /// root ([`FlayerWorkspace::own_kinds`]), with its templates.
    fn of(workspace: &FlayerWorkspace) -> Library {
        let own = workspace.own();
        let catalog = Catalog::discover_kinds(slice::from_ref(&own), &workspace.own_kinds());
        let (artifacts, mut failures) = holdings(&own, &catalog, None);
        let templates = templates(Some(workspace), &own, &mut failures);
        Library {
            directories: directories(&own),
            artifacts,
            templates,
            failures,
        }
    }
}

impl Project {
    /// `project` as a snapshot shows one, against what `workspace`'s ledger
    /// records when that one links it, with `workspace`'s templates: for a
    /// front end showing a project no workspace links, `workspace` being the
    /// one above it, if any, as `mind` would consult it.
    pub fn read(project: &MindProject, workspace: Option<&FlayerWorkspace>) -> Project {
        let entry = workspace
            .and_then(|workspace| workspace.linked_as(project.root()))
            .map(paths::as_route);
        Project::opened(entry, project, workspace)
    }

    /// One entry of `workspace`'s registry, named as that entry spells it
    /// rather than as the first entry pointing at the same directory does.
    fn of_member(member: Member, workspace: &FlayerWorkspace) -> Project {
        let entry = paths::as_route(&member.entry);
        match member.project {
            Ok(project) => Project::opened(Some(entry), &project, Some(workspace)),
            Err(error) => Project::stale(entry, member.root, &error),
        }
    }

    fn opened(
        entry: Option<String>,
        project: &MindProject,
        workspace: Option<&FlayerWorkspace>,
    ) -> Project {
        let (status, catalog, placed) = ProjectStatus::read_placed(project, &Kind::ALL, workspace);
        let (artifacts, mut failures) = holdings(project, &catalog, Some((&status, &placed)));
        let templates = templates(workspace, project, &mut failures);
        let settings = project.settings();
        Project {
            entry,
            path: project.root().to_path_buf(),
            name: Some(project.name().to_owned()),
            marker: Some(project.marker_file()),
            id: settings.map(|settings| settings.id.clone()),
            agents: status.agents,
            settings: settings.map(ProjectSettings::of),
            stale: None,
            directories: directories(project),
            artifacts,
            templates,
            unreached: status.unreached,
            missed: status.missed,
            misplaced: project
                .misplaced_targets()
                .iter()
                .map(|misplaced| Said::of(misplaced.code(), misplaced))
                .collect(),
            failures,
        }
    }

    fn stale(entry: String, root: PathBuf, error: &WorkspaceError) -> Project {
        Project {
            entry: Some(entry),
            path: root,
            name: None,
            marker: None,
            id: None,
            agents: Vec::new(),
            settings: None,
            stale: Some(Said::of(error.code(), error)),
            directories: Vec::new(),
            artifacts: Vec::new(),
            templates: Vec::new(),
            unreached: Vec::new(),
            missed: Vec::new(),
            misplaced: Vec::new(),
            failures: Vec::new(),
        }
    }
}

impl ProjectSettings {
    fn of(settings: &Settings) -> ProjectSettings {
        ProjectSettings {
            agents: settings.agents.clone(),
            share: settings.share,
            edit_tracked: settings.edit_tracked,
            targets: Kind::ALL
                .into_iter()
                .filter_map(|kind| {
                    Some(KindTargets {
                        kind,
                        folders: settings
                            .targets
                            .get(kind)?
                            .iter()
                            .map(|folder| paths::as_route(folder))
                            .collect(),
                    })
                })
                .collect(),
        }
    }
}

/// What `project` holds, read in `catalog`, with how each stands where
/// `status` says, and the failures no artifact can be named by.
///
/// A file that does not read is listed where it would have been, by the
/// name its place gives it ([`lifecycle::identify`]), so it can be opened
/// and fixed from a front end; and in place of a copy of that name in the
/// project's agents' folders, since a row is named by its name alone and a
/// change to that name reaches the project's own copy, this one. Only what
/// cannot be pinned to an artifact, a folder that cannot be listed, is left
/// a failure on its own. An install nothing of which is found is listed as
/// the report lists it, from which it can be put back.
fn holdings(
    project: &MindProject,
    catalog: &Catalog,
    status: Option<(&ProjectStatus, &[Vec<String>])>,
) -> (Vec<Held>, Vec<Said>) {
    let root = project.root();
    let row = |kind: Kind, name: &str| -> Option<(&ArtifactStatus, &Vec<String>)> {
        let (status, placed) = status?;
        let index = status
            .artifacts
            .iter()
            .position(|held| held.kind == kind && held.name == name)?;
        Some((&status.artifacts[index], &placed[index]))
    };
    let mut held: Vec<Held> = catalog
        .artifacts()
        .iter()
        .map(|artifact| {
            let row = row(artifact.kind(), artifact.name());
            Held {
                kind: artifact.kind(),
                name: artifact.name().to_owned(),
                summary: artifact.summary().map(str::to_owned),
                state: row.map(|(row, _)| row.state),
                agents: row.map(|(row, _)| row.agents.clone()).unwrap_or_default(),
                origin: row.and_then(|(row, _)| row.origin.clone()),
                targets: row.map_or_else(
                    || {
                        artifact
                            .copies()
                            .iter()
                            .map(|copy| install::route(root, copy.path()))
                            .collect()
                    },
                    |(_, placed)| placed.clone(),
                ),
                issues: artifact.validate(),
                unread: None,
            }
        })
        .collect();

    let mut failures = Vec::new();
    for failure in catalog.failures() {
        let Some((kind, name)) = lifecycle::identify(project, failure.path()) else {
            failures.push(Said::of(failure.code(), failure));
            continue;
        };
        let row = row(kind, &name);
        let unread = Held {
            summary: None,
            state: row.map(|(row, _)| row.state),
            agents: row.map(|(row, _)| row.agents.clone()).unwrap_or_default(),
            origin: row.and_then(|(row, _)| row.origin.clone()),
            targets: row.map(|(_, placed)| placed.clone()).unwrap_or_default(),
            issues: Vec::new(),
            unread: Some(Said {
                code: failure.code(),
                message: failure
                    .source()
                    .map_or_else(|| failure.to_string(), ToString::to_string),
            }),
            kind,
            name,
        };
        match held
            .iter_mut()
            .find(|other| other.kind == unread.kind && other.name == unread.name)
        {
            Some(other) => *other = unread,
            None => held.push(unread),
        }
    }

    if let Some((status, placed)) = status {
        for (row, placed) in status.artifacts.iter().zip(placed) {
            if held
                .iter()
                .any(|other| other.kind == row.kind && other.name == row.name)
            {
                continue;
            }
            held.push(Held {
                kind: row.kind,
                name: row.name.clone(),
                summary: None,
                state: Some(row.state),
                agents: row.agents.clone(),
                origin: row.origin.clone(),
                targets: placed.clone(),
                issues: Vec::new(),
                unread: None,
            });
        }
    }
    held.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    (held, failures)
}

/// What `mind add --template` can start from in `project`, with
/// `workspace`'s; one that cannot be read among `failures`.
fn templates(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    failures: &mut Vec<Said>,
) -> Vec<Template> {
    template::templates(workspace, project).unwrap_or_else(|error| {
        failures.push(Said::of(error.code(), &error));
        Vec::new()
    })
}

/// Where `project` keeps each kind, as a route from it; a kind it keeps no
/// folder for is left out.
fn directories(project: &MindProject) -> Vec<Directory> {
    Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let directory = project.directory_for(kind)?;
            Some(Directory {
                kind,
                path: install::route(project.root(), &directory),
            })
        })
        .collect()
}

/// What `read` answered, or nothing, with why among `warnings`.
fn answered<T: Default>(read: Result<T, LedgerError>, warnings: &mut Vec<Said>) -> T {
    read.unwrap_or_else(|error| {
        warnings.push(Said::of(error.code(), &error));
        T::default()
    })
}

impl LoadedWorkspace {
    /// Every workspace `workspace` loads, as a snapshot lists them: for a
    /// front end showing a project no workspace links, what `mind load` has
    /// put into the one above it.
    pub fn of(workspace: &FlayerWorkspace) -> Vec<LoadedWorkspace> {
        loaded(workspace, &install::offered_by_loads(workspace).offers)
    }
}

/// Every workspace `workspace` loads, with how many of `offers`, what the
/// loads offer, each comes with.
fn loaded(workspace: &FlayerWorkspace, offers: &[Offer]) -> Vec<LoadedWorkspace> {
    let loads = workspace.loads();
    loads
        .iter()
        .enumerate()
        .map(|(index, loaded)| {
            let entry = paths::as_route(&loaded.entry);
            // A second spelling of one workspace offers what the first does,
            // which the offers name.
            let first = loads[..index]
                .iter()
                .find(|earlier| same_directory(&earlier.root, &loaded.root))
                .map_or_else(|| entry.clone(), |earlier| paths::as_route(&earlier.entry));
            match &loaded.workspace {
                Ok(source) => LoadedWorkspace {
                    entry,
                    path: loaded.root.clone(),
                    name: Some(source.name().to_owned()),
                    offers: offers
                        .iter()
                        .filter(|offer| {
                            matches!(offer, Offer::Loaded { entry: from, .. } if *from == first)
                        })
                        .count(),
                    error: None,
                },
                Err(error) => LoadedWorkspace {
                    entry,
                    path: loaded.root.clone(),
                    name: None,
                    offers: 0,
                    error: Some(Said::of(error.code(), error)),
                },
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "view_test.rs"]
mod tests;
