//! An MCP server's definition rendered into OpenCode's project config: one
//! entry under `mcp`, written and taken out again through
//! [`crate::config_entry`], every byte outside it kept (D-10).
//!
//! OpenCode has two maintained lines, and two shapes of a server: V2's,
//! `mcp.servers.<name>`, which V2 reads and V1 has read since 1.18.24, and
//! V1's, `mcp.<name>` with `enabled`, which V2 reads too (#18's
//! `opencode-mcp-v2`, `opencode-mcp-1-19` and `opencode-mcp-v1-shape`,
//! verified on 2.2.5, 2.1.3 and 1.19.4). So an entry is V2's minimal
//! shape (D-12), and V1's only in a file that already keeps its servers in
//! that shape ([`Shape`]), never the two mixed in one entry. Neither carries
//! `codemode`, `protocol` or `timeout.startup`, on which V1 warns, nor any
//! key of Mindweaver's, since an entry takes no key OpenCode does not know.
//!
//! A program is `{type: "local", command: [command, ...args], environment,
//! cwd}` and an address `{type: "remote", url, headers}`, each key after
//! the command or the address written only when it holds something, so a
//! definition has one entry in each shape, and one canonical form, the form
//! whose digest the ledger will keep (#93). V1's has no key for a program's
//! folder, so an entry written in it leaves `cwd` out, and says so
//! ([`Warning::FolderInV1`]).
//!
//! Each input is written where its placeholder stands as `{env:VAR}`, the
//! variable it is read from (D-11), which OpenCode fills in over the file's
//! text before it parses it, in the command, its first word included, the
//! environment, the URL and the headers, inside a longer value too (the
//! OpenCode runs, verified). The command's first word and the folder are
//! written as the definition holds them: no input's kind puts one there.
//!
//! What OpenCode cannot be given of a definition is
//! [`McpDefinition::inexpressible`]'s to say, never a second copy of its
//! rule here, and it is asked before anything else is: server-sent events,
//! which V2's shape does not take, and text OpenCode would fill in where no
//! input stands, `{env:` or `{file:` written as it is (`text_substituted`),
//! refuse the entry.
//!
//! Nothing here knows of the ledger. Whether an entry of the name is
//! Mindweaver's to replace or take out, and whether Mindweaver made the
//! file, are the caller's, as its record says (#93, #94); and the caller
//! holds the workspace's lock, as one calling
//! [`ConfigFile`](crate::config_entry::ConfigFile) does.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::agent::Agent;
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::Value;
use crate::mcp::{self, Inexpressible, McpDefinition, Transport, TransportType};
use crate::value::{ArtifactName, InputName, VariableName};

/// The object in OpenCode's config that holds its servers, in either shape.
pub const MCP: &str = "mcp";

/// The object in [`MCP`] that holds every server by its name in V2's shape.
pub const SERVERS: &str = "servers";

/// Which of OpenCode's two shapes a server is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// V2's, `mcp.servers.<name>`: what Mindweaver writes (D-12), which V2
    /// reads and V1 has read since 1.18.24.
    V2,
    /// V1's, `mcp.<name>` with `enabled`, which V2 reads as well: written
    /// only into a file that keeps a server in it already, so that the
    /// file's servers keep one shape.
    V1,
}

impl Shape {
    /// The key path of the server `name` in this shape.
    pub fn key(self, name: &str) -> Vec<&str> {
        match self {
            Shape::V2 => vec![MCP, SERVERS, name],
            Shape::V1 => vec![MCP, name],
        }
    }
}

/// What OpenCode is given of a definition: its entry, in the shape it is
/// written in, and what is said of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key it is written under, the definition's name.
    pub name: ArtifactName,
    /// Where the key is: under `mcp.servers`, or under `mcp`.
    pub shape: Shape,
    /// What the key holds.
    pub value: Value,
    /// What the entry is written without, in the order the definition
    /// holds it.
    pub warnings: Vec<Warning>,
}

impl Entry {
    /// The entry's canonical JSON form ([`Value::canonical`]): what its
    /// fingerprint is the digest of, for the ledger to keep (#93), its
    /// members sorted, as every fingerprint's are, so an entry somebody
    /// lays out again is still the one Mindweaver wrote.
    pub fn canonical(&self) -> String {
        self.value.canonical()
    }
}

/// The entry OpenCode is given for `definition` in `shape`, or why it is
/// given none.
///
/// Refused, before anything else, when `validate` would call the
/// definition invalid ([`artifact::mcp_strict_issues`]), so a token it
/// holds as it is never reaches a file; then for anything
/// [`McpDefinition::inexpressible`] says refuses OpenCode, a key its extras
/// for OpenCode give that the entry may write among them; and for an input
/// read from no variable, which no entry could be given but as a value,
/// and which that report does not say: it is no agent's limit, but a value
/// this is not handed yet (#93).
pub fn entry(definition: &McpDefinition, shape: Shape) -> Result<Entry, OpenCodeMcpError> {
    let name = definition.name.clone();
    let unfit = artifact::mcp_strict_issues(definition, &name, &name, &[]);
    if !unfit.is_empty() {
        return Err(OpenCodeMcpError::Unfit {
            name: name.to_string(),
            issues: unfit,
        });
    }
    let mut warnings = Vec::new();
    for found in definition.inexpressible(Agent::OpenCode) {
        if found.refuses() {
            return Err(OpenCodeMcpError::Inexpressible {
                name: name.to_string(),
                shape: found,
            });
        }
        warnings.push(Warning::Dropped {
            name: name.to_string(),
            shape: found,
        });
    }

    let mut read = Vec::new();
    for input in &definition.inputs {
        let Some(variable) = &input.env_var else {
            return Err(OpenCodeMcpError::InputUnread {
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

    // Each under its key in `keys`, in its order, which is what the report
    // holds OpenCode's extras to: `enabled` where OpenCode's docs write it,
    // after the command or the address.
    let enabled = shape == Shape::V1;
    let mut members: Vec<(&str, Value)> = Vec::new();
    match &definition.transport {
        Transport::Stdio(stdio) => {
            let mut command = vec![Value::from(stdio.command.as_str())];
            command.extend(stdio.args.iter().map(|arg| referred(arg)));
            members.push(("type", Value::from("local")));
            members.push(("command", Value::Array(command)));
            if enabled {
                members.push(("enabled", Value::Bool(true)));
            }
            if !stdio.env.is_empty() {
                members.push(("environment", table(&stdio.env)));
            }
            match (shape, &stdio.cwd) {
                (Shape::V2, Some(cwd)) => members.push(("cwd", Value::from(cwd.as_str()))),
                (Shape::V1, Some(_)) => warnings.push(Warning::FolderInV1 {
                    name: name.to_string(),
                }),
                (_, None) => {}
            }
        }
        // Server-sent events refused the entry above, as the report says.
        Transport::Http(endpoint) | Transport::Sse(endpoint) => {
            members.push(("type", Value::from("remote")));
            members.push(("url", referred(&endpoint.url)));
            if enabled {
                members.push(("enabled", Value::Bool(true)));
            }
            if !endpoint.headers.is_empty() {
                members.push(("headers", table(&endpoint.headers)));
            }
        }
    }
    let written = keys(definition.transport.kind());
    debug_assert!(members.iter().all(|(key, _)| written.contains(key)));
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    // Passed on as written, beside what the transport renders, never over
    // it: an extra holding a key the entry may write refused the entry above
    // ([`Inexpressible::ExtraRepeated`]). A TOML date and time, which JSON
    // has no value for, goes as the text it is written in.
    for (key, value) in definition.extras_for(Agent::OpenCode).into_iter().flatten() {
        members.push((key.clone(), Value::from(value)));
    }
    Ok(Entry {
        name,
        shape,
        value: Value::Object(members),
        warnings,
    })
}

/// The keys OpenCode's entry may be written with from a transport of
/// `kind`, in either shape, in the order written, each after the command or
/// the address only when it holds something.
///
/// What [`McpDefinition::inexpressible`] holds OpenCode's extras to, every
/// one of them whether or not this definition or this file's shape writes
/// it, so that what `show` says of a definition turns neither on which keys
/// it fills nor on which file it is written into
/// ([`Inexpressible::ExtraRepeated`]).
pub(crate) const fn keys(kind: TransportType) -> &'static [&'static str] {
    match kind {
        TransportType::Stdio => &["type", "command", "enabled", "environment", "cwd"],
        TransportType::Http | TransportType::Sse => &["type", "url", "enabled", "headers"],
    }
}

/// What is said of an entry written all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// Something of the definition the entry is written without
    /// ([`Inexpressible::refuses`] says it is no refusal).
    #[error("`{name}`: {shape}")]
    Dropped { name: String, shape: Inexpressible },
    /// A program's folder, which V1's shape has no key for: an entry written
    /// in it, into a file that keeps its servers so, is written without it,
    /// and the program is started where OpenCode starts it. Not the report's
    /// to say, since V2's shape, the one Mindweaver writes, has the key, and
    /// a definition is given V1's only for the file it goes into.
    #[error("`{name}`: the transport's `cwd` is left out, since OpenCode's V1 shape, which the file's servers are written in, has no key for it")]
    FolderInV1 { name: String },
}

codes!(Warning {
    Self::FolderInV1 { .. } => "opencode_mcp_v1_cwd",
} carried {
    Self::Dropped { shape, .. } => shape,
});

/// Why OpenCode is given no entry, or the file is not changed.
#[derive(Debug, Error)]
pub enum OpenCodeMcpError {
    #[error("`{name}` is written for no agent while it is invalid: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}` cannot be given to OpenCode: {shape}")]
    Inexpressible { name: String, shape: Inexpressible },
    #[error("`{name}` cannot be given to OpenCode: input `{input}` is read from no environment variable, and its entry could be given it only as its value")]
    InputUnread { name: String, input: InputName },
}

codes!(OpenCodeMcpError {
    Self::Unfit { .. } => "opencode_mcp_unfit",
    Self::InputUnread { .. } => "opencode_mcp_input_unread",
} carried {
    Self::Inexpressible { shape, .. } => shape,
});

/// `text` with each placeholder of an input in `read` written as
/// OpenCode's reference to the variable it is read from, `{env:VAR}`, and
/// every other byte as it is, a brace naming no input among them: an
/// `{env:` or a `{file:` among them refuses the entry before this is asked
/// ([`Inexpressible::TextSubstituted`]).
fn referring(text: &str, read: &[(&InputName, &VariableName)]) -> String {
    mcp::referring(text, read, |variable| format!("{{env:{variable}}}"))
}

#[cfg(test)]
#[path = "opencode_mcp_test.rs"]
mod tests;
