//! Whose names a file on the disk was written under: Mindweaver's, which
//! every build from #229 on writes, or Mindflayer's, which the builds before
//! it wrote (D-26).
//!
//! Mindweaver was Mindflayer, and what a build of it wrote outside its own
//! code is spelled the old way: a workspace's `.mindflayer`, its
//! `flayer.toml` and `mindflayer.db`, the markers of the blocks in a
//! project's files and in git's exclude file, Claude's rules folder. Each is
//! read beside its new name, and moved to it only when somebody asks, as a
//! format is moved only when asked. What a build makes is spelled the new
//! way; what it finds keeps the names it was found under, so a block an
//! older build wrote is replaced under its own markers, and a workspace one
//! made is written in its own folder, marker and ledger. Never a new one beside an old one:
//! two workspaces in one folder, or two blocks in one file, would each be
//! read by one build and missed by the other.
//!
//! One word for the pair rather than a flag on each name, so a reader that
//! finds one of a block's markers knows how its others are spelled, and a
//! workspace says once which name each of its files goes by. Each name
//! itself is held in the constant of the module that writes it, the old one
//! beside it.

use std::fmt;

use crate::ledger::{FLAYER_LEDGER_FILE, LEDGER_FILE};
use crate::workspace::{FLAYER_CONFIG, FLAYER_DIR, WEAVER_CONFIG, WEAVER_DIR};

/// Which product's names something on the disk is written under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Names {
    /// Mindweaver's: what everything made from now on is written under.
    Mindweaver,
    /// Mindflayer's: what a build from before #229 wrote, read as it was
    /// written and moved only when asked.
    Mindflayer,
}

impl Names {
    /// Every one, in the order a folder is looked in: the new before the
    /// old, so where both are there the new is read and the old said, as a
    /// project's newest marker is read before an older one beside it.
    pub const ALL: [Names; 2] = [Names::Mindweaver, Names::Mindflayer];

    /// The product they are the names of, as a sentence says it.
    pub const fn product(self) -> &'static str {
        match self {
            Names::Mindweaver => "Mindweaver",
            Names::Mindflayer => "Mindflayer",
        }
    }

    /// The folder a workspace is identified by: [`WEAVER_DIR`] or
    /// [`FLAYER_DIR`].
    pub const fn workspace_dir(self) -> &'static str {
        match self {
            Names::Mindweaver => WEAVER_DIR,
            Names::Mindflayer => FLAYER_DIR,
        }
    }

    /// The marker inside it: [`WEAVER_CONFIG`] or [`FLAYER_CONFIG`].
    pub const fn workspace_config(self) -> &'static str {
        match self {
            Names::Mindweaver => WEAVER_CONFIG,
            Names::Mindflayer => FLAYER_CONFIG,
        }
    }

    /// The ledger inside it: [`LEDGER_FILE`] or [`FLAYER_LEDGER_FILE`],
    /// each copy kept before a schema step named after it.
    pub const fn ledger_file(self) -> &'static str {
        match self {
            Names::Mindweaver => LEDGER_FILE,
            Names::Mindflayer => FLAYER_LEDGER_FILE,
        }
    }
}

impl fmt::Display for Names {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.product())
    }
}

#[cfg(test)]
#[path = "names_test.rs"]
mod tests;
