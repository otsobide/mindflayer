//! An MCP server's definition rendered into Codex's project config,
//! `.codex/config.toml`: one table, `[mcp_servers.<name>]`, written and
//! taken out again through [`crate::config_entry`], every byte outside it
//! kept, comments and the tables beside it included (D-10).
//!
//! Codex was never run here (the owner has no Codex subscription), so what
//! this rests on is its documentation's word, docs-only in
//! `docs/agents.md`, and #18's checks for it (`codex-mcp-*`) stand pending.
//!
//! Codex has no interpolation (D-11): what its config holds is the text it
//! means, `${VAR}` among it. A variable is named only by the keys that say
//! one, each in a shape of its own: `env_vars` passes a variable on to a
//! program whole and under its own name, `env_http_headers` reads a
//! header's whole value from one, and `bearer_token_env_var` the token of
//! `Authorization: Bearer`. So an input is written into one of those where
//! it stands in that shape ([`passed_on`], [`bearer`]), and every other
//! value as the definition holds it: `env` and `http_headers` with its own
//! text alone, and the `command`, `args`, `url` and `cwd` as written.
//!
//! What Codex cannot be given of a definition is
//! [`McpDefinition::inexpressible`]'s to say, never a second copy of its
//! rule here, and it is asked before anything else is: a secret anywhere
//! but in those shapes, and server-sent events, refuse the entry. The
//! shapes are this module's, as the keys of Claude's entry are
//! `claude_mcp`'s, and the report asks them of every place a secret is
//! written.
//!
//! An input that is no secret is written the same way, its variable read
//! where Codex reads one. Written anywhere else, it could be given only as
//! its value, which no install hands a renderer yet (#93), so it refuses
//! the entry with a code of the renderer's own
//! ([`CodexMcpError::InputInText`]), as one read from no variable does: it
//! is no limit of Codex's, and the report does not say it.
//!
//! Never `experimental_environment` nor `environment_id`, which by their
//! names say the environment Codex runs a server in ([`WITHHELD`]): the
//! transport writes neither, and extras giving one refuse the entry.
//!
//! Each key is written only when it holds something, since Codex reads a
//! key left out as empty and a `cwd` or a `bearer_token_env_var` has no
//! empty value to write: a definition still has one entry, and one
//! canonical form, the form whose digest the ledger will keep (#93), its
//! members sorted, so an entry Codex or a person writes again in another
//! layout is the one Mindweaver wrote.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::agent::Agent;
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::Value;
use crate::mcp::{self, Inexpressible, McpDefinition, Transport, TransportType};
use crate::value::{ArtifactName, InputName, VariableName};

/// Where Codex reads a project's servers, from its root.
pub const FILE: &str = ".codex/config.toml";

/// The table in [`FILE`] that holds every server by its name.
pub const SERVERS: &str = "mcp_servers";

/// What Codex is given of a definition: its table under [`SERVERS`], and
/// what is said of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key it is written under, the definition's name.
    pub name: ArtifactName,
    /// What the key holds.
    pub value: Value,
    /// What the entry is written without, in the order the definition
    /// holds it.
    pub warnings: Vec<Warning>,
}

impl Entry {
    /// The entry's canonical JSON form ([`Value::canonical`]): what its
    /// fingerprint is the digest of, for the ledger to keep (#93). Parsed
    /// and its keys sorted at every depth, since `codex mcp add` writes
    /// every entry it keeps again, and a person may write one inline or
    /// as tables: the same server however it is laid out.
    pub fn canonical(&self) -> String {
        self.value.canonical()
    }
}

/// The entry Codex is given for `definition`, or why it is given none.
///
/// Refused, before anything else, when `validate` would call the
/// definition invalid ([`artifact::mcp_strict_issues`]), so a token it
/// holds as it is never reaches a file; then for anything
/// [`McpDefinition::inexpressible`] says refuses Codex; and for an input
/// read from no variable, or one written where Codex reads none, which no
/// entry could be given but as its value, and which that report does not
/// say: it is no agent's limit, but a value this is not handed yet (#93).
pub fn entry(definition: &McpDefinition) -> Result<Entry, CodexMcpError> {
    let name = definition.name.clone();
    let unfit = artifact::mcp_strict_issues(definition, &name, &name, &[]);
    if !unfit.is_empty() {
        return Err(CodexMcpError::Unfit {
            name: name.to_string(),
            issues: unfit,
        });
    }
    let mut warnings = Vec::new();
    for shape in definition.inexpressible(Agent::Codex) {
        if shape.refuses() {
            return Err(CodexMcpError::Inexpressible {
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
            return Err(CodexMcpError::InputUnread {
                name: name.to_string(),
                input: input.name.clone(),
            });
        };
        read.push((&input.name, mcp::written(&input.name), variable));
    }
    // The first input `text` holds, in the order declared: a value holding
    // one is written only where Codex reads its variable.
    let held = |text: &str| {
        read.iter()
            .find(|(_, placeholder, _)| text.contains(placeholder.as_str()))
    };
    let in_text = |input: &InputName, at: String| CodexMcpError::InputInText {
        name: name.to_string(),
        input: input.clone(),
        at,
    };
    let text = |at: String, text: &str| match held(text) {
        Some((input, ..)) => Err(in_text(input, at)),
        None => Ok(Value::from(text)),
    };

    // Each under its key in `keys`, in its order, which is what the report
    // holds Codex's extras to.
    let mut members: Vec<(&str, Value)> = Vec::new();
    match &definition.transport {
        Transport::Stdio(stdio) => {
            let args = stdio
                .args
                .iter()
                .enumerate()
                .map(|(index, arg)| text(format!("transport.args[{index}]"), arg))
                .collect::<Result<Vec<_>, _>>()?;
            let (mut env, mut passed) = (Vec::new(), Vec::new());
            for (key, value) in &stdio.env {
                match held(value) {
                    None => env.push((key.clone(), Value::from(value.as_str()))),
                    Some((_, placeholder, variable))
                        if passed_on(key, value, placeholder, variable) =>
                    {
                        passed.push(Value::from(variable.as_str()));
                    }
                    Some((input, ..)) => {
                        return Err(in_text(input, mcp::keyed("transport.env", key)))
                    }
                }
            }
            members.push(("command", Value::from(stdio.command.as_str())));
            if !args.is_empty() {
                members.push(("args", Value::Array(args)));
            }
            if !env.is_empty() {
                members.push(("env", Value::Object(env)));
            }
            if !passed.is_empty() {
                members.push(("env_vars", Value::Array(passed)));
            }
            if let Some(cwd) = &stdio.cwd {
                members.push(("cwd", Value::from(cwd.as_str())));
            }
        }
        Transport::Http(endpoint) | Transport::Sse(endpoint) => {
            let url = text(String::from("transport.url"), &endpoint.url)?;
            let (mut literal, mut whole, mut token) = (Vec::new(), Vec::new(), None);
            for (header, value) in &endpoint.headers {
                match held(value) {
                    None => literal.push((header.clone(), Value::from(value.as_str()))),
                    Some((_, placeholder, variable)) if value == placeholder => {
                        whole.push((header.clone(), Value::from(variable.as_str())));
                    }
                    Some((_, placeholder, variable))
                        if bearer(&endpoint.headers, header, placeholder) =>
                    {
                        token = Some(Value::from(variable.as_str()));
                    }
                    Some((input, ..)) => {
                        return Err(in_text(input, mcp::keyed("transport.headers", header)))
                    }
                }
            }
            members.push(("url", url));
            if !literal.is_empty() {
                members.push(("http_headers", Value::Object(literal)));
            }
            if !whole.is_empty() {
                members.push(("env_http_headers", Value::Object(whole)));
            }
            if let Some(token) = token {
                members.push(("bearer_token_env_var", token));
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
    // it: an extra holding a key the entry writes refused the entry above
    // ([`Inexpressible::ExtraRepeated`]).
    for (key, value) in definition.extras_for(Agent::Codex).into_iter().flatten() {
        members.push((key.clone(), Value::from(value)));
    }
    Ok(Entry {
        name,
        value: Value::Object(members),
        warnings,
    })
}

/// The keys Codex's entry may be written with from a transport of `kind`,
/// in the order written, each only when it holds something.
///
/// What [`McpDefinition::inexpressible`] holds Codex's extras to, every
/// one of them whether or not this definition writes it, so that what
/// `show` says of a definition does not turn on which keys it fills
/// ([`Inexpressible::ExtraRepeated`]).
pub(crate) const fn keys(kind: TransportType) -> &'static [&'static str] {
    match kind {
        TransportType::Stdio => &["command", "args", "env", "env_vars", "cwd"],
        TransportType::Http | TransportType::Sse => &[
            "url",
            "http_headers",
            "env_http_headers",
            "bearer_token_env_var",
        ],
    }
}

/// Keys of Codex's for the environment it runs a server in, which are never
/// written: not by the transport, which has nothing to say of one, and not
/// from the extras either, which the report refuses for them
/// ([`Inexpressible::ExtraWithheld`]). An entry of Mindweaver's is for a
/// server the project's own machine starts or reaches, as every other
/// agent's is, and one Codex ran elsewhere would be a server no other agent
/// is given.
pub(crate) const WITHHELD: &[&str] = &["experimental_environment", "environment_id"];

/// Whether the program's environment variable `key`, holding `value`, is
/// one `env_vars` gives the program from `variable`, the input written
/// `placeholder` is read from: only the placeholder whole, under the
/// variable's own name, since `env_vars` names a variable it passes on as
/// it is and nothing else.
///
/// The rule the report holds a secret of a program's environment to, as
/// well as where the renderer writes an input's variable.
pub(crate) fn passed_on(
    key: &str,
    value: &str,
    placeholder: &str,
    variable: &VariableName,
) -> bool {
    value == placeholder && key == variable.as_str()
}

/// Whether `header` of `headers` is `Authorization: Bearer` and the input
/// written `placeholder`, which `bearer_token_env_var` reads from the
/// input's variable: its name and its scheme in either case, as HTTP reads
/// them, and the first header so named, since the one key writes one
/// header and a second would have nowhere to go.
///
/// The rule the report holds a secret header to beside a whole value, as
/// well as where the renderer writes an input's variable.
pub(crate) fn bearer(headers: &BTreeMap<String, String>, header: &str, placeholder: &str) -> bool {
    let first = headers
        .keys()
        .find(|name| name.eq_ignore_ascii_case("authorization"));
    first.is_some_and(|first| first == header)
        && headers.get(header).is_some_and(|value| {
            value.split_once(' ').is_some_and(|(scheme, token)| {
                scheme.eq_ignore_ascii_case("bearer") && token == placeholder
            })
        })
}

/// What is said of an entry written all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// Something of the definition the entry is written without
    /// ([`Inexpressible::refuses`] says it is no refusal).
    #[error("`{name}`: {shape}")]
    Dropped { name: String, shape: Inexpressible },
}

codes!(Warning {
} carried {
    Self::Dropped { shape, .. } => shape,
});

/// Why Codex is given no entry, or the file is not changed.
#[derive(Debug, Error)]
pub enum CodexMcpError {
    #[error("`{name}` is written for no agent while it is invalid: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}` cannot be given to Codex: {shape}")]
    Inexpressible { name: String, shape: Inexpressible },
    #[error("`{name}` cannot be given to Codex: input `{input}` is read from no environment variable, and its entry could be given it only as its value")]
    InputUnread { name: String, input: InputName },
    #[error("`{name}` cannot be given to Codex: input `{input}` is written in `{at}`, where Codex reads no environment variable into it, so its entry could be given it there only as its value")]
    InputInText {
        name: String,
        input: InputName,
        at: String,
    },
}

codes!(CodexMcpError {
    Self::Unfit { .. } => "codex_mcp_unfit",
    Self::InputUnread { .. } => "codex_mcp_input_unread",
    Self::InputInText { .. } => "codex_mcp_input_in_text",
} carried {
    Self::Inexpressible { shape, .. } => shape,
});

#[cfg(test)]
#[path = "codex_mcp_test.rs"]
mod tests;
