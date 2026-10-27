//! Whether Claude Code reads a project's `AGENTS.md`.
//!
//! The block of rules in `AGENTS.md` ([`crate::agents_md`]) reaches Codex
//! and OpenCode wherever it is, but Claude reads that file natively only
//! when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` is in the
//! folder it starts in or any folder above it (docs/agents.md, verified on
//! 2.1.309 but for `.claude/CLAUDE.md`, seen only among the names it looks
//! for). One predicate decides it, [`reads_agents_md`], so whatever gives
//! Claude its rules asks the same question and never reads the block as
//! reaching Claude where it does not (D-03).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::AgentHomes;

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
