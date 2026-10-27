//! Whether Claude Code reads a project's `AGENTS.md`, and the import that
//! makes it read the file where a `CLAUDE.md` is in the way.
//!
//! The block of rules in `AGENTS.md` ([`crate::agents_md`]) reaches Codex
//! and OpenCode wherever it is, but Claude reads that file natively only
//! when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` is in the
//! folder it starts in or any folder above it (docs/agents.md, verified on
//! 2.1.309 but for `.claude/CLAUDE.md`, seen only among the names it looks
//! for). One predicate decides it, [`reads_agents_md`], so whatever gives
//! Claude its rules asks the same question and never reads the block as
//! reaching Claude where it does not (D-03).
//!
//! Where the project's own `CLAUDE.md` is what is in the way, one line in
//! it brings `AGENTS.md` in, between the markers the block of `AGENTS.md`
//! has, at the top of a file that is otherwise somebody else's:
//!
//! ```text
//! <!-- mindflayer:begin -->
//! @AGENTS.md
//! <!-- mindflayer:end -->
//!
//! # Working here
//! ```
//!
//! The layout the probe verified, with the file loaded once (docs/agents.md).
//! Everything outside the block comes back byte for byte, but for the empty
//! line just below the end marker, which is the block's ([`ClaudeMd`]), and a
//! block that is not exactly one well-formed one is refused, never repaired,
//! as in `AGENTS.md`: the markers are read by the same search
//! (`agents_md::locate`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::AgentHomes;
use crate::agents_md::{self, Malformed, BEGIN, END};
use crate::lines::{self, Line};

/// The file Claude reads as a folder's instructions, and its user memory
/// file's name in its own folder.
pub const CLAUDE_MD: &str = "CLAUDE.md";
/// The one a person keeps to themselves, beside `CLAUDE.md`. Mindflayer
/// never makes one: a `CLAUDE.local.md` alone stops Claude reading
/// `AGENTS.md`, without a word to anybody (D-03).
pub const CLAUDE_LOCAL_MD: &str = "CLAUDE.local.md";

/// Each name that keeps Claude from reading `AGENTS.md` when it is in a
/// folder Claude looks in, as a route of folder names from that folder, in
/// the order Claude's check lists them.
const NAMES: [&[&str]; 3] = [&[CLAUDE_MD], &[".claude", CLAUDE_MD], &[CLAUDE_LOCAL_MD]];

/// Whether Claude Code, started in `folder`, reads the `AGENTS.md` there:
/// whether no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` is in
/// `folder` or in a folder above it ([`searched`]).
///
/// The user memory file, `CLAUDE.md` in Claude's own folder, which `homes`
/// names, never counts, even where the walk up from a project under the
/// user's home finds it as that home's `.claude/CLAUDE.md`: Claude reads it
/// for every project and goes on reading `AGENTS.md` beside it (verified).
/// Told by the file it is, both resolved, so a spelling through a link is
/// still the memory file. No folder in `homes` leaves nothing out, as a
/// test hands over, which reads such a file as one more `CLAUDE.md`.
///
/// Folders above the git root count too (verified: a `CLAUDE.md` in the
/// folder a person keeps their repositories in stops the read), and so does
/// anything at one of those names, a link followed to whatever it names; a
/// link to nothing is nothing there.
pub fn reads_agents_md(folder: &Path, homes: &AgentHomes) -> Result<bool, ClaudeMdError> {
    Ok(stopping(folder, homes)?.is_empty())
}

/// Every file that keeps Claude from reading `AGENTS.md` when it starts in
/// `folder`, the nearest folder's first, each folder's in the order of
/// [`NAMES`]: spelled from `folder` resolved, as Claude, started there, has
/// it from the operating system.
fn stopping(folder: &Path, homes: &AgentHomes) -> Result<Vec<PathBuf>, ClaudeMdError> {
    let start = fs::canonicalize(folder).map_err(|source| ClaudeMdError::Read {
        path: folder.to_path_buf(),
        source,
    })?;
    let memory = homes
        .claude_config_dir
        .as_deref()
        .and_then(|folder| fs::canonicalize(folder.join(CLAUDE_MD)).ok());
    let mut found = Vec::new();
    for folder in searched(&start) {
        for name in NAMES {
            let mut path = folder.to_path_buf();
            path.extend(name);
            match fs::metadata(&path) {
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    continue
                }
                Err(source) => return Err(ClaudeMdError::Read { path, source }),
            }
            if memory.is_some() && fs::canonicalize(&path).ok() == memory {
                continue;
            }
            found.push(path);
        }
    }
    Ok(found)
}

/// The folders Claude looks in, started in `start`: `start` and every folder
/// above it, nearest first, but the filesystem's root.
///
/// Not the root, because Claude's own check leaves it out: its debug log
/// counted four folders searched from a start four below `/`, and five from
/// one five below, each one short of the folders up to and including `/`
/// (docs/agents.md). Counting `/` would have this say Claude reads no
/// `AGENTS.md` where it does, with a `CLAUDE.md` at `/` and none nearer, and
/// give Claude every rule about every file twice, in the block and in its
/// rules folder. Leaving it out errs the other way only if Claude does read
/// that far after all, and a `CLAUDE.md` at `/` is one only the machine's
/// administrator can put there.
fn searched(start: &Path) -> impl Iterator<Item = &Path> {
    start
        .ancestors()
        .take_while(|folder| folder.parent().is_some())
}

/// What a file Windows wrote may open with: kept ahead of the block, which
/// goes at the top, where it would otherwise put the mark in the middle of
/// the file and leave its begin marker one that never reads back as itself.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A `CLAUDE.md`'s text, split around Mindflayer's import.
///
/// Kept as lines with their own endings ([`crate::lines`]), as `AGENTS.md`
/// is, so what is outside the block is written back exactly as read, and
/// the block takes whichever ending the file already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeMd {
    /// Whether the file opens with a byte order mark, kept ahead of all.
    bom: bool,
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    begin: Line,
    /// Every line between the markers, as read or as written.
    lines: Vec<Line>,
    end: Line,
    /// The empty line just below the end marker, when there is one.
    ///
    /// A block added to a file with anything in it is set apart from what
    /// follows by an empty line, the layout the probe verified. Taken for
    /// the block's when read, as the one above `AGENTS.md`'s block is, since
    /// nothing tells it from one written by hand, so dropping the block gives
    /// back the file as it was before the block was added.
    spacer: Option<Line>,
}

impl ClaudeMd {
    /// Read a `CLAUDE.md`'s bytes.
    ///
    /// Refused as `AGENTS.md` is ([`Malformed`]), and also when the block
    /// holds a rule's marker, which Mindflayer writes only in `AGENTS.md`: a
    /// `CLAUDE.md` holding one is a copy of that file, kept beside it, and
    /// replacing its rules with the import would take them from whoever
    /// reads that copy, while importing beside them would give them twice.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let (bom, bytes) = match bytes.strip_prefix(BOM) {
            Some(rest) => (true, rest),
            None => (false, bytes),
        };
        let lines = lines::split(bytes);
        let ending = lines::ending(&lines);
        let Some(span) = agents_md::locate(&lines)? else {
            return Ok(ClaudeMd {
                bom,
                before: lines,
                block: None,
                after: Vec::new(),
                ending,
            });
        };
        if let Some((index, _)) = span.rules.first() {
            return Err(Malformed::Ruled { line: index + 1 });
        }

        let mut lines = lines.into_iter();
        let before: Vec<Line> = lines.by_ref().take(span.begin).collect();
        let begin = lines.next().expect("the begin marker was found");
        let inside: Vec<Line> = lines.by_ref().take(span.end - span.begin - 1).collect();
        let end = lines.next().expect("the end marker was found");
        let mut after: Vec<Line> = lines.collect();
        let spacer = after
            .first()
            .is_some_and(|line| line.text.is_empty())
            .then(|| after.remove(0));
        Ok(ClaudeMd {
            bom,
            before,
            block: Some(Block {
                begin,
                lines: inside,
                end,
                spacer,
            }),
            after,
            ending,
        })
    }

    /// Whether the file holds Mindflayer's block.
    pub fn has_block(&self) -> bool {
        self.block.is_some()
    }

    /// Whether the block holds `import` and nothing else: whether Claude,
    /// reading the file, is brought to `AGENTS.md` by it. Trailing
    /// whitespace is forgiven, as on a marker.
    pub fn imports(&self, import: &str) -> bool {
        self.block.as_ref().is_some_and(|block| {
            matches!(&block.lines[..], [line] if line.text.trim_ascii_end() == import.as_bytes())
        })
    }

    /// Make the block hold `import` alone, and whether that changed
    /// anything.
    ///
    /// Everything between the markers is Mindflayer's, so it is replaced
    /// whole, as in `AGENTS.md`. A file with no block gains one at the top,
    /// on lines of its own and an empty line apart from what follows; one
    /// somebody moved is changed where it is.
    pub fn set_import(&mut self, import: &str) -> bool {
        let ending = self.ending;
        let lines = vec![Line::new(import, ending)];
        if let Some(block) = &mut self.block {
            if block.lines == lines {
                return false;
            }
            block.lines = lines;
            return true;
        }
        // At the top rather than after what the file holds, where the block
        // of `AGENTS.md` goes: the layout the probe verified, and the import
        // stays where it is whatever somebody writes below it.
        self.after = std::mem::take(&mut self.before);
        let spacer = (!self.after.is_empty()).then(|| Line::new("", ending));
        self.block = Some(Block {
            begin: Line::new(BEGIN, ending),
            lines,
            end: Line::new(END, ending),
            spacer,
        });
        true
    }

    /// Take the block out, markers, import and the empty line below them,
    /// and whether there was one.
    pub fn drop_import(&mut self) -> bool {
        let Some(block) = self.block.take() else {
            return false;
        };
        // An end marker without a newline is the file's last line, so the
        // file ended without one and still does once the block is gone.
        if block.end.end.is_empty() {
            if let Some(last) = self.before.last_mut() {
                last.end = b"";
            }
        }
        let after = std::mem::take(&mut self.after);
        self.before.extend(after);
        true
    }

    /// The file as it is to be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.bom {
            out.extend_from_slice(BOM);
        }
        let mut emit = |line: &Line| {
            out.extend_from_slice(&line.text);
            out.extend_from_slice(line.end);
        };
        self.before.iter().for_each(&mut emit);
        if let Some(block) = &self.block {
            emit(&block.begin);
            block.lines.iter().for_each(&mut emit);
            emit(&block.end);
            block.spacer.iter().for_each(&mut emit);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

/// Why whether Claude reads a project's `AGENTS.md` could not be told.
#[derive(Debug, Error)]
pub enum ClaudeMdError {
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[cfg(test)]
#[path = "claude_md_test.rs"]
mod tests;
