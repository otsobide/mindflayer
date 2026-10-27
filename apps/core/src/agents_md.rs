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
//! byte. A block that is not exactly one well-formed block is refused rather
//! than repaired, as in git's exclude file ([`crate::exclude`]): guessing
//! where a missing marker belongs could swallow somebody's text into the
//! block, and the next render would replace it.

use thiserror::Error;

use crate::artifact::{issue_list, name_issues};
use crate::lines::{self, Line};

/// The line that opens Mindflayer's block.
pub const BEGIN: &str = "<!-- mindflayer:begin -->";
/// The line that closes it.
pub const END: &str = "<!-- mindflayer:end -->";
/// What opens the marker heading one rule's section, before the rule's
/// route; [`CLOSE`] follows the route.
pub const RULE: &str = "<!-- mindflayer:rule ";
/// What closes a rule's marker, after its route.
pub const CLOSE: &str = " -->";

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
        let before: Vec<Line> = lines.by_ref().take(begin).collect();
        let begin_line = lines.next().expect("the begin marker was found");
        let inside: Vec<Line> = lines.by_ref().take(end - begin - 1).collect();
        let end_line = lines.next().expect("the end marker was found");
        Ok(AgentsMd {
            before,
            block: Some(Block {
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
    /// lines of its own. No section drops the block, giving back the file as
    /// it was before the block was written.
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
        // newline back.
        let mut close = ending;
        if let Some(last) = self.before.last_mut() {
            if last.end.is_empty() {
                last.end = ending;
                close = b"";
            }
        }
        self.block = Some(Block {
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
}

#[cfg(test)]
#[path = "agents_md_test.rs"]
mod tests;
