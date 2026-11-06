//! What an agent is given to start or reach an MCP server, as its own entry
//! spells it: what an install says of each agent before it writes anything
//! (#95).
//!
//! Read off each renderer's entry ([`crate::claude_mcp::Entry::runs`],
//! [`crate::codex_mcp::Entry::runs`], [`crate::opencode_mcp::Entry::runs`])
//! rather than off the definition: an input reaches Claude as `${VAR}` and
//! OpenCode as `{env:VAR}`, and Codex is given a program's arguments as
//! they are, so what each agent will run is what its entry holds, and a
//! front end spelling it again from the definition would say what no agent
//! is given.

use std::fmt;

use crate::config_entry::Value;

/// The program an agent starts, or the address it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runs {
    /// A program: what the agent runs, and each argument after it, as the
    /// entry spells them.
    Program { command: String, args: Vec<String> },
    /// An address, as the entry spells it, which holds no credential: the
    /// definition's goes through [`crate::url::redacted`].
    Address { url: String },
}

impl Runs {
    /// What the object `entry` holds as a program, its command under
    /// `command` and its arguments under `args`, or as an address under
    /// `url`: none when it holds neither, which no renderer writes.
    pub(crate) fn of(entry: &Value) -> Option<Runs> {
        if let Some(url) = member(entry, "url").and_then(text) {
            return Some(Runs::Address {
                url: url.to_owned(),
            });
        }
        let command = member(entry, "command").and_then(text)?;
        Some(Runs::Program {
            command: command.to_owned(),
            args: words(member(entry, "args"))?,
        })
    }

    /// What the object `entry` holds as a program whose command and
    /// arguments are one array under `command`, OpenCode's shape, or as an
    /// address under `url`.
    pub(crate) fn of_line(entry: &Value) -> Option<Runs> {
        if let Some(url) = member(entry, "url").and_then(text) {
            return Some(Runs::Address {
                url: url.to_owned(),
            });
        }
        let mut line = words(member(entry, "command"))?.into_iter();
        Some(Runs::Program {
            command: line.next()?,
            args: line.collect(),
        })
    }
}

/// A program as a shell line that runs it, each word that a shell would
/// split or read as something else quoted, so an argument holding a space
/// reads as the one argument it is; an address as it is.
///
/// A reference to a variable, `${VAR}` or `{env:VAR}`, is left unquoted:
/// it is what the agent fills in, and quoting it would read as though it
/// were given the text.
impl fmt::Display for Runs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Runs::Address { url } => f.write_str(url),
            Runs::Program { command, args } => {
                f.write_str(&quoted(command))?;
                for arg in args {
                    write!(f, " {}", quoted(arg))?;
                }
                Ok(())
            }
        }
    }
}

/// `word` as one word of a shell line: as it is when it holds only what no
/// shell splits or reads as anything but itself, a `$` only where it opens
/// a reference (`${`), and otherwise between single quotes, a single quote
/// in it closed, escaped and opened again.
fn quoted(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,{}~^".contains(c);
    let mut chars = word.chars().peekable();
    let mut bare = !word.is_empty();
    while let Some(c) = chars.next() {
        if !(plain(c) || c == '$' && chars.peek() == Some(&'{')) {
            bare = false;
            break;
        }
    }
    if bare {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// The member `key` of the object `value`.
fn member<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(members) => members
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

/// The text `value` holds, when it is text.
fn text(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// Each word of the array `value`, none when there is no array, as Codex's
/// entry leaves `args` out when it holds none; refused when one is no text.
fn words(value: Option<&Value>) -> Option<Vec<String>> {
    match value {
        None => Some(Vec::new()),
        Some(Value::Array(words)) => words
            .iter()
            .map(|word| text(word).map(str::to_owned))
            .collect(),
        Some(_) => None,
    }
}

#[cfg(test)]
#[path = "runs_test.rs"]
mod tests;
