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
use crate::claude_rules::{self, Mode};
use crate::exclude;
use crate::lines::{self, Line};
use crate::settings::Share;
use crate::workspace::MindProject;

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

/// The project's own files an import may go in, in the order one is chosen,
/// each with the line that brings in the `AGENTS.md` at the project's root
/// from where the file is.
///
/// `CLAUDE.md` first, the one the probe verified the import in. From
/// `.claude`, `@../AGENTS.md`: Claude takes an import's path from the file
/// it is in, not from where it starts (docs/agents.md, docs-only). Never
/// `CLAUDE.local.md`, which is a person's own and kept to themselves, so an
/// import there would reach nobody else, and would leave the project with
/// rules that live in a file no teammate has.
const OWN: [(&[&str], &str); 2] = [
    (&[CLAUDE_MD], "@AGENTS.md"),
    (&[".claude", CLAUDE_MD], "@../AGENTS.md"),
];

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
            let path = joined(folder, name);
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

/// How Claude Code reaches the block of rules in a project's `AGENTS.md`,
/// as [`reach`] decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Claude reads `AGENTS.md` itself: nothing in the project or above it
    /// keeps it from doing so ([`reads_agents_md`]).
    Native,
    /// Claude reads the block as one of its own files, the project's
    /// `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` here, which is
    /// its `AGENTS.md`, one file through a link, as some teams keep the two.
    /// The block is written through the link ([`crate::agents_md::render`]),
    /// so no import is: one would bring the file into itself, between the
    /// markers the block already holds.
    Same(PathBuf),
    /// Claude reads the block through the import in this file, the
    /// project's own `CLAUDE.md` or `.claude/CLAUDE.md`, there already or to
    /// be added.
    Import(PathBuf),
    /// Claude reads no block, and is to have every rule from its own rules
    /// folder ([`Mode::Fallback`]), for this reason.
    Fallback(Fallback),
}

impl Reach {
    /// Which rules Claude's own rules folder is to hold
    /// ([`crate::claude_rules::render`]): those about some files only while
    /// Claude reads the block, and every rule while it does not, so Claude
    /// reads each rule once and misses none.
    pub fn mode(&self) -> Mode {
        match self {
            Reach::Native | Reach::Same(_) | Reach::Import(_) => Mode::Scoped,
            Reach::Fallback(_) => Mode::Fallback,
        }
    }
}

/// Why Claude reads no block of rules in a project, and gets them from its
/// own rules folder instead: said, and no failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Fallback {
    /// No block was written into `AGENTS.md`: no rule about every file was
    /// asked for, or the block was not rendered.
    #[error("no block of rules in AGENTS.md for Claude Code to read")]
    NoBlock,
    /// What keeps Claude from reading `AGENTS.md` in the project is its
    /// `CLAUDE.local.md`, and no `CLAUDE.md` of its own is there to import
    /// it from.
    #[error(
        "{0}: Claude Code reads it in place of AGENTS.md, and it is a person's own, so no \
         import is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    LocalOnly(PathBuf),
    /// What keeps Claude from reading `AGENTS.md` is above the project, the
    /// nearest such file given, and the project has no `CLAUDE.md` of its own
    /// to import it from. Never edited: it is not the project's, and Claude
    /// reads it in every project below.
    #[error(
        "{0}: above the project, where Claude Code reads it in place of AGENTS.md, and \
         Mindflayer edits nothing above a project; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Above(PathBuf),
    /// The project's own `CLAUDE.md`, which git tracks in a local project
    /// that does not set `edit_tracked` (D-02).
    #[error(
        "{0}: git tracks it, and the project is local without edit_tracked, so no import of \
         AGENTS.md is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Tracked(PathBuf),
    /// The project's own `CLAUDE.md`, reached through a link to `target`.
    #[error(
        "{file}: a link to {target}, so no import of AGENTS.md is added through it; Claude \
         Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Linked { file: PathBuf, target: PathBuf },
    /// The project's own `CLAUDE.md`, of which whether git tracks it cannot
    /// be told.
    #[error(
        "{file}: whether git tracks it cannot be told ({detail}), so no import of AGENTS.md \
         is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Untold { file: PathBuf, detail: String },
}

/// How Claude Code, started at `project`'s root, is to reach the block of
/// rules in its `AGENTS.md`, given whether the block is there
/// (`block_written`, what [`crate::agents_md::render`] said).
///
/// - [`Reach::Native`] when Claude reads `AGENTS.md` itself
///   ([`reads_agents_md`], with `homes` naming the user memory file it
///   leaves out).
/// - [`Reach::Same`] when one of Claude's own files in the project is that
///   `AGENTS.md`, through a link.
/// - [`Reach::Import`] when the project's own `CLAUDE.md` or
///   `.claude/CLAUDE.md` is there and may be edited: in a shared project,
///   in one a `mind.toml` marks, which was committed by design (D-02), and
///   in a local one while git does not track the file or its settings say
///   `edit_tracked`. Chosen in the order of [`OWN`], but one already
///   holding the import keeps it, so the import never moves from file to
///   file, and is kept even where it may no longer be edited, since Claude
///   reads it there all the same.
/// - [`Reach::Fallback`] otherwise: no block, only a `CLAUDE.local.md`, a
///   `CLAUDE.md` only above the project, a project's own one git tracks
///   without `edit_tracked`, or one reached through a link, which is never
///   followed to write, as its import would then mean whatever the link
///   leads to.
///
/// Reads, and writes nothing. A `CLAUDE.md` of the project's own that holds
/// a malformed block is refused, as writing the import into it would be,
/// since what Claude reads there cannot be told.
pub fn reach(
    project: &MindProject,
    homes: &AgentHomes,
    block_written: bool,
) -> Result<Reach, ClaudeMdError> {
    if !block_written {
        return Ok(Reach::Fallback(Fallback::NoBlock));
    }
    let root = project.root();
    let stopping = stopping(root, homes)?;
    let Some(nearest) = stopping.first() else {
        return Ok(Reach::Native);
    };
    let real = resolved(root)?;
    // The project's file at `name` while it is one that keeps Claude from
    // reading `AGENTS.md`, as the project spells it: never the user memory
    // file, which a project at the user's home holds as `.claude/CLAUDE.md`.
    let own = |name: &[&str]| {
        stopping
            .contains(&joined(&real, name))
            .then(|| joined(root, name))
    };

    let agents = fs::canonicalize(root.join(agents_md::FILE)).ok();
    for name in NAMES {
        if let Some(file) = own(name) {
            if agents.is_some() && fs::canonicalize(&file).ok() == agents {
                return Ok(Reach::Same(file));
            }
        }
    }

    let mut chosen = None;
    let mut refused = None;
    for (name, import) in OWN {
        let Some(file) = own(name) else {
            continue;
        };
        let edit = edit(project, &real, &file, name)?;
        // A link's import would mean whatever the link leads to, so only a
        // file in its place is asked whether it imports.
        let looked = !matches!(edit, Edit::Refused(Fallback::Linked { .. }));
        if looked && read(&file)?.imports(import) {
            return Ok(Reach::Import(file));
        }
        match edit {
            Edit::Allowed => {
                chosen.get_or_insert(file);
            }
            Edit::Refused(reason) => {
                refused.get_or_insert(reason);
            }
        }
    }
    if let Some(file) = chosen {
        return Ok(Reach::Import(file));
    }
    if let Some(reason) = refused {
        return Ok(Reach::Fallback(reason));
    }
    // No file of the project's own: what is in the way is a person's own
    // `CLAUDE.local.md`, or something above the project.
    let local = joined(&real, &[CLAUDE_LOCAL_MD]);
    Ok(Reach::Fallback(if *nearest == local {
        Fallback::LocalOnly(root.join(CLAUDE_LOCAL_MD))
    } else {
        Fallback::Above(nearest.clone())
    }))
}

/// Whether the project's own `file`, at `name` from its root, may be edited
/// to hold the import, the root being `real` resolved.
enum Edit {
    Allowed,
    Refused(Fallback),
}

fn edit(
    project: &MindProject,
    real: &Path,
    file: &Path,
    name: &[&str],
) -> Result<Edit, ClaudeMdError> {
    let target = resolved(file)?;
    if target != joined(real, name) {
        return Ok(Edit::Refused(Fallback::Linked {
            file: file.to_path_buf(),
            target,
        }));
    }
    // A project a `mind.toml` marks has no settings, and was committed by
    // design, as a shared one is.
    let Some(settings) = project.settings() else {
        return Ok(Edit::Allowed);
    };
    if settings.share == Share::Shared || settings.edit_tracked {
        return Ok(Edit::Allowed);
    }
    let route: PathBuf = name.iter().collect();
    Ok(match exclude::tracks_file(project.root(), &route) {
        Ok(false) => Edit::Allowed,
        Ok(true) => Edit::Refused(Fallback::Tracked(file.to_path_buf())),
        Err(error) => Edit::Refused(Fallback::Untold {
            file: file.to_path_buf(),
            detail: error.to_string(),
        }),
    })
}

/// `folder` with the route `name` below it.
fn joined(folder: &Path, name: &[&str]) -> PathBuf {
    let mut path = folder.to_path_buf();
    path.extend(name);
    path
}

fn resolved(path: &Path) -> Result<PathBuf, ClaudeMdError> {
    fs::canonicalize(path).map_err(|source| ClaudeMdError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// The `CLAUDE.md` at `file`, parsed.
fn read(file: &Path) -> Result<ClaudeMd, ClaudeMdError> {
    let bytes = fs::read(file).map_err(|source| ClaudeMdError::Read {
        path: file.to_path_buf(),
        source,
    })?;
    ClaudeMd::parse(&bytes).map_err(|problem| ClaudeMdError::Malformed {
        file: file.to_path_buf(),
        problem,
    })
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

/// Why how Claude reads a project's `AGENTS.md` could not be told, or the
/// import could not be written.
#[derive(Debug, Error)]
pub enum ClaudeMdError {
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{file}: {problem}, so nothing was written")]
    Malformed { file: PathBuf, problem: Malformed },
}

#[cfg(test)]
#[path = "claude_md_test.rs"]
mod tests;
