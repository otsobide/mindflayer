//! Bringing what was installed into projects up to date with where it came
//! from, and putting back what has gone since: `flayer update` and `mind
//! update` (D-04).
//!
//! Every installation the ledger records stands somewhere ([`Standing`]):
//! current, outdated, edited, missing or orphaned. An update writes the
//! outdated and the missing again from the place its record says it came
//! from, as that place offers it now, and nothing else: a current one has
//! nothing to change, an orphaned one nothing to be written from, and an
//! edited one is somebody's work, left alone with every other copy of it
//! unless the update is forced, as an install leaves one. What it writes it
//! writes as an install does, through the same body
//! ([`install::install_held`]), so the copies, their record and what keeps
//! them out of git are an install's: a copy a local project lost is put
//! back with its exclude entry. A `git clean -dX`, the one that goes into
//! the copies' folders, takes the project's `.mind` too, and an update can
//! find the project only once `mind init` has marked it again.
//!
//! A rule a project marked by its settings received is a section of the
//! block in its `AGENTS.md`, rendered with every other rule it has, so a
//! project's rules are brought up to date together, in one render
//! ([`rules::update_held`]). It gives each rule as its offer holds it now,
//! keeps a rule somebody edited, every copy of it, unless the update is
//! forced to write over that rule, and keeps a rule no longer offered as
//! the project holds it, where an install's render refuses one. So
//! updating one rule of such a project brings each of its other outdated or
//! missing rules along, and the plan names them: leaving them as they were
//! would mean reading each back from the block, as a render reads back only
//! an edit it keeps.
//!
//! What a gathered source offers is not refreshed here: an update copies
//! what the shelf holds now, and a gather is what moves the shelf.
//!
//! [`plan`] says what an update would do, writing nothing, and [`apply`]
//! does it, holding the workspace's lock from the plan it makes again until
//! the last copy is written, as `reapply` and `migrate` do. [`Updates`] is
//! the plan of every project a workspace links, as a front end reads it:
//! what the panel's page of updates draws.

use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::agent::AgentHomes;
use crate::catalog::Reference;
use crate::code::{codes, Said};
use crate::gather::Scrub;
use crate::install::rules::{self, Rendered};
use crate::install::{
    self, Candidate, Edits, Found, InstallError, Installation, Refused, Standing,
};
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError};
use crate::paths;
use crate::status::State;
use crate::view::StateCount;
use crate::workspace::{same_directory, FlayerWorkspace, MindProject, WorkspaceError};

/// What an update would do: [`plan`], or what [`apply`] did.
#[derive(Debug)]
pub struct Plan {
    /// Each project asked, in the order asked, each directory once.
    pub projects: Vec<ProjectPlan>,
    /// Each reference asked, as typed, that names nothing installed in any
    /// project asked: a typo far more often than not, and said as one.
    pub unmatched: Vec<String>,
}

impl Plan {
    /// Whether it writes nothing at all.
    pub fn is_empty(&self) -> bool {
        self.projects
            .iter()
            .flat_map(|project| &project.installations)
            .all(|update| !update.step.writes())
    }
}

/// One project's part of an update.
#[derive(Debug)]
pub struct ProjectPlan {
    pub root: PathBuf,
    pub name: String,
    /// Every installation asked, by kind and then by name: each one the
    /// references name, or every one the ledger records when they name
    /// none, and with a rule of a project marked by its settings that is
    /// written, every other outdated or missing rule of it, which its
    /// render brings along.
    pub installations: Vec<Update>,
    /// What rendering the project's rules came to, once applied: none until
    /// then, or when no rule of it is written.
    pub rendered: Option<Result<Rendered, InstallError>>,
    /// Each rule of it edited and not written over, asked or not: what its
    /// render keeps, every copy as the project holds it, since a render
    /// writes every rule, and keeping only the copy edited would hand the
    /// agents two versions of one rule.
    left: Vec<String>,
}

/// One installation, how it stands, and what an update does with it.
#[derive(Debug)]
pub struct Update {
    pub kind: Kind,
    pub name: String,
    /// Where it came from, as `--from` takes it: none for a record from
    /// before origins were kept.
    pub origin: Option<String>,
    pub standing: Standing,
    pub step: Step,
    /// What writing it came to, once applied: none until then, for one an
    /// update does not write, and for a rule of a project marked by its
    /// settings, whose project's render writes it
    /// ([`ProjectPlan::rendered`]).
    pub done: Option<Result<Installation, InstallError>>,
    /// What it is written from, when it is.
    candidate: Option<Candidate>,
}

/// What an update does with one installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Current: nothing to write.
    Keep,
    /// Outdated: every copy written again, as its origin offers it now.
    Refresh,
    /// Missing: these copies put back, and every other brought up to date
    /// with them.
    PutBack(Vec<PathBuf>),
    /// Edited, and forced: these copies written over, every other with
    /// them.
    WriteOver(Vec<PathBuf>),
    /// Edited: these copies left alone, and every other with them, since
    /// writing over the rest would hand the agents reading both folders
    /// two versions of one artifact; `--force` writes over them.
    Leave(Vec<PathBuf>),
    /// Orphaned, edited or not: nothing offers it where it came from, so
    /// there is nothing to write it from, and it stays as the project holds
    /// it until it is uninstalled. Said, never a failure.
    Orphaned,
    /// Offered from each of these places, and recorded before origins were
    /// kept, with no copy left that tells which: left, for an install whose
    /// `--from` says which.
    Unsure(Vec<String>),
}

impl Step {
    /// Whether an update writes anything for it.
    pub fn writes(&self) -> bool {
        matches!(self, Step::Refresh | Step::PutBack(_) | Step::WriteOver(_))
    }

    /// Why it is not written, when an install not forced would have been
    /// refused for the same reason: what a front end says it with, by the
    /// code of that install's refusal, since a code names one condition
    /// wherever it is met. Here rather than in each front end, which would
    /// otherwise decide for itself which of core's conditions a code names.
    ///
    /// Only a copy changed since: an orphaned or an unsure one is left for
    /// want of a place to write it from, which no install is refused for.
    pub fn refused(&self) -> Option<Refused> {
        match self {
            Step::Leave(_) => Some(Refused::Edited),
            Step::Keep
            | Step::Refresh
            | Step::PutBack(_)
            | Step::WriteOver(_)
            | Step::Orphaned
            | Step::Unsure(_) => None,
        }
    }

    /// What it is, as a word: the step without the copies or the places it
    /// names, which a front end reads beside it.
    pub fn does(&self) -> Does {
        match self {
            Step::Keep => Does::Keep,
            Step::Refresh => Does::Refresh,
            Step::PutBack(_) => Does::PutBack,
            Step::WriteOver(_) => Does::WriteOver,
            Step::Leave(_) => Does::Leave,
            Step::Orphaned => Does::Orphaned,
            Step::Unsure(_) => Does::Unsure,
        }
    }

    /// The copies it names, put back, written over or left: none for a step
    /// that names none.
    fn copies(&self) -> &[PathBuf] {
        match self {
            Step::PutBack(paths) | Step::WriteOver(paths) | Step::Leave(paths) => paths,
            Step::Keep | Step::Refresh | Step::Orphaned | Step::Unsure(_) => &[],
        }
    }

    /// Why an update leaves `update`, an installation in the project
    /// `project` at `root`, as the project holds it, when it does: an edit
    /// it does not write over, an offer gone, or a place it came from that
    /// cannot be told.
    fn left(&self, update: &Update, project: &str, root: &Path) -> Option<Left> {
        let (kind, name, project) = (update.kind, update.name.clone(), project.to_owned());
        match self {
            Step::Leave(paths) => Some(Left::Edited {
                kind,
                name,
                project,
                at: paths
                    .iter()
                    .map(|path| install::route(root, path))
                    .collect(),
            }),
            Step::Orphaned => Some(Left::Orphaned {
                kind,
                name,
                project,
                origin: update.origin.clone(),
                edited: update.standing == Standing::Edited,
            }),
            Step::Unsure(origins) => Some(Left::Unsure {
                kind,
                name,
                project,
                origins: origins.clone(),
            }),
            Step::Keep | Step::Refresh | Step::PutBack(_) | Step::WriteOver(_) => None,
        }
    }
}

/// What an update does with one installation, as a word: [`Step`], without
/// what it names.
///
/// Declaration order is [`Step`]'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Does {
    Keep,
    Refresh,
    PutBack,
    WriteOver,
    Leave,
    Orphaned,
    Unsure,
}

impl Does {
    /// How it is written in JSON.
    pub const fn slug(self) -> &'static str {
        match self {
            Does::Keep => "keep",
            Does::Refresh => "refresh",
            Does::PutBack => "put_back",
            Does::WriteOver => "write_over",
            Does::Leave => "leave",
            Does::Orphaned => "orphaned",
            Does::Unsure => "unsure",
        }
    }
}

/// By its slug, for the reason a state is: one spelling, which a renamed
/// variant cannot change.
impl Serialize for Does {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(feature = "ts")]
crate::typescript::words! {
    /// What an update does with one installation.
    Does by slug { Keep, Refresh, PutBack, WriteOver, Leave, Orphaned, Unsure }
}

/// Why an update leaves an installation as the project holds it: what a
/// front end says beside it, by a code ([`Step::left`]).
///
/// Said by core rather than by each front end from the step, so the panel
/// gives the reason in core's words and acts on its code. An edit left
/// alone is the condition an install refuses an edited copy for, and goes
/// by that refusal's code (`edited`), as a plan holds a box back by it; an
/// offer gone and a place that cannot be told have codes of their own, no
/// install being refused for either.
#[derive(Debug, Error)]
pub enum Left {
    /// Changed since it was installed, at these copies, each a route from
    /// its project, and not forced.
    #[error(
        "{kind} `{name}` in {project} was changed since it was installed, at {}, so an update \
         leaves it alone, every copy of it; forcing the update writes over the change",
        .at.join(", ")
    )]
    Edited {
        kind: Kind,
        name: String,
        project: String,
        at: Vec<String>,
    },
    /// Nothing offers it where it came from, so nothing can be written: the
    /// orphaned standing, and an edited one beneath it, which only a forced
    /// uninstall takes out.
    #[error(
        "{kind} `{name}` in {project} is no longer offered {}, so an update leaves it as it \
         is; {}",
        offered_by(.origin),
        taken_out(*.edited)
    )]
    Orphaned {
        kind: Kind,
        name: String,
        project: String,
        origin: Option<String>,
        edited: bool,
    },
    /// Offered from each of these places, and recorded before origins were
    /// kept, with no copy left that tells which.
    #[error(
        "{kind} `{name}` in {project} is offered by {}, and was installed before Mindflayer \
         kept where from, so an update leaves it as it is; `install --from` says which",
        .origins.join(", ")
    )]
    Unsure {
        kind: Kind,
        name: String,
        project: String,
        origins: Vec<String>,
    },
}

/// Where an orphaned installation is no longer offered, as its record names
/// it: by its origin, or where it came from, for a record from before
/// origins were kept.
fn offered_by(origin: &Option<String>) -> String {
    origin.as_deref().map_or_else(
        || String::from("where it came from"),
        |origin| format!("by {origin}"),
    )
}

/// What takes an orphaned installation out: an uninstall, forced when it was
/// `edited` since, as an uninstall by name leaves an edited copy.
fn taken_out(edited: bool) -> &'static str {
    if edited {
        "it was changed since it was installed, so only a forced uninstall takes it out"
    } else {
        "uninstalling it takes it out"
    }
}

codes!(Left {
    Self::Orphaned { .. } => "no_longer_offered",
    Self::Unsure { .. } => "origin_unknown",
} echoed {
    Self::Edited { .. } => "edited",
});

/// What [`apply`] did.
#[derive(Debug)]
pub struct Applied {
    /// What was planned under the lock, each installation written with
    /// what came of it ([`Update::done`], [`ProjectPlan::rendered`]): one
    /// that failed is said there, beside the rest, rather than stopping
    /// them.
    pub plan: Plan,
    /// What opening the ledger to write took out of it, for the front end
    /// to say.
    pub scrub: Scrub,
}

/// Why an update could not be planned or made at all, before anything was
/// written.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
}

codes!(UpdateError {} carried {
    Self::Workspace(error) => error,
    Self::Ledger(error) => error,
});

/// What updating `projects`, every one a project `workspace` links, would
/// do, against what its ledger records of them: nothing written, the ledger
/// read without being made.
///
/// Only the installations `references` name, by kind and name as typed
/// (`deploy`, `rule:git/commits`), or every one when they name none. An
/// edited copy is written over only when `edits` says so, as `--force` does.
pub fn plan(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    references: &[Reference],
    edits: Edits,
) -> Result<Plan, UpdateError> {
    let ledger = workspace.ledger_for_reading()?;
    plan_with(workspace, &ledger, projects, references, edits)
}

/// Update `projects`: [`plan`], done.
///
/// Holds the workspace's lock from before the plan is made again, against
/// the projects read again, until the last copy and record are written, so
/// an install, an uninstall or another update in the workspace comes wholly
/// before it or wholly after it. The ledger is opened to write before the
/// lock, since its scrub takes the lock when it has clones to rewrite, and
/// not at all while there is none, which a workspace that never installed
/// anything has no reason to be given: it records nothing to update. Nor
/// is the lock taken in a default workspace nothing has made yet, which
/// has nowhere to keep one and records nothing either: the plan, empty, is
/// what is said.
///
/// `homes` are the agents' own folders, which the front end decides, as
/// for an install: a copy that would land in one is refused there too.
///
/// Refused as a whole only when the lock or the ledger cannot be had, or a
/// project cannot be read again, all before anything is written. What
/// fails after is said of the installation it failed for, and the rest go
/// ahead: each one is written whole or not at all, as an install is, and
/// the same update run again finishes the rest.
pub fn apply(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    references: &[Reference],
    edits: Edits,
    homes: &AgentHomes,
) -> Result<Applied, UpdateError> {
    if !workspace.is_made() {
        return Ok(Applied {
            plan: plan(workspace, projects, references, edits)?,
            scrub: Scrub::default(),
        });
    }
    let (ledger, scrub) = if workspace.ledger_path().is_file() {
        workspace.ledger_to_write()?
    } else {
        (workspace.ledger_for_reading()?, Scrub::default())
    };
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    // Read again under the lock: the projects handed in may be minutes old,
    // and their settings say where an install writes.
    let projects = projects
        .iter()
        .map(|project| MindProject::open(project.root()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut plan = plan_with(workspace, &ledger, &projects, references, edits)?;
    for planned in &mut plan.projects {
        let Some(project) = projects
            .iter()
            .find(|project| project.root() == planned.root)
        else {
            continue;
        };
        let rendered_together = project.settings().is_some();
        let mut renders = false;
        let mut forced: Vec<String> = Vec::new();
        for update in &mut planned.installations {
            if !update.step.writes() {
                continue;
            }
            // Matched on the kind, so a third one has to say how it is
            // written.
            let rendered = match update.kind {
                Kind::Skill => false,
                Kind::Rule => rendered_together,
            };
            if rendered {
                renders = true;
                if matches!(update.step, Step::WriteOver(_)) {
                    forced.push(update.name.clone());
                }
                continue;
            }
            let Some(candidate) = &update.candidate else {
                continue;
            };
            let edits = match update.step {
                Step::WriteOver(_) => Edits::Forced,
                _ => Edits::Kept,
            };
            update.done = Some(install::install_held(
                workspace, &ledger, project, candidate, homes, edits,
            ));
        }
        if renders {
            planned.rendered = Some(rules::update_held(
                workspace,
                &ledger,
                project,
                homes,
                &forced,
                &planned.left,
            ));
        }
    }
    Ok(Applied { plan, scrub })
}

/// [`plan`], against `ledger`.
fn plan_with(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    projects: &[MindProject],
    references: &[Reference],
    edits: Edits,
) -> Result<Plan, UpdateError> {
    let named = |kind: Kind, name: &str| {
        references.is_empty()
            || references.iter().any(|reference| {
                reference.name() == name && reference.kind().is_none_or(|asked| asked == kind)
            })
    };
    let mut matched = vec![false; references.len()];
    let mut planned: Vec<ProjectPlan> = Vec::new();
    for project in projects {
        if planned
            .iter()
            .any(|seen| same_directory(&seen.root, project.root()))
        {
            continue;
        }
        let mut updates: Vec<(bool, Update)> = Vec::new();
        for kind in Kind::ALL {
            for surveyed in install::surveyed(workspace, ledger, project, kind)? {
                let name = surveyed.record.name.clone();
                for (reference, seen) in references.iter().zip(&mut matched) {
                    *seen |= reference.name() == name
                        && reference.kind().is_none_or(|asked| asked == kind);
                }
                let (step, candidate) = step(workspace, &surveyed, edits);
                updates.push((
                    named(kind, &name),
                    Update {
                        kind,
                        name,
                        origin: surveyed.record.origin.clone(),
                        standing: surveyed.standing,
                        step,
                        done: None,
                        candidate,
                    },
                ));
            }
        }
        // Rendered together, so one written brings the rest along: said,
        // since what is written should be what the plan says.
        let renders = project.settings().is_some()
            && updates
                .iter()
                .any(|(asked, update)| *asked && update.kind == Kind::Rule && update.step.writes());
        let left = updates
            .iter()
            .filter(|(asked, update)| {
                update.kind == Kind::Rule
                    && update.standing == Standing::Edited
                    && !(*asked && matches!(update.step, Step::WriteOver(_)))
            })
            .map(|(_, update)| update.name.clone())
            .collect();
        let installations = updates
            .into_iter()
            .filter(|(asked, update)| {
                *asked
                    || (renders
                        && update.kind == Kind::Rule
                        && matches!(update.step, Step::Refresh | Step::PutBack(_)))
            })
            .map(|(_, update)| update)
            .collect();
        planned.push(ProjectPlan {
            root: project.root().to_path_buf(),
            name: project.name().to_owned(),
            installations,
            rendered: None,
            left,
        });
    }
    let unmatched = references
        .iter()
        .zip(&matched)
        .filter(|(_, seen)| !**seen)
        .map(|(reference, _)| reference.typed().to_owned())
        .collect();
    Ok(Plan {
        projects: planned,
        unmatched,
    })
}

/// What an update does with the installation `surveyed`, and what it is
/// written from when it is written: in the order a standing is said in,
/// edited first, so an edit is never written over unless `edits` forces it.
fn step(
    workspace: &FlayerWorkspace,
    surveyed: &install::Surveyed,
    edits: Edits,
) -> (Step, Option<Candidate>) {
    let step = match surveyed.standing {
        // Nothing to write it from, forced or not.
        Standing::Orphaned => return (Step::Orphaned, None),
        Standing::Edited if surveyed.offered.is_empty() => return (Step::Orphaned, None),
        Standing::Edited if edits == Edits::Kept => Step::Leave(surveyed.found(Found::Edited)),
        Standing::Edited => Step::WriteOver(surveyed.found(Found::Edited)),
        Standing::Missing => Step::PutBack(surveyed.found(Found::Missing)),
        Standing::Outdated => Step::Refresh,
        // Never said of an installation the ledger records, which is
        // neither absent nor somebody's.
        Standing::Current | Standing::Absent | Standing::Foreign => return (Step::Keep, None),
    };
    if !step.writes() {
        return (step, None);
    }
    match surveyed.source(workspace) {
        Ok(Some(offer)) => (
            step,
            Some(Candidate {
                offer: offer.clone(),
                standing: surveyed.standing,
            }),
        ),
        Ok(None) => (Step::Orphaned, None),
        Err(origins) => (Step::Unsure(origins), None),
    }
}

/// What updating every project a workspace links would do, as the panel's
/// page of updates draws it: each installation an update says something
/// of, outdated, edited, missing or orphaned, how it stands, what an update
/// does with it, and why it leaves one it leaves.
///
/// A read, as a dry run is ([`plan`]): the ledger opened without being
/// made, nothing written. A query of its own rather than a part of the
/// snapshot, which a live update reads again on every change: telling what
/// an update does reads where each installation came from again, past what
/// telling how it stands reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Updates {
    /// Each project the workspace links holding an installation an update
    /// says something of, in the order the registry lists them, each
    /// directory once, by the first entry spelling it.
    pub projects: Vec<ProjectUpdates>,
    /// How many of their installations stand in each state an update says
    /// something of, every such state in report order, one none stands in
    /// among them ([`State::asks_for_a_command`]): what a front end lists
    /// them by, each list counted.
    pub states: Vec<StateCount>,
    /// Whether updating every project writes something: what `flayer update
    /// --all --yes` would, rather than leave and say.
    pub writes: bool,
    /// Each entry that opens no project, by why: whether it holds anything
    /// to update cannot be told, so it is said, beside the rest.
    pub unread: Vec<Said>,
}

/// One project's installations an update says something of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProjectUpdates {
    /// The entry, as the registry spells it, which a task names it by.
    pub entry: String,
    /// What its marker calls it.
    pub name: String,
    /// Each of them, by kind and then name.
    pub installations: Vec<Pending>,
    /// Whether updating the project writes something.
    pub writes: bool,
}

/// One installation an update says something of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Pending {
    pub kind: Kind,
    pub name: String,
    /// Where it came from, as `--from` takes it: none for a record from
    /// before origins were kept.
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    pub origin: Option<String>,
    /// How it stands, as `status` says it.
    pub state: State,
    /// What an update does with it.
    pub does: Does,
    /// The copies that names, each a route from the project, `/`
    /// separated: put back, written over or left.
    pub copies: Vec<String>,
    /// Why an update leaves it as the project holds it, when it does
    /// ([`Left`]).
    pub left: Option<Said>,
    /// Whether a change reaches it only forced: a copy changed since it was
    /// installed, which an update not forced leaves alone and, once nothing
    /// offers it any more, an uninstall not forced leaves too. Said here so
    /// a front end sends `--force` by core's word, rather than telling from
    /// the state, the step, or both, which of core's refusals its change
    /// would meet.
    pub forced: bool,
}

impl Updates {
    /// What updating every project `workspace` links would do, against
    /// what its ledger records of them, every copy edited since left alone:
    /// the dry run of `flayer update --all`, as a front end reads it.
    ///
    /// Refused as a whole only when the ledger cannot be read; an entry
    /// that opens no project is said among the rest.
    pub fn read(workspace: &FlayerWorkspace) -> Result<Updates, UpdateError> {
        let mut unread = Vec::new();
        let mut entries: Vec<(String, PathBuf)> = Vec::new();
        let mut projects: Vec<MindProject> = Vec::new();
        for member in workspace.members() {
            match member.project {
                Ok(project) => {
                    entries.push((paths::as_route(&member.entry), project.root().to_owned()));
                    projects.push(project);
                }
                Err(error) => unread.push(Said::of(error.code(), &error)),
            }
        }
        let plan = plan(workspace, &projects, &[], Edits::Kept)?;
        let mut listed: Vec<ProjectUpdates> = Vec::new();
        for project in &plan.projects {
            // Planned once a directory, by the first project opened there.
            let Some((entry, _)) = entries
                .iter()
                .find(|(_, root)| same_directory(root, &project.root))
            else {
                continue;
            };
            let installations: Vec<Pending> = project
                .installations
                .iter()
                .filter(|update| update.step != Step::Keep)
                .filter_map(|update| {
                    Some(Pending {
                        kind: update.kind,
                        name: update.name.clone(),
                        origin: update.origin.clone(),
                        state: State::of_standing(update.standing)?,
                        does: update.step.does(),
                        copies: update
                            .step
                            .copies()
                            .iter()
                            .map(|path| install::route(&project.root, path))
                            .collect(),
                        left: update
                            .step
                            .left(update, &project.name, &project.root)
                            .map(|left| Said::of(left.code(), &left)),
                        forced: update.standing == Standing::Edited,
                    })
                })
                .collect();
            if installations.is_empty() {
                continue;
            }
            listed.push(ProjectUpdates {
                entry: entry.clone(),
                name: project.name.clone(),
                writes: project
                    .installations
                    .iter()
                    .any(|update| update.step.writes()),
                installations,
            });
        }
        let states = State::ALL
            .into_iter()
            .filter(|state| state.asks_for_a_command())
            .map(|state| StateCount {
                state,
                count: listed
                    .iter()
                    .flat_map(|project| &project.installations)
                    .filter(|pending| pending.state == state)
                    .count(),
            })
            .collect();
        Ok(Updates {
            writes: !plan.is_empty(),
            projects: listed,
            states,
            unread,
        })
    }
}

#[cfg(test)]
#[path = "update_test.rs"]
mod tests;
