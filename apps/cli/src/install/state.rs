//! What the install screen knows, and what a key does to it.
//!
//! Kept apart from the drawing and from the terminal so a test can drive it
//! the way a person does. Nothing here touches a file: pressing space marks a
//! box, and marking a box is a statement of intent that is carried out in one
//! batch when the screen is done with.

use mindflayer_core::install::{Candidate, Standing};
use mindflayer_core::MindProject;
use ratatui::crossterm::event::KeyCode;

/// Which column the keyboard is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Skills,
}

/// What marking a box asks for, once it disagrees with how it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Install,
    Remove,
}

/// One shelf entry as it stands with one project, and what the box says now.
#[derive(Debug, Clone)]
pub struct Row {
    pub candidate: Candidate,
    /// What the box shows.
    pub checked: bool,
    /// What it showed when the screen opened, which is what "pending" is
    /// measured against.
    pub was: bool,
}

impl Row {
    pub fn new(candidate: Candidate) -> Self {
        let checked = candidate.standing.present();
        Self {
            candidate,
            checked,
            was: checked,
        }
    }

    /// An artifact of this name is in the project and Mindflayer did not put
    /// it there, so it is neither replaced nor deleted.
    pub fn foreign(&self) -> bool {
        self.candidate.standing == Standing::Foreign
    }

    /// What this row is asking for, if anything.
    pub fn pending(&self) -> Option<Pending> {
        match (self.was, self.checked) {
            (false, true) => Some(Pending::Install),
            (true, false) => Some(Pending::Remove),
            _ => None,
        }
    }
}

/// One project, and every shelf entry seen from it.
#[derive(Debug)]
pub struct Target {
    pub project: MindProject,
    pub rows: Vec<Row>,
    /// Where the cursor sits in this project's list, remembered so moving away
    /// and back does not lose your place.
    pub cursor: usize,
}

impl Target {
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
        Self {
            targets,
            focus: Focus::Projects,
            cursor: 0,
            message: None,
            confirming: false,
        }
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
            _ => match self.focus {
                Focus::Projects => self.in_projects(key),
                Focus::Skills => self.in_skills(key),
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
                if self.current().is_some_and(|target| !target.rows.is_empty()) {
                    self.focus = Focus::Skills;
                } else {
                    self.message = Some(String::from("nothing on the shelf to install"));
                }
            }
            KeyCode::Esc => return Step::Quit,
            _ => {}
        }
        Step::Stay
    }

    fn in_skills(&mut self, key: KeyCode) -> Step {
        let Some(target) = self.targets.get_mut(self.cursor) else {
            return Step::Stay;
        };
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                target.cursor = target.cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if target.cursor + 1 < target.rows.len() {
                    target.cursor += 1;
                }
            }
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') => {
                self.focus = Focus::Projects;
            }
            KeyCode::Char(' ') => self.message = toggle(target),
            _ => {}
        }
        Step::Stay
    }
}

/// Mark or unmark the row under the cursor, or say why not.
fn toggle(target: &mut Target) -> Option<String> {
    let cursor = target.cursor;
    let row = target.rows.get(cursor)?;

    if row.foreign() {
        return Some(format!(
            "{} is already in {} and Mindflayer did not put it there, so it is left alone",
            row.candidate.name(),
            target.name()
        ));
    }

    let checked = !row.checked;
    let name = row.candidate.name().to_owned();
    target.rows[cursor].checked = checked;

    // One name, one artifact. Two shelves can offer `deploy`, but a project
    // has one directory called `deploy`, so marking one unmarks the other
    // rather than letting both be applied and the second win silently.
    if checked {
        let mut displaced = None;
        for (index, other) in target.rows.iter_mut().enumerate() {
            if index != cursor && other.checked && other.candidate.name() == name {
                other.checked = false;
                displaced = Some(other.candidate.origin());
            }
        }
        if let Some(origin) = displaced {
            return Some(format!("{name}: unmarked the one from {origin}"));
        }
    }
    None
}
