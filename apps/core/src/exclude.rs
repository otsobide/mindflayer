//! Keeping paths out of git without touching the team's `.gitignore`.
//!
//! A project in local mode keeps what Mindflayer installs into it out of git
//! (D-02), and the file that can say so without being committed is
//! `$GIT_COMMON_DIR/info/exclude`: git reads it like a `.gitignore` but never
//! tracks it, and every linked worktree of the repository reads the same one.
//! Mindflayer writes there only between `# mindflayer:begin` and
//! `# mindflayer:end`, one group per project:
//!
//! ```text
//! # mindflayer:begin
//! # project packages/app
//! /packages/app/.agents/skills/commit-style/
//! # project packages/web
//! /packages/web/.agents/skills/review/
//! # mindflayer:end
//! ```
//!
//! Several projects can share one repository, so each owns a group under its
//! own header and an edit for one never rewrites another's lines. Everything
//! outside the block is somebody else's and comes back byte for byte.
//!
//! Not `.gitignore`, which is the team's: it is committed, so a line there
//! puts copies one machine has into every teammate's diff. Not
//! `core.excludesFile` either, which is the user's and spans every
//! repository they have. Mindflayer edits neither, ever.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::paths;
use crate::workspace::replace_bytes;

/// The line that opens Mindflayer's block.
pub const BEGIN: &str = "# mindflayer:begin";
/// The line that closes it.
pub const END: &str = "# mindflayer:end";
/// What opens one project's group inside the block, before the project's key.
pub const GROUP: &str = "# project ";

const LF: &[u8] = b"\n";
const CRLF: &[u8] = b"\r\n";

/// Keep `entries` of the project at `project` out of git, in the group
/// headed `key`.
///
/// `key` is whatever names the project for good: its id, or until projects
/// have one, its route. It is only compared, never parsed.
pub fn add(project: &Path, key: &str, entries: &[Entry]) -> Result<Excluded, ExcludeError> {
    change(project, key, entries, ExcludeFile::add)
}

/// Stop keeping `entries` of the project at `project` out of git, dropping
/// its group when that empties it.
pub fn remove(project: &Path, key: &str, entries: &[Entry]) -> Result<Excluded, ExcludeError> {
    change(project, key, entries, ExcludeFile::remove)
}

/// What [`add`] or [`remove`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Excluded {
    /// The exclude file was rewritten.
    Written { file: PathBuf },
    /// It already said so, and was left alone.
    Unchanged { file: PathBuf },
    /// No git work tree holds the project, so nothing was written.
    ///
    /// Not a failure, since a project need not be in git, but not silent
    /// either: what local mode installs is then kept out of nothing, and a
    /// repository made there later would offer all of it to be committed.
    /// [`Excluded::warning`] says so.
    Outside { project: PathBuf },
}

impl Excluded {
    /// What a front end should warn about, if anything.
    pub fn warning(&self) -> Option<String> {
        match self {
            Excluded::Outside { project } => Some(format!(
                "{}: not in a git work tree, so nothing keeps what is installed there out of git",
                project.display()
            )),
            Excluded::Written { .. } | Excluded::Unchanged { .. } => None,
        }
    }
}

/// Where a project's entries are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// `info/exclude` in the repository's common dir, which need not exist
    /// yet.
    pub file: PathBuf,
    /// The project's directory relative to the work tree root: `.` at the
    /// root.
    pub route: PathBuf,
}

/// The exclude file git reads for the work tree `project` is in, and where
/// in that work tree it is. `None` when no git work tree holds it.
///
/// The common dir, not the git dir: a linked worktree has a git dir of its
/// own under the main repository's `.git/worktrees/`, but git reads
/// `info/exclude` only from the common dir, so every worktree shares it.
///
/// The repository is opened isolated, reading neither the environment nor
/// the user's git config. Core never reads the environment, and `GIT_DIR`
/// pointing somewhere else must not send one project's entries into another
/// repository's file.
pub fn locate(project: &Path) -> Result<Option<Location>, ExcludeError> {
    let repository =
        match gix::discover_opts(project, Default::default(), gix::open::Options::isolated()) {
            Ok(repository) => repository,
            Err(gix::discover::Error::Discover(
                gix::discover::upwards::Error::NoGitRepository { .. }
                | gix::discover::upwards::Error::NoGitRepositoryWithinCeiling { .. }
                | gix::discover::upwards::Error::NoGitRepositoryWithinFs { .. },
            )) => return Ok(None),
            Err(error) => {
                return Err(ExcludeError::Repository {
                    project: project.to_path_buf(),
                    detail: error.to_string(),
                })
            }
        };
    // A bare repository has no work tree, so nothing in it is untracked.
    let Some(worktree) = repository.workdir() else {
        return Ok(None);
    };
    let route = route(worktree, project).ok_or_else(|| ExcludeError::Route {
        project: project.to_path_buf(),
        worktree: worktree.to_path_buf(),
    })?;
    // A linked worktree names its common dir from its own git dir, as
    // `../..`, and that is how gix hands it back. Normalized, so what a
    // front end reports is the file a person would go and open.
    Ok(Some(Location {
        file: paths::normalize(&repository.common_dir().join("info").join("exclude")),
        route,
    }))
}

/// Where `project` sits below `worktree`, or `None` when it is not below it.
///
/// Lexically first, as everywhere else. Discovery may hand back the work tree
/// spelled the way the filesystem resolves it (`/private/tmp` for `/tmp` on a
/// Mac), and then only resolving both says where the project is.
fn route(worktree: &Path, project: &Path) -> Option<PathBuf> {
    let below = |route: &Path| {
        route
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    };
    let project = std::path::absolute(project).ok()?;
    paths::relative_to(&project, worktree)
        .filter(|route| below(route))
        .or_else(|| {
            let worktree = fs::canonicalize(worktree).ok()?;
            let project = fs::canonicalize(&project).ok()?;
            let route = project.strip_prefix(&worktree).ok()?.to_path_buf();
            Some(if route.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                route
            })
        })
}

/// One edit of the block, read, made and written back whole.
///
/// Nothing is written unless the edit changes something, and nothing at all
/// when the block cannot be read: a failure here is collected by the caller,
/// never a reason to leave the file half rewritten.
fn change(
    project: &Path,
    key: &str,
    entries: &[Entry],
    edit: fn(&mut ExcludeFile, &str, &[String]) -> Result<bool, ExcludeError>,
) -> Result<Excluded, ExcludeError> {
    // What the caller asked for is checked before looking for a repository,
    // so a bad key or entry is refused the same way inside git or out.
    check(key, &[])?;
    for entry in entries {
        entry.pattern(Path::new(""))?;
    }
    let Some(location) = locate(project)? else {
        return Ok(Excluded::Outside {
            project: project.to_path_buf(),
        });
    };
    let patterns = entries
        .iter()
        .map(|entry| entry.pattern(&location.route))
        .collect::<Result<Vec<_>, _>>()?;

    let bytes = match fs::read(&location.file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(source) => {
            return Err(ExcludeError::Read {
                path: location.file,
                source,
            })
        }
    };
    let mut file = ExcludeFile::parse(&bytes).map_err(|problem| ExcludeError::Malformed {
        file: location.file.clone(),
        problem,
    })?;
    if !edit(&mut file, key, &patterns)? {
        return Ok(Excluded::Unchanged {
            file: location.file,
        });
    }

    if let Some(info) = location.file.parent() {
        fs::create_dir_all(info).map_err(|source| ExcludeError::Write {
            path: info.to_path_buf(),
            source,
        })?;
    }
    replace_bytes(&location.file, &file.to_bytes())
        .map_err(|(path, source)| ExcludeError::Write { path, source })?;
    Ok(Excluded::Written {
        file: location.file,
    })
}

/// One path to keep out of git, relative to the project it is in.
///
/// Which paths those are is the caller's to decide (an installed skill's
/// folders, an `AGENTS.md` Mindflayer created); this only says how a path is
/// written down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// One file.
    File(PathBuf),
    /// A directory and everything under it.
    Directory(PathBuf),
}

impl Entry {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Entry::File(path.into())
    }

    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Entry::Directory(path.into())
    }

    /// The path, relative to the project.
    pub fn path(&self) -> &Path {
        match self {
            Entry::File(path) | Entry::Directory(path) => path,
        }
    }

    /// The exclude line for this entry in a project at `route` below the work
    /// tree root (empty for a project at the root).
    ///
    /// Anchored with a leading `/`: entries in the exclude file are relative
    /// to the work tree, and an unanchored `.agents/skills/review/` would keep
    /// every folder of that name out of git, a teammate's in another project
    /// included. A directory ends in `/`, so a file that happens to take its
    /// name is not caught. Each segment is written literally, its glob
    /// characters escaped, so a route like `apps/[legacy]` names that folder
    /// rather than a class of letters.
    pub fn pattern(&self, route: &Path) -> Result<String, ExcludeError> {
        let mut line = String::new();
        for (path, must_name) in [(route, false), (self.path(), true)] {
            let refused = || ExcludeError::Path {
                path: path.to_path_buf(),
            };
            let mut named = false;
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::Normal(segment) => {
                        let segment = segment.to_str().ok_or_else(refused)?;
                        if segment.chars().any(char::is_control) {
                            return Err(refused());
                        }
                        line.push('/');
                        line.push_str(&escape(segment));
                        named = true;
                    }
                    // Climbing out or starting over would aim the line at
                    // something outside the project, which is not this
                    // module's to hide.
                    Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                        return Err(refused())
                    }
                }
            }
            // An empty route is a project at the root. An empty entry would be
            // the whole project, or the whole work tree.
            if must_name && !named {
                return Err(refused());
            }
        }
        match self {
            Entry::Directory(_) => line.push('/'),
            // Git drops trailing spaces from a pattern unless each is
            // escaped, and a file name may end in one.
            Entry::File(_) => {
                let kept = line.trim_end_matches(' ').len();
                let spaces = line.len() - kept;
                line.truncate(kept);
                line.push_str(&"\\ ".repeat(spaces));
            }
        }
        Ok(line)
    }
}

/// A path segment with git's glob characters escaped, so it matches only
/// itself.
fn escape(segment: &str) -> String {
    let mut escaped = String::with_capacity(segment.len());
    for character in segment.chars() {
        if matches!(character, '*' | '?' | '[' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// An exclude file's text, split around Mindflayer's block.
///
/// Kept as lines with their own endings rather than as one string to search
/// and splice: what is outside the block is re-emitted exactly as read, CRLF,
/// a missing final newline and bytes that are not UTF-8 included, and a new
/// line is written with whichever ending the file already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeFile {
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    text: Vec<u8>,
    /// `\n`, `\r\n`, or nothing for a last line without one.
    end: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    begin: Line,
    /// Lines between the begin marker and the first group. Mindflayer never
    /// writes any, so they are somebody's, and they are kept.
    loose: Vec<Line>,
    groups: Vec<Group>,
    end: Line,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    key: Vec<u8>,
    header: Line,
    lines: Vec<Line>,
}

impl ExcludeFile {
    /// Read an exclude file's bytes. An absent file is an empty one.
    ///
    /// A block that is not exactly one well-formed block is refused rather
    /// than repaired: guessing where a missing end marker belongs could
    /// swallow somebody's patterns into Mindflayer's block, and the next edit
    /// would rewrite them.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let lines = split(bytes);
        let ending = lines
            .iter()
            .map(|line| line.end)
            .find(|end| !end.is_empty())
            .unwrap_or(LF);

        let mut begin = None;
        let mut end = None;
        for (index, line) in lines.iter().enumerate() {
            let number = index + 1;
            if is(line, BEGIN) {
                if begin.is_some() {
                    return Err(Malformed::Reopened { line: number });
                }
                begin = Some(index);
            } else if is(line, END) {
                if begin.is_none() || end.is_some() {
                    return Err(Malformed::Unopened { line: number });
                }
                end = Some(index);
            }
        }

        let (begin, end) = match (begin, end) {
            (None, _) => {
                return Ok(ExcludeFile {
                    before: lines,
                    block: None,
                    after: Vec::new(),
                    ending,
                })
            }
            (Some(begin), None) => return Err(Malformed::Unclosed { line: begin + 1 }),
            (Some(begin), Some(end)) => (begin, end),
        };

        let mut lines = lines.into_iter();
        let before: Vec<Line> = lines.by_ref().take(begin).collect();
        let begin_line = lines.next().expect("the begin marker was found");
        let mut loose = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        for (offset, line) in lines.by_ref().take(end - begin - 1).enumerate() {
            if let Some(key) = key_of(&line) {
                if groups.iter().any(|group| group.key == key) {
                    return Err(Malformed::Regrouped {
                        line: begin + offset + 2,
                        key: String::from_utf8_lossy(key).into_owned(),
                    });
                }
                groups.push(Group {
                    key: key.to_vec(),
                    header: line,
                    lines: Vec::new(),
                });
            } else if let Some(group) = groups.last_mut() {
                group.lines.push(line);
            } else {
                loose.push(line);
            }
        }
        let end_line = lines.next().expect("the end marker was found");
        Ok(ExcludeFile {
            before,
            block: Some(Block {
                begin: begin_line,
                loose,
                groups,
                end: end_line,
            }),
            after: lines.collect(),
            ending,
        })
    }

    /// Put `patterns` in `key`'s group, after what it already holds, creating
    /// the group at the end of the block and the block at the end of the file
    /// when there is none. Whether anything changed.
    pub fn add(&mut self, key: &str, patterns: &[String]) -> Result<bool, ExcludeError> {
        check(key, patterns)?;
        let ending = self.ending;
        let mut fresh: Vec<&String> = Vec::new();
        let held = self.block.as_ref().and_then(|block| {
            block
                .groups
                .iter()
                .find(|group| group.key == key.as_bytes())
        });
        for pattern in patterns {
            let present = held.is_some_and(|group| {
                group
                    .lines
                    .iter()
                    .any(|line| line.text == pattern.as_bytes())
            });
            if !present && !fresh.contains(&pattern) {
                fresh.push(pattern);
            }
        }
        if fresh.is_empty() {
            return Ok(false);
        }

        if self.block.is_none() {
            // The new block goes on lines of its own, so a last line without
            // a newline gets one. Its text is untouched; only the ending it
            // lacked is added, and it stays when the block goes.
            if let Some(last) = self.before.last_mut() {
                if last.end.is_empty() {
                    last.end = ending;
                }
            }
            self.block = Some(Block {
                begin: Line::new(BEGIN, ending),
                loose: Vec::new(),
                groups: Vec::new(),
                end: Line::new(END, ending),
            });
        }
        let block = self.block.as_mut().expect("there is a block now");
        let group = match block
            .groups
            .iter()
            .position(|group| group.key == key.as_bytes())
        {
            Some(index) => &mut block.groups[index],
            None => {
                block.groups.push(Group {
                    key: key.as_bytes().to_vec(),
                    header: Line::new(&format!("{GROUP}{key}"), ending),
                    lines: Vec::new(),
                });
                block.groups.last_mut().expect("a group was just pushed")
            }
        };
        group
            .lines
            .extend(fresh.into_iter().map(|pattern| Line::new(pattern, ending)));
        Ok(true)
    }

    /// Take `patterns` out of `key`'s group, dropping the group when nothing
    /// is left in it and the block when no group is. Whether anything
    /// changed.
    pub fn remove(&mut self, key: &str, patterns: &[String]) -> Result<bool, ExcludeError> {
        check(key, patterns)?;
        let Some(block) = self.block.as_mut() else {
            return Ok(false);
        };
        let Some(index) = block
            .groups
            .iter()
            .position(|group| group.key == key.as_bytes())
        else {
            return Ok(false);
        };
        let group = &mut block.groups[index];
        let held = group.lines.len();
        group.lines.retain(|line| {
            !patterns
                .iter()
                .any(|pattern| line.text == pattern.as_bytes())
        });
        if group.lines.len() == held {
            return Ok(false);
        }

        if group.lines.iter().all(Line::is_blank) {
            block.groups.remove(index);
        }
        if block.groups.is_empty() && block.loose.iter().all(Line::is_blank) {
            self.block = None;
            let after = std::mem::take(&mut self.after);
            self.before.extend(after);
        }
        Ok(true)
    }

    /// The file as it is to be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut emit = |line: &Line| {
            out.extend_from_slice(&line.text);
            out.extend_from_slice(line.end);
        };
        self.before.iter().for_each(&mut emit);
        if let Some(block) = &self.block {
            emit(&block.begin);
            block.loose.iter().for_each(&mut emit);
            for group in &block.groups {
                emit(&group.header);
                group.lines.iter().for_each(&mut emit);
            }
            emit(&block.end);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

impl Line {
    fn new(text: &str, end: &'static [u8]) -> Self {
        Line {
            text: text.as_bytes().to_vec(),
            end,
        }
    }

    fn is_blank(&self) -> bool {
        self.text.trim_ascii().is_empty()
    }
}

/// The file's lines, each with the ending it had.
fn split(bytes: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match rest.iter().position(|&byte| byte == b'\n') {
            Some(at) => {
                let (text, end) = match rest[..at].strip_suffix(b"\r") {
                    Some(text) => (text, CRLF),
                    None => (&rest[..at], LF),
                };
                lines.push(Line {
                    text: text.to_vec(),
                    end,
                });
                rest = &rest[at + 1..];
            }
            None => {
                lines.push(Line {
                    text: rest.to_vec(),
                    end: b"",
                });
                break;
            }
        }
    }
    lines
}

/// Whether a line is the marker `marker`. Trailing whitespace is forgiven,
/// as git forgives it in a pattern; leading whitespace is not, because git
/// reads `  # mindflayer:begin` as a pattern rather than a comment.
fn is(line: &Line, marker: &str) -> bool {
    line.text.trim_ascii_end() == marker.as_bytes()
}

/// The key a group header names, when the line is one.
fn key_of(line: &Line) -> Option<&[u8]> {
    line.text
        .strip_prefix(GROUP.as_bytes())
        .map(<[u8]>::trim_ascii_end)
        .filter(|key| !key.is_empty())
}

/// Refuse a key or a pattern that cannot be written as one line of the
/// block. A line break in either would put a line of its choosing into the
/// file, and a key with space around it would not read back as itself.
fn check(key: &str, patterns: &[String]) -> Result<(), ExcludeError> {
    if key.is_empty() || key.trim() != key || key.chars().any(char::is_control) {
        return Err(ExcludeError::Key {
            key: key.to_owned(),
        });
    }
    if let Some(pattern) = patterns
        .iter()
        .find(|pattern| pattern.is_empty() || pattern.chars().any(char::is_control))
    {
        return Err(ExcludeError::Pattern {
            pattern: pattern.clone(),
        });
    }
    Ok(())
}

/// Why the block in an exclude file is not one Mindflayer can edit. Lines are
/// counted from 1.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Malformed {
    #[error("line {line}: `# mindflayer:begin` is never closed by `# mindflayer:end`")]
    Unclosed { line: usize },
    #[error("line {line}: `# mindflayer:end` closes no `# mindflayer:begin`")]
    Unopened { line: usize },
    #[error("line {line}: a second `# mindflayer:begin`, where there can be one block")]
    Reopened { line: usize },
    #[error("line {line}: a second group for `{key}` in the block")]
    Regrouped { line: usize, key: String },
}

/// Why a project's paths could not be kept out of git.
#[derive(Debug, Error)]
pub enum ExcludeError {
    #[error("{key:?} cannot name a group in the exclude file: a key is one line with no space around it")]
    Key { key: String },
    #[error("{pattern:?} cannot be written as one line of the exclude file")]
    Pattern { pattern: String },
    #[error("{path}: no line of the exclude file can name it, so it cannot be kept out of git")]
    Path { path: PathBuf },
    #[error("{project}: the git repository it is in cannot be opened: {detail}")]
    Repository { project: PathBuf, detail: String },
    #[error("{project}: not below {worktree}, the work tree of the repository it is in")]
    Route { project: PathBuf, worktree: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{file}: {problem}, so nothing was written")]
    Malformed { file: PathBuf, problem: Malformed },
}

#[cfg(test)]
#[path = "exclude_test.rs"]
mod tests;
