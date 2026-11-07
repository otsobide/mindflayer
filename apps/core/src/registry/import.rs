//! A server version the registry gives, mapped to a definition (#106).
//!
//! The registry says how a server is started, a package of a registry
//! type, or reached, a remote at an address, each with what somebody
//! supplies for it; a definition says the same of one transport (D-10). So
//! the first package that maps to a definition becomes a program an agent
//! starts, in the order the registry lists them, and failing every one the
//! first remote that maps to one becomes an address: a program first,
//! since every agent is given one, where two refuse a server reached over
//! server-sent events. What no definition can say, a package of a type
//! nothing here starts, is a manual step ([`Manual`]), coded, never guessed
//! at; what is mapped with something left out is said beside it
//! ([`Warning`]).
//!
//! Every value somebody supplies is an input read from an environment
//! variable, and never a value the definition holds (D-11): an environment
//! variable or a header the registry gives no value for is one whole, read
//! from a variable of its name, and a `{variable}` in a value the registry
//! does give is one where it stands. A secret stays one (`isSecret`), and
//! nothing the registry gives for a secret is written: a default, an
//! example (`placeholder`), or a fixed value, each left out and said. A
//! default or an example of what is no secret is left out unsaid, since
//! format 1 has no key for either and neither is a value anybody keeps.
//!
//! What the mapping makes is held to the strict function every definition
//! Mindweaver writes is held to, its name to the names a server may be
//! given and the rest to every problem `validate` reports, and one that
//! falls short is no definition but a manual step saying why, the next
//! package or remote tried: a token the registry wrote as a value that is
//! no secret is refused there, as `add` refuses it. What it makes is
//! written into a library by [`crate::lifecycle::import`] (#107).

use std::collections::BTreeMap;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

use super::server::{self, Argument, Package};
use super::ServerVersion;
use crate::artifact::{issue_list, mcp_name_strict_issues, ValidationIssue};
use crate::code::{codes, Said};
use crate::mcp::{written, Endpoint, Input, InputKind, McpDefinition, Stdio, Transport, Upstream};
use crate::skill::MAX_NAME_SEGMENT_LEN;
use crate::text::{printable, Utc};
use crate::value::{ArtifactName, InputName, Redacted, VariableName};

/// What a server version maps to: a definition, or the manual steps that
/// stand in its place, and what was said on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The definition, `None` when nothing the registry gives maps to one.
    pub definition: Option<McpDefinition>,
    /// What somebody does by hand in its place: each package and remote
    /// that could not be mapped, or why what was mapped is no definition.
    /// Empty whenever there is a definition.
    pub manual: Vec<Manual>,
    /// What was left out of the definition, or is worth knowing of it.
    pub warnings: Vec<Warning>,
}

/// By hand: the definition as the text its file would hold, and what is
/// said beside it as [`Said`]s, its code and its message.
impl Serialize for Imported {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut imported = serializer.serialize_struct("Imported", 3)?;
        imported.serialize_field(
            "definition",
            &self.definition.as_ref().map(McpDefinition::text),
        )?;
        imported.serialize_field(
            "manual",
            &self
                .manual
                .iter()
                .map(|step| Said::of(step.code(), step))
                .collect::<Vec<_>>(),
        )?;
        imported.serialize_field(
            "warnings",
            &self
                .warnings
                .iter()
                .map(|warning| Said::of(warning.code(), warning))
                .collect::<Vec<_>>(),
        )?;
        imported.end()
    }
}

impl ServerVersion {
    /// This server version as a definition named `name`, or named after
    /// the last part of what the registry names it when no name is given
    /// (`weather` for `io.github.acme/weather`).
    pub fn import(&self, name: Option<&str>) -> Imported {
        let name = name.map_or_else(|| suggested(&self.name), str::to_owned);
        // Every way of starting it is given the one name, so a name no
        // server may be given is said once, before any is tried.
        let named = mcp_name_strict_issues(&name, &[]);
        if !named.is_empty() {
            return Imported {
                definition: None,
                manual: vec![Manual::NameUnfit {
                    name: printable(&name),
                    issues: named,
                }],
                warnings: Vec::new(),
            };
        }
        let packages = self
            .server
            .packages
            .iter()
            .enumerate()
            .map(|(at, each)| package(at, each));
        let remotes = self
            .server
            .remotes
            .iter()
            .enumerate()
            .map(|(at, each)| remote(at, each));
        let mut manual = Vec::new();
        for way in packages.chain(remotes) {
            match way.and_then(|drafted| self.held(&name, drafted)) {
                Ok((definition, warnings)) => {
                    return Imported {
                        definition: Some(definition),
                        manual: Vec::new(),
                        warnings,
                    }
                }
                Err(step) => manual.push(step),
            }
        }
        if manual.is_empty() {
            manual.push(Manual::Unrunnable);
        }
        Imported {
            definition: None,
            manual,
            warnings: Vec::new(),
        }
    }

    /// `drafted`, named `name`, given its description and where it came
    /// from, and held to every problem `validate` reports: one it would
    /// call invalid is a step, and the next way of starting the server is
    /// tried.
    fn held(&self, name: &str, drafted: Drafted) -> Result<(McpDefinition, Vec<Warning>), Manual> {
        let definition = McpDefinition {
            name: ArtifactName::from(name),
            description: self.description.clone(),
            transport: drafted.transport,
            inputs: drafted.inputs,
            provenance: Some(Upstream {
                registry: Some(self.registry.clone()),
                upstream_name: Some(self.name.clone()),
                version: Some(self.version.clone()),
                fetched_at: Some(Utc::of(self.fetched_at).to_string()),
                status: self.status.as_ref().map(|status| status.slug().to_owned()),
            }),
            extras: BTreeMap::new(),
        };
        let issues: Vec<ValidationIssue> = definition
            .issues()
            .into_iter()
            .filter(|issue| !issue.is_warning())
            .collect();
        if issues.is_empty() {
            Ok((definition, drafted.warnings))
        } else {
            Err(Manual::Invalid {
                at: drafted.at,
                issues,
            })
        }
    }
}

/// A name a server may be given, made of the last part of what the
/// registry names it: every character no agent keys a server by made a
/// hyphen, hyphens run together, none at either end, and no longer than
/// every name may be. Held to the rules like any other once made, so one
/// an agent keeps for itself is refused, and given another by whoever
/// imports it.
fn suggested(upstream: &str) -> String {
    let last = upstream.rsplit('/').next().unwrap_or(upstream);
    let mut name = String::new();
    for c in last.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            c
        } else {
            '-'
        };
        if !(c == '-' && name.ends_with('-')) {
            name.push(c);
        }
    }
    let cut: String = name
        .trim_matches('-')
        .chars()
        .take(MAX_NAME_SEGMENT_LEN)
        .collect();
    cut.trim_end_matches('-').to_owned()
}

/// What one package or remote maps to, before it is named.
#[derive(Debug, Clone)]
struct Drafted {
    /// Where the package or the remote is in the registry's `server.json`.
    at: String,
    transport: Transport,
    inputs: Vec<Input>,
    warnings: Vec<Warning>,
}

/// How a package of a registry type is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runtime {
    Npm,
    Pypi,
    Oci,
    Nuget,
}

impl Runtime {
    /// The program, and what it is started with before anything else.
    fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Runtime::Npm => ("npx", &["-y"]),
            Runtime::Pypi => ("uvx", &[]),
            Runtime::Oci => ("docker", &["run", "-i", "--rm"]),
            Runtime::Nuget => ("dnx", &[]),
        }
    }

    /// The host of the registry the program reads packages from: a package
    /// of another registry is not one the program would start, but one of
    /// the same name there. `None` for an image, whose registry is named in
    /// its reference.
    fn reads_from(self) -> Option<&'static str> {
        match self {
            Runtime::Npm => Some("registry.npmjs.org"),
            Runtime::Pypi => Some("pypi.org"),
            Runtime::Nuget => Some("api.nuget.org"),
            Runtime::Oci => None,
        }
    }

    /// The package `identifier` at `version`, as the program takes it.
    fn spec(self, identifier: &str, version: Option<&str>, base: Option<&str>) -> String {
        match (self, version) {
            (Runtime::Npm | Runtime::Nuget, Some(version)) => format!("{identifier}@{version}"),
            (Runtime::Pypi, Some(version)) => format!("{identifier}=={version}"),
            (Runtime::Npm | Runtime::Pypi | Runtime::Nuget, None) => identifier.to_owned(),
            (Runtime::Oci, _) => image(identifier, version, base),
        }
    }
}

/// An image's reference, as `docker run` takes it: `identifier` as it is
/// when it names its registry and its tag, as the 2025-12-11 schema writes
/// one; otherwise, as the 2025-09-29 schema wrote one, the registry `base`
/// names put before it and `version` after it as its tag. A registry left
/// out would have `docker` pull an image of the same name from Docker Hub.
fn image(identifier: &str, version: Option<&str>, base: Option<&str>) -> String {
    let names_registry = identifier.split_once('/').is_some_and(|(first, _)| {
        first.contains('.') || first.contains(':') || first == "localhost"
    });
    let mut image = identifier.to_owned();
    if !names_registry {
        if let Some(host) = base.and_then(host_of) {
            image = format!("{host}/{image}");
        }
    }
    let last = image.rsplit('/').next().unwrap_or(&image);
    let tagged = image.contains('@') || last.contains(':');
    if let (false, Some(version)) = (tagged, version) {
        image = format!("{image}:{version}");
    }
    image
}

/// The host of `url`, and its port when it names one.
fn host_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// Whether `text` can be one word of a command line as it is: something,
/// not taken for an option, and neither split nor ended by what it holds.
fn fit(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('-')
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The package at `packages[at]`, as a program an agent starts.
fn package(at: usize, package: &Package) -> Result<Drafted, Manual> {
    let at = format!("packages[{at}]");
    let identifier = Redacted::new(&printable(&package.identifier));
    let runtime = match package.registry_type.as_str() {
        "npm" => Runtime::Npm,
        "pypi" => Runtime::Pypi,
        "oci" => Runtime::Oci,
        "nuget" => Runtime::Nuget,
        "mcpb" => return Err(Manual::Mcpb { at, identifier }),
        "cargo" => return Err(Manual::Cargo { at, identifier }),
        other => {
            return Err(Manual::TypeUnknown {
                at,
                registry_type: Redacted::new(&printable(other)),
                identifier,
            })
        }
    };
    if let Some(transport) = package
        .transport
        .as_ref()
        .filter(|transport| !transport.kind.is_empty() && transport.kind != "stdio")
    {
        return Err(Manual::NotStdio {
            at,
            transport: Redacted::new(&printable(&transport.kind)),
        });
    }
    let unfit = |at: &str, what, text: &str| Manual::Unfit {
        at: at.to_owned(),
        what,
        text: Redacted::new(&printable(text)),
    };
    if !fit(&package.identifier) {
        return Err(unfit(&at, "identifier", &package.identifier));
    }
    if let Some(version) = package.version.as_deref().filter(|version| !fit(version)) {
        return Err(unfit(&at, "version", version));
    }
    let base = package.registry_base_url.as_deref();
    if let (Some(reads_from), Some(base)) = (runtime.reads_from(), base) {
        if host_of(base).as_deref() != Some(reads_from) {
            return Err(Manual::Elsewhere {
                at,
                registry_type: printable(&package.registry_type),
                base: Redacted::new(&printable(base)),
                runtime: runtime.command().0,
            });
        }
    }

    let (command, first) = runtime.command();
    let mut args: Vec<String> = first.iter().map(|word| (*word).to_owned()).collect();
    let mut draft = Draft::default();
    let mut env = BTreeMap::new();
    for (index, variable) in package.environment_variables.iter().enumerate() {
        let at = format!("{at}.environmentVariables[{index}]");
        let name = &variable.name;
        // A name `docker run -e` would read a value out of, and one no
        // environment holds.
        if !fit(name) || name.contains('=') {
            return Err(unfit(&at, "environment variable's name", name));
        }
        let read_from = Some(VariableName::from(name.as_str())).filter(VariableName::is_portable);
        let value = draft.valued(&variable.input, InputKind::Env, name, read_from, &at);
        // A container is given none of `docker`'s environment but what it
        // is told to pass on, each by its name alone, so that its value is
        // read from `docker`'s, which the agent sets, and never written in
        // an argument.
        if runtime == Runtime::Oci {
            args.extend([String::from("-e"), name.clone()]);
        }
        env.insert(name.clone(), value);
    }
    draft.arguments(
        &package.runtime_arguments,
        &mut args,
        &format!("{at}.runtimeArguments"),
    );
    args.push(runtime.spec(&package.identifier, package.version.as_deref(), base));
    draft.arguments(
        &package.package_arguments,
        &mut args,
        &format!("{at}.packageArguments"),
    );
    Ok(Drafted {
        at,
        transport: Transport::Stdio(Stdio {
            command: command.to_owned(),
            args,
            env,
            cwd: None,
        }),
        inputs: draft.inputs,
        warnings: draft.warnings,
    })
}

/// The remote at `remotes[at]`, as an address an agent reaches.
fn remote(at: usize, remote: &server::Transport) -> Result<Drafted, Manual> {
    let at = format!("remotes[{at}]");
    let events = match remote.kind.as_str() {
        "streamable-http" => false,
        "sse" => true,
        other => {
            return Err(Manual::RemoteUnknown {
                at,
                transport: Redacted::new(&printable(other)),
            })
        }
    };
    let mut draft = Draft::default();
    if events {
        draft.warnings.push(Warning::Sse { at: at.clone() });
    }
    let (url, _) = draft.templated(
        remote.url.as_deref().unwrap_or_default(),
        &remote.variables,
        InputKind::UrlVar,
        false,
        &format!("{at}.url"),
    );
    let mut headers = BTreeMap::new();
    for (index, header) in remote.headers.iter().enumerate() {
        let at = format!("{at}.headers[{index}]");
        let value = draft.valued(&header.input, InputKind::Header, &header.name, None, &at);
        headers.insert(printable(&header.name), value);
    }
    let endpoint = Endpoint::read(&url, headers, &draft.inputs);
    Ok(Drafted {
        at,
        transport: if events {
            Transport::Sse(endpoint)
        } else {
            Transport::Http(endpoint)
        },
        inputs: draft.inputs,
        warnings: draft.warnings,
    })
}

/// The inputs one transport declares, and what is said of them.
#[derive(Debug, Default)]
struct Draft {
    inputs: Vec<Input>,
    warnings: Vec<Warning>,
}

impl Draft {
    /// Declare `input`, once: a variable the registry names in two values
    /// of one place is one input, a secret when either says it is and
    /// required when either does. Merged rather than the first kept, or the
    /// order the registry lists them in would decide whether a secret is
    /// one.
    fn declare(&mut self, input: Input) {
        match self
            .inputs
            .iter_mut()
            .find(|each| each.name == input.name && each.kind == input.kind)
        {
            Some(declared) => {
                declared.secret |= input.secret;
                declared.required |= input.required;
                if declared.description.is_none() {
                    declared.description = input.description;
                }
            }
            None => self.inputs.push(input),
        }
    }

    /// The value the registry's `spec` is written as where it is `at`, of
    /// `kind`: its `value` with each variable in it an input, or, when it
    /// gives none, an input named `name` whole, read from `variable` or a
    /// variable named after it.
    fn valued(
        &mut self,
        spec: &server::Input,
        kind: InputKind,
        name: &str,
        variable: Option<VariableName>,
        at: &str,
    ) -> String {
        self.left_out(spec, spec.is_secret, at);
        if let Some(value) = &spec.value {
            let (text, declared) = self.templated(value, &spec.variables, kind, spec.is_secret, at);
            if !spec.is_secret || declared {
                return text;
            }
            // A secret the registry gives the whole of: what no file
            // holds, read from the environment as one given no value is.
            self.warnings.push(Warning::SecretDropped {
                at: at.to_owned(),
                what: "value",
            });
        }
        let input = InputName::from(input_name(name));
        self.declare(Input {
            name: input.clone(),
            kind,
            description: described(spec),
            required: spec.is_required,
            secret: spec.is_secret,
            env_var: Some(variable.unwrap_or_else(|| variable_for(name))),
        });
        written(&input)
    }

    /// `text`, a value the registry writes `{variables}` in, with each of
    /// `variables` it names declared an input of `kind` where it stands, a
    /// secret when it says so or `secret` does, or written as its fixed
    /// value when it has one and is no secret. Whether any was declared.
    fn templated(
        &mut self,
        text: &str,
        variables: &BTreeMap<String, server::Input>,
        kind: InputKind,
        secret: bool,
        at: &str,
    ) -> (String, bool) {
        let mut text = printable(text);
        let mut declared = false;
        for (name, variable) in variables {
            let placeholder = format!("{{{name}}}");
            if !text.contains(&placeholder) {
                continue;
            }
            let at = format!("{at}.variables.{}", printable(name));
            let secret = secret || variable.is_secret;
            self.left_out(variable, secret, &at);
            match &variable.value {
                Some(value) if !secret => {
                    text = text.replace(&placeholder, &printable(value));
                    continue;
                }
                Some(_) => self.warnings.push(Warning::SecretDropped {
                    at: at.clone(),
                    what: "value",
                }),
                None => {}
            }
            self.declare(Input {
                name: InputName::from(name.as_str()),
                kind,
                description: described(variable),
                required: variable.is_required,
                secret,
                env_var: Some(variable_for(name)),
            });
            declared = true;
        }
        (text, declared)
    }

    /// Say a secret's default and its example left out, each where it is.
    fn left_out(&mut self, spec: &server::Input, secret: bool, at: &str) {
        if !secret {
            return;
        }
        for (what, given) in [
            ("default", &spec.default),
            ("placeholder", &spec.placeholder),
        ] {
            if given.is_some() {
                self.warnings.push(Warning::SecretDropped {
                    at: at.to_owned(),
                    what,
                });
            }
        }
    }

    /// Each of `arguments` written into `args`: a positional one as its
    /// value, a named one as its flag and its value; one the registry gives
    /// no value for an input when it is required, and left out, said, when
    /// it is not, since the server starts without it.
    fn arguments(&mut self, arguments: &[Argument], args: &mut Vec<String>, at: &str) {
        for (index, argument) in arguments.iter().enumerate() {
            let at = format!("{at}[{index}]");
            let spec = &argument.input;
            let given = spec.value.is_some() || spec.is_required;
            let left_out = |why| Warning::ArgumentLeftOut {
                at: at.clone(),
                why,
            };
            match argument.kind.as_str() {
                "positional" if given => {
                    let hint = argument.value_hint.as_deref().unwrap_or("value");
                    args.push(self.valued(spec, InputKind::Arg, hint, None, &at));
                }
                "named" if given => {
                    let Some(flag) = argument
                        .name
                        .as_deref()
                        .map(printable)
                        .filter(|flag| !flag.is_empty())
                    else {
                        self.warnings
                            .push(left_out("a named argument with no name"));
                        continue;
                    };
                    let value = self.valued(
                        spec,
                        InputKind::Arg,
                        flag.trim_start_matches('-'),
                        None,
                        &at,
                    );
                    args.extend([flag, value]);
                }
                "positional" | "named" => {
                    self.warnings
                        .push(left_out("an optional argument with no value"));
                }
                _ => self
                    .warnings
                    .push(left_out("an argument of a type this build does not know")),
            }
        }
    }
}

/// What the registry says an input is, printable, when it says anything.
fn described(spec: &server::Input) -> Option<String> {
    spec.description
        .as_deref()
        .map(printable)
        .filter(|description| !description.is_empty())
}

/// An input's name made of `text`: every character no placeholder can be
/// written of made an underscore, so a flag's or a hint's name is one.
fn input_name(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        String::from("value")
    } else {
        name
    }
}

/// The variable an input named `name` is read from: its name in capitals,
/// every character a shell takes in none made an underscore, and one put
/// before a digit it would start with, so every agent refers to it alike.
fn variable_for(name: &str) -> VariableName {
    let mut variable: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if !variable.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        variable.insert(0, '_');
    }
    VariableName::from(variable)
}

/// Something of a server version that no definition says, for somebody to
/// do by hand. Each `at` is where it is in the registry's `server.json`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Manual {
    /// An MCP bundle, a file an agent that takes bundles installs whole.
    #[error("{at}: an MCP bundle at {identifier}, which no agent is given as a command: install it by hand into an agent that takes bundles")]
    Mcpb { at: String, identifier: Redacted },
    /// A Rust crate, which nothing here installs or starts.
    #[error("{at}: the Rust crate `{identifier}`, which nothing here installs: install it with cargo, then add the server it starts with `add mcp --command`")]
    Cargo { at: String, identifier: Redacted },
    /// A registry type this build does not know how to start.
    #[error("{at}: a `{registry_type}` package (`{identifier}`), which this build does not know how to start: start it by hand")]
    TypeUnknown {
        at: String,
        registry_type: Redacted,
        identifier: Redacted,
    },
    /// A package that serves at an address once started, which a
    /// definition, a program or an address, cannot say both of.
    #[error("{at}: a package that serves over `{transport}` once started, which one definition cannot say: start it by hand, then add the address it serves with `add mcp --url`")]
    NotStdio { at: String, transport: Redacted },
    /// An identifier, a version or a variable's name a command line would
    /// read as an option or as more than one word.
    #[error("{at}: its {what} `{text}` would be read as an option, or as other than one word, so no command line is written with it")]
    Unfit {
        at: String,
        what: &'static str,
        text: Redacted,
    },
    /// A package from another registry than the one its program reads.
    #[error("{at}: the {registry_type} package is from {base}, where `{runtime}` does not read packages, so it would start one of the same name from elsewhere: install it from there by hand")]
    Elsewhere {
        at: String,
        registry_type: String,
        base: Redacted,
        runtime: &'static str,
    },
    /// A remote reached over a transport this build does not know.
    #[error("{at}: a remote server reached over `{transport}`, which this build does not know")]
    RemoteUnknown { at: String, transport: Redacted },
    /// No package and no remote at all.
    #[error("it names no package and no remote server, so there is nothing to start or reach")]
    Unrunnable,
    /// The name it would be given is none a server may be.
    #[error("`{name}` is no name a server can be given: {}; give it another", printable(&issue_list(.issues)))]
    NameUnfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    /// What a package or a remote maps to breaks a rule every definition
    /// Mindweaver writes is held to.
    #[error("{at}: what it maps to is no definition Mindweaver writes: {}", printable(&issue_list(.issues)))]
    Invalid {
        at: String,
        issues: Vec<ValidationIssue>,
    },
}

codes!(Manual {
    Self::Mcpb { .. } => "registry_package_mcpb",
    Self::Cargo { .. } => "registry_package_cargo",
    Self::TypeUnknown { .. } => "registry_package_type_unknown",
    Self::NotStdio { .. } => "registry_package_not_stdio",
    Self::Unfit { .. } => "registry_package_unfit",
    Self::Elsewhere { .. } => "registry_package_elsewhere",
    Self::RemoteUnknown { .. } => "registry_remote_type_unknown",
    Self::Unrunnable => "registry_server_unrunnable",
    Self::NameUnfit { .. } => "registry_name_unfit",
    Self::Invalid { .. } => "registry_definition_invalid",
});

/// What is said of a definition mapped all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// Reached over server-sent events, which two agents are not given.
    #[error("{at}: reached over server-sent events, which the MCP specification has replaced with streamable HTTP, and which Codex and OpenCode are not given")]
    Sse { at: String },
    /// A default, an example or a fixed value given for a secret.
    #[error("{at}: the registry gives a {what} for a secret, which is left out: its value is read from the environment variable its input names")]
    SecretDropped { at: String, what: &'static str },
    /// An argument the definition is written without.
    #[error("{at}: {why}, left out: add it to `args` by hand if the server is wanted with it")]
    ArgumentLeftOut { at: String, why: &'static str },
}

codes!(Warning {
    Self::Sse { .. } => "registry_remote_sse",
    Self::SecretDropped { .. } => "registry_secret_dropped",
    Self::ArgumentLeftOut { .. } => "registry_argument_left_out",
});

#[cfg(test)]
#[path = "import_test.rs"]
mod tests;
