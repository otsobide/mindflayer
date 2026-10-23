//! The minds screen: every mind project a workspace could manage, ticked where
//! it does, to link and unlink in one pass.
//!
//! What the link form and `u` do one project at a time, for everything under
//! the workspace at once. Ticking only marks. Applying runs the commands the
//! marks amount to — `mind -C <path> init` for a repository that is not a
//! project yet, then one `flayer link` and one `flayer unlink` — so, as
//! everywhere in the TUI, the screen does nothing a command could not.

use ratatui::crossterm::event::KeyCode;

use super::state::{Task, Workspace};

/// Where a row stands now, which decides what ticking or unticking it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Registered. `opens` is false for an entry whose directory no longer
    /// holds a project — exactly the one worth unticking.
    Linked { opens: bool },
    /// Under the workspace and not registered. `mind` is false for a git
    /// repository that is not a mind project yet, which linking makes one
    /// first.
    Found { mind: bool },
}

/// What applying one row would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Link,
    /// `mind init` in it, then link it.
    InitAndLink,
    Unlink,
}

/// One directory the workspace manages, or could.
#[derive(Debug, Clone)]
pub struct Row {
    /// The path a command is given: the entry as the marker spells it, or the
    /// route from the workspace root.
    pub path: String,
    pub standing: Standing,
    pub ticked: bool,
}

impl Row {
    /// What this row asks for, if its box disagrees with how it started.
    pub fn change(&self) -> Option<Change> {
        match (self.standing, self.ticked) {
            (Standing::Found { mind: true }, true) => Some(Change::Link),
            (Standing::Found { mind: false }, true) => Some(Change::InitAndLink),
            (Standing::Linked { .. }, false) => Some(Change::Unlink),
            _ => None,
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
    /// Every registered project, ticked, and every one found under the
    /// workspace that is not, unticked — in one list by path, because the
    /// question it answers is "which of these", not "which kind".
    pub fn of(workspace: &Workspace) -> Self {
        let mut rows: Vec<Row> = workspace
            .members
            .iter()
            .filter(|member| !member.own)
            .map(|member| Row {
                path: member.entry.clone(),
                standing: Standing::Linked {
                    opens: member.holdings.is_ok(),
                },
                ticked: true,
            })
            .collect();
        rows.extend(workspace.unlinked.iter().map(|unlinked| Row {
            path: unlinked.route.clone(),
            standing: Standing::Found {
                mind: unlinked.mind,
            },
            ticked: false,
        }));
        rows.sort_by(|a, b| a.path.cmp(&b.path));
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
                Change::Link => (link + 1, init, unlink),
                Change::InitAndLink => (link + 1, init + 1, unlink),
                Change::Unlink => (link, init, unlink + 1),
            })
    }

    /// Whether anything at all is marked.
    pub fn idle(&self) -> bool {
        self.rows.iter().all(|row| row.change().is_none())
    }

    /// The commands the marks amount to, or nothing when nothing is marked.
    pub fn task(&self) -> Option<Task> {
        let (mut init, mut link, mut unlink) = (Vec::new(), Vec::new(), Vec::new());
        for row in &self.rows {
            match row.change() {
                Some(Change::Link) => link.push(row.path.clone()),
                Some(Change::InitAndLink) => {
                    init.push(row.path.clone());
                    link.push(row.path.clone());
                }
                Some(Change::Unlink) => unlink.push(row.path.clone()),
                None => {}
            }
        }
        (!link.is_empty() || !unlink.is_empty()).then_some(Task::Minds { init, link, unlink })
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
                .filter(|row| row.change() == Some(Change::InitAndLink))
                .map(|row| row.path.as_str())
                .collect();
            let (is, it) = if init == 1 {
                ("is", "it")
            } else {
                ("are", "each")
            };
            asked.push(format!(
                "{} {is} not a mind project yet: `mind init` makes {it} one — a .mind, and empty skills/ and rules/.",
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
