//! An MCP server's definition rendered into Claude Code's `.mcp.json`: one
//! entry, `mcpServers.<name>`, written and taken out again through
//! [`crate::config_entry`], every byte outside it kept (D-10).
//!
//! Claude reads a project's servers from `.mcp.json` at its root, a strict
//! JSON file, and loses every one of them to a comment or a trailing comma
//! ("MCP config is not valid JSON", #18's `claude-mcp-comment` and
//! `claude-mcp-comma` in `docs/agents.md`). So the file is read as
//! [`Format::Json`], and one only a looser reader takes is refused with a
//! code of its own ([`ClaudeMcpError::NotStrict`]), saying why, rather than
//! given an entry Claude would never load, or rewritten into one it would,
//! which is its owner's to do.
//!
//! The entry always says its `type`. An http entry somebody wrote without
//! one is dropped by Claude alone and costs the file nothing else
//! (`claude-mcp-untyped`), so it is no reason to refuse the file, and an
//! entry of Mindweaver's never is one. A program is `{type, command, args,
//! env}` and an address `{type, url, headers}`, each key written whether
//! or not it holds anything, so a definition has one entry, and one
//! canonical form, whatever it leaves empty.
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
//! rule here, and it is asked before the file is read: a shape that
//! refuses the agent refuses the entry, and one the entry is written
//! without (a program's `cwd`) is a [`Warning`]. Text Claude would read
//! as a reference where no input stands, `${` written as it is, is one
//! that refuses (`text_expanded`): Claude would give the server a
//! variable's value in its place, where a brace naming no input means
//! what it says.
//!
//! No approval is written. A person approves a project's servers when
//! Claude asks, and a `-p` run starts every server the file names, approved
//! or not (`claude-mcp`), so an entry runs there as soon as it is written.
//!
//! Nothing here knows of the ledger. Whether an entry of the name is
//! Mindweaver's to replace or take out, and whether Mindweaver made the
//! file, are the caller's, as its record says (#93, #94); and the caller
//! holds the workspace's lock, as one calling [`ConfigFile`] does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::Agent;
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::{self, ConfigFile, Edit, EntryError, FileError, Format, Value, Written};
use crate::mcp::{Inexpressible, McpDefinition, Transport, TransportType};
use crate::value::{ArtifactName, InputName, VariableName};

/// Where Claude reads a project's servers, from its root.
pub const FILE: &str = ".mcp.json";

/// The object in [`FILE`] that holds every server by its name.
pub const SERVERS: &str = "mcpServers";

/// The most a [`FILE`] is read for: 2 MiB.
///
/// A project's servers come to a few KiB, so a file past this is not one
/// somebody keeps servers in, and it would be held in memory whole, as
/// text, as a tree of every byte and as what an edit is checked against.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

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

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The project's [`FILE`].
    pub file: PathBuf,
    /// [`Written::Created`] when the file was not there and is made now,
    /// which the caller keeps, to hand back to [`remove`] as `created`.
    pub written: Written,
    pub entry: Entry,
}

/// The entry Claude is given for `definition`, or why it is given none.
///
/// Refused, before anything else, when `validate` would call the
/// definition invalid ([`artifact::mcp_strict_issues`]), so a token it
/// holds as it is never reaches a file; then for anything
/// [`McpDefinition::inexpressible`] says refuses Claude, a key its extras
/// for Claude give that the entry writes already among them; and for an
/// input read from no variable, which no entry could be given but as a
/// value, and which that report does not say: it is no agent's limit, but
/// a value this is not handed yet (#93).
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

    // Each under its key in `keys`, in its order, which is what the report
    // holds Claude's extras to.
    let values = match &definition.transport {
        Transport::Stdio(stdio) => vec![
            Value::from(definition.transport.slug()),
            Value::from(stdio.command.as_str()),
            Value::Array(stdio.args.iter().map(|arg| referred(arg)).collect()),
            table(&stdio.env),
        ],
        Transport::Http(endpoint) | Transport::Sse(endpoint) => vec![
            Value::from(definition.transport.slug()),
            referred(&endpoint.url),
            table(&endpoint.headers),
        ],
    };
    let written = keys(definition.transport.kind());
    debug_assert_eq!(written.len(), values.len());
    let mut members: Vec<(String, Value)> = written
        .iter()
        .map(|key| (*key).to_owned())
        .zip(values)
        .collect();
    match definition.transport {
        Transport::Sse(_) => warnings.push(Warning::ServerSentEvents {
            name: name.to_string(),
        }),
        Transport::Stdio(_) | Transport::Http(_) => {}
    }
    // Passed on as written, beside what the transport renders, never over
    // it: an extra holding a key the entry writes refused the entry above
    // ([`Inexpressible::ExtraRepeated`]). A TOML date and time, which JSON
    // has no value for, goes as the text it is written in.
    for (key, value) in definition.extras_for(Agent::Claude).into_iter().flatten() {
        members.push((key.clone(), Value::from(value)));
    }
    Ok(Entry {
        name,
        value: Value::Object(members),
        warnings,
    })
}

/// The keys Claude's entry is written with from a transport of `kind`, in
/// the order written, whether or not each holds anything.
///
/// What [`McpDefinition::inexpressible`] holds Claude's extras to, so that
/// one giving a key the entry writes is said wherever the report is, by
/// `show` too, rather than found only once an entry is made
/// ([`Inexpressible::ExtraRepeated`]).
pub(crate) const fn keys(kind: TransportType) -> &'static [&'static str] {
    match kind {
        TransportType::Stdio => &["type", "command", "args", "env"],
        TransportType::Http | TransportType::Sse => &["type", "url", "headers"],
    }
}

/// Write `definition`'s entry into the [`FILE`] at `root`, a project's
/// root, in place of one of the same name or after the servers already
/// there, and make the file when it is not there yet.
///
/// The entry is made first ([`entry`]), so a definition refused reads and
/// writes nothing. The file is read up to [`MAX_BYTES`], strictly, and
/// written through [`ConfigFile`], which keeps every byte outside the
/// entry, makes a new file with `create_new` and leaves one that already
/// holds the entry as it is.
pub fn render(root: &Path, definition: &McpDefinition) -> Result<Rendered, ClaudeMcpError> {
    let entry = entry(definition)?;
    let file = config(root)?;
    let key = [SERVERS, entry.name.as_str()];
    let edit = match file.get(&key).map_err(|error| loose(&file, error))? {
        None => Edit::Insert(entry.value.clone()),
        Some(_) => Edit::Replace(entry.value.clone()),
    };
    let written = file
        .edit(&key, &edit)
        .map_err(|error| loose(&file, error))?;
    Ok(Rendered {
        file: file.path(),
        written,
        entry,
    })
}

/// Take the entry keyed `name` out of the [`FILE`] at `root`, every other
/// byte as it was, and take the file away with it when `created`, that
/// Mindweaver made it, and nothing else is left in it: no other server, and
/// nothing beside [`SERVERS`].
///
/// [`Written::Removed`] when the file went, [`Written::Changed`] when the
/// entry did and the file stays, and [`Written::Unchanged`] when there was
/// no such entry, nor a file to take. A file Mindweaver made that somebody
/// has written in since is theirs too, and stays.
pub fn remove(root: &Path, name: &str, created: bool) -> Result<Written, ClaudeMcpError> {
    let file = config(root)?;
    // Taken whole when the entry is all it holds, rather than written
    // without it first only to be taken away.
    if created {
        let taken = file
            .take(|document| holds_only(document, name))
            .map_err(|error| loose(&file, error))?;
        if taken == Written::Removed {
            return Ok(taken);
        }
    }
    let key = [SERVERS, name];
    if file
        .get(&key)
        .map_err(|error| loose(&file, error))?
        .is_none()
    {
        return Ok(Written::Unchanged);
    }
    file.edit(&key, &Edit::Remove)
        .map_err(|error| loose(&file, error))
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

/// Why Claude is given no entry, or the file is not changed.
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
    #[error("{path}: not strict JSON ({detail}), which is how Claude Code reads it, loading none of the servers of a file that holds a comment or a trailing comma, so it was left as it is")]
    NotStrict { path: PathBuf, detail: String },
    #[error(transparent)]
    File(#[from] FileError),
}

codes!(ClaudeMcpError {
    Self::Unfit { .. } => "claude_mcp_unfit",
    Self::InputUnread { .. } => "claude_mcp_input_unread",
    Self::NotStrict { .. } => "mcp_json_not_strict",
} carried {
    Self::Inexpressible { shape, .. } => shape,
    Self::File(error) => error,
});

/// The project's [`FILE`], read strictly and up to [`MAX_BYTES`].
fn config(root: &Path) -> Result<ConfigFile, ClaudeMcpError> {
    Ok(ConfigFile::new(root, FILE, Format::Json)?.capped(MAX_BYTES))
}

/// `error`, said as [`ClaudeMcpError::NotStrict`] when the file it was
/// refused for is not strict JSON and would be but for comments and
/// trailing commas ([`ConfigFile::is_loose_json`]): a file somebody wrote
/// for a reader looser than Claude's, rather than one broken for every
/// reader, which a raw control character or a no-break space between its
/// tokens makes it, and which keeps the error it was refused with.
fn loose(file: &ConfigFile, error: FileError) -> ClaudeMcpError {
    let FileError::Entry {
        path,
        source:
            EntryError::Malformed {
                format: Format::Json,
                detail,
            },
    } = error
    else {
        return error.into();
    };
    // Read again for this alone: a read that fails now leaves the file
    // said as it was refused.
    if file.is_loose_json().unwrap_or(false) {
        ClaudeMcpError::NotStrict { path, detail }
    } else {
        ClaudeMcpError::File(FileError::Entry {
            path,
            source: EntryError::Malformed {
                format: Format::Json,
                detail,
            },
        })
    }
}

/// Whether `document` holds nothing but the server keyed `name`, or not
/// even that: nothing a person wrote beside an entry of Mindweaver's.
fn holds_only(document: &Value, name: &str) -> bool {
    config_entry::holds_only(document, &[SERVERS, name])
}

/// `text` with each placeholder of an input in `read` written as Claude's
/// reference to the variable it is read from, and every other byte as it
/// is, a brace naming no input among them: a `${` among them refuses the
/// entry before this is asked ([`Inexpressible::TextExpanded`]).
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

#[cfg(test)]
#[path = "claude_mcp_test.rs"]
mod tests;
