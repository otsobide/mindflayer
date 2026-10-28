//! What a workspace holds, read for a front end to draw: the snapshot every
//! front end reads, the TUI first and the web panel next, starting with
//! what one project holds.
//!
//! One read rather than each front end putting its view together from the
//! reports, as the TUI does: which failure belongs to which artifact, which
//! install nothing of is found, are answers, and two front ends working them
//! out would come to two of them. Core builds and orders it, from the same
//! types the CLI prints, and a front end only chooses how to show it.
//!
//! - **A read, and nothing more.** No file is made or changed: the ledger is
//!   opened only through [`FlayerWorkspace::ledger_for_reading`].
//! - **Its names are a contract**, as a report's are: every field by its
//!   name, a kind, an agent and a state by the word a command line takes,
//!   and every URL through [`redacted`](crate::url::redacted), where it is
//!   written.
//! - **Failures are collected**, beside what they are about: a project's in
//!   the project.

use std::error::Error as _;
use std::path::PathBuf;

use serde::Serialize;

use crate::agent::Agent;
use crate::artifact::ValidationIssue;
use crate::catalog::Catalog;
use crate::code::Said;
use crate::install;
use crate::kind::Kind;
use crate::lifecycle;
use crate::paths;
use crate::settings::{ProjectId, Settings, Share};
use crate::status::{ArtifactStatus, Missed, ProjectStatus, State, Unreached};
use crate::template::{self, Template};
use crate::workspace::{FlayerWorkspace, MarkerFile, MindProject};

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
