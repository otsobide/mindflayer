//! A definition written out as the text of its file.
//!
//! What a definition made by core rather than typed by somebody is written
//! as, one imported from a registry first among them: every key the format
//! defines that it holds, in the order a file written by hand puts them,
//! with a comment above each part saying what it is for, as `add` writes
//! one ([`super::Given::text`]). Written from the parts rather than by
//! serializing the definition, which would write no comment, and would
//! write a table's keys where TOML puts them rather than where a reader
//! looks for them.
//!
//! What is written is what the definition keeps. An address is kept
//! without any credential its userinfo was typed with ([`Redacted`]), so a
//! definition read from a file with one is written without it, and the
//! inputs only its userinfo held are written nowhere.
//!
//! [`Redacted`]: crate::value::Redacted

use std::fmt::Write as _;

use super::given::{inline, quoted};
use super::{Input, McpDefinition, Transport, Upstream, MCP_FORMAT};

impl McpDefinition {
    /// The text of the definition's file: what [`McpDefinition::parse`]
    /// reads back as the definition it is.
    pub fn text(&self) -> String {
        let mut text = String::from(
            "# An MCP server, as Mindweaver keeps it: each agent's own entry for it\n\
             # is written from this file, and `show` says how each agent is given it.\n",
        );
        let _ = writeln!(text, "format = {MCP_FORMAT}");
        let _ = writeln!(text, "name = {}", quoted(&self.name));
        let _ = writeln!(text, "description = {}", quoted(&self.description));
        text.push('\n');
        transport(&self.transport, &mut text);
        for (at, each) in self.inputs.iter().enumerate() {
            text.push('\n');
            if at == 0 {
                text.push_str(
                    "# What is supplied when an agent starts it, each written between braces\n\
                     # where it goes and read from the environment variable it names: no value\n\
                     # is written here, and a secret never.\n",
                );
            }
            input(each, &mut text);
        }
        if let Some(upstream) = &self.provenance {
            text.push('\n');
            provenance(upstream, &mut text);
        }
        if !self.extras.is_empty() {
            let mut extras = toml::Table::new();
            extras.insert(
                String::from("extras"),
                toml::Value::Table(
                    self.extras
                        .iter()
                        .map(|(agent, table)| (agent.clone(), toml::Value::Table(table.clone())))
                        .collect(),
                ),
            );
            text.push_str(
                "\n# What each agent is given beside the entry written from the rest, by\n\
                 # the agent's name.\n",
            );
            // A table of tables, which TOML writes as one header each, the
            // agent's name in it: every key below it is the agent's.
            text.push_str(&toml::to_string(&extras).unwrap_or_default());
        }
        text
    }
}

/// `transport`, written into `text` as its table.
fn transport(transport: &Transport, text: &mut String) {
    match transport {
        Transport::Stdio(stdio) => {
            text.push_str(
                "# How an agent starts it: the program, what it is started with, and what\n\
                 # its environment is given beside the agent's own.\n\
                 [transport]\n\
                 type = \"stdio\"\n",
            );
            let _ = writeln!(text, "command = {}", quoted(&stdio.command));
            if !stdio.args.is_empty() {
                let items: Vec<String> = stdio.args.iter().map(|arg| quoted(arg)).collect();
                let _ = writeln!(text, "args = [{}]", items.join(", "));
            }
            if !stdio.env.is_empty() {
                let env: Vec<(String, String)> = stdio
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                let _ = writeln!(text, "env = {}", inline(&env));
            }
            if let Some(cwd) = &stdio.cwd {
                let _ = writeln!(text, "cwd = {}", quoted(cwd));
            }
        }
        Transport::Http(endpoint) | Transport::Sse(endpoint) => {
            let over = match transport {
                Transport::Sse(_) => "over server-sent events",
                Transport::Http(_) | Transport::Stdio(_) => "over streamable HTTP",
            };
            let _ = write!(
                text,
                "# Where an agent reaches it, {over}, and what every request to it\n\
                 # carries.\n\
                 [transport]\n\
                 type = \"{}\"\n",
                transport.slug()
            );
            let _ = writeln!(text, "url = {}", quoted(&endpoint.url));
            if !endpoint.headers.is_empty() {
                let headers: Vec<(String, String)> = endpoint
                    .headers
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                let _ = writeln!(text, "headers = {}", inline(&headers));
            }
        }
    }
}

/// `input`, written into `text` as one table of `inputs`, each key it
/// leaves at its default left out.
fn input(input: &Input, text: &mut String) {
    text.push_str("[[inputs]]\n");
    let _ = writeln!(text, "name = {}", quoted(&input.name));
    let _ = writeln!(text, "kind = {}", quoted(input.kind.slug()));
    if let Some(description) = &input.description {
        let _ = writeln!(text, "description = {}", quoted(description));
    }
    if input.required {
        text.push_str("required = true\n");
    }
    if input.secret {
        text.push_str("secret = true\n");
    }
    if let Some(variable) = &input.env_var {
        let _ = writeln!(text, "env_var = {}", quoted(variable));
    }
}

/// `upstream`, written into `text` as the `provenance` table.
fn provenance(upstream: &Upstream, text: &mut String) {
    text.push_str(
        "# Where it came from, as the registry it was imported from said it.\n\
         [provenance]\n",
    );
    if let Some(registry) = &upstream.registry {
        let _ = writeln!(text, "registry = {}", quoted(registry));
    }
    if let Some(name) = &upstream.upstream_name {
        let _ = writeln!(text, "upstream_name = {}", quoted(name));
    }
    if let Some(version) = &upstream.version {
        let _ = writeln!(text, "version = {}", quoted(version));
    }
    if let Some(fetched_at) = &upstream.fetched_at {
        // As the TOML date and time it reads as, the way a file written by
        // hand writes one, and quoted when it is not one.
        match fetched_at.parse::<toml::value::Datetime>() {
            Ok(moment) if moment.to_string() == *fetched_at => {
                let _ = writeln!(text, "fetched_at = {moment}");
            }
            _ => {
                let _ = writeln!(text, "fetched_at = {}", quoted(fetched_at));
            }
        }
    }
    if let Some(status) = &upstream.status {
        let _ = writeln!(text, "status = {}", quoted(status));
    }
}

#[cfg(test)]
#[path = "file_test.rs"]
mod tests;
