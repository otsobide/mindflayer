//! A project's rules, rendered into a block of its `AGENTS.md`.
//!
//! The root `AGENTS.md` is the one file of a project that Claude Code, Codex
//! and OpenCode all read, and Codex has no way to include another (D-03, on
//! the facts in docs/agents.md). So a rule reaches a project's agents as a
//! section of a block Mindflayer owns in that file, between two markers, in a
//! file that is otherwise somebody else's:
//!
//! ```text
//! <!-- mindflayer:begin -->
//! <!-- mindflayer:rule git/no-force-push -->
//! Never force-push a branch somebody else may have pulled.
//!
//! <!-- mindflayer:rule review/tone -->
//! Say what a change does before what is wrong with it.
//! <!-- mindflayer:end -->
//! ```
//!
//! HTML comments, because they are the one mark markdown has that is not
//! text: Claude strips a block-level comment before the file reaches the
//! model and keeps what lies between two of them (verified on 2.1.309), and
//! to Codex and OpenCode, which read the file as it is, a comment is a line
//! that says nothing. A heading of Mindflayer's own was the alternative, and
//! every agent would read it as one more instruction.
//!
//! Everything outside the block is somebody else's and comes back byte for
//! byte, but for the empty line just above the begin marker, which is the
//! block's (`Block::spacer`). A block that is not exactly one well-formed
//! block is refused rather than repaired, as in git's exclude file ([`crate::exclude`]): guessing
//! where a missing marker belongs could swallow somebody's text into the
//! block, and the next render would replace it.
//!
//! Which rules a project is given, and when, is the caller's to decide, and
//! so is keeping a record of what was written (#45); whether Claude reads the
//! file at all is #44's. This only renders what it is handed.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::agent::AgentHomes;
use crate::artifact::{issue_list, name_issues, Artifact, ArtifactError, Declared};
use crate::catalog::{report_order, spelled};
use crate::exclude;
use crate::install;
use crate::lines::{self, Line};
use crate::paths;
use crate::workspace::{replace_bytes, same_directory, FlayerWorkspace, MindProject};

/// The line that opens Mindflayer's block.
pub const BEGIN: &str = "<!-- mindflayer:begin -->";
/// The line that closes it.
pub const END: &str = "<!-- mindflayer:end -->";
/// What opens the marker heading one rule's section, before the rule's
/// route; [`CLOSE`] follows the route.
pub const RULE: &str = "<!-- mindflayer:rule ";
/// What closes a rule's marker, after its route.
pub const CLOSE: &str = " -->";

/// The file the block is rendered into, at a project's root.
pub const FILE: &str = "AGENTS.md";
/// What Codex reads in a folder in place of its `AGENTS.md`.
pub const OVERRIDE: &str = "AGENTS.override.md";

/// How many bytes of the `AGENTS.md` files from the git root down Codex
/// reads, unless its config says otherwise (`project_doc_max_bytes`): it
/// cuts off whatever comes past them (docs/agents.md, docs-only).
pub const CODEX_BUDGET: u64 = 32 * 1024;
/// Where those files are near enough to [`CODEX_BUDGET`] to say so: an
/// eighth short of it, room for a rule or two more of a page each, so the
/// warning comes a render before Codex starts cutting rather than after.
pub const CODEX_NEAR: u64 = CODEX_BUDGET / 8 * 7;

/// What each marker's comment opens with. A comment that opens with it and
/// is none of them is one written wrong, and refused.
const MARK: &[u8] = b"mindflayer:";

/// An `AGENTS.md`'s text, split around Mindflayer's block.
///
/// Kept as lines with their own endings ([`crate::lines`]), as git's exclude
/// file is, so what is outside the block is written back exactly as read, and
/// the block takes whichever ending the file already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsMd {
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// The empty line just above the begin marker, when there is one.
    ///
    /// A block added after what a file holds is set apart from it by an empty
    /// line, so that once Claude strips the markers the file's last paragraph
    /// and the first rule's are still two, as the blank line between two
    /// sections keeps two rules two; it is also the layout the probe verified
    /// on Claude (docs/agents.md). Taken for the block's when read, since
    /// nothing tells it from one written by hand, so that dropping the block
    /// gives back the file as it was before the block was added. One somebody
    /// put there goes with the block too: the price of that round trip,
    /// which leaving it would break for every file Mindflayer added to.
    spacer: Option<Line>,
    begin: Line,
    /// Every line between the markers, as read or as rendered.
    lines: Vec<Line>,
    /// The route each section's marker names, in the order they come.
    routes: Vec<String>,
    end: Line,
}

/// One rule as the block holds it: its route, and the lines of its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    route: String,
    /// The body's lines without their endings, less the blank lines it opens
    /// and closes with.
    lines: Vec<String>,
}

impl Section {
    /// The section for `rule`, or `None` for a rule about some files only.
    ///
    /// A rule whose front matter states `paths`
    /// ([`RuleManifest::is_scoped`](crate::rule::RuleManifest::is_scoped))
    /// goes to Claude's rules folder alone (D-03, #43), where Claude loads it
    /// once one of those files is read: in `AGENTS.md` every agent would read
    /// it for every file. A `paths` written in a shape that is not read says
    /// nothing, so that rule has its section. The body is what an agent is
    /// given, [`Artifact::contents`], read from the file as it is now.
    pub fn of(rule: &Artifact) -> Result<Option<Self>, AgentsMdError> {
        let Declared::Rule(manifest) = rule.declared() else {
            return Err(AgentsMdError::NotARule {
                name: rule.qualified_name(),
            });
        };
        if manifest.is_scoped() {
            return Ok(None);
        }
        let body = rule.contents()?;
        Section::new(rule.name(), &body).map(Some)
    }

    /// The section for the rule at `route`, whose body, its front matter
    /// already taken off, is `body`.
    ///
    /// The route is held to the rules every name is ([`name_issues`]), which
    /// is also what keeps its marker one that reads back as itself: a route
    /// of lowercase words joined by single hyphens and slashes holds no space
    /// and no `--`, so it can neither end the comment early nor read as two
    /// words. A body line that would read back as one of the markers is
    /// refused too, since the block it was written into would end, or open a
    /// section, where the rule meant neither.
    ///
    /// The blank lines a body opens and closes with are left out, so how a
    /// rule's file spaces its front matter, or ends, does not change how the
    /// block is laid out: one blank line between two sections, whatever each
    /// file holds. Every other line is kept as written, trailing spaces
    /// included, since two of them are a line break in markdown.
    pub fn new(route: impl Into<String>, body: &str) -> Result<Self, AgentsMdError> {
        let route = route.into();
        let issues = name_issues(&route);
        if !issues.is_empty() {
            return Err(AgentsMdError::Route {
                problem: issue_list(&issues),
                route,
            });
        }
        // `str::lines` takes a `\r` off only before a `\n`; one left at the
        // end of a line would be read back as that line's ending, and the
        // block would differ from its own rendering on every render.
        let mut lines: Vec<String> = body
            .lines()
            .map(|line| line.trim_end_matches('\r').to_owned())
            .collect();
        let blank = |line: &String| line.trim().is_empty();
        while lines.last().is_some_and(blank) {
            lines.pop();
        }
        let opening = lines.iter().take_while(|line| blank(line)).count();
        lines.drain(..opening);
        if let Some(line) = lines
            .iter()
            .find(|line| !matches!(marker(line.as_bytes()), Marker::Text))
        {
            return Err(AgentsMdError::Marked {
                line: line.clone(),
                route,
            });
        }
        Ok(Section { route, lines })
    }

    /// The rule's route, which names it in its marker.
    pub fn route(&self) -> &str {
        &self.route
    }
}

/// Render `rules` into the block of `project`'s `AGENTS.md`, each that has a
/// [section](Section::of) once, in the order a listing gives them
/// ([`report_order`]).
///
/// The block holds those sections and nothing else afterwards. A file that
/// is not there is made with `create_new`, so one somebody made meanwhile is
/// never written over, and only then is [`Written::Created`] said: the
/// caller keeps that (#45) and hands it back as `created` on a later render,
/// so that when no rule is left the file goes too, unless somebody has
/// written something in it since. Mindflayer takes away only a file it made.
/// A file that is there is replaced whole through a temporary and a rename
/// ([`replace_bytes`]), every byte outside the block as it was, and one that
/// already holds what was asked is not written at all, modification time
/// included.
///
/// A block that cannot be read, a rule that cannot be, and two rules of one
/// name are refused before anything is written: a section left out because
/// its rule could not be read would take that rule away from every agent
/// until the next render. The caller collects the refusal, as it does any
/// other one project's failure.
///
/// So is a block left in a folder whose `AGENTS.md` the agents of other
/// projects read too ([`misplaced`]): `workspace` is the one whose links say
/// where those are, and `homes` the agents' own folders, which the front end
/// decides as it does for an install. A render that leaves no block goes
/// ahead there, since it takes rules away from those agents rather than
/// giving them any. A file that is a link is written through to what it
/// points at, as a marker shared by a link is, but only inside the project
/// ([`linked_inside`]).
///
/// What the file then holds is checked against what Codex reads of it, and
/// anything that keeps the rules from Codex is said ([`Warning`]): only
/// while there is a block, since without one nothing of Mindflayer's is
/// there to miss.
///
/// Takes no lock: the caller holds the workspace's, as one calling
/// [`crate::config_entry::ConfigFile`] does, and the lock is not
/// re-entrant.
pub fn render<'a>(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    rules: impl IntoIterator<Item = &'a Artifact>,
    created: bool,
    homes: &AgentHomes,
) -> Result<Rendered, AgentsMdError> {
    let file = project.root().join(FILE);
    linked_inside(project.root(), &file)?;
    let mut rules: Vec<&Artifact> = rules.into_iter().collect();
    rules.sort_by(|a, b| report_order(a, b));
    let mut sections = Vec::new();
    for rule in rules {
        sections.extend(Section::of(rule)?);
    }
    // Only a render that leaves a block is held to where it is: one that
    // drops it is how a block put there before a project was linked below,
    // or before the one at the workspace's root was unlinked, comes out
    // again, and refusing it would keep the very leak the refusal is for.
    if !sections.is_empty() {
        if let Some(refused) = misplaced(workspace, homes, project.root(), &file) {
            return Err(refused.into());
        }
    }

    let read = read(&file)?;
    let mut text = AgentsMd::parse(read.as_deref().unwrap_or_default()).map_err(|problem| {
        AgentsMdError::Malformed {
            file: file.clone(),
            problem,
        }
    })?;
    text.set_block(&sections)?;
    let bytes = text.to_bytes();

    let left_empty = !text.has_block() && text.is_blank_outside();
    let written = match &read {
        None if !text.has_block() => Written::Unchanged,
        None => {
            create(&file, &bytes)?;
            Written::Created
        }
        // A link somebody put in place of the file is theirs, and so is
        // what it points at: emptied, never taken away.
        Some(_) if created && left_empty && is_plain_file(&file) => {
            fs::remove_file(&file).map_err(|source| AgentsMdError::Write {
                path: file.clone(),
                source,
            })?;
            Written::Removed
        }
        Some(old) if *old == bytes => Written::Unchanged,
        Some(_) => {
            replace_bytes(&file, &bytes)
                .map_err(|(path, source)| AgentsMdError::Write { path, source })?;
            Written::Changed
        }
    };
    let warnings = if text.has_block() {
        codex_warnings(project.root())
    } else {
        Vec::new()
    };
    Ok(Rendered {
        routes: text.routes().into_iter().map(str::to_owned).collect(),
        file,
        written,
        warnings,
    })
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The project's `AGENTS.md`, whether or not it is there now.
    pub file: PathBuf,
    pub written: Written,
    /// The route of each rule the block holds, in its order: none when there
    /// is no block. What the caller records as written (#45), since a rule
    /// declaring `paths` is handed in and given no section.
    pub routes: Vec<String>,
    /// What keeps the block from reaching Codex, or will, in the order
    /// [`Warning`] lists them.
    pub warnings: Vec<Warning>,
}

impl Rendered {
    /// Whether the file was not there, and this render made it.
    pub fn created(&self) -> bool {
        self.written == Written::Created
    }
}

/// What [`render`] did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// It was not there, and now holds the block: made with `create_new`.
    Created,
    /// It was replaced whole, through a temporary and a rename.
    Changed,
    /// It was taken away: Mindflayer made it, and the block was all that was
    /// left in it.
    Removed,
    /// It already held what was asked, or was not there and was asked to
    /// hold nothing, and was left as it was.
    Unchanged,
}

/// Something about where a rendered `AGENTS.md` sits that keeps its rules
/// from Codex, in part or whole: said, and no reason not to render.
///
/// Codex reads one file a folder, `AGENTS.override.md` before `AGENTS.md`,
/// from the git root down to where it starts, root first, and cuts the whole
/// off at a budget (docs/agents.md, docs-only). Each is worked out for Codex
/// started at the project's root, where the file is. Claude and OpenCode read
/// past all three, so none is said for them.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// An `AGENTS.override.md` beside the file, which Codex reads in its
    /// place.
    #[error("{file}: Codex reads it in place of the AGENTS.md beside it, so the rules rendered there do not reach Codex")]
    Overridden { file: PathBuf },
    /// No git work tree holds the project, so Codex, finding no root to read
    /// down from, reads only the folder it starts in.
    #[error("{project}: in no git work tree, so Codex reads its AGENTS.md only when started in that folder itself")]
    NoGitRoot { project: PathBuf },
    /// What Codex reads from the git root down comes to [`CODEX_NEAR`] or
    /// more, and not past [`CODEX_BUDGET`].
    #[error("{project}: the AGENTS.md files Codex reads there come to {bytes} bytes, close to the {CODEX_BUDGET} it reads before cutting off the rest")]
    NearBudget {
        project: PathBuf,
        bytes: u64,
        /// The file Codex reads in each folder, root first.
        files: Vec<PathBuf>,
    },
    /// What Codex reads from the git root down comes to more than
    /// [`CODEX_BUDGET`], so it cuts off the end, where the project's own
    /// file is.
    #[error("{project}: the AGENTS.md files Codex reads there come to {bytes} bytes, past the {CODEX_BUDGET} it reads, so it cuts off the rest")]
    OverBudget {
        project: PathBuf,
        bytes: u64,
        /// The file Codex reads in each folder, root first.
        files: Vec<PathBuf>,
    },
    /// The repository, or a file Codex would read, could not be looked at,
    /// so how much Codex reads is not known.
    #[error("{project}: how much of its AGENTS.md Codex reads cannot be told: {detail}")]
    Unmeasured { project: PathBuf, detail: String },
}

/// What [`Warning`]s the `AGENTS.md` at `project`'s root, as it is on disk,
/// calls for.
fn codex_warnings(project: &Path) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let beside = project.join(OVERRIDE);
    if beside.is_file() {
        warnings.push(Warning::Overridden { file: beside });
    }
    let unmeasured = |detail: String| Warning::Unmeasured {
        project: project.to_path_buf(),
        detail,
    };
    // Every folder from the git root down to the project, root first.
    let folders = match exclude::work_tree(project) {
        Ok(Some((root, route))) => {
            let mut folder = root;
            let mut folders = vec![folder.clone()];
            for component in route.components() {
                if let Component::Normal(name) = component {
                    folder.push(name);
                    folders.push(folder.clone());
                }
            }
            folders
        }
        Ok(None) => {
            warnings.push(Warning::NoGitRoot {
                project: project.to_path_buf(),
            });
            vec![project.to_path_buf()]
        }
        Err(error) => {
            warnings.push(unmeasured(error.to_string()));
            return warnings;
        }
    };
    let mut bytes = 0;
    let mut files = Vec::new();
    for folder in folders {
        for name in [OVERRIDE, FILE] {
            let path = folder.join(name);
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => {
                    bytes += metadata.len();
                    files.push(path);
                    break;
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    warnings.push(unmeasured(format!("{}: {error}", path.display())));
                    return warnings;
                }
            }
        }
    }
    let project = project.to_path_buf();
    if bytes > CODEX_BUDGET {
        warnings.push(Warning::OverBudget {
            project,
            bytes,
            files,
        });
    } else if bytes >= CODEX_NEAR {
        warnings.push(Warning::NearBudget {
            project,
            bytes,
            files,
        });
    }
    warnings
}

/// Why `file` is no place for rules rendered for the project at `folder`,
/// when agents beyond that project's would read them there.
///
/// Claude reads every `AGENTS.md` from where it starts up to the root, and
/// OpenCode V2 up to the home directory, so one in a folder above a project
/// is read in that project too (docs/agents.md, docs-only for reading more
/// than one): rules rendered into a monorepo's root would reach every
/// package linked below it, beside the package's own. So a folder above a
/// project the workspace links is refused, a stale entry's included, since
/// its project may only have moved for a moment. So is the workspace's own
/// root, unless a project it links is there: `flayer init` and the
/// workspace's own artifacts put nothing in it ([`FlayerWorkspace::own`] is
/// a holder at that root, not a project), while a project at the root is a
/// project like any other, and its `AGENTS.md` its own (D-03).
///
/// So is a `file` inside one of the agents' own folders, as an install's
/// copy is ([`crate::install::InstallError::UserScope`]): a project at
/// `$CODEX_HOME` would write the `AGENTS.md` Codex reads in every project,
/// and one at `~/.claude` the `.claude/AGENTS.md` Claude finds above every
/// project in the user's home. Inside rather than equal, for the reason an
/// install gives: nothing in one of those folders is any one project's.
///
/// Every renderer of a project's rules asks this of the file it would
/// write, so none of them leaves rules where the others refuse to.
pub(crate) fn misplaced(
    workspace: &FlayerWorkspace,
    homes: &AgentHomes,
    folder: &Path,
    file: &Path,
) -> Option<Misplaced> {
    if let Some((file, home, reader)) = install::in_agent_home(homes, &[file.to_path_buf()]) {
        return Some(Misplaced::UserScope { file, home, reader });
    }
    if same_directory(folder, workspace.root()) && !workspace.is_linked(folder) {
        return Some(Misplaced::WorkspaceRoot {
            folder: folder.to_path_buf(),
        });
    }
    let below: Vec<PathBuf> = workspace
        .members()
        .into_iter()
        .map(|member| member.root)
        .filter(|root| is_below(root, folder))
        .collect();
    if !below.is_empty() {
        return Some(Misplaced::AboveProjects {
            folder: folder.to_path_buf(),
            projects: below,
        });
    }
    None
}

/// Why rules are not written where a project would have them: what
/// [`misplaced`] finds, for each renderer to refuse in words of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Misplaced {
    /// The file is inside the folder `reader` reads for every project.
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    /// The project is at the workspace's root, which links no project there.
    WorkspaceRoot { folder: PathBuf },
    /// The project is above these, which the workspace links.
    AboveProjects {
        folder: PathBuf,
        projects: Vec<PathBuf>,
    },
}

impl From<Misplaced> for AgentsMdError {
    fn from(misplaced: Misplaced) -> Self {
        match misplaced {
            Misplaced::UserScope { file, home, reader } => {
                AgentsMdError::UserScope { file, home, reader }
            }
            Misplaced::WorkspaceRoot { folder } => AgentsMdError::WorkspaceRoot { folder },
            Misplaced::AboveProjects { folder, projects } => {
                AgentsMdError::AboveProjects { folder, projects }
            }
        }
    }
}

/// Whether `path` is inside `folder` and not `folder` itself: by the route
/// from one to the other, then through the filesystem, so a spelling through
/// a link is still below what it leads into.
fn is_below(path: &Path, folder: &Path) -> bool {
    let down = |route: PathBuf| {
        route
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    };
    if paths::relative_to(path, folder).is_some_and(down) {
        return true;
    }
    match (fs::canonicalize(path), fs::canonicalize(folder)) {
        (Ok(path), Ok(folder)) => path
            .strip_prefix(&folder)
            .is_ok_and(|route| route.components().next().is_some()),
        _ => false,
    }
}

/// Refuse an `AGENTS.md` that is a link to a file outside `project`.
///
/// One that is a link to a file in the project, `AGENTS.md` to `CLAUDE.md`
/// as some teams keep the two, is written through, as [`replace_bytes`]
/// follows a link to the file it names rather than replacing the link; #44's
/// import, between the same markers in that `CLAUDE.md`, has yet to allow
/// for it (docs/architecture.md, open questions). One
/// leading out is not: a repository can hold a link to the user's own
/// `~/.codex/AGENTS.md`, which every project would then be given this one's
/// rules through. Refused rather than replaced by a file of the project's
/// own, which would undo what somebody linked on purpose. A link to nothing
/// is let by: there is nothing to read, and `create_new` makes nothing
/// through it.
fn linked_inside(project: &Path, file: &Path) -> Result<(), AgentsMdError> {
    let is_link = fs::symlink_metadata(file).is_ok_and(|metadata| metadata.is_symlink());
    if !is_link {
        return Ok(());
    }
    let Ok(target) = fs::canonicalize(file) else {
        return Ok(());
    };
    let root = fs::canonicalize(project).map_err(|source| AgentsMdError::Read {
        path: project.to_path_buf(),
        source,
    })?;
    if target.starts_with(&root) {
        Ok(())
    } else {
        Err(AgentsMdError::LinkedOut {
            file: file.to_path_buf(),
            target,
        })
    }
}

/// The file's bytes, `None` when it is not there.
fn read(path: &Path) -> Result<Option<Vec<u8>>, AgentsMdError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(AgentsMdError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Write a new file whole, refusing one that is there by now.
///
/// `create_new` rather than a temporary renamed into place, which would put
/// this file over one somebody made after the read found none. A write that
/// fails takes away what it made, so no half of a block is left to be refused
/// as malformed later.
fn create(path: &Path, contents: &[u8]) -> Result<(), AgentsMdError> {
    let write_error = |source| AgentsMdError::Write {
        path: path.to_path_buf(),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(write_error)?;
    if let Err(source) = file.write_all(contents) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(write_error(source));
    }
    Ok(())
}

/// Whether `path` is a file itself, not a link to one.
fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

impl AgentsMd {
    /// Read an `AGENTS.md`'s bytes. An absent file is an empty one.
    ///
    /// Line by line, not as markdown: a marker quoted in somebody's code
    /// fence is a marker here, and refused unless it makes one well-formed
    /// block. Telling a fence apart was the alternative, at the price of a
    /// markdown parser for a mistake that is refused, never acted on.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let lines = lines::split(bytes);
        let ending = lines::ending(&lines);

        let mut begin = None;
        let mut end = None;
        let mut routes: Vec<String> = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let number = index + 1;
            match marker(&line.text) {
                Marker::Text => {}
                Marker::Begin => {
                    if begin.is_some() {
                        return Err(Malformed::Reopened { line: number });
                    }
                    begin = Some(index);
                }
                Marker::End => {
                    if begin.is_none() || end.is_some() {
                        return Err(Malformed::Unopened { line: number });
                    }
                    end = Some(index);
                }
                Marker::Rule(route) => {
                    if begin.is_none() || end.is_some() {
                        return Err(Malformed::Stray { line: number });
                    }
                    if routes.iter().any(|seen| seen == route) {
                        return Err(Malformed::Repeated {
                            line: number,
                            route: route.to_owned(),
                        });
                    }
                    routes.push(route.to_owned());
                }
                Marker::Miswritten => return Err(Malformed::Miswritten { line: number }),
            }
        }

        let (begin, end) = match (begin, end) {
            (None, _) => {
                return Ok(AgentsMd {
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
        let mut before: Vec<Line> = lines.by_ref().take(begin).collect();
        let spacer = before.pop_if(|line| line.text.is_empty());
        let begin_line = lines.next().expect("the begin marker was found");
        let inside: Vec<Line> = lines.by_ref().take(end - begin - 1).collect();
        let end_line = lines.next().expect("the end marker was found");
        Ok(AgentsMd {
            before,
            block: Some(Block {
                spacer,
                begin: begin_line,
                lines: inside,
                routes,
                end: end_line,
            }),
            after: lines.collect(),
            ending,
        })
    }

    /// The route of each rule the block holds, in the order it holds them:
    /// none when there is no block.
    pub fn routes(&self) -> Vec<&str> {
        self.block
            .iter()
            .flat_map(|block| &block.routes)
            .map(String::as_str)
            .collect()
    }

    /// Whether the file holds Mindflayer's block.
    pub fn has_block(&self) -> bool {
        self.block.is_some()
    }

    /// Whether nothing but blank lines is left outside the block: what a
    /// file Mindflayer made holds once somebody has written nothing in it.
    pub fn is_blank_outside(&self) -> bool {
        self.before.iter().chain(&self.after).all(Line::is_blank)
    }

    /// Make the block hold `sections`, in that order and nothing else, and
    /// whether that changed anything.
    ///
    /// Everything between the markers is Mindflayer's, so it is replaced
    /// whole, a line somebody wrote there included; telling an edited section
    /// from the one rendered is for the ledger's record of what was written
    /// (D-04, #45). A file with no block gains one after what it holds, on
    /// lines of its own and an empty line apart from it (`Block::spacer`).
    /// No section drops the block, that empty line included, giving back the
    /// file as it was before the block was written.
    pub fn set_block(&mut self, sections: &[Section]) -> Result<bool, AgentsMdError> {
        for (index, section) in sections.iter().enumerate() {
            if sections[..index]
                .iter()
                .any(|other| other.route == section.route)
            {
                return Err(AgentsMdError::Twice {
                    route: section.route.clone(),
                });
            }
        }
        if sections.is_empty() {
            return Ok(self.drop_block());
        }

        let ending = self.ending;
        let lines = rendered(sections, ending);
        let routes = sections
            .iter()
            .map(|section| section.route.clone())
            .collect();
        if let Some(block) = &mut self.block {
            if block.lines == lines {
                return Ok(false);
            }
            block.lines = lines;
            block.routes = routes;
            return Ok(true);
        }
        // The new block goes on lines of its own, so a last line without a
        // newline gets one, and the end marker goes without one instead: the
        // file still ends the way it did, and dropping the block takes the
        // newline back. The empty line above it comes after anything at all,
        // a blank last line included: one left out there would read back as
        // a spacer, and dropping the block would take the file's own.
        let mut close = ending;
        if let Some(last) = self.before.last_mut() {
            if last.end.is_empty() {
                last.end = ending;
                close = b"";
            }
        }
        let spacer = (!self.before.is_empty()).then(|| Line::new("", ending));
        self.block = Some(Block {
            spacer,
            begin: Line::new(BEGIN, ending),
            lines,
            routes,
            end: Line::new(END, close),
        });
        Ok(true)
    }

    /// Take the block out, markers and all, and whether there was one.
    fn drop_block(&mut self) -> bool {
        let Some(block) = self.block.take() else {
            return false;
        };
        // An end marker without a newline is the file's last line, so the
        // file ended without one and still does once the block is gone.
        let after = std::mem::take(&mut self.after);
        self.before.extend(after);
        if block.end.end.is_empty() {
            if let Some(last) = self.before.last_mut() {
                last.end = b"";
            }
        }
        true
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
            block.spacer.iter().for_each(&mut emit);
            emit(&block.begin);
            block.lines.iter().for_each(&mut emit);
            emit(&block.end);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

/// The lines between the markers for `sections`: each rule's marker, then its
/// body, with a blank line between one section and the next.
///
/// The blank line keeps two rules two paragraphs once Claude has stripped the
/// markers, which would otherwise leave the last line of one against the
/// first of the next.
fn rendered(sections: &[Section], ending: &'static [u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            lines.push(Line::new("", ending));
        }
        lines.push(Line::new(
            &format!("{RULE}{}{CLOSE}", section.route),
            ending,
        ));
        lines.extend(section.lines.iter().map(|line| Line::new(line, ending)));
    }
    lines
}

/// What a line of the file is to the block.
#[derive(Debug, PartialEq, Eq)]
enum Marker<'a> {
    /// Anything that is not a marker.
    Text,
    Begin,
    End,
    /// A rule's marker, and the route it names.
    Rule(&'a str),
    /// A comment that opens as a marker does and is none of them: indented,
    /// spaced otherwise, naming a mark there is not, or a route that is no
    /// name.
    Miswritten,
}

/// What `text`, one line without its ending, is to the block.
///
/// A marker is the line exactly as Mindflayer writes it, trailing whitespace
/// forgiven as an editor may add some. Anything else that opens as a comment
/// naming a mark is a marker written wrong, and refused rather than read as
/// text: read as text, a hand-indented begin marker would leave the next
/// render to write a second block below the first.
fn marker(text: &[u8]) -> Marker<'_> {
    let line = text.trim_ascii_end();
    if line == BEGIN.as_bytes() {
        return Marker::Begin;
    }
    if line == END.as_bytes() {
        return Marker::End;
    }
    if let Some(route) = line
        .strip_prefix(RULE.as_bytes())
        .and_then(|rest| rest.strip_suffix(CLOSE.as_bytes()))
    {
        return match std::str::from_utf8(route) {
            Ok(route) if name_issues(route).is_empty() => Marker::Rule(route),
            _ => Marker::Miswritten,
        };
    }
    let opens_a_mark = text
        .trim_ascii()
        .strip_prefix(b"<!--")
        .map(<[u8]>::trim_ascii_start)
        .and_then(|comment| comment.get(..MARK.len()))
        .is_some_and(|start| start.eq_ignore_ascii_case(MARK));
    if opens_a_mark {
        Marker::Miswritten
    } else {
        Marker::Text
    }
}

/// Why the block in an `AGENTS.md` is not one Mindflayer can replace. Lines
/// are counted from 1.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Malformed {
    #[error("line {line}: `{BEGIN}` is never closed by `{END}`")]
    Unclosed { line: usize },
    #[error("line {line}: `{END}` closes no `{BEGIN}`")]
    Unopened { line: usize },
    #[error("line {line}: a second `{BEGIN}`, where there can be one block")]
    Reopened { line: usize },
    #[error("line {line}: a rule's marker outside the block")]
    Stray { line: usize },
    #[error("line {line}: a second section for the rule `{route}` in the block")]
    Repeated { line: usize, route: String },
    #[error(
        "line {line}: written like one of Mindflayer's markers but none of them, \
         which are each a line of their own, as Mindflayer writes them"
    )]
    Miswritten { line: usize },
}

/// Why a project's rules could not be rendered into its `AGENTS.md`.
#[derive(Debug, Error)]
pub enum AgentsMdError {
    #[error("rule `{route}` cannot be named in AGENTS.md: {problem}")]
    Route { route: String, problem: String },
    #[error(
        "rule `{route}` holds a line that would be read back as one of Mindflayer's markers, \
         so it is not written into AGENTS.md: {line}"
    )]
    Marked { route: String, line: String },
    #[error("two rules are named `{route}`, and AGENTS.md holds one section a name")]
    Twice { route: String },
    #[error("`{name}` is not a rule, so it has no section in AGENTS.md")]
    NotARule { name: String },
    #[error(
        "{folder}: the workspace's own folder, where it links no project, so no AGENTS.md \
         is written there: the agents of every project below would read it"
    )]
    WorkspaceRoot { folder: PathBuf },
    #[error(
        "{folder}: above {}, which the workspace links, so no AGENTS.md is written there: \
         their agents would read it too",
        spelled(.projects)
    )]
    AboveProjects {
        folder: PathBuf,
        projects: Vec<PathBuf>,
    },
    #[error(
        "{file} is inside {home}, which {reader} reads for every project, so no rules are \
         written into it: Mindflayer gives rules to one project at a time"
    )]
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error(
        "{file}: a link to {target}, outside the project, so it is not written through: \
         what it points at is not the project's to change"
    )]
    LinkedOut { file: PathBuf, target: PathBuf },
    #[error(transparent)]
    Rule(#[from] ArtifactError),
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
#[path = "agents_md_test.rs"]
mod tests;
