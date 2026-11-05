//! What `add` is told of an MCP server it writes a definition for: how an
//! agent starts the server or reaches it, as a command line's options say,
//! and the definition's text written from them.
//!
//! Every value is a name or a literal the definition holds as it is, as
//! #86's model has it. A literal is written as typed (`--env K=V`,
//! `--header K=V`), and judged as every value of a transport is, so one
//! shaped like a token is refused. Every other option declares an input,
//! read from the variable it names and never given a value (D-11), and
//! named after that variable, so its placeholder says which one: `{VAR}`.
//! Which of those is a secret is said here, once ([`Program`],
//! [`Address`]): what reaches a server through its environment or a
//! header is, and what goes into its arguments or its address is not,
//! being what a process list, a log or a proxy shows, and what Codex has
//! no way to read a secret into.
//!
//! Nothing here judges what it writes. The text is read back and held to
//! the one strict function `validate` and `rename` hold a definition to
//! ([`crate::create`]), so `add` refuses exactly what `validate` would call
//! invalid. What no definition can hold at all, two transports, or an
//! option of one given with the other, no value of this type can say: a
//! front end's grammar refuses it before one is made.

use std::fmt::Write as _;

use crate::mcp::written;
use crate::value::{InputName, VariableName};

/// How an agent starts an MCP server, or reaches one: a program or an
/// address, never both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Given {
    /// A program an agent starts, `--command`.
    Program(Program),
    /// A server at an address, `--url`.
    Address(Address),
}

/// A program an agent starts, and what it is started with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    /// The program, as the agent finds it (`--command`).
    pub command: String,
    /// What it is started with, in order (`--arg`), before every argument
    /// read from a variable.
    pub args: Vec<String>,
    /// What its environment is given, each value as written (`--env
    /// K=V`), before every secret.
    pub env: Vec<(String, String)>,
    /// Each variable passed on to its environment under its own name, a
    /// secret (`--secret-env VAR`): the shape every agent can give one,
    /// Codex's `env_vars` among them.
    pub secret_env: Vec<VariableName>,
    /// Each argument read from a variable, after those written as they are
    /// (`--arg-env VAR`). No secret: a program's arguments are what a
    /// process list shows, and Codex reads no secret into one.
    pub arg_env: Vec<VariableName>,
}

/// A server at an address, and what every request to it carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Address {
    /// Where the server is, as typed (`--url`): checked for a credential
    /// before anything is written, since the file would keep one, and
    /// never kept or said whole once one is found.
    pub url: String,
    /// Whether it is reached over server-sent events rather than streamable
    /// HTTP (`--sse`).
    pub events: bool,
    /// What every request carries, each value as written (`--header K=V`),
    /// before every secret.
    pub headers: Vec<(String, String)>,
    /// Each header whose whole value is read from a variable, a secret
    /// (`--secret-header H=VAR`): what Codex reads one into
    /// (`env_http_headers`).
    pub secret_headers: Vec<(String, VariableName)>,
    /// The variable `Authorization: Bearer` is followed by, a secret
    /// (`--bearer-env VAR`): what Codex reads one into
    /// (`bearer_token_env_var`).
    pub bearer: Option<VariableName>,
    /// Each part of the address read from a variable, by the name the
    /// address writes between braces where it goes (`--url-var NAME=VAR`).
    /// No secret: an address is what a log or a proxy shows, and Codex
    /// reads no secret into one.
    pub url_vars: Vec<(InputName, VariableName)>,
}

/// The header a bearer token is sent in.
const AUTHORIZATION: &str = "Authorization";

/// A key of a transport's table given more than once, which no TOML file
/// can hold: the table, as the definition names it, and the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeated {
    pub table: &'static str,
    pub key: String,
}

impl Given {
    /// The first key of `env` or `headers` given twice, by an option that
    /// writes a value and one that reads it, or by one option twice.
    ///
    /// Exactly as given: two headers that differ by case are two keys to
    /// the file, which holds them both, and to `validate`, which says
    /// nothing of them.
    pub fn repeated(&self) -> Option<Repeated> {
        let (table, keys): (&'static str, Vec<&str>) = match self {
            Given::Program(program) => (
                "env",
                program
                    .env
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .chain(program.secret_env.iter().map(VariableName::as_str))
                    .collect(),
            ),
            Given::Address(address) => (
                "headers",
                address
                    .headers
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .chain(address.secret_headers.iter().map(|(key, _)| key.as_str()))
                    .chain(address.bearer.iter().map(|_| AUTHORIZATION))
                    .collect(),
            ),
        };
        keys.iter().enumerate().find_map(|(at, key)| {
            keys[..at].contains(key).then(|| Repeated {
                table,
                key: (*key).to_owned(),
            })
        })
    }

    /// The text of a definition of `name`, described as `description`,
    /// written from what is given: a template of its own, its comments
    /// saying what each part is for, as a marker's say what each key is,
    /// and every value quoted as TOML quotes it, so that one holding a
    /// quote or a line break reads back as it was typed.
    ///
    /// A key given twice ([`repeated`](Self::repeated)) is written twice,
    /// so the text does not read: ask that first.
    pub fn text(&self, name: &str, description: &str) -> String {
        let mut text = String::from(
            "# An MCP server, as Mindweaver keeps it: each agent's own entry for it\n\
             # is written from this file, and `show` says how each agent is given it.\n",
        );
        let _ = writeln!(text, "format = {}", super::MCP_FORMAT);
        let _ = writeln!(text, "name = {}", quoted(name));
        let _ = writeln!(text, "description = {}", quoted(description));
        text.push('\n');
        let inputs = match self {
            Given::Program(program) => program.write(&mut text),
            Given::Address(address) => address.write(&mut text),
        };
        for (at, input) in inputs.iter().enumerate() {
            text.push('\n');
            if at == 0 {
                text.push_str(
                    "# What is supplied when an agent starts it, each written between braces\n\
                     # where it goes and read from the environment variable it names: no value\n\
                     # is written here, and a secret never.\n",
                );
            }
            input.write(&mut text);
        }
        text
    }
}

/// An input a definition declares, as `add` writes it.
struct Declared<'a> {
    name: &'a str,
    kind: &'static str,
    secret: bool,
    variable: &'a VariableName,
}

impl Declared<'_> {
    fn write(&self, text: &mut String) {
        text.push_str("[[inputs]]\n");
        let _ = writeln!(text, "name = {}", quoted(self.name));
        let _ = writeln!(text, "kind = {}", quoted(self.kind));
        if self.secret {
            text.push_str("secret = true\n");
        }
        let _ = writeln!(text, "env_var = {}", quoted(self.variable));
    }
}

impl Program {
    /// Its transport, written into `text`, and the inputs it declares.
    fn write<'a>(&'a self, text: &mut String) -> Vec<Declared<'a>> {
        text.push_str(
            "# How an agent starts it: the program, what it is started with, and what\n\
             # its environment is given beside the agent's own.\n\
             [transport]\n\
             type = \"stdio\"\n",
        );
        let _ = writeln!(text, "command = {}", quoted(&self.command));
        let args: Vec<String> = self
            .args
            .iter()
            .cloned()
            .chain(self.arg_env.iter().map(placeholder))
            .collect();
        if !args.is_empty() {
            let items: Vec<String> = args.iter().map(|arg| quoted(arg)).collect();
            let _ = writeln!(text, "args = [{}]", items.join(", "));
        }
        let env: Vec<(String, String)> = self
            .env
            .iter()
            .cloned()
            .chain(
                self.secret_env
                    .iter()
                    .map(|variable| (variable.to_string(), placeholder(variable))),
            )
            .collect();
        if !env.is_empty() {
            let _ = writeln!(text, "env = {}", inline(&env));
        }
        self.secret_env
            .iter()
            .map(|variable| Declared {
                name: variable.as_str(),
                kind: "env",
                secret: true,
                variable,
            })
            .chain(self.arg_env.iter().map(|variable| Declared {
                name: variable.as_str(),
                kind: "arg",
                secret: false,
                variable,
            }))
            .collect()
    }
}

impl Address {
    /// Its transport, written into `text`, and the inputs it declares.
    fn write<'a>(&'a self, text: &mut String) -> Vec<Declared<'a>> {
        let (over, word) = if self.events {
            ("over server-sent events", "sse")
        } else {
            ("over streamable HTTP", "http")
        };
        let _ = write!(
            text,
            "# Where an agent reaches it, {over}, and what every request to it\n\
             # carries.\n\
             [transport]\n\
             type = \"{word}\"\n"
        );
        let _ = writeln!(text, "url = {}", quoted(&self.url));
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .cloned()
            .chain(
                self.secret_headers
                    .iter()
                    .map(|(header, variable)| (header.clone(), placeholder(variable))),
            )
            .chain(self.bearer.iter().map(|variable| {
                (
                    AUTHORIZATION.to_owned(),
                    format!("Bearer {}", placeholder(variable)),
                )
            }))
            .collect();
        if !headers.is_empty() {
            let _ = writeln!(text, "headers = {}", inline(&headers));
        }
        self.secret_headers
            .iter()
            .map(|(_, variable)| Declared {
                name: variable.as_str(),
                kind: "header",
                secret: true,
                variable,
            })
            .chain(self.bearer.iter().map(|variable| Declared {
                name: variable.as_str(),
                kind: "header",
                secret: true,
                variable,
            }))
            .chain(self.url_vars.iter().map(|(name, variable)| Declared {
                name: name.as_str(),
                kind: "url_var",
                secret: false,
                variable,
            }))
            .collect()
    }
}

/// Where an input named after `variable` is written: `{VAR}`.
fn placeholder(variable: &VariableName) -> String {
    written(&InputName::from(variable.as_str()))
}

/// `text` as a TOML string, quoted and escaped as TOML writes one.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

/// `entries` as an inline table, in the order given: each key bare where a
/// TOML key may be, and quoted where a dot or a space would read as
/// another.
fn inline(entries: &[(String, String)]) -> String {
    let entries: Vec<String> = entries
        .iter()
        .map(|(key, value)| format!("{} = {}", bare_or_quoted(key), quoted(value)))
        .collect();
    format!("{{ {} }}", entries.join(", "))
}

/// `key` as TOML spells it: bare when it may be, quoted otherwise.
fn bare_or_quoted(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if bare {
        key.to_owned()
    } else {
        quoted(key)
    }
}

#[cfg(test)]
#[path = "given_test.rs"]
mod tests;
