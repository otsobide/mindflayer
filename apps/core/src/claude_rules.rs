//! A project's rules, rendered as Claude Code's rule files.
//!
//! Claude reads every markdown file under a project's `.claude/rules`, at any
//! depth, and one whose front matter states `paths` only once it reads a
//! file those patterns match (docs/agents.md, verified on 2.1.309). No other
//! agent reads that folder, and none honours `paths`. So a rule about some
//! files only reaches its project as a file there, and never as a section of
//! the block in `AGENTS.md` ([`crate::agents_md`]), where every agent would
//! read it for every file (D-03):
//!
//! ```text
//! .claude/rules/mindflayer/rust/errors.md
//! ```
//!
//! The same files give Claude the rules about every file when it reads no
//! block, in [`Mode::Fallback`]. Which mode applies is the caller's to say,
//! from whether Claude reads `AGENTS.md` (#44), never from whether a block
//! was written, so Claude reads each rule once and misses none.
//!
//! A file holds the rule as written, front matter and all: Claude takes
//! `paths` from it and strips the rest before the rule reaches the model
//! (docs/agents.md, docs-only), and a file that is the rule byte for byte
//! is one somebody can open and recognise. Below `mindflayer`, a folder of
//! Mindflayer's own beside whatever somebody keeps in `.claude/rules`, a
//! file is filed by the rule's route, folders and all, as in the library,
//! so two rules never share one.
//!
//! Which files are Mindflayer's is told by what the caller hands back of an
//! earlier render ([`Recorded`]): each file's route, and the hash of what it
//! held ([`hash::bytes`]). A file is Mindflayer's while it holds exactly
//! that, and anything else at a rule's place is somebody's, written over
//! and taken away by nothing. Keeping the record is the caller's (#45), as
//! is deciding which rules a project is given and when.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{issue_list, name_issues, Artifact, ArtifactError, Declared};
use crate::catalog::report_order;
use crate::hash;
use crate::workspace::{replace_bytes_unresolved, MindProject};

/// The folders, from a project's root, the files go in.
const FOLDER: [&str; 3] = [".claude", "rules", "mindflayer"];

/// What a rule's file is called after the last word of its route: the
/// extension Claude reads in `.claude/rules`.
const EXTENSION: &str = ".md";

/// Where the files go, relative to a project's root:
/// `.claude/rules/mindflayer`.
///
/// A folder of Mindflayer's own rather than `.claude/rules` itself, where
/// somebody may keep rules by hand: a route can never name one of theirs,
/// and the folder goes once the last file in it does.
pub fn folder() -> PathBuf {
    FOLDER.iter().collect()
}

/// The file the rule at `route` goes to, relative to a project's root.
///
/// Joined a folder at a time, so it is the same path on every platform.
pub fn file_for(route: &str) -> PathBuf {
    let mut path = folder();
    path.extend(route.split('/'));
    let name = route.rsplit('/').next().unwrap_or(route);
    path.set_file_name(format!("{name}{EXTENSION}"));
    path
}

/// Which rules a render writes, as its caller decides from whether Claude
/// reads the block in `AGENTS.md` (#44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Only the rules about some files: Claude reads the others in the block
    /// of `AGENTS.md`, natively or through an import, and a file of theirs
    /// here would have it read them twice.
    Scoped,
    /// Every rule: Claude reads no block, so the rules about every file come
    /// here too, or it would miss them.
    Fallback,
}

/// A rule file an earlier render left Mindflayer's: its route, and the hash
/// of what it held ([`hash::bytes`]).
///
/// What the caller keeps (#45), from [`Rendered::recorded`], and hands back
/// to the next render, which takes a file as Mindflayer's only while it
/// holds what was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub route: String,
    pub hash: String,
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Each rule's file the render wrote or found, and each route recorded,
    /// in the order of their routes.
    pub files: Vec<RuleFile>,
    /// What there is to say, file by file in the same order.
    pub warnings: Vec<Warning>,
}

impl Rendered {
    /// What the caller keeps and hands back to the next render: every file
    /// left Mindflayer's, with what it holds.
    pub fn recorded(&self) -> Vec<Recorded> {
        self.files
            .iter()
            .filter_map(|file| {
                Some(Recorded {
                    route: file.route.clone(),
                    hash: file.hash.clone()?,
                })
            })
            .collect()
    }
}

/// One rule's file, and what a render did to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    /// The rule's route, which names the file.
    pub route: String,
    /// Where the file is, or would be.
    pub path: PathBuf,
    pub written: Written,
    /// The hash of what the file holds while it is Mindflayer's, for the
    /// caller to record; none once the render leaves nothing of Mindflayer's
    /// there.
    pub hash: Option<String>,
}

/// What [`render`] did to one rule's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// It was not there, and now holds the rule: made with `create_new`.
    Created,
    /// It held what was recorded, and was replaced whole by the rule as it
    /// is now.
    Changed,
    /// It already held the rule as it is now, and was left as it was.
    Unchanged,
    /// It held what was recorded, and its rule is no longer asked for, so it
    /// was taken away.
    Removed,
    /// It was recorded, and was not there any more, nor asked for: nothing
    /// is left to record.
    Gone,
    /// It was recorded, and holds something else now: somebody changed it,
    /// so it is left as it is, and its record kept (D-04).
    Edited,
    /// It was not recorded, and something is there: somebody's, left alone,
    /// and the rule is not written.
    Foreign,
    /// It could not be looked at, written or taken away; [`Warning::Failed`]
    /// says why.
    Failed,
}

/// Something a render has to say about a rule's file, which keeps neither
/// it nor any other file from being rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// Somebody's file is where the rule's would go.
    #[error(
        "{path}: not a file Mindflayer wrote, so rule `{route}` is not written there and \
         does not reach Claude Code"
    )]
    Foreign { route: String, path: PathBuf },
    /// A file Mindflayer wrote has been changed since.
    #[error("{path}: changed since Mindflayer wrote rule `{route}` there, so it is left as it is")]
    Edited { route: String, path: PathBuf },
    /// The file could not be looked at, written or taken away.
    #[error("{path}: rule `{route}`: {error}")]
    Failed {
        route: String,
        path: PathBuf,
        error: String,
    },
}

/// Render `rules` into `project`'s `.claude/rules/mindflayer`, each `mode`
/// takes as one file holding the rule as written, and take away each file
/// recorded whose rule is no longer asked for.
///
/// `recorded` is what the caller kept of earlier renders
/// ([`Rendered::recorded`]), and decides what is Mindflayer's: a file that
/// holds what was recorded for its route is replaced when its rule has
/// changed, and taken away when its rule is no longer asked for, with every
/// folder that leaves empty up to and including `mindflayer`. Anything else
/// at a rule's place is somebody's: a file nothing recorded, or one changed
/// since it was written, is left exactly as it is, and said
/// ([`Written::Foreign`], [`Written::Edited`]). A file that is not there is
/// made with `create_new`, so one somebody made meanwhile is never written
/// over; one that is there is replaced through a temporary and a rename,
/// never followed through a link. A render that changes nothing writes
/// nothing.
///
/// A rule that is not a rule, cannot be read or is named no name, two rules
/// of one name, and a route recorded that is no name are refused before
/// anything is written, as the block of `AGENTS.md` refuses them. A file
/// that cannot be looked at, written or taken away is the one rule's
/// failure, collected ([`Warning::Failed`]) while every other file is
/// rendered: each file stands on its own, and the caller has to be told of
/// each one that was written, to record it.
///
/// Takes no lock: the caller holds the workspace's, as one calling
/// [`crate::agents_md::render`] does, and the lock is not re-entrant.
pub fn render<'a>(
    project: &MindProject,
    rules: impl IntoIterator<Item = &'a Artifact>,
    mode: Mode,
    recorded: &[Recorded],
) -> Result<Rendered, ClaudeRulesError> {
    let root = project.root();
    let mut rules: Vec<&Artifact> = rules.into_iter().collect();
    rules.sort_by(|a, b| report_order(a, b));
    let mut asked: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rule in rules {
        let Declared::Rule(manifest) = rule.declared() else {
            return Err(ClaudeRulesError::NotARule {
                name: rule.qualified_name(),
            });
        };
        let taken = match mode {
            Mode::Scoped => manifest.is_scoped(),
            Mode::Fallback => true,
        };
        if !taken {
            continue;
        }
        let route = named(rule.name())?;
        let file = rule.file();
        let bytes = fs::read(&file).map_err(|source| ArtifactError::Read { path: file, source })?;
        if asked.contains_key(&route) {
            return Err(ClaudeRulesError::Twice { route });
        }
        asked.insert(route, bytes);
    }
    // By route, the hash given last for one given twice.
    let mut owned: BTreeMap<String, &str> = BTreeMap::new();
    for record in recorded {
        owned.insert(named(&record.route)?, &record.hash);
    }

    let mut routes: Vec<&String> = asked.keys().chain(owned.keys()).collect();
    routes.sort();
    routes.dedup();
    let plans: Vec<(String, PathBuf, Plan)> = routes
        .into_iter()
        .map(|route| {
            let path = root.join(file_for(route));
            let plan = plan(
                asked.get(route).map(Vec::as_slice),
                owned.get(route).copied(),
                look(&path),
            );
            (route.clone(), path, plan)
        })
        .collect();

    // Every file written before any is taken away, so a rule renamed is
    // never missing from Claude's folder between its old file and its new.
    let top = root.join(folder());
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut removals = Vec::new();
    for (route, path, plan) in plans {
        let recorded = owned.get(&route).map(|hash| (*hash).to_owned());
        let (written, hash) = match plan {
            Plan::Create(bytes) => match create(&path, &bytes) {
                Ok(()) => (Written::Created, Some(hash::bytes(&bytes))),
                Err(error) => {
                    warnings.push(failed(&route, &path, &error));
                    (Written::Failed, None)
                }
            },
            Plan::Replace(bytes) => match replace_bytes_unresolved(&path, &bytes) {
                Ok(()) => (Written::Changed, Some(hash::bytes(&bytes))),
                Err((_, error)) => {
                    warnings.push(failed(&route, &path, &error));
                    (Written::Failed, recorded)
                }
            },
            Plan::Keep(bytes) => (Written::Unchanged, Some(hash::bytes(&bytes))),
            Plan::Edited => {
                warnings.push(Warning::Edited {
                    route: route.clone(),
                    path: path.clone(),
                });
                (Written::Edited, recorded)
            }
            Plan::Foreign => {
                warnings.push(Warning::Foreign {
                    route: route.clone(),
                    path: path.clone(),
                });
                (Written::Foreign, None)
            }
            Plan::Unlooked(error) => {
                warnings.push(failed(&route, &path, &error));
                (Written::Failed, recorded)
            }
            // Said once it is done, below.
            Plan::Remove => {
                removals.push(files.len());
                (Written::Removed, None)
            }
            Plan::Gone => (Written::Gone, None),
        };
        files.push(RuleFile {
            route,
            path,
            written,
            hash,
        });
    }
    for index in removals {
        let file = &mut files[index];
        if let Err(error) = fs::remove_file(&file.path) {
            warnings.push(failed(&file.route, &file.path, &error));
            file.written = Written::Failed;
            file.hash = owned.get(&file.route).map(|hash| (*hash).to_owned());
        }
    }
    // Pruned after a file gone by itself as after one taken away, so the
    // folders it leaves go the same way.
    for file in &files {
        if matches!(file.written, Written::Removed | Written::Gone) {
            if let Some(parent) = file.path.parent() {
                prune(&top, parent);
            }
        }
    }
    // Back in route order, a removal's failure among them.
    warnings.sort_by(|a, b| a.route().cmp(b.route()));
    Ok(Rendered { files, warnings })
}

impl Warning {
    /// The route of the rule it is about.
    fn route(&self) -> &str {
        match self {
            Warning::Foreign { route, .. }
            | Warning::Edited { route, .. }
            | Warning::Failed { route, .. } => route,
        }
    }
}

/// `route` held to the rules every name is ([`name_issues`]), since it
/// becomes folders and a file: lowercase words joined by single hyphens and
/// slashes never climb out of the folder, nor name one twice.
fn named(route: &str) -> Result<String, ClaudeRulesError> {
    let issues = name_issues(route);
    if issues.is_empty() {
        Ok(route.to_owned())
    } else {
        Err(ClaudeRulesError::Route {
            route: route.to_owned(),
            problem: issue_list(&issues),
        })
    }
}

/// What a render is to do at one rule's place.
enum Plan {
    /// Nothing is there: write the rule.
    Create(Vec<u8>),
    /// Mindflayer's file is there, holding what was recorded: write the rule
    /// as it is now over it.
    Replace(Vec<u8>),
    /// The rule as it is now is there already.
    Keep(Vec<u8>),
    /// Mindflayer's file is there, and its rule no longer asked for.
    Remove,
    /// The file recorded is not there, and its rule no longer asked for.
    Gone,
    /// What is there was recorded, and has changed since.
    Edited,
    /// What is there was never recorded.
    Foreign,
    /// What is there cannot be looked at.
    Unlooked(io::Error),
}

/// What to do at a rule's place, given the rule as it is now when it is
/// asked for, the hash recorded there when one is, and what is `found`.
fn plan(asked: Option<&[u8]>, recorded: Option<&str>, found: Found) -> Plan {
    let ours = |bytes: &[u8]| recorded.is_some_and(|hash| hash::bytes(bytes) == hash);
    match (asked, found) {
        (_, Found::Unreadable(error)) => Plan::Unlooked(error),
        (Some(rule), Found::Absent) => Plan::Create(rule.to_vec()),
        // Holding the rule byte for byte makes a recorded file current, even
        // one somebody edited back into it, but never makes an unrecorded
        // one Mindflayer's: it would then be taken away with its rule.
        (Some(rule), Found::File(held)) if recorded.is_some() && held == rule => Plan::Keep(held),
        (Some(rule), Found::File(held)) if ours(&held) => Plan::Replace(rule.to_vec()),
        (None, Found::File(held)) if ours(&held) => Plan::Remove,
        (None, Found::Absent) => Plan::Gone,
        (_, Found::File(_) | Found::Other) if recorded.is_some() => Plan::Edited,
        (_, Found::File(_) | Found::Other) => Plan::Foreign,
    }
}

/// What is at a rule's place.
enum Found {
    Absent,
    /// A file, not a link to one, and what it holds.
    File(Vec<u8>),
    /// A link, a folder, or anything else Mindflayer never writes there.
    Other,
    Unreadable(io::Error),
}

/// What is at `path`, a link looked at rather than followed: Mindflayer
/// writes none, so one there is somebody's, and what it points at too.
fn look(path: &Path) -> Found {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Found::Absent,
        Err(error) => Found::Unreadable(error),
        Ok(metadata) if metadata.is_file() => match fs::read(path) {
            Ok(bytes) => Found::File(bytes),
            Err(error) => Found::Unreadable(error),
        },
        Ok(_) => Found::Other,
    }
}

/// Write a new rule file whole, with the folders it needs, refusing one that
/// is there by now.
///
/// `create_new` rather than a temporary renamed into place, which would put
/// this file over one somebody made after the look found none. A write that
/// fails takes away what it made, so no half of a rule is left for Claude to
/// read, or to be taken for somebody's later.
fn create(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(contents) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Remove the folders a removal leaves empty, from `from` up to and
/// including `top`, the `mindflayer` folder.
///
/// Not past it: `.claude/rules` and `.claude` may be somebody's even when
/// empty, and Claude makes `.claude` itself, so nothing tells whether
/// Mindflayer made them. Below it, a folder only groups routes, and an empty
/// one groups nothing.
fn prune(top: &Path, from: &Path) {
    let mut current = Some(from);
    while let Some(folder) = current {
        if !folder.starts_with(top) || fs::remove_dir(folder).is_err() || folder == top {
            break;
        }
        current = folder.parent();
    }
}

fn failed(route: &str, path: &Path, error: &io::Error) -> Warning {
    Warning::Failed {
        route: route.to_owned(),
        path: path.to_path_buf(),
        error: error.to_string(),
    }
}

/// Why a project's rules could not be rendered into Claude's rules folder.
#[derive(Debug, Error)]
pub enum ClaudeRulesError {
    #[error("rule `{route}` cannot be named in Claude's rules folder: {problem}")]
    Route { route: String, problem: String },
    #[error("two rules are named `{route}`, and Claude's rules folder holds one file a name")]
    Twice { route: String },
    #[error("`{name}` is not a rule, so it has no file in Claude's rules folder")]
    NotARule { name: String },
    #[error(transparent)]
    Rule(#[from] ArtifactError),
}

#[cfg(test)]
#[path = "claude_rules_test.rs"]
mod tests;
