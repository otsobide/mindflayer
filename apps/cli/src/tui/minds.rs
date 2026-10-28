//! The minds screen: every mind project a workspace could manage, ticked where
//! it does, to link and unlink in one pass.
//!
//! What the link form and `u` do one project at a time, for everything under
//! the workspace at once, or under the folders `flayer minds` was given,
//! listed as `flayer scan --suggest` lists them: what is not linked first,
//! most worth linking first, each with why, then what is.
//! Ticking only marks. Applying runs the commands the
//! marks amount to (`mind -C <path> init` for a repository that is not a
//! project yet, then one `flayer link` and one `flayer unlink`), so, as
//! everywhere in the TUI, the screen does nothing a command could not.

use ratatui::crossterm::event::KeyCode;

use super::state::{Task, Workspace};
use crate::commands::project::worth_linking;
use crate::task::MindsChange;

/// Where a row stands now, which decides what ticking or unticking it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Registered. `opens` is false for an entry whose directory no longer
    /// holds a project: exactly the one worth unticking.
    Linked { opens: bool },
    /// Found by a scan and not registered. `mind` is false for a git
    /// repository that is not a mind project yet, which linking makes one
    /// first.
    Found { mind: bool },
}

/// One directory the workspace manages, or could.
#[derive(Debug, Clone)]
pub struct Row {
    /// The path a command is given: the entry as the marker spells it, or
    /// what a scan spelled from the workspace root, the route from there or
    /// the path of one outside it.
    pub path: String,
    pub standing: Standing,
    pub ticked: bool,
    /// Why it is worth linking, in the words `flayer scan --suggest` says it
    /// with; empty for one linked.
    pub why: String,
}

impl Row {
    /// What this row asks for, if its box disagrees with how it started.
    pub fn change(&self) -> Option<MindsChange> {
        let started = matches!(self.standing, Standing::Linked { .. });
        (self.ticked != started).then(|| self.offers())
    }

    /// What turning its box over from how it started would ask for, ticked
    /// or not now: what the screen keeps room for beside it.
    pub fn offers(&self) -> MindsChange {
        match self.standing {
            Standing::Found { mind: true } => MindsChange::Link,
            Standing::Found { mind: false } => MindsChange::InitAndLink,
            Standing::Linked { .. } => MindsChange::Unlink,
        }
    }
}

/// What a key asks the TUI to do about the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Stay,
    /// Close it without applying anything.
    Leave,
    /// Ask to apply what is marked.
    Apply,
}

/// The screen.
#[derive(Debug, Clone)]
pub struct Minds {
    pub rows: Vec<Row>,
    pub cursor: usize,
}

impl Minds {
    /// Every one found that is not registered, unticked, in the order the
    /// scan ranked them, then every registered project, ticked, by its
    /// entry: the order `flayer scan --suggest` lists them in, because what
    /// the screen is opened for is what to link next.
    ///
    /// Those found are not sorted here: which is most worth linking is
    /// core's to say, and a screen sorting them could rank them differently
    /// from the command it shows. Those registered are, by the entry the
    /// marker spells, since they come from the marker rather than the scan,
    /// which may not reach them. Every registered one, even given folders to
    /// look under: a folder names where to look for more, not which of the
    /// workspace's own to show, and a project linked from elsewhere is one
    /// to untick too.
    pub fn of(workspace: &Workspace) -> Self {
        let mut rows: Vec<Row> = workspace
            .unlinked
            .iter()
            .map(|unlinked| Row {
                path: unlinked.route.clone(),
                standing: Standing::Found {
                    mind: unlinked.mind,
                },
                ticked: false,
                why: worth_linking(
                    &unlinked.route,
                    unlinked.moved.as_deref(),
                    unlinked.reason,
                    &unlinked.agent_files,
                ),
            })
            .collect();
        let mut linked: Vec<Row> = workspace
            .members
            .iter()
            .filter(|member| !member.own)
            .map(|member| Row {
                path: member.entry.clone(),
                standing: Standing::Linked {
                    opens: member.holdings.is_ok(),
                },
                ticked: true,
                why: String::new(),
            })
            .collect();
        linked.sort_by(|a, b| a.path.cmp(&b.path));
        rows.extend(linked);
        Self { rows, cursor: 0 }
    }

    /// Feed it a key.
    pub fn press(&mut self, key: KeyCode) -> Key {
        match key {
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if self.cursor + 1 < self.rows.len() => {
                self.cursor += 1;
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(row) = self.rows.get_mut(self.cursor) {
                    row.ticked = !row.ticked;
                }
            }
            KeyCode::Char('a') => return Key::Apply,
            KeyCode::Esc | KeyCode::Char('q') => return Key::Leave,
            _ => {}
        }
        Key::Stay
    }

    /// How many to link, how many of those need `mind init` first, and how
    /// many to unlink.
    pub fn counts(&self) -> (usize, usize, usize) {
        self.rows
            .iter()
            .filter_map(Row::change)
            .fold((0, 0, 0), |(link, init, unlink), change| match change {
                MindsChange::Link => (link + 1, init, unlink),
                MindsChange::InitAndLink => (link + 1, init + 1, unlink),
                MindsChange::Unlink => (link, init, unlink + 1),
            })
    }

    /// Whether anything at all is marked.
    pub fn idle(&self) -> bool {
        self.rows.iter().all(|row| row.change().is_none())
    }

    /// The commands the marks amount to, or nothing when nothing is marked:
    /// the batch every front end builds from its marks ([`Task::minds`]).
    pub fn task(&self) -> Option<Task> {
        Task::minds(
            self.rows
                .iter()
                .filter_map(|row| Some((row.path.clone(), row.change()?))),
        )
    }

    /// What applying asks, before anything is written: `mind init` writes into
    /// somebody's repository, so it is named rather than implied.
    pub fn question(&self) -> Vec<String> {
        let (link, init, unlink) = self.counts();
        let mut asked = Vec::new();
        asked.push(match (link, unlink) {
            (0, unlink) => format!("Unlink {unlink}?"),
            (link, 0) => format!("Link {link}?"),
            (link, unlink) => format!("Link {link} and unlink {unlink}?"),
        });
        asked.push(String::new());
        if init > 0 {
            let names: Vec<&str> = self
                .rows
                .iter()
                .filter(|row| row.change() == Some(MindsChange::InitAndLink))
                .map(|row| row.path.as_str())
                .collect();
            let (is, it) = if init == 1 {
                ("is", "it")
            } else {
                ("are", "each")
            };
            asked.push(format!(
                "{} {is} not a mind project yet: `mind init` makes {it} one (a .mind kept out of git, and empty skills/ and rules/).",
                names.join(", "),
            ));
        }
        if unlink > 0 {
            asked.push(String::from(
                "Unlinking only makes the workspace forget; nothing on disk is deleted.",
            ));
        }
        asked
    }
}
