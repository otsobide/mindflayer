//! What each agent cannot be given of a definition as it is written: the
//! report a renderer asks before it writes an agent's entry (#90 to #92),
//! and `show` says of every agent.
//!
//! Not `validate`'s. A shape one agent cannot express says nothing wrong
//! with the definition, which every other agent is given whole: a server
//! that speaks only server-sent events is no less a server for Codex
//! having no way to reach it. So a definition is never called invalid for
//! what is here, nor warned of on every `validate`, which would say the
//! same thing of the same server for as long as it is kept, with nothing
//! its owner could change but the server. The renderer for the agent
//! refuses it, saying why in these words, or writes it without what the
//! agent has no place for.
//!
//! Each agent's shapes are a function matched on the agent with no
//! wildcard, so a new agent says what it cannot express before it
//! compiles. Each fact is docs-only, from D-11 and #87, the variables
//! Claude reads as empty toward a remote server from its binary
//! ([`super::blanked`]), and what Codex passes on to a program from its
//! documentation (#91), but what OpenCode fills in over its config's text,
//! `{env:}` as #18's runs saw it and `{file:}` as its documentation says
//! (#92); `docs/agents.md`'s "MCP servers" says what #18's runs on the
//! binaries moved, and what a later one would.
//!
//! Not an input read from no variable either, which a renderer refuses
//! (`claude_mcp_input_unread`) while no install hands it the value (#93):
//! every agent could be given that value as it is, so it is no agent's
//! limit, and `show` would say it of all three only until then.

use serde::Serialize;
use thiserror::Error;

use crate::agent::Agent;
use crate::claude_mcp;
use crate::code::{codes, Said};
use crate::codex_mcp;
use crate::mcp::{blanked, keyed, written, Input, InputKind, McpDefinition, Transport};
use crate::value::{InputName, VariableName};

/// Something a definition holds that one agent cannot be given as it is
/// written, said of the agent the report is for.
///
/// What the renderer for that agent is refused by, or leaves out
/// ([`refuses`](Self::refuses)), and so an error with a code of its own,
/// for the panel to tell one from another as it tells a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Inexpressible {
    /// A secret input written in a program's `args`: an agent with no
    /// interpolation, Codex, starts a program with its arguments as text,
    /// so the only way to give it one would be its value.
    #[error(
        "secret input `{input}` is written in the transport's `args`, which hold no reference \
         to an environment variable there"
    )]
    SecretInArgument { input: InputName },
    /// A secret input written in an address's `url`, which Codex reads as
    /// text as it reads an argument.
    #[error(
        "secret input `{input}` is written in the transport's `url`, which holds no reference \
         to an environment variable there"
    )]
    SecretInUrl { input: InputName },
    /// A secret input written in a header beside other text: Codex reads a
    /// header from a variable only as its whole value (`env_http_headers`),
    /// or as `Authorization: Bearer` and the token (`bearer_token_env_var`).
    #[error(
        "secret input `{input}` is written in header `{header}` beside other text, and a header \
         is read from an environment variable there only as its whole value, or as \
         `Authorization: Bearer` and the token"
    )]
    SecretHeaderComposite { header: String, input: InputName },
    /// A secret input written in a program's environment variable beside
    /// other text: Codex passes a variable on to a program only whole
    /// (`env_vars`), so the text around it has nowhere to go.
    #[error(
        "secret input `{input}` is written in environment variable `{key}` beside other text, \
         and a program is given a variable there only whole, under its own name"
    )]
    SecretEnvComposite { key: String, input: InputName },
    /// A secret input that is the whole value of a program's environment
    /// variable named otherwise than the variable it is read from: Codex
    /// passes a variable on under its own name (`env_vars`), so the program
    /// would find it there, and nothing under the name it looks for.
    #[error(
        "secret input `{input}` is written as environment variable `{key}`, and a program is \
         given a variable there only whole, under its own name, `{variable}`"
    )]
    SecretEnvRenamed {
        key: String,
        input: InputName,
        variable: VariableName,
    },
    /// Server-sent events, which the MCP specification has since replaced
    /// with streamable HTTP, and which neither Codex nor OpenCode's V2
    /// shape, the one Mindweaver writes (D-12), takes.
    #[error("the sse transport, server-sent events, is not one taken there")]
    ServerSentEvents,
    /// A program's folder, which Claude's entry has no key for: the program
    /// is started where Claude starts it, and the rest is written.
    #[error("the transport's `cwd` is left out, since a program is started there in no folder it is told")]
    WorkingDirectory,
    /// An input of an address read from a variable Claude reads as empty in
    /// a remote server's `url` and `headers`, a credential of its own or of
    /// a tool on the machine ([`super::blanked`]): written as a reference,
    /// it would reach the server with nothing in it.
    #[error(
        "input `{input}` is read from `{variable}`, which is read as empty there in a remote \
         server's `url` and `headers`, so that a credential it holds never reaches one"
    )]
    VariableBlanked {
        input: InputName,
        variable: VariableName,
    },
    /// `${` written as it is in a value Claude expands, where no input's
    /// placeholder stands: the opening of a reference to an environment
    /// variable there (`${VAR}`, `${VAR:-default}`), which would give the
    /// server the variable's value, a default or nothing in place of the
    /// text, where a brace naming no input means what it says.
    #[error(
        "`{at}` holds `${{` as it is, which opens a reference to an environment variable \
         there, so the text written in it cannot be given"
    )]
    TextExpanded { at: String },
    /// `{env:` or `{file:` written as it is in a value OpenCode is given,
    /// where no input's placeholder stands: OpenCode fills both in over its
    /// config file's text before it reads the file ([`SUBSTITUTED`]), a
    /// variable's value or a file's contents in place of the text, where a
    /// brace naming no input means what it says.
    #[error(
        "`{at}` holds `{opening}` as it is, which is filled in there over the file's text \
         before it is read, so the text written in it cannot be given"
    )]
    TextSubstituted { at: String, opening: &'static str },
    /// A key the agent's extras give that its entry is written with from
    /// the transport: passed on beside it, one of the two would be lost,
    /// and which one would be the order of a file's keys. Said of Claude
    /// and Codex, whose entries' keys are their renderers'
    /// (`claude_mcp::keys`, `codex_mcp::keys`).
    #[error(
        "its extras for the agent give `{key}`, a key the entry there is written with from the \
         transport, so one of the two would be lost"
    )]
    ExtraRepeated { key: String },
    /// A key the agent's extras give that is never written for it: Codex's
    /// `experimental_environment` and `environment_id`, by their names the
    /// environment it runs a server in, where an entry of Mindweaver's is
    /// for a server the project's own machine starts or reaches. Said of
    /// Codex (`codex_mcp::WITHHELD`).
    #[error(
        "its extras for the agent give `{key}`, a key for the environment it runs a server in, \
         which is never written there"
    )]
    ExtraWithheld { key: String },
}

codes!(Inexpressible {
    Self::SecretInArgument { .. } => "secret_in_argument",
    Self::SecretInUrl { .. } => "secret_in_url",
    Self::SecretHeaderComposite { .. } => "secret_header_composite",
    Self::SecretEnvComposite { .. } => "secret_env_composite",
    Self::SecretEnvRenamed { .. } => "secret_env_renamed",
    Self::ServerSentEvents => "sse_unsupported",
    Self::WorkingDirectory => "cwd_unsupported",
    Self::VariableBlanked { .. } => "variable_blanked",
    Self::TextExpanded { .. } => "text_expanded",
    Self::TextSubstituted { .. } => "text_substituted",
    Self::ExtraRepeated { .. } => "extra_repeated",
    Self::ExtraWithheld { .. } => "extra_withheld",
});

impl Inexpressible {
    /// Whether the agent is given nothing of the definition for it, rather
    /// than the definition without it: a secret it has no reference for,
    /// which only its value could stand for, and a transport it does not
    /// take. A folder left out leaves a server that starts all the same.
    ///
    /// Matched in full, so a new shape says which it is.
    pub fn refuses(&self) -> bool {
        match self {
            Inexpressible::SecretInArgument { .. }
            | Inexpressible::SecretInUrl { .. }
            | Inexpressible::SecretHeaderComposite { .. }
            | Inexpressible::SecretEnvComposite { .. }
            | Inexpressible::SecretEnvRenamed { .. }
            | Inexpressible::ServerSentEvents
            | Inexpressible::VariableBlanked { .. }
            | Inexpressible::TextExpanded { .. }
            | Inexpressible::TextSubstituted { .. }
            | Inexpressible::ExtraRepeated { .. }
            | Inexpressible::ExtraWithheld { .. } => true,
            Inexpressible::WorkingDirectory => false,
        }
    }
}

/// What one agent cannot be given of a definition, as `show` says it: the
/// agent, by the word a command line takes for it, then what refuses it
/// the definition and what it is given the definition without, each by
/// its code and in its words.
///
/// Both lists empty, and the agent is given the definition whole; one
/// thing refused, and it is given none of it, whatever else is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Expressibility {
    pub agent: Agent,
    /// What it cannot be given at all, in the order the definition holds
    /// it: any one of them, and its renderer writes it no entry.
    pub refused: Vec<Said>,
    /// What its entry is written without, in the order the definition
    /// holds it.
    pub dropped: Vec<Said>,
}

impl McpDefinition {
    /// What `agent` cannot be given of this definition as it is written,
    /// in the order the definition holds it: its transport, then each
    /// input in the order declared, then the agent's extras.
    ///
    /// What a renderer asks before it writes the agent's entry, and the
    /// one place each agent's limits are said.
    pub fn inexpressible(&self, agent: Agent) -> Vec<Inexpressible> {
        match agent {
            Agent::Claude => self.for_claude(),
            Agent::Codex => self.for_codex(),
            Agent::OpenCode => self.for_opencode(),
        }
    }

    /// What every agent cannot be given of this definition, one report for
    /// each, in report order ([`Agent::ALL`]).
    pub fn expressibility(&self) -> Vec<Expressibility> {
        Agent::ALL
            .into_iter()
            .map(|agent| {
                let (refused, dropped): (Vec<Inexpressible>, Vec<Inexpressible>) = self
                    .inexpressible(agent)
                    .into_iter()
                    .partition(Inexpressible::refuses);
                let said = |shapes: Vec<Inexpressible>| {
                    shapes
                        .iter()
                        .map(|shape| Said::of(shape.code(), shape))
                        .collect()
                };
                Expressibility {
                    agent,
                    refused: said(refused),
                    dropped: said(dropped),
                }
            })
            .collect()
    }

    /// Claude expands `${VAR}` in a program's command, arguments and
    /// environment and in an address and its headers (D-11), and reaches
    /// every transport: so it cannot be given text there that opens a
    /// reference as it is written, its entry for a program has no folder to
    /// start it in, toward an address it reads the credentials it keeps
    /// from one as empty, and its extras go beside the keys the entry
    /// writes, never over one.
    fn for_claude(&self) -> Vec<Inexpressible> {
        // Where an input stands, the renderer writes a reference of its
        // own, so text is held to this only around one; but not in a
        // program's command, which it writes as the definition holds it, no
        // input's kind being written there.
        let placeholders: Vec<String> = self
            .inputs
            .iter()
            .map(|input| written(&input.name))
            .collect();
        let expanded = |at: String, text: &str, placeholders: &[String]| {
            opens(text, placeholders, "${").then_some(Inexpressible::TextExpanded { at })
        };
        let mut shapes = Vec::new();
        match &self.transport {
            Transport::Stdio(stdio) => {
                shapes.extend(expanded(
                    String::from("transport.command"),
                    &stdio.command,
                    &[],
                ));
                for (index, arg) in stdio.args.iter().enumerate() {
                    shapes.extend(expanded(
                        format!("transport.args[{index}]"),
                        arg,
                        &placeholders,
                    ));
                }
                for (key, value) in &stdio.env {
                    shapes.extend(expanded(keyed("transport.env", key), value, &placeholders));
                }
                if stdio.cwd.is_some() {
                    shapes.push(Inexpressible::WorkingDirectory);
                }
            }
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                shapes.extend(expanded(
                    String::from("transport.url"),
                    endpoint.url.as_str(),
                    &placeholders,
                ));
                for (key, value) in &endpoint.headers {
                    shapes.extend(expanded(
                        keyed("transport.headers", key),
                        value,
                        &placeholders,
                    ));
                }
                shapes.extend(
                    self.inputs
                        .iter()
                        .filter(|input| match input.kind {
                            InputKind::Header | InputKind::UrlVar => true,
                            // Written nowhere in an address.
                            InputKind::Env | InputKind::Arg => false,
                        })
                        .filter_map(|input| {
                            let variable = input.env_var.as_ref()?;
                            blanked::by_claude(variable).then(|| Inexpressible::VariableBlanked {
                                input: input.name.clone(),
                                variable: variable.clone(),
                            })
                        }),
                );
            }
        }
        let entry = claude_mcp::keys(self.transport.kind());
        shapes.extend(
            self.extras_for(Agent::Claude)
                .into_iter()
                .flatten()
                .filter(|(key, _)| entry.contains(&key.as_str()))
                .map(|(key, _)| Inexpressible::ExtraRepeated { key: key.clone() }),
        );
        shapes
    }

    /// Codex has no interpolation: a secret reaches a server only through
    /// the keys that name a variable (`env_vars`, `env_http_headers`,
    /// `bearer_token_env_var`, D-11), each of which reads one only in a
    /// shape of its own ([`codex_mcp::passed_on`], [`codex_mcp::bearer`]),
    /// so a secret anywhere else, or in any other shape, has no spelling
    /// there; it does not take server-sent events; and its extras go beside
    /// the keys its entry writes, never over one, and never say where Codex
    /// runs the server.
    fn for_codex(&self) -> Vec<Inexpressible> {
        let mut shapes = Vec::new();
        if let Transport::Sse(_) = self.transport {
            shapes.push(Inexpressible::ServerSentEvents);
        }
        // By where each secret is written, not by its kind: a placeholder
        // stands for its input wherever it is written, so one of an
        // environment's written in an argument too is a secret argument.
        for input in self.inputs.iter().filter(|input| input.secret) {
            let placeholder = written(&input.name);
            match &self.transport {
                Transport::Stdio(stdio) => {
                    if stdio.args.iter().any(|arg| arg.contains(&placeholder)) {
                        shapes.push(Inexpressible::SecretInArgument {
                            input: input.name.clone(),
                        });
                    }
                    shapes.extend(self.unpassed_env(input));
                }
                Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                    // One its userinfo held is written in the address too,
                    // though no address is kept with it.
                    if endpoint.url.contains(&placeholder)
                        || endpoint.withheld.contains(&input.name)
                    {
                        shapes.push(Inexpressible::SecretInUrl {
                            input: input.name.clone(),
                        });
                    }
                    for header in self.composite_headers(&input.name) {
                        shapes.push(Inexpressible::SecretHeaderComposite {
                            header,
                            input: input.name.clone(),
                        });
                    }
                }
            }
        }
        let entry = codex_mcp::keys(self.transport.kind());
        shapes.extend(
            self.extras_for(Agent::Codex)
                .into_iter()
                .flatten()
                .filter_map(|(key, _)| {
                    let key = key.clone();
                    if entry.contains(&key.as_str()) {
                        Some(Inexpressible::ExtraRepeated { key })
                    } else if codex_mcp::WITHHELD.contains(&key.as_str()) {
                        Some(Inexpressible::ExtraWithheld { key })
                    } else {
                        None
                    }
                }),
        );
        shapes
    }

    /// Each of a program's environment variables that holds `input` and is
    /// not one Codex passes on: it gives a program a variable only whole,
    /// under its own name (`env_vars`), so `input` written beside other
    /// text, or as the whole value of a variable named otherwise than the
    /// one it is read from, has no spelling there. An input read from no
    /// variable at all is `validate`'s to call invalid, and none of this.
    fn unpassed_env(&self, input: &Input) -> Vec<Inexpressible> {
        let Transport::Stdio(stdio) = &self.transport else {
            return Vec::new();
        };
        let placeholder = written(&input.name);
        stdio
            .env
            .iter()
            .filter(|(_, value)| value.contains(&placeholder))
            .filter_map(|(key, value)| {
                if *value != placeholder {
                    return Some(Inexpressible::SecretEnvComposite {
                        key: key.clone(),
                        input: input.name.clone(),
                    });
                }
                let variable = input.env_var.as_ref()?;
                let passed = codex_mcp::passed_on(key, value, &placeholder, variable);
                (!passed).then(|| Inexpressible::SecretEnvRenamed {
                    key: key.clone(),
                    input: input.name.clone(),
                    variable: variable.clone(),
                })
            })
            .collect()
    }

    /// OpenCode fills `{env:VAR}` and `{file:path}` in over its config
    /// file's text before it reads the file (D-11), so it cannot be given
    /// text that opens either as it is written in any value its entry
    /// writes; and its V2 shape, the one Mindweaver writes (D-12), reaches a
    /// program or an address over streamable HTTP, never server-sent
    /// events.
    fn for_opencode(&self) -> Vec<Inexpressible> {
        // Where an input stands, the renderer writes a reference of its
        // own, so text is held to this only around one; but not in a
        // program's command or its folder, which it writes as the
        // definition holds them, no input's kind being written there.
        let placeholders: Vec<String> = self
            .inputs
            .iter()
            .map(|input| written(&input.name))
            .collect();
        let substituted = |at: String, text: &str, placeholders: &[String]| {
            SUBSTITUTED
                .into_iter()
                .find(|opening| opens(text, placeholders, opening))
                .map(|opening| Inexpressible::TextSubstituted { at, opening })
        };
        let mut shapes = Vec::new();
        if let Transport::Sse(_) = self.transport {
            shapes.push(Inexpressible::ServerSentEvents);
        }
        match &self.transport {
            Transport::Stdio(stdio) => {
                shapes.extend(substituted(
                    String::from("transport.command"),
                    &stdio.command,
                    &[],
                ));
                for (index, arg) in stdio.args.iter().enumerate() {
                    shapes.extend(substituted(
                        format!("transport.args[{index}]"),
                        arg,
                        &placeholders,
                    ));
                }
                for (key, value) in &stdio.env {
                    shapes.extend(substituted(
                        keyed("transport.env", key),
                        value,
                        &placeholders,
                    ));
                }
                if let Some(cwd) = &stdio.cwd {
                    shapes.extend(substituted(String::from("transport.cwd"), cwd, &[]));
                }
            }
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                shapes.extend(substituted(
                    String::from("transport.url"),
                    endpoint.url.as_str(),
                    &placeholders,
                ));
                for (key, value) in &endpoint.headers {
                    shapes.extend(substituted(
                        keyed("transport.headers", key),
                        value,
                        &placeholders,
                    ));
                }
            }
        }
        shapes
    }

    /// Every header, by its name as written, that holds `input` and is not
    /// a header Codex reads a secret into: the input as its whole value,
    /// or `Authorization` holding `Bearer` and the input, in either case,
    /// the first header so named ([`codex_mcp::bearer`]).
    fn composite_headers(&self, input: &InputName) -> Vec<String> {
        let (Transport::Http(endpoint) | Transport::Sse(endpoint)) = &self.transport else {
            return Vec::new();
        };
        let placeholder = written(input);
        endpoint
            .headers
            .iter()
            .filter(|(_, value)| value.contains(&placeholder))
            .filter(|(header, value)| {
                **value != placeholder
                    && !codex_mcp::bearer(&endpoint.headers, header, &placeholder)
            })
            .map(|(header, _)| header.clone())
            .collect()
    }
}

/// What OpenCode fills in over its config file's text before it reads it:
/// `{env:NAME}`, a variable's value, which #18's runs saw it do in the
/// command, the environment, the URL and the headers, and `{file:path}`, a
/// file's contents, its documentation's word.
const SUBSTITUTED: [&str; 2] = ["{env:", "{file:"];

/// Whether `text` holds `opening` anywhere but where one of `placeholders`
/// stands: what a reference an agent fills in opens with, written as it
/// is, Claude's `${` or one of OpenCode's [`SUBSTITUTED`].
///
/// Any `opening`, rather than only what an agent's pattern takes: for
/// Claude 2.1.318 (read from its binary) a name and then `}`, or `:-`, a
/// default and `}`. One that pattern passes over may be closed by a
/// reference the renderer writes after it (`${a:-{token}}`,
/// `{env:a{token}`), and a later agent may take more. Never one just
/// before a placeholder, which the renderer writes as the text and then
/// its reference: Claude's `$${VAR}`, the `$` and then the variable's
/// value.
fn opens(text: &str, placeholders: &[String], opening: &str) -> bool {
    // A placeholder holds no brace inside, so no two of them overlap, and
    // splitting at each in turn splits at all of them.
    let mut pieces = vec![text];
    for placeholder in placeholders {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| piece.split(placeholder.as_str()))
            .collect();
    }
    pieces.iter().any(|piece| piece.contains(opening))
}

#[cfg(test)]
#[path = "expressible_test.rs"]
mod tests;
