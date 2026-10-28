//! What the install screen knows, and what a key does to it.
//!
//! Kept apart from the drawing and from the terminal so a test can drive it
//! the way a person does. Nothing here touches a file: pressing space marks a
//! box, and marking a box is a statement of intent, carried out when the
//! screen is done with as the command lines every mark amounts to
//! ([`Screen::task`]).
//!
//! Every kind is offered, in one column filtered to one kind at a time:
//! `tab` shows the next. A filter rather than a column a kind, so the screen
//! keeps its two columns however many kinds there are, and rather than every
//! kind in one list, where a skill and a rule of one name would sit side by
//! side as two boxes for what reads as one thing.
//!
//! `v` turns the screen round, by artifact: every offer of the kind shown on
//! the left, and on the right every project, ticked where it holds the one
//! under the cursor. The same boxes either way round, each a project's row
//! of an offer, so a mark made one way is there the other way, and what is
//! applied is one batch whichever way it was marked. `v` rather than `tab`,
//! the key such a view would have had before rules could be installed:
//! `tab` shows the next kind, which means the same either way round, so the
//! two cannot share it.

use mindflayer_core::install::{self, Candidate, Standing};
use mindflayer_core::{Kind, MindProject};
use ratatui::crossterm::event::KeyCode;

use crate::commands::install::named;
use crate::task::Task;

/// Which column the keyboard is in, and so which way round the screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    /// What can be installed into the project on the left, of the kind
    /// shown ([`Screen::kind`]).
    Offers,
    /// Every offer of the kind shown, from wherever it is offered: the left
    /// column of the screen turned round by artifact.
    Artifacts,
    /// Every project, ticked where it holds the artifact on the left: the
    /// right column of the screen turned round.
    Receivers,
}

impl Focus {
    /// Whether the screen is turned round by artifact.
    pub fn by_artifact(self) -> bool {
        matches!(self, Focus::Artifacts | Focus::Receivers)
    }
}

/// What marking a box asks for, once it disagrees with how it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Install,
    Remove,
}

/// One offer as it stands with one project, and what the box says now.
#[derive(Debug, Clone)]
pub struct Row {
    pub candidate: Candidate,
    /// What the box shows.
    pub checked: bool,
    /// What it showed when the screen opened, which is what "pending" is
    /// measured against.
    pub was: bool,
    /// Another offer of the same kind and name was checked in its place:
    /// installing that one replaces this one's copy, so this one is not
    /// removed.
    pub superseded: bool,
}

impl Row {
    pub fn new(candidate: Candidate) -> Self {
        let checked = candidate.standing.present();
        Self {
            candidate,
            checked,
            was: checked,
            superseded: false,
        }
    }

    /// An artifact of this name is in the project and Mindflayer did not put
    /// it there, so it is neither replaced nor deleted.
    pub fn foreign(&self) -> bool {
        self.candidate.standing == Standing::Foreign
    }

    /// Mindflayer installed it, and a copy was changed since, so the screen,
    /// which never forces, neither replaces nor deletes it.
    pub fn edited(&self) -> bool {
        self.candidate.standing == Standing::Edited
    }

    /// Mindflayer installed it, and the place it came from offers something
    /// else by now: its box is held as any install's is, since the screen
    /// marks what goes in and out, and `flayer update` brings it up to date.
    pub fn outdated(&self) -> bool {
        self.candidate.standing == Standing::Outdated
    }

    /// Mindflayer installed it, and a copy has gone since: its box opens
    /// empty, as core says ([`Standing::present`]), and marking it puts every
    /// copy back.
    pub fn missing(&self) -> bool {
        self.candidate.standing == Standing::Missing
    }

    /// Whether it is an offer of the same artifact as `other`: one kind and
    /// one name, which a project holds one of. A skill and a rule of one
    /// name are two.
    fn same(&self, other: &Row) -> bool {
        self.candidate.kind() == other.candidate.kind()
            && self.candidate.name() == other.candidate.name()
    }

    /// Whether it is the very offer `other` is, as another project sees it:
    /// one kind and name from one place, which the screen turned round lists
    /// once. Two places offering one name are two of its rows, as they are
    /// two boxes in a project.
    pub fn same_offer(&self, other: &Row) -> bool {
        self.same(other) && self.candidate.origin() == other.candidate.origin()
    }

    /// What this row is asking for, if anything.
    pub fn pending(&self) -> Option<Pending> {
        if self.superseded {
            return None;
        }
        match (self.was, self.checked) {
            (false, true) => Some(Pending::Install),
            (true, false) => Some(Pending::Remove),
            _ => None,
        }
    }
}

/// One project, and every offer of every kind seen from it.
#[derive(Debug)]
pub struct Target {
    pub project: MindProject,
    /// What picks it out of the workspace on a command line, as core says
    /// ([`FlayerWorkspace::naming`]): the `-p` the lines its marks amount
    /// to name it by.
    ///
    /// [`FlayerWorkspace::naming`]: mindflayer_core::FlayerWorkspace::naming
    pub picked_by: String,
    /// Every kind's, each kind's together in the order [`Kind::ALL`] gives.
    pub rows: Vec<Row>,
    /// The row the cursor sits on, among those of the kind shown, remembered
    /// so moving away and back does not lose your place.
    pub cursor: usize,
}

impl Target {
    /// When several offers of one name are in the project (they share its
    /// one folder, so standing alone says each of them is), keep the box of
    /// the one whose copy is there, as `current` tells, and clear the rest;
    /// the first, if none matches because the copy was changed since.
    pub fn settle(&mut self, current: impl Fn(&Candidate) -> bool) {
        let mut held: Vec<usize> = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            if row.was && !row.foreign() && !held.iter().any(|&seen| self.rows[seen].same(row)) {
                held.push(index);
            }
        }
        for first in held {
            let same: Vec<usize> = (0..self.rows.len())
                .filter(|&index| self.rows[index].same(&self.rows[first]))
                .collect();
            if same.len() < 2 {
                continue;
            }
            let chosen = same
                .iter()
                .copied()
                .find(|&index| current(&self.rows[index].candidate))
                .unwrap_or(same[0]);
            for index in same {
                let row = &mut self.rows[index];
                row.was = index == chosen;
                row.checked = row.was;
            }
        }
    }

    pub fn new(project: MindProject, picked_by: String, candidates: Vec<Candidate>) -> Self {
        Self {
            project,
            picked_by,
            rows: candidates.into_iter().map(Row::new).collect(),
            cursor: 0,
        }
    }

    pub fn name(&self) -> &str {
        self.project.name()
    }

    /// What to say in place of its offers of `kind` when nothing of it can
    /// be installed into the project at all, as core says
    /// ([`install::receives`]): pointing at the shelf would send somebody to
    /// gather for a project that cannot take what they gather. `None` when
    /// something can.
    pub fn unreachable(&self, kind: Kind) -> Option<String> {
        install::receives(&self.project, kind)
            .err()
            .map(|error| error.to_string())
    }

    /// Its row of the offer `artifact` is, by its place in [`Target::rows`]:
    /// none when it is offered nothing of that kind, having nowhere to put
    /// it ([`Target::unreachable`]).
    pub fn row_of(&self, artifact: &Row) -> Option<usize> {
        self.rows.iter().position(|row| row.same_offer(artifact))
    }

    /// The rows of `kind`, by their place in [`Target::rows`].
    pub fn shown(&self, kind: Kind) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&index| self.rows[index].candidate.kind() == kind)
            .collect()
    }

    /// Put the cursor on a row of `kind`, its first, unless it is on one.
    fn show(&mut self, kind: Kind) {
        let shown = self.shown(kind);
        if !shown.contains(&self.cursor) {
            if let Some(&first) = shown.first() {
                self.cursor = first;
            }
        }
    }

    /// How many of this project's boxes disagree with how they started.
    pub fn changes(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.pending().is_some())
            .count()
    }
}

/// What a key press asks the caller to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Redraw and keep going.
    Stay,
    /// Carry out everything that was marked.
    Apply,
    /// Leave without touching anything.
    Quit,
}

/// The screen.
#[derive(Debug)]
pub struct Screen {
    pub targets: Vec<Target>,
    pub focus: Focus,
    /// The kind the offers column shows: skills first, the kind installing
    /// was about before rules could be installed too.
    pub kind: Kind,
    /// Where the cursor sits in the project column.
    pub cursor: usize,
    /// Where it sits among the artifacts of the kind shown, turned round
    /// ([`Screen::artifacts`]).
    pub artifact: usize,
    /// Where it sits among the projects, turned round: a place in
    /// [`Screen::targets`], as [`Screen::cursor`] is.
    pub receiver: usize,
    /// One line saying why the last key did nothing, when it did nothing.
    pub message: Option<String>,
    /// Whether the confirmation is up. Applying removes files, so it is asked
    /// for rather than assumed.
    pub confirming: bool,
}

impl Screen {
    pub fn new(targets: Vec<Target>) -> Self {
        let mut screen = Self {
            targets,
            focus: Focus::Projects,
            kind: Kind::Skill,
            cursor: 0,
            artifact: 0,
            receiver: 0,
            message: None,
            confirming: false,
        };
        screen.show(Kind::Skill);
        screen
    }

    /// Show the offers of `kind`, each project's cursor on one of them, and
    /// the cursor turned round on the first.
    fn show(&mut self, kind: Kind) {
        self.kind = kind;
        self.artifact = 0;
        for target in &mut self.targets {
            target.show(kind);
        }
    }

    /// Every offer of the kind shown, once each, as the screen turned round
    /// lists them: a row of each, from the first project offered it, in the
    /// order the projects are offered them. A project with nowhere to put
    /// the kind is offered none, so it adds none.
    pub fn artifacts(&self) -> Vec<&Row> {
        let mut listed: Vec<&Row> = Vec::new();
        for target in &self.targets {
            for index in target.shown(self.kind) {
                let row = &target.rows[index];
                if !listed.iter().any(|seen| seen.same_offer(row)) {
                    listed.push(row);
                }
            }
        }
        listed
    }

    /// The artifact the cursor is on, turned round.
    pub fn chosen(&self) -> Option<&Row> {
        self.artifacts().get(self.artifact).copied()
    }

    /// How many projects' boxes of the offer `artifact` is disagree with how
    /// they started.
    pub fn changes_of(&self, artifact: &Row) -> usize {
        self.targets
            .iter()
            .filter_map(|target| target.row_of(artifact).map(|index| &target.rows[index]))
            .filter(|row| row.pending().is_some())
            .count()
    }

    /// Turn the screen round, keeping the box under the cursor under it: a
    /// project's row of an offer is the offer's row of that project. The
    /// column the keyboard is in stays the same side, so `v` from a box
    /// lands on that box, and from the left column on the left column.
    fn turn(&mut self) {
        let kind = self.kind;
        if self.focus.by_artifact() {
            let chosen = self.chosen().cloned();
            self.cursor = self.receiver;
            let Some(target) = self.targets.get_mut(self.cursor) else {
                self.focus = Focus::Projects;
                return;
            };
            if let Some(index) = chosen.as_ref().and_then(|row| target.row_of(row)) {
                target.cursor = index;
            }
            let offered = !target.shown(kind).is_empty();
            self.focus = match self.focus {
                Focus::Receivers if offered => Focus::Offers,
                _ => Focus::Projects,
            };
            return;
        }
        self.receiver = self.cursor;
        let held = self
            .current()
            .and_then(|target| target.rows.get(target.cursor))
            .filter(|row| row.candidate.kind() == kind)
            .cloned();
        self.artifact = held
            .and_then(|held| {
                self.artifacts()
                    .iter()
                    .position(|row| row.same_offer(&held))
            })
            .unwrap_or(0);
        self.focus = match self.focus {
            Focus::Offers => Focus::Receivers,
            _ => Focus::Artifacts,
        };
    }

    /// The kind `tab` shows next, or `shift+tab` (`back`), round again past
    /// the last.
    pub fn next_kind(&self, back: bool) -> Kind {
        let count = Kind::ALL.len();
        let at = Kind::ALL
            .iter()
            .position(|&kind| kind == self.kind)
            .unwrap_or(0);
        Kind::ALL[if back { at + count - 1 } else { at + 1 } % count]
    }

    /// The project the cursor is on.
    pub fn current(&self) -> Option<&Target> {
        self.targets.get(self.cursor)
    }

    /// Everything marked, as (project, row) pairs.
    pub fn pending(&self) -> Vec<(usize, usize, Pending)> {
        let mut pending = Vec::new();
        for (target, rows) in self.targets.iter().enumerate() {
            for (row, entry) in rows.rows.iter().enumerate() {
                if let Some(what) = entry.pending() {
                    pending.push((target, row, what));
                }
            }
        }
        pending
    }

    /// How many installs and how many removals are waiting.
    pub fn counts(&self) -> (usize, usize) {
        self.pending()
            .iter()
            .fold((0, 0), |(install, remove), (_, _, what)| match what {
                Pending::Install => (install + 1, remove),
                Pending::Remove => (install, remove + 1),
            })
    }

    /// Whether anything at all is marked.
    pub fn idle(&self) -> bool {
        self.pending().is_empty()
    }

    /// Everything marked, as the command lines it amounts to: an install of
    /// each offer into every project ticked to take it, and an uninstall of
    /// each artifact from every project unticked, kind by kind, each line
    /// where the first box it holds is found going down the projects.
    ///
    /// A line an offer rather than a line a box, since `flayer install` and
    /// `flayer uninstall` take a `-p` for each project, which is how the
    /// screen turned round marks one: either way round, one artifact's
    /// ticks are one line. An install always says `--from`, though a name
    /// one place offers needs none: the box is one offer, and its line
    /// names that one, so it installs what was ticked or nothing when run
    /// again after a second place starts to offer the name, and whether a
    /// name needs it is core's to say ([`install::offered`]), not the
    /// screen's to work out again. An uninstall is of the name, whichever
    /// offer the copy came from, as the command takes it.
    pub fn task(&self) -> Task {
        let mut changes: Vec<Task> = Vec::new();
        for kind in Kind::ALL {
            for (target, row, what) in self.pending() {
                let target = &self.targets[target];
                let candidate = &target.rows[row].candidate;
                if candidate.kind() != kind {
                    continue;
                }
                let name = candidate.name();
                let origin = candidate.origin();
                let same = |change: &Task| match (what, change) {
                    (
                        Pending::Install,
                        Task::Install {
                            kind: of,
                            name: called,
                            from: Some(from),
                            ..
                        },
                    ) => *of == kind && called == name && *from == origin,
                    (
                        Pending::Remove,
                        Task::Uninstall {
                            kind: of,
                            name: called,
                            ..
                        },
                    ) => *of == kind && called == name,
                    _ => false,
                };
                let project = target.picked_by.clone();
                match changes.iter_mut().find(|change| same(change)) {
                    Some(Task::Install { projects, .. } | Task::Uninstall { projects, .. }) => {
                        projects.push(project);
                    }
                    _ => changes.push(match what {
                        Pending::Install => Task::Install {
                            projects: vec![project],
                            kind,
                            name: name.to_owned(),
                            from: Some(origin),
                        },
                        Pending::Remove => Task::Uninstall {
                            projects: vec![project],
                            kind,
                            name: name.to_owned(),
                        },
                    }),
                }
            }
        }
        Task::ApplyInstall { changes }
    }

    /// Feed it a key.
    pub fn press(&mut self, key: KeyCode) -> Step {
        // A message explains the key before this one, so it goes as soon as
        // another is pressed rather than sitting there being about nothing.
        self.message = None;

        if self.confirming {
            return self.confirm(key);
        }
        match key {
            KeyCode::Char('q') => Step::Quit,
            KeyCode::Char('a') => self.ask(),
            KeyCode::Tab | KeyCode::BackTab => {
                self.show(self.next_kind(key == KeyCode::BackTab));
                Step::Stay
            }
            KeyCode::Char('v') => {
                self.turn();
                Step::Stay
            }
            _ => match self.focus {
                Focus::Projects => self.in_projects(key),
                Focus::Offers => self.in_offers(key),
                Focus::Artifacts => self.in_artifacts(key),
                Focus::Receivers => self.in_receivers(key),
            },
        }
    }

    /// Applying writes and deletes, so it is confirmed. Nothing marked is not
    /// a question worth asking.
    fn ask(&mut self) -> Step {
        if self.idle() {
            self.message = Some(String::from("nothing marked"));
            return Step::Stay;
        }
        self.confirming = true;
        Step::Stay
    }

    fn confirm(&mut self, key: KeyCode) -> Step {
        match key {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.confirming = false;
                Step::Apply
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.confirming = false;
                Step::Stay
            }
            _ => Step::Stay,
        }
    }

    fn in_projects(&mut self, key: KeyCode) -> Step {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < self.targets.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => {
                let kind = self.kind;
                if self
                    .current()
                    .is_some_and(|target| !target.shown(kind).is_empty())
                {
                    self.focus = Focus::Offers;
                } else {
                    let said = self.current().and_then(|target| target.unreachable(kind));
                    self.message = Some(said.unwrap_or_else(|| nothing(kind).to_owned()));
                }
            }
            KeyCode::Esc => return Step::Quit,
            _ => {}
        }
        Step::Stay
    }

    fn in_offers(&mut self, key: KeyCode) -> Step {
        let kind = self.kind;
        let Some(target) = self.targets.get_mut(self.cursor) else {
            return Step::Stay;
        };
        // Among the rows of the kind shown, by their place among all.
        let shown = target.shown(kind);
        let at = shown.iter().position(|&index| index == target.cursor);
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(at) = at.filter(|&at| at > 0) {
                    target.cursor = shown[at - 1];
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(&next) = at.and_then(|at| shown.get(at + 1)) {
                    target.cursor = next;
                }
            }
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') => {
                self.focus = Focus::Projects;
            }
            // Only a row the column shows: a kind with none shows none.
            KeyCode::Char(' ') if at.is_some() => self.message = toggle(target, target.cursor),
            _ => {}
        }
        Step::Stay
    }

    /// The left column turned round: the artifacts of the kind shown.
    fn in_artifacts(&mut self, key: KeyCode) -> Step {
        let count = self.artifacts().len();
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.artifact = self.artifact.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.artifact + 1 < count {
                    self.artifact += 1;
                }
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => {
                if self.chosen().is_some() {
                    self.focus = Focus::Receivers;
                } else {
                    self.message = Some(self.none_offered());
                }
            }
            KeyCode::Esc => return Step::Quit,
            _ => {}
        }
        Step::Stay
    }

    /// The right column turned round: every project, for the artifact on
    /// the left. A project offered nothing of its kind has no box, and says
    /// why instead, as the screen the right way round says it in place of
    /// its offers.
    fn in_receivers(&mut self, key: KeyCode) -> Step {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.receiver = self.receiver.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.receiver + 1 < self.targets.len() {
                    self.receiver += 1;
                }
            }
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') => {
                self.focus = Focus::Artifacts;
            }
            KeyCode::Char(' ') => {
                let kind = self.kind;
                let Some(chosen) = self.chosen().cloned() else {
                    self.message = Some(self.none_offered());
                    return Step::Stay;
                };
                let Some(target) = self.targets.get_mut(self.receiver) else {
                    return Step::Stay;
                };
                self.message = match target.row_of(&chosen) {
                    Some(index) => toggle(target, index),
                    None => Some(
                        target
                            .unreachable(kind)
                            .unwrap_or_else(|| nothing(kind).to_owned()),
                    ),
                };
            }
            _ => {}
        }
        Step::Stay
    }

    /// What to say turned round when nothing of the kind shown is offered
    /// to any project: that no project can take it, when none can, as each
    /// says the right way round, or else where that kind comes from.
    pub fn none_offered(&self) -> String {
        if self.none_can_take() {
            return format!(
                "no project here can take {}s; v turns the screen back, where each says why",
                self.kind
            );
        }
        nothing(self.kind).to_owned()
    }

    /// Whether every project has nowhere to put the kind shown, as core
    /// says of each ([`Target::unreachable`]).
    pub fn none_can_take(&self) -> bool {
        !self.targets.is_empty()
            && self
                .targets
                .iter()
                .all(|target| target.unreachable(self.kind).is_some())
    }
}

/// What to say for a project offered nothing of `kind`, though it could take
/// it: where that kind comes from. A rule is never gathered, so the shelf
/// is not where to look for one.
pub fn nothing(kind: Kind) -> &'static str {
    // Matched on the kind, so a third one has to say where it comes from.
    match kind {
        Kind::Skill => {
            "nothing on the shelf to install, and nothing loaded or of the workspace's own"
        }
        Kind::Rule => "no rules to install: none of the workspace's own, and none loaded",
    }
}

/// Mark or unmark the row at `cursor`, or say why not: the one under the
/// project's own cursor, or, on the screen turned round by artifact, the
/// one of the artifact chosen there, which leaves the project's cursor
/// where it was.
fn toggle(target: &mut Target, cursor: usize) -> Option<String> {
    let row = target.rows.get(cursor)?;

    if row.foreign() {
        return Some(format!(
            "{} is already in {} and Mindflayer did not put it there, so it is left alone",
            named(row.candidate.kind(), row.candidate.name()),
            target.name()
        ));
    }
    // Every offer of the name stands as its one installation, so each of
    // them is refused alike, and no other can be marked over it.
    if row.edited() {
        return Some(format!(
            "{} in {} was changed since it was installed, so it is left alone; --force on \
             `flayer install` or `flayer uninstall` reaches it",
            named(row.candidate.kind(), row.candidate.name()),
            target.name()
        ));
    }

    let checked = !row.checked;
    let name = named(row.candidate.kind(), row.candidate.name());
    let this = row.clone();

    // One kind and name, one artifact. Two places can offer the skill
    // `deploy`, but a project has one directory called `deploy`, so marking
    // one unmarks the other rather than letting both be applied and the
    // second win silently, and the one it displaces is not removed as well,
    // because installing this one replaces it. Unmarking it again puts back
    // what it displaced. A rule called `deploy` is another thing, and keeps
    // its own box.
    let mut displaced = None;
    for (index, other) in target.rows.iter_mut().enumerate() {
        if index == cursor || !other.same(&this) {
            continue;
        }
        if checked && (other.checked || other.was) {
            if other.checked {
                displaced = Some(other.candidate.origin());
            }
            other.checked = false;
            other.superseded = true;
        } else if !checked && other.superseded {
            other.superseded = false;
            other.checked = other.was;
        }
    }
    let row = &mut target.rows[cursor];
    row.checked = checked;
    row.superseded = false;

    displaced.map(|origin| format!("{name}: unmarked the one from {origin}"))
}
