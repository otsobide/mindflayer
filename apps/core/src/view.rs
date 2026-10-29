//! Everything a workspace holds, read in one go for a front end to draw: the
//! snapshot every front end reads, the TUI first and the web panel next.
//!
//! One read rather than each front end putting its view together from the
//! reports, as the TUI did: which failure belongs to which artifact, which
//! install nothing of is found, how many of its own a loaded workspace
//! offers, are answers, and two front ends working them out would come to
//! two of them. Core builds and orders it, from the same types the CLI
//! prints, and a front end only chooses how to show it. A [`Listing`] is a
//! part of it, its holders read for the kinds asked: what `list --json`
//! prints, so a script reads a project as a front end draws it.
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
/// holds, and not when one is added. A [`Listing`] is printed in the CLI's
/// envelope, so a change that moves this moves that version too when it
/// is to a holder or what it holds.
pub const FORMAT: u32 = 1;

/// How many of the latest actions a snapshot carries, newest first: enough
/// to say what happened lately, and a bound on what every update carries.
pub const HISTORY: usize = 20;

/// Everything a workspace holds, as every front end shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct KindTargets {
    pub kind: Kind,
    /// Each folder as the settings spell it, `/` separated.
    pub folders: Vec<String>,
}

/// Where a holder keeps one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Directory {
    pub kind: Kind,
    /// A route from the holder, `/` separated, or the path itself when no
    /// route leads there.
    pub path: String,
}

/// One artifact a holder holds, and how it stands there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Held {
    pub kind: Kind,
    pub name: String,
    /// The one line a listing shows; none for one whose file does not read,
    /// and for an install nothing of which is found.
    pub summary: Option<String>,
    /// How its name stands in its project, as `status` says; none for the
    /// library's own, which are no project's. Only a copy that reads has a
    /// standing, so beside an [`unread`](Self::unread) one it is that of a
    /// copy of the name elsewhere, in an agent's folder, and none when
    /// there is no such copy.
    pub state: Option<State>,
    /// The agents among its project's that read a copy of it, in report
    /// order.
    pub agents: Vec<Agent>,
    /// Where what was installed came from, as `--from` takes it.
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    pub origin: Option<String>,
    /// Where it is held, as routes from its holder, `/` separated, in path
    /// order: each copy discovery found, the one whose file does not read
    /// among them, and each place the ledger records putting one, found or
    /// not.
    pub targets: Vec<String>,
    /// Everything `validate` says of it, warnings included, each saying
    /// whether it is one.
    pub issues: Vec<ValidationIssue>,
    /// The copy whose file does not read, when one does not, and why.
    /// Listed all the same, by the name its place gives it, because a file
    /// that does not read is the one most in need of opening.
    pub unread: Option<Unread>,
}

/// A copy of an artifact whose file does not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Unread {
    /// Where the copy is, as a route from its holder, `/` separated: its
    /// place in the folder the holder keeps its kind in, the one place a
    /// file that does not read is named by ([`lifecycle::identify`]). Said
    /// on its own, as well as among [`Held::targets`], since a copy that
    /// reads can sit beside it under the same name, elsewhere.
    pub path: String,
    pub code: &'static str,
    /// The reason alone, without the path, which [`path`](Self::path)
    /// says: written through [`redacted`](crate::url::redacted), as a
    /// [`Said`]'s message is.
    #[serde(serialize_with = "crate::url::serialize_redacted")]
    pub message: String,
}

/// A workspace another one loads, and how much of its own it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

/// The holders a listing reads, each as a snapshot holds it, read for the
/// kinds the listing asks for: what `list --json` prints.
///
/// A part of the snapshot rather than a shape of its own, so what a script
/// prints of a project is what a front end draws of it, every name the
/// same, and a test comparing the two compares like with like. Read for the
/// kinds asked, as the listing reads them, rather than read whole and cut
/// down after: a folder of a kind not asked for is not listed, so it is no
/// failure of the reply's either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Listing {
    /// The workspace's own, when the listing reads them: none for one
    /// project's listing.
    pub library: Option<Library>,
    /// Every entry in the workspace's registry, as the snapshot lists them,
    /// or the one project a listing is of.
    pub projects: Vec<Project>,
}

impl Listing {
    /// What `workspace` holds of `kinds`: its own, and every entry in its
    /// registry, against its ledger.
    pub fn of_workspace(workspace: &FlayerWorkspace, kinds: &[Kind]) -> Listing {
        Listing {
            library: Some(Library::of(workspace, kinds)),
            projects: workspace
                .members()
                .into_iter()
                .map(|member| Project::of_member(member, workspace, kinds))
                .collect(),
        }
    }

    /// What `project` holds of `kinds`, read as [`Project::read`] reads it,
    /// against what `workspace` records when that one links it.
    pub fn of_project(
        project: &MindProject,
        workspace: Option<&FlayerWorkspace>,
        kinds: &[Kind],
    ) -> Listing {
        Listing {
            library: None,
            projects: vec![Project::read_kinds(project, workspace, kinds)],
        }
    }
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
            .map(|member| Project::of_member(member, workspace, &Kind::ALL))
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
            library: Library::of(workspace, &Kind::ALL),
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
    /// `workspace`'s own, of the kinds among `kinds` it keeps apart from a
    /// project at its root ([`FlayerWorkspace::own_kinds`]), with its
    /// templates and its folders of those kinds.
    fn of(workspace: &FlayerWorkspace, kinds: &[Kind]) -> Library {
        let own = workspace.own();
        let own_kinds: Vec<Kind> = workspace
            .own_kinds()
            .into_iter()
            .filter(|kind| kinds.contains(kind))
            .collect();
        let catalog = Catalog::discover_kinds(slice::from_ref(&own), &own_kinds);
        let (artifacts, mut failures) = holdings(&own, &catalog, None);
        let templates = templates(Some(workspace), &own, kinds, &mut failures);
        Library {
            directories: directories(&own, kinds),
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
        Project::read_kinds(project, workspace, &Kind::ALL)
    }

    /// [`read`](Self::read), for `kinds` alone.
    fn read_kinds(
        project: &MindProject,
        workspace: Option<&FlayerWorkspace>,
        kinds: &[Kind],
    ) -> Project {
        let entry = workspace
            .and_then(|workspace| workspace.linked_as(project.root()))
            .map(paths::as_route);
        Project::opened(entry, project, workspace, kinds)
    }

    /// One entry of `workspace`'s registry, named as that entry spells it
    /// rather than as the first entry pointing at the same directory does,
    /// read for `kinds`.
    fn of_member(member: Member, workspace: &FlayerWorkspace, kinds: &[Kind]) -> Project {
        let entry = paths::as_route(&member.entry);
        match member.project {
            Ok(project) => Project::opened(Some(entry), &project, Some(workspace), kinds),
            Err(error) => Project::stale(entry, member.root, &error),
        }
    }

    /// `project` read for `kinds`: what it holds of them, the folders and
    /// the templates it has of them, and what of them its agents miss.
    fn opened(
        entry: Option<String>,
        project: &MindProject,
        workspace: Option<&FlayerWorkspace>,
        kinds: &[Kind],
    ) -> Project {
        let (status, catalog, placed) = ProjectStatus::read_placed(project, kinds, workspace);
        let (artifacts, mut failures) = holdings(project, &catalog, Some((&status, &placed)));
        let templates = templates(workspace, project, kinds, &mut failures);
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
            directories: directories(project, kinds),
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
/// change to that name reaches the project's own copy, this one. The row
/// keeps the report's standing of the name, which only a copy that reads
/// has, and adds this copy's place to where it is held, naming it in
/// `unread` too, so that no reader takes the other copy for it. Only what
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
        // Identified, so the project keeps a folder for the kind, and the
        // file is in it.
        let path = lifecycle::place(project, kind, &name).map_or_else(
            || install::route(root, failure.path()),
            |place| install::route(root, &place),
        );
        let mut targets = row.map(|(_, placed)| placed.clone()).unwrap_or_default();
        targets.push(path.clone());
        targets.sort();
        targets.dedup();
        let unread = Held {
            summary: None,
            state: row.map(|(row, _)| row.state),
            agents: row.map(|(row, _)| row.agents.clone()).unwrap_or_default(),
            origin: row.and_then(|(row, _)| row.origin.clone()),
            targets,
            issues: Vec::new(),
            unread: Some(Unread {
                path,
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
/// `workspace`'s, of `kinds`; one that cannot be read among `failures`.
fn templates(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    kinds: &[Kind],
    failures: &mut Vec<Said>,
) -> Vec<Template> {
    let mut templates = template::templates(workspace, project).unwrap_or_else(|error| {
        failures.push(Said::of(error.code(), &error));
        Vec::new()
    });
    templates.retain(|template| kinds.contains(&template.kind));
    templates
}

/// Where `project` keeps each of `kinds`, as a route from it, in report
/// order; a kind it keeps no folder for is left out.
fn directories(project: &MindProject, kinds: &[Kind]) -> Vec<Directory> {
    Kind::ALL
        .into_iter()
        .filter(|kind| kinds.contains(kind))
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
