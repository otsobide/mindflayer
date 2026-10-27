//! What the install screen knows, and what a key does to it.
//!
//! Kept apart from the drawing and from the terminal so a test can drive it
//! the way a person does. Nothing here touches a file: pressing space marks a
//! box, and marking a box is a statement of intent that is carried out in one
//! batch when the screen is done with.
//!
//! Every kind is offered, in one column filtered to one kind at a time:
//! `tab` shows the next. A filter rather than a column a kind, so the screen
//! keeps its two columns however many kinds there are, and rather than every
//! kind in one list, where a skill and a rule of one name would sit side by
//! side as two boxes for what reads as one thing.

use mindflayer_core::install::{self, Candidate, Standing};
use mindflayer_core::{Kind, MindProject};
use ratatui::crossterm::event::KeyCode;

use crate::commands::install::named;

/// Which column the keyboard is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    /// What can be installed into the project on the left, of the kind
    /// shown ([`Screen::kind`]).
    Offers,
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

    pub fn new(project: MindProject, candidates: Vec<Candidate>) -> Self {
        Self {
            project,
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

    /// Make the box at `index` say `checked` as a space on it would, other
    /// offers of its kind and name included, saying whether it now does: a
    /// foreign row's box cannot be changed.
    ///
    /// For marks made on another screen of the same projects, carried here
    /// when the shelf changed under them.
    pub fn mark(&mut self, index: usize, checked: bool) -> bool {
        if self
            .rows
            .get(index)
            .is_some_and(|row| row.checked != checked)
        {
            let cursor = std::mem::replace(&mut self.cursor, index);
            toggle(self);
            self.cursor = cursor;
        }
        self.rows
            .get(index)
            .is_some_and(|row| row.checked == checked)
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
            message: None,
            confirming: false,
        };
        screen.show(Kind::Skill);
        screen
    }

    /// Show the offers of `kind`, each project's cursor on one of them.
    fn show(&mut self, kind: Kind) {
        self.kind = kind;
        for target in &mut self.targets {
            target.show(kind);
        }
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
            _ => match self.focus {
                Focus::Projects => self.in_projects(key),
                Focus::Offers => self.in_offers(key),
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
            KeyCode::Char(' ') if at.is_some() => self.message = toggle(target),
            _ => {}
        }
        Step::Stay
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

/// Mark or unmark the row under the cursor, or say why not.
fn toggle(target: &mut Target) -> Option<String> {
    let cursor = target.cursor;
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
