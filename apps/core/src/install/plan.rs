//! A batch of assignments planned before any of it runs: which offers each
//! project holds, and what a selection of boxes amounts to (#78).
//!
//! Every front end that marks boxes, the install screen and the panel's
//! matrix alike, asks here, so the rules of a batch are said once, rather
//! than once a front end and once more in TypeScript (D-18):
//!
//! - **Which offer of a name a project holds** ([`holding`]): one box a
//!   name, the one its installation came from, however many places offer
//!   it.
//! - **What a selection amounts to** ([`Plan::of`]): an install of each
//!   offer asked for into every project asking it, an uninstall of each
//!   name asked away, by its name; an offer asked for where another of its
//!   name is held replaces it rather than taking it out first, and the
//!   changes go kind by kind, each where the first box asking it is.
//! - **What cannot be planned** ([`Conflict`], [`Locked`]): two offers of one
//!   name asked of one project, which has one folder of that name, and a
//!   change to a copy that is not Mindflayer's to change, held back by the
//!   code the command would refuse it with ([`Standing::refused`]).
//!
//! What it plans is what the command lines run, never more: each change is
//! an `install` or an `uninstall` a front end turns into its line, so the
//! batch still goes through the parsers, and an install still refuses there
//! whatever the disk says by the time it runs.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    is_current, kinds_named, offered_for, project_entry, receives, survey, surveyed, Candidate,
    InstallError, Offer, Refused, Standing, Surveyed,
};
use crate::catalog::{Reference, QUALIFIER};
use crate::code::{codes, Said};
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError};
use crate::policy::{self, PolicyError};
use crate::url::redacted;
use crate::workspace::{FlayerWorkspace, MindProject};

/// What `selection` amounts to in `workspace` as the disk stands now: each
/// box read as it stands ([`Asked`]), then planned ([`Plan::of`]).
///
/// A read, as a snapshot is: no lock, and the ledger only as
/// [`FlayerWorkspace::ledger_for_reading`] opens it, so nothing is made,
/// scrubbed or written, and a default workspace nobody has made stays
/// unmade. What it plans is still only a plan: the lines a front end runs
/// for it go through the commands, which refuse whatever the disk says by
/// then.
///
/// Each box names its project by its entry, as [`policy::linked`] takes
/// one, and one the workspace does not link refuses the whole plan, as a
/// `-p` that picks nothing refuses its whole line: a selection made of a
/// workspace that has changed since is no reason to plan the rest without
/// asking again. Each box the disk cannot be asked about is [`Locked`],
/// first, by the code of why, and the rest go ahead:
///
/// - **A project that does not open**, by the error opening it gave.
/// - **An offer asked for that its origin does not offer the project**,
///   as `not_offered`, or as what [`receives`] says when the project has
///   nowhere to put its kind at all, as `no_target`.
/// - **A bare name more than one kind is offered as from its origin**, as
///   `offered_as_kinds`, for the reference to say which, as an install of
///   it says.
///
/// All but the boxes of its name in its project, when it is asked for:
/// each of those that asks a change is locked by the same why, and none is
/// planned, as none is beside an edited copy. Planned without it, a held
/// offer asked away beside it would be taken out rather than replaced
/// ([`Plan::of`]), leaving the project without the name when another offer
/// of it was what was asked for, which a load that stopped opening or a
/// gathered entry gone since the snapshot does as much as a typo. Planning
/// the rest as though it were not there was the alternative, and is what
/// did that.
///
/// A box is read against the offers of its kind ([`survey`]), by its
/// origin as printed, which every listing prints and `--from` takes; and,
/// where none of them is from its origin, against what the ledger records
/// ([`surveyed`]), so an installation nothing offers any more where it came
/// from is asked away by the origin the snapshot says it came from. A box
/// asked away from a place nothing of its name came from asks for what is
/// so, and is not planned.
pub fn plan(workspace: &FlayerWorkspace, selection: &[Cell]) -> Result<Plan, PlanError> {
    let ledger = workspace.ledger_for_reading().map_err(InstallError::from)?;
    let mut read = Read::default();
    let mut read_off = Vec::with_capacity(selection.len());
    for cell in selection {
        read_off.push(read.ask(workspace, &ledger, cell)?);
    }
    // Every box asked for that could not be asked about, and why, before
    // any is planned: a box of its name may come before it.
    let unread: Vec<(&Cell, Said)> = selection
        .iter()
        .zip(&read_off)
        .filter_map(|(cell, read)| match read {
            Err(why) if cell.wanted => Some((cell, why.clone())),
            _ => None,
        })
        .collect();
    let mut asked = Vec::new();
    let mut unasked = Vec::new();
    for (cell, read) in selection.iter().zip(read_off) {
        match read {
            Ok(Some(one)) => match unread.iter().find(|(wanted, _)| names(wanted, &one)) {
                Some((_, why)) => {
                    // One asking for what is so changes nothing to hold.
                    if one.held != one.cell.wanted {
                        unasked.push(Locked {
                            cell: one.cell,
                            why: why.clone(),
                        });
                    }
                }
                None => asked.push(one),
            },
            Ok(None) => {}
            Err(why) => unasked.push(Locked {
                cell: cell.clone(),
                why,
            }),
        }
    }
    let mut plan = Plan::of(&asked);
    plan.locked.splice(0..0, unasked);
    Ok(plan)
}

/// Whether `cell` names the name `one` is of, in its project, as whichever
/// kind its reference may be: a bare name holds back the boxes of each kind
/// it could be, since which of them it meant is what could not be read.
fn names(cell: &Cell, one: &Asked) -> bool {
    let reference = Reference::parse(&cell.reference);
    cell.project == one.cell.project
        && reference.name() == one.name
        && kinds_named(&reference).contains(&one.kind)
}

/// Why a selection could not be planned at all: a project the workspace
/// does not link, or the ledger, or the offers, which could not be read.
#[derive(Debug, Error)]
pub enum PlanError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Install(#[from] InstallError),
}

codes!(PlanError {} carried {
    Self::Policy(error) => error,
    Self::Install(error) => error,
});

/// What a plan has read of the disk so far: each project once, and each
/// kind of each once, however many boxes ask about it.
#[derive(Default)]
struct Read {
    /// Each project by the entry a box named it by, or why it does not open.
    projects: Vec<(String, Result<MindProject, Said>)>,
    /// What each project is offered of each kind, by its entry.
    kinds: Vec<(String, Kind, Seen)>,
}

/// What one project is offered of one kind, and how each offer stands.
struct Seen {
    candidates: Vec<Candidate>,
    /// Whether the project holds each of `candidates` ([`holding`]).
    held: Vec<bool>,
    /// Every installation of the kind the ledger records in the project.
    recorded: Vec<Surveyed>,
    /// Why nothing of the kind can go into the project, when nothing can.
    receives: Result<(), Said>,
}

impl Seen {
    /// How the offer of `name` from `origin` stands, and whether the
    /// project holds it from there; or, offered from elsewhere or not at
    /// all, how the installation the ledger records from there stands, which
    /// it holds. None when neither is.
    fn find(&self, name: &str, origin: &str) -> Option<(Standing, bool)> {
        let named = |candidate: &&Candidate| candidate.name() == name;
        if let Some(at) = self
            .candidates
            .iter()
            .position(|candidate| candidate.name() == name && candidate.origin() == origin)
        {
            return Some((self.candidates[at].standing, self.held[at]));
        }
        let recorded = self.recorded.iter().find(|surveyed| {
            surveyed.record.name == name && surveyed.record.origin.as_deref() == Some(origin)
        })?;
        // Every offer of a name stands as its one installation, which an
        // offer of it says as a survey tells it.
        let standing = self
            .candidates
            .iter()
            .find(named)
            .map_or(recorded.standing, |candidate| candidate.standing);
        Some((standing, standing.present()))
    }
}

impl Read {
    /// `cell` as it stands, or none when it is asked away from where
    /// nothing is held; or why the disk cannot be asked about it.
    fn ask(
        &mut self,
        workspace: &FlayerWorkspace,
        ledger: &Ledger,
        cell: &Cell,
    ) -> Result<Result<Option<Asked>, Said>, PlanError> {
        let project = match self.project(workspace, &cell.project)? {
            Ok(project) => project.clone(),
            Err(why) => return Ok(Err(why.clone())),
        };
        let reference = Reference::parse(&cell.reference);
        let name = reference.name();
        let origin = redacted(&cell.origin).into_owned();
        let kinds = kinds_named(&reference);
        let kind = match kinds[..] {
            [kind] => kind,
            _ => {
                let mut offering = Vec::new();
                for &kind in &kinds {
                    let seen = self.seen(workspace, ledger, &project, &cell.project, kind)?;
                    if seen.find(name, &origin).is_some() {
                        offering.push(kind);
                    }
                }
                match offering[..] {
                    [kind] => kind,
                    [] => kinds[0],
                    _ => {
                        return Ok(Err(said_of(&InstallError::OfferedAsKinds {
                            name: name.to_owned(),
                            kinds: offering,
                        })))
                    }
                }
            }
        };
        let seen = self.seen(workspace, ledger, &project, &cell.project, kind)?;
        Ok(match seen.find(name, &origin) {
            // The box as it was found, by its origin as printed: two boxes
            // naming one offer, one with a credential typed into it, are
            // one origin, and no change installs from what was typed.
            Some((standing, held)) => Ok(Some(Asked {
                cell: Cell {
                    origin,
                    ..cell.clone()
                },
                kind,
                name: name.to_owned(),
                standing,
                held,
            })),
            None if cell.wanted => Err(match &seen.receives {
                Err(why) => why.clone(),
                Ok(()) => said_of(&InstallError::NotOffered {
                    name: name.to_owned(),
                    from: Some(origin),
                }),
            }),
            None => Ok(None),
        })
    }

    /// The project `entry` names, opened once, or why it does not open.
    fn project(
        &mut self,
        workspace: &FlayerWorkspace,
        entry: &str,
    ) -> Result<&Result<MindProject, Said>, PlanError> {
        let at = match self.projects.iter().position(|(seen, _)| seen == entry) {
            Some(at) => at,
            None => {
                let opened = policy::linked(workspace, entry)?
                    .project
                    .map_err(|error| Said::of(error.code(), &error));
                self.projects.push((entry.to_owned(), opened));
                self.projects.len() - 1
            }
        };
        Ok(&self.projects[at].1)
    }

    /// What `project`, which `entry` names, is offered of `kind`, read once.
    fn seen(
        &mut self,
        workspace: &FlayerWorkspace,
        ledger: &Ledger,
        project: &MindProject,
        entry: &str,
        kind: Kind,
    ) -> Result<&Seen, PlanError> {
        let at = match self
            .kinds
            .iter()
            .position(|(seen, of, _)| seen == entry && *of == kind)
        {
            Some(at) => at,
            None => {
                let candidates = survey(workspace, ledger, project, kind)?;
                let held =
                    holding(workspace, ledger, project, &candidates).map_err(InstallError::from)?;
                let recorded =
                    surveyed(workspace, ledger, project, kind).map_err(InstallError::from)?;
                let receives = receives(project, kind).map_err(|error| said_of(&error));
                self.kinds.push((
                    entry.to_owned(),
                    kind,
                    Seen {
                        candidates,
                        held,
                        recorded,
                        receives,
                    },
                ));
                self.kinds.len() - 1
            }
        };
        Ok(&self.kinds[at].2)
    }
}

/// What `error` says, by its code.
fn said_of(error: &InstallError) -> Said {
    Said::of(error.code(), error)
}

/// Whether `project` holds each of `candidates`, every offer it is surveyed
/// for ([`super::survey`]): what the box of each shows.
///
/// A name the project holds ticks one of its offers' boxes, since a project
/// has one copy of a name in each folder it installs in, whichever offer it
/// came from: the one its installation came from, as the ledger records it
/// ([`offered_for`]); among several a record from before origins were kept
/// may have come from, or when what it came from offers it no more, the
/// one whose content its copy is ([`is_current`]), and the first when none
/// is. Somebody's ticks every one, being there whichever was asked for and
/// moved by none. A name it does not hold, or whose every copy has gone,
/// ticks none: marking it puts the copies back.
///
/// By the record before the content: an installation outdated or edited
/// since is no offer's content, and the one it came from is still the one
/// an update brings it up to date from, which the panel reads off the
/// snapshot's `origin` too. The content alone, which every installation was
/// told by before origins were kept, ticked the first offer of the name for
/// either.
pub fn holding(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidates: &[Candidate],
) -> Result<Vec<bool>, LedgerError> {
    let entry = project_entry(workspace, project);
    let offers: Vec<Offer> = candidates
        .iter()
        .map(|candidate| candidate.offer.clone())
        .collect();
    let mut held: Vec<bool> = candidates
        .iter()
        .map(|candidate| candidate.standing.present())
        .collect();
    let mut told: Vec<(Kind, &str)> = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let named = (candidate.kind(), candidate.name());
        if !held[index] || candidate.standing == Standing::Foreign || told.contains(&named) {
            continue;
        }
        told.push(named);
        let same: Vec<usize> = (0..candidates.len())
            .filter(|&other| (candidates[other].kind(), candidates[other].name()) == named)
            .collect();
        if same.len() < 2 {
            continue;
        }
        let came: Vec<usize> = match ledger.installation(&entry, named.0, named.1)? {
            Some(record) => offered_for(&record, &offers)
                .into_iter()
                .filter_map(|offer| offers.iter().position(|at| std::ptr::eq(at, offer)))
                .collect(),
            None => Vec::new(),
        };
        let among = if came.is_empty() { &same } else { &came };
        let chosen = match among[..] {
            [one] => one,
            _ => among
                .iter()
                .copied()
                .find(|&at| is_current(workspace, ledger, project, &candidates[at]))
                .unwrap_or(among[0]),
        };
        for at in same {
            held[at] = at == chosen;
        }
    }
    Ok(held)
}

/// One box of a selection: whether `project` should hold what `reference`
/// names, as `origin` offers it.
///
/// `{"project": "alpha", "reference": "skill:deploy", "origin": "workspace",
/// "wanted": true}` in JSON, which the panel sends: the project by its entry
/// as the workspace's marker spells it, as a snapshot lists it, the artifact
/// as a command line names it, and the origin as `--from` takes it, which
/// every listing prints. A field it does not name is refused, as a task's
/// is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub project: String,
    pub reference: String,
    // Written as every URL is, whoever built the cell: one a browser sent
    // comes back inside a lock or a conflict as it was sent.
    #[serde(serialize_with = "crate::url::serialize_redacted")]
    pub origin: String,
    /// Whether the project should hold it from there: ticked, or not.
    pub wanted: bool,
}

impl Cell {
    /// The box of `kind` `name` from `origin` in `project`, its reference
    /// qualified with its kind, as a line installing it names it.
    pub fn of(project: &str, kind: Kind, name: &str, origin: &str, wanted: bool) -> Cell {
        Cell {
            project: project.to_owned(),
            reference: format!("{}{QUALIFIER}{name}", kind.slug()),
            origin: origin.to_owned(),
            wanted,
        }
    }
}

/// A box as it stands, and what it is asked to be: what [`Plan::of`] plans
/// from, once the disk has been read for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// The box, by its origin as printed ([`redacted`]): what an install
    /// of it is from, and what its name's boxes are told apart by.
    pub cell: Cell,
    /// What its reference names.
    pub kind: Kind,
    pub name: String,
    /// How its name stands with its project, as every offer of the name
    /// stands ([`super::survey`]).
    pub standing: Standing,
    /// Whether the project holds it from its origin ([`holding`]).
    pub held: bool,
}

/// What a selection amounts to: the changes that carry it out, and what of
/// it cannot be carried out, and why.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Plan {
    /// Each `install` and `uninstall` to run, in the order to run them.
    pub changes: Vec<Change>,
    /// Boxes that cannot all be as asked, and so are not planned at all.
    pub conflicts: Vec<Conflict>,
    /// Boxes whose change is not Mindflayer's to make, or cannot be asked of
    /// the project at all.
    pub locked: Vec<Locked>,
}

/// One line of a batch: an offer installed into each project, or a name
/// taken back out of each.
///
/// Tagged as a task is, by its name in snake_case beside its fields, so
/// each reads as the task that runs it: `{"type": "install", "projects":
/// ["alpha"], "kind": "skill", "name": "deploy", "from": "workspace"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    /// `flayer install -p <project>... --from <from> <kind:name>`: always
    /// from where the box is, though a name offered from one place needs
    /// none, so the line installs what was ticked or nothing when run again
    /// after a second place starts to offer the name.
    Install {
        projects: Vec<String>,
        kind: Kind,
        name: String,
        #[serde(serialize_with = "crate::url::serialize_redacted")]
        from: String,
    },
    /// `flayer uninstall -p <project>... <kind:name>`: the name, whichever
    /// offer its copy came from, as the command takes it.
    Uninstall {
        projects: Vec<String>,
        kind: Kind,
        name: String,
    },
}

/// Boxes of one name in one project that cannot all be as asked, and why:
/// none of them is planned, since which of them was meant is not something
/// to guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Conflict {
    pub project: String,
    pub kind: Kind,
    pub name: String,
    /// Each box that clashes, as it was asked.
    pub cells: Vec<Cell>,
    /// Why, by its code ([`Clash`]).
    pub why: Said,
}

/// A box whose change is not made, and why, by the code the change would be
/// refused with: `foreign` and `edited` as an install or an uninstall gives
/// them ([`Refused`]), or the error asking the project for it gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Locked {
    pub cell: Cell,
    pub why: Said,
}

/// Why boxes of one name in one project clash: what a [`Conflict`] says.
#[derive(Debug, Error)]
pub enum Clash {
    #[error("`{name}` is asked of {project} from more than one place ({}), and a project holds one {kind} of a name: ask for one", .origins.join(", "))]
    WantedTwice {
        project: String,
        kind: Kind,
        name: String,
        origins: Vec<String>,
    },
    #[error("`{name}` from {origin} is asked both to be in {project} and not to be")]
    BothWays {
        project: String,
        name: String,
        origin: String,
    },
}

codes!(Clash {
    Self::WantedTwice { .. } => "wanted_twice",
    Self::BothWays { .. } => "asked_both_ways",
});

impl Plan {
    /// What `asked` amounts to: every box asked of a project, each as it
    /// stands, in the order they were marked.
    ///
    /// Boxes of one name in one project are planned together, since the
    /// project has one copy of the name in each folder it installs in:
    ///
    /// - **One offer asked for**, and not held from there, is installed from
    ///   there; and so is one held, when another held offer of its name is
    ///   asked away, as an installation nothing offers any more is held
    ///   beside the offer of its name. An offer held and asked away beside it
    ///   is replaced by that install, never taken out first.
    /// - **None asked for**: a held offer asked away takes the name out.
    /// - **Two asked for**, or one box asked both ways, is a [`Conflict`],
    ///   and nothing of the name is planned there.
    /// - **A change to somebody's copy, or one edited since**, is
    ///   [`Locked`], by the code the command would refuse it with, and not
    ///   planned: the panel says it before a line runs, as the install
    ///   screen holds the box.
    ///
    /// Each offer's install is one change, with every project asking it, in
    /// the order asked, and each name's uninstall another, as `flayer
    /// install` and `flayer uninstall` take a `-p` a project: either way
    /// round a matrix is marked, one artifact's boxes are one line. Kind by
    /// kind, in [`Kind::ALL`]'s order, each where the first box asking it
    /// is: an install at the box asked for, an uninstall at the first box
    /// asked away.
    pub fn of(asked: &[Asked]) -> Plan {
        let mut plan = Plan::default();
        // Which box asks each change, by its place in `asked`.
        let mut asking: Vec<(usize, Decided)> = Vec::new();
        for group in groups(asked) {
            match decide(asked, &group) {
                Decision::Nothing => {}
                Decision::Change(at, decided) => asking.push((at, decided)),
                Decision::Clashes(conflicts) => plan.conflicts.extend(conflicts),
                Decision::Locked(locked) => plan.locked.extend(locked),
            }
        }
        asking.sort_by_key(|(at, _)| *at);
        for kind in Kind::ALL {
            for (at, decided) in &asking {
                let one = &asked[*at];
                if one.kind != kind {
                    continue;
                }
                let project = one.cell.project.clone();
                let same = |change: &&mut Change| match (decided, &**change) {
                    (
                        Decided::Install,
                        Change::Install {
                            kind, name, from, ..
                        },
                    ) => *kind == one.kind && *name == one.name && *from == one.cell.origin,
                    (Decided::Uninstall, Change::Uninstall { kind, name, .. }) => {
                        *kind == one.kind && *name == one.name
                    }
                    _ => false,
                };
                match plan.changes.iter_mut().find(same) {
                    Some(Change::Install { projects, .. } | Change::Uninstall { projects, .. }) => {
                        if !projects.contains(&project) {
                            projects.push(project);
                        }
                    }
                    None => plan.changes.push(match decided {
                        Decided::Install => Change::Install {
                            projects: vec![project],
                            kind,
                            name: one.name.clone(),
                            from: one.cell.origin.clone(),
                        },
                        Decided::Uninstall => Change::Uninstall {
                            projects: vec![project],
                            kind,
                            name: one.name.clone(),
                        },
                    }),
                }
            }
        }
        plan
    }
}

/// What one name in one project comes to.
enum Decision {
    Nothing,
    /// A change, asked by the box at that place in what was asked.
    Change(usize, Decided),
    Clashes(Vec<Conflict>),
    Locked(Vec<Locked>),
}

#[derive(Clone, Copy)]
enum Decided {
    Install,
    Uninstall,
}

/// One origin's boxes of a name in a project: every box asked of it, the
/// same box asked twice alike counted once.
struct Origin {
    /// The place of each, in what was asked, the first first.
    at: Vec<usize>,
    wanted: bool,
    held: bool,
    /// Asked both to be held and not.
    both: bool,
}

/// The boxes of `asked`, by their place in it, a group to each project and
/// name, in the order each was first asked.
fn groups(asked: &[Asked]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (at, one) in asked.iter().enumerate() {
        let alike = |group: &&mut Vec<usize>| {
            let first = &asked[group[0]];
            first.cell.project == one.cell.project
                && first.kind == one.kind
                && first.name == one.name
        };
        match groups.iter_mut().find(alike) {
            Some(group) => group.push(at),
            None => groups.push(vec![at]),
        }
    }
    groups
}

/// What the boxes at `group`, one project's of one name, come to.
fn decide(asked: &[Asked], group: &[usize]) -> Decision {
    let first = &asked[group[0]];
    let (project, kind, name) = (&first.cell.project, first.kind, &first.name);
    let mut origins: Vec<(&str, Origin)> = Vec::new();
    for &at in group {
        let one = &asked[at];
        match origins
            .iter_mut()
            .find(|(origin, _)| *origin == one.cell.origin)
        {
            Some((_, seen)) => {
                seen.both |= seen.wanted != one.cell.wanted;
                seen.at.push(at);
            }
            None => origins.push((
                &one.cell.origin,
                Origin {
                    at: vec![at],
                    wanted: one.cell.wanted,
                    held: one.held,
                    both: false,
                },
            )),
        }
    }
    let cells = |at: &[usize]| at.iter().map(|&at| asked[at].cell.clone()).collect();

    let both: Vec<Conflict> = origins
        .iter()
        .filter(|(_, seen)| seen.both)
        .map(|(origin, seen)| Conflict {
            project: project.clone(),
            kind,
            name: name.clone(),
            cells: cells(&seen.at),
            why: said(&Clash::BothWays {
                project: project.clone(),
                name: name.clone(),
                origin: (*origin).to_owned(),
            }),
        })
        .collect();
    if !both.is_empty() {
        return Decision::Clashes(both);
    }
    let wanted: Vec<&(&str, Origin)> = origins.iter().filter(|(_, seen)| seen.wanted).collect();
    if wanted.len() > 1 {
        let firsts: Vec<usize> = wanted.iter().map(|(_, seen)| seen.at[0]).collect();
        return Decision::Clashes(vec![Conflict {
            project: project.clone(),
            kind,
            name: name.clone(),
            cells: cells(&firsts),
            why: said(&Clash::WantedTwice {
                project: project.clone(),
                kind,
                name: name.clone(),
                origins: wanted
                    .iter()
                    .map(|(origin, _)| (*origin).to_owned())
                    .collect(),
            }),
        }]);
    }

    // Held, and asked away.
    let going: Vec<usize> = origins
        .iter()
        .filter(|(_, seen)| seen.held && !seen.wanted)
        .map(|(_, seen)| seen.at[0])
        .collect();
    let (at, decided, mut changing) = match wanted.first() {
        Some((_, seen)) if !seen.held || !going.is_empty() => {
            let mut changing = going.clone();
            if !seen.held {
                changing.insert(0, seen.at[0]);
            }
            (seen.at[0], Decided::Install, changing)
        }
        Some(_) => return Decision::Nothing,
        None => match going.first() {
            Some(&at) => (at, Decided::Uninstall, going.clone()),
            None => return Decision::Nothing,
        },
    };
    // Every box of a name stands as its one installation does, so any of
    // them says whether a change may reach it.
    match group.iter().find_map(|&at| asked[at].standing.refused()) {
        Some(refused) => {
            changing.sort_unstable();
            Decision::Locked(
                changing
                    .into_iter()
                    .map(|at| Locked {
                        cell: asked[at].cell.clone(),
                        why: held_back(refused, name, project),
                    })
                    .collect(),
            )
        }
        None => Decision::Change(at, decided),
    }
}

/// What `clash` says, by its code.
fn said(clash: &Clash) -> Said {
    Said {
        code: clash.code(),
        message: clash.to_string(),
    }
}

/// Why a change to `name` in `project` is held back, as a change reaching
/// it would refuse it: by the refusal's code.
fn held_back(refused: Refused, name: &str, project: &str) -> Said {
    // Matched on the refusal, so another has to say how it is said here.
    let message = match refused {
        Refused::Foreign => format!(
            "`{name}` is in {project}, and Mindflayer did not put it there, so it is left alone"
        ),
        Refused::Edited => format!(
            "`{name}` in {project} was changed since it was installed, so it is left alone unless a change to it is forced"
        ),
        Refused::NotInstalled => format!("nothing called `{name}` is installed in {project}"),
    };
    Said {
        code: refused.code(),
        message,
    }
}

#[cfg(test)]
#[path = "plan_test.rs"]
mod tests;
