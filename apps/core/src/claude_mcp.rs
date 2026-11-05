//! What Claude Code is given of an MCP server's definition: one entry of
//! its `.mcp.json`, `mcpServers.<name>` (D-10).
//!
//! The entry always says its `type`. An http entry somebody wrote without
//! one is dropped by Claude alone and costs the file nothing else
//! (`claude-mcp-untyped`, #18's in `docs/agents.md`), and an entry of
//! Mindweaver's never is one. A program is `{type, command, args, env}`
//! and an address `{type, url, headers}`, each key written whether or not
//! it holds anything, so a definition has one entry, and one canonical
//! form, whatever it leaves empty.
//!
//! Each input is written where its placeholder stands as `${VAR}`, the
//! variable it is read from (D-11): Claude expands one in `args`, `env`,
//! `url` and `headers`, inside a longer value too (`claude-mcp`, verified
//! on 2.1.318). The `command` is written as the definition holds it: no
//! input's kind puts one there, and that Claude expands a reference there
//! as well is its documentation's word and its binary's, which none of
//! #18's checks tried, so nothing written here rests on it.
//!
//! What Claude cannot be given of a definition is
//! [`McpDefinition::inexpressible`]'s to say, never a second copy of its
//! rule here: a shape that refuses the agent refuses the entry, and one the
//! entry is written without (a program's `cwd`) is a [`Warning`].
//!
//! No approval is written. A person approves a project's servers when
//! Claude asks, and a `-p` run starts every server the file names, approved
//! or not (`claude-mcp`), so an entry runs there as soon as it is written.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::agent::Agent;
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::Value;
use crate::mcp::{Inexpressible, McpDefinition, Transport};
use crate::value::{ArtifactName, InputName, VariableName};

/// The object in `.mcp.json` that holds every server by its name.
pub const SERVERS: &str = "mcpServers";

/// What Claude is given of a definition: its entry under [`SERVERS`], and
/// what is said of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key it is written under, the definition's name.
    pub name: ArtifactName,
    /// What the key holds.
    pub value: Value,
    /// What the entry is written without, and a transport Claude calls
    /// deprecated, in the order the definition holds them.
    pub warnings: Vec<Warning>,
}

impl Entry {
    /// The entry's canonical JSON form ([`Value::canonical`]): what its
    /// fingerprint is the digest of, for the ledger to keep (#93).
    pub fn canonical(&self) -> String {
        self.value.canonical()
    }
}

/// The entry Claude is given for `definition`, or why it is given none.
///
/// Refused, before anything else, when `validate` would call the
/// definition invalid ([`artifact::mcp_strict_issues`]), so a token it
/// holds as it is never reaches a file; then for anything
/// [`McpDefinition::inexpressible`] says refuses Claude; and for an input
/// read from no variable, which no entry could be given but as a value,
/// and a key its extras for Claude give that the entry writes already.
pub fn entry(definition: &McpDefinition) -> Result<Entry, ClaudeMcpError> {
    let name = definition.name.clone();
    let unfit = artifact::mcp_strict_issues(definition, &name, &name, &[]);
    if !unfit.is_empty() {
        return Err(ClaudeMcpError::Unfit {
            name: name.to_string(),
            issues: unfit,
        });
    }
    let mut warnings = Vec::new();
    for shape in definition.inexpressible(Agent::Claude) {
        if shape.refuses() {
            return Err(ClaudeMcpError::Inexpressible {
                name: name.to_string(),
                shape,
            });
        }
        warnings.push(Warning::Dropped {
            name: name.to_string(),
            shape,
        });
    }

    let mut read = Vec::new();
    for input in &definition.inputs {
        let Some(variable) = &input.env_var else {
            return Err(ClaudeMcpError::InputUnread {
                name: name.to_string(),
                input: input.name.clone(),
            });
        };
        read.push((&input.name, variable));
    }
    let referred = |text: &str| Value::String(referring(text, &read));
    let table = |pairs: &BTreeMap<String, String>| {
        Value::object(
            pairs
                .iter()
                .map(|(key, text)| (key.clone(), referred(text))),
        )
    };

    let mut members: Vec<(String, Value)> = match &definition.transport {
        Transport::Stdio(stdio) => vec![
            (
                String::from("type"),
                Value::from(definition.transport.slug()),
            ),
            (String::from("command"), Value::from(stdio.command.as_str())),
            (
                String::from("args"),
                Value::Array(stdio.args.iter().map(|arg| referred(arg)).collect()),
            ),
            (String::from("env"), table(&stdio.env)),
        ],
        Transport::Http(endpoint) | Transport::Sse(endpoint) => vec![
            (
                String::from("type"),
                Value::from(definition.transport.slug()),
            ),
            (String::from("url"), referred(&endpoint.url)),
            (String::from("headers"), table(&endpoint.headers)),
        ],
    };
    match definition.transport {
        Transport::Sse(_) => warnings.push(Warning::ServerSentEvents {
            name: name.to_string(),
        }),
        Transport::Stdio(_) | Transport::Http(_) => {}
    }
    // Passed on as written, beside what the transport renders, never over
    // it: an extra holding a key the entry writes would leave one of the
    // two lost, and which one would be the order of a file's keys.
    for (key, value) in definition.extras_for(Agent::Claude).into_iter().flatten() {
        if members.iter().any(|(written, _)| written == key) {
            return Err(ClaudeMcpError::ExtraRepeated {
                name: name.to_string(),
                key: key.clone(),
            });
        }
        members.push((key.clone(), value_of(value)));
    }
    Ok(Entry {
        name,
        value: Value::Object(members),
        warnings,
    })
}

/// What is said of an entry written all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// Reached over server-sent events, which the MCP specification has
    /// replaced with streamable HTTP and Claude Code's documentation calls
    /// deprecated: still written, since Claude still takes it.
    #[error("`{name}` is reached over sse, which Claude Code calls deprecated: reach it over streamable HTTP where the server offers that")]
    ServerSentEvents { name: String },
    /// Something of the definition the entry is written without
    /// ([`Inexpressible::refuses`] says it is no refusal).
    #[error("`{name}`: {shape}")]
    Dropped { name: String, shape: Inexpressible },
}

codes!(Warning {
    Self::ServerSentEvents { .. } => "claude_mcp_sse_deprecated",
} carried {
    Self::Dropped { shape, .. } => shape,
});

/// Why Claude is given no entry.
#[derive(Debug, Error)]
pub enum ClaudeMcpError {
    #[error("`{name}` is written for no agent while it is invalid: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}` cannot be given to Claude Code: {shape}")]
    Inexpressible { name: String, shape: Inexpressible },
    #[error("`{name}` cannot be given to Claude Code: input `{input}` is read from no environment variable, and its entry could be given it only as its value")]
    InputUnread { name: String, input: InputName },
    #[error("`{name}` cannot be given to Claude Code: its extras for Claude give `{key}`, which the entry writes from its transport")]
    ExtraRepeated { name: String, key: String },
}

codes!(ClaudeMcpError {
    Self::Unfit { .. } => "claude_mcp_unfit",
    Self::InputUnread { .. } => "claude_mcp_input_unread",
    Self::ExtraRepeated { .. } => "claude_mcp_extra_repeated",
} carried {
    Self::Inexpressible { shape, .. } => shape,
});

/// `text` with each placeholder of an input in `read` written as Claude's
/// reference to the variable it is read from, and every other byte as it
/// is, a brace naming no input among them.
///
/// In one pass from the left, so a reference written in place of one
/// placeholder is never read again as another: `${A}` holds `{A}`, which
/// another input may be named.
fn referring(text: &str, read: &[(&InputName, &VariableName)]) -> String {
    let mut written = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        written.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let named = after.find('}').and_then(|close| {
            let name = &after[..close];
            read.iter()
                .find(|(input, _)| input.as_str() == name)
                .map(|(_, variable)| (close, variable))
        });
        match named {
            Some((close, variable)) => {
                written.push_str("${");
                written.push_str(variable.as_str());
                written.push('}');
                rest = &after[close + 1..];
            }
            None => {
                written.push('{');
                rest = after;
            }
        }
    }
    written.push_str(rest);
    written
}

/// An extra as an entry holds it, every value as it is; a TOML date and
/// time, which JSON has no value for, as the text it is written in, as
/// [`crate::config_entry`] reads one.
fn value_of(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => Value::Integer(*number),
        toml::Value::Float(number) => Value::Float(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        toml::Value::Datetime(moment) => Value::String(moment.to_string()),
        toml::Value::Array(values) => Value::Array(values.iter().map(value_of).collect()),
        toml::Value::Table(table) => Value::object(
            table
                .iter()
                .map(|(key, value)| (key.clone(), value_of(value))),
        ),
    }
}

#[cfg(test)]
#[path = "claude_mcp_test.rs"]
mod tests;
