//! What the commands that read artifacts one by one say of each, for a
//! front end to print: `show`, one artifact in full ([`Shown`]), and
//! `validate`, what is wrong with each and the counts its verdict follows
//! ([`Validation`]).
//!
//! Built from the catalog the command read, the one its text is laid out
//! from, so the two cannot list different artifacts, and with no ledger:
//! neither command reads one, and an artifact's standing is the
//! snapshot's to say ([`crate::view`]). So neither says who put a copy
//! where it is, which read without the ledger would call an installed copy
//! somebody's. What the snapshot also says of an artifact goes by the
//! snapshot's name for it (`kind`, `name`, `summary`, `issues`), and
//! everything here is held to the rule every report is: each field by its
//! name, a contract once printed.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Serialize, Serializer};

use crate::artifact::{Artifact, ArtifactError, Declared, ValidationIssue};
use crate::catalog::DiscoveryFailure;
use crate::kind::Kind;
use crate::mcp::{Expressibility, Input, InputKind, Transport, TransportType};
use crate::paths;
use crate::value::{ArtifactName, InputName, Redacted, VariableName};

/// One artifact in full, as `show` shows it: what it is, where it is, what
/// its front matter declares that `show` prints, and the body after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Shown {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The directory a skill is, or the file a rule is: its first copy's,
    /// the one `show` reads.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The root of what holds it: its project, or the workspace for the
    /// workspace's own.
    #[serde(serialize_with = "paths::lossy")]
    pub project: PathBuf,
    /// The file it is read from: a skill's `SKILL.md`, or the rule itself.
    #[serde(serialize_with = "paths::lossy")]
    pub file: PathBuf,
    /// The one line a listing shows; none when there is nothing to show.
    pub summary: Option<String>,
    /// What it says it is for: always for a skill, and for a rule only when
    /// its front matter says.
    pub description: Option<String>,
    /// A skill's `allowed-tools`, when it narrows them.
    pub allowed_tools: Option<Vec<String>>,
    /// A skill's `license`, when it names one.
    pub license: Option<String>,
    /// A skill's `compatibility`, when written as the string it should be.
    pub compatibility: Option<String>,
    /// Each `metadata` entry of a skill that maps a string to a string, in
    /// the order written: the rest are `validate`'s to report. None for a
    /// rule, which has no `metadata`.
    #[serde(serialize_with = "in_order")]
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<std::collections::BTreeMap<String, String>>")
    )]
    #[cfg_attr(
        feature = "schema",
        schemars(with = "Option<std::collections::BTreeMap<String, String>>")
    )]
    pub metadata: Option<Vec<(String, String)>>,
    /// The files a rule is about, as glob patterns, when it says.
    pub paths: Option<Vec<String>>,
    /// How an MCP server is started or reached, as its definition says:
    /// none for a skill or a rule, which no agent starts.
    pub transport: Option<ShownTransport>,
    /// What somebody supplies for an MCP server, each input in the order
    /// its definition declares them, every one by the variable it is read
    /// from and never by a value, which no definition holds: none for a
    /// skill or a rule.
    pub inputs: Option<Vec<ShownInput>>,
    /// What each agent cannot be given of an MCP server's definition, every
    /// agent's in report order ([`crate::mcp::McpDefinition::expressibility`]):
    /// none for a skill or a rule, which every agent reads as it is.
    pub expressibility: Option<Vec<Expressibility>>,
    /// What follows the front matter, as the file has it now.
    pub body: String,
}

impl Shown {
    /// `artifact` in full, its body read from its file again, as `show`
    /// reads it: a file gone since it was listed is an error, as it is for
    /// `show`.
    pub fn of(artifact: &Artifact) -> Result<Shown, ArtifactError> {
        let body = artifact.contents()?;
        let mut shown = Shown {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            path: artifact.path().to_path_buf(),
            project: artifact.project().to_path_buf(),
            file: artifact.file(),
            summary: artifact.summary().map(str::to_owned),
            description: artifact.description().map(str::to_owned),
            allowed_tools: None,
            license: None,
            compatibility: None,
            metadata: None,
            paths: None,
            transport: None,
            inputs: None,
            expressibility: None,
            body,
        };
        match artifact.declared() {
            Declared::Skill(manifest) => {
                shown.allowed_tools.clone_from(&manifest.allowed_tools);
                shown.license.clone_from(&manifest.license);
                shown.compatibility = manifest.compatibility.stated().map(str::to_owned);
                shown.metadata = Some(manifest.metadata.entries.clone());
            }
            Declared::Rule(manifest) => {
                shown.paths = manifest.paths.stated().cloned();
            }
            // What each agent cannot be given of its transport and inputs
            // is core's to say, beside them.
            Declared::Mcp(definition) => {
                shown.transport = Some(ShownTransport::of(&definition.transport));
                shown.inputs = Some(definition.inputs.iter().map(ShownInput::of).collect());
                shown.expressibility = Some(definition.expressibility());
            }
        }
        Ok(shown)
    }
}

/// How an MCP server is started or reached, as `show` lays it out: the
/// transport by the word its definition writes as `type`, then what that
/// transport holds, as written, a key of the other transport empty.
///
/// One record for every transport rather than a variant each, so that its
/// `type` goes out as the word the format writes ([`TransportType::slug`])
/// rather than one a derive would spell from a variant. Its address is held
/// as the definition holds it, without any credential its userinfo was
/// typed with ([`Redacted`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ShownTransport {
    /// `stdio` for a program, `http` or `sse` for an address.
    #[serde(rename = "type")]
    pub transport: TransportType,
    /// The program an agent starts; none for an address.
    pub command: Option<String>,
    /// What the program is started with, in order.
    pub args: Vec<String>,
    /// What the program's environment is given, each value as written,
    /// an input's placeholder among it.
    pub env: BTreeMap<String, String>,
    /// The folder the program is started in, when the definition names
    /// one.
    pub cwd: Option<String>,
    /// Where the server is; none for a program.
    pub url: Option<Redacted>,
    /// What every request to the address carries, each value as written.
    pub headers: BTreeMap<String, String>,
}

impl ShownTransport {
    /// What `transport` holds, laid out.
    pub fn of(transport: &Transport) -> ShownTransport {
        let mut shown = ShownTransport {
            transport: transport.kind(),
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            headers: BTreeMap::new(),
        };
        match transport {
            Transport::Stdio(stdio) => {
                shown.command = Some(stdio.command.clone());
                shown.args.clone_from(&stdio.args);
                shown.env.clone_from(&stdio.env);
                shown.cwd.clone_from(&stdio.cwd);
            }
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                shown.url = Some(endpoint.url.clone());
                shown.headers.clone_from(&endpoint.headers);
            }
        }
        shown
    }
}

/// One input of an MCP server, as `show` lays it out: what it is called
/// where it is written, where that may be, by the word its definition
/// writes as `kind`, and the variable it is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ShownInput {
    pub name: InputName,
    pub kind: InputKind,
    /// What it is, for whoever supplies it, when the definition says.
    pub description: Option<String>,
    pub required: bool,
    pub secret: bool,
    /// The environment variable an agent reads it from: what a secret is
    /// said by, its value being nowhere in the definition.
    pub env_var: Option<VariableName>,
}

impl ShownInput {
    /// `input`, laid out.
    pub fn of(input: &Input) -> ShownInput {
        ShownInput {
            name: input.name.clone(),
            kind: input.kind,
            description: input.description.clone(),
            required: input.required,
            secret: input.secret,
            env_var: input.env_var.clone(),
        }
    }
}

/// Entries as an object, in the order written: a map would sort them, and a
/// list of pairs would read as something other than the mapping it is.
fn in_order<S: Serializer>(
    entries: &Option<Vec<(String, String)>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match entries {
        None => serializer.serialize_none(),
        Some(entries) => serializer.collect_map(entries.iter().map(|(key, value)| (key, value))),
    }
}

/// What `validate` found: each artifact it checked, with what is wrong with
/// it, and the counts its verdict follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Validation {
    /// Each artifact checked, in the order the catalog lists them.
    pub artifacts: Vec<Checked>,
    /// How many of them do not pass.
    pub invalid: usize,
    /// How many files could not be read at all
    /// ([`DiscoveryFailure::is_unreadable`]): none is among `artifacts`, and
    /// each is a warning beside them. Counted, so a check of a project whose
    /// files would not read does not pass for having found nothing wrong.
    pub unreadable: usize,
    /// How many of them have copies that do not all hold the same, which
    /// the agents reading them do not all load alike.
    pub differing: usize,
}

/// One artifact `validate` checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Checked {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The directory a skill is, or the file a rule is: its first copy's.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The root of what holds it: its project, or the workspace for the
    /// workspace's own and what is on its shelf.
    #[serde(serialize_with = "paths::lossy")]
    pub project: PathBuf,
    /// Everything wrong with it, every copy's, warnings included, each
    /// saying whether it is one.
    pub issues: Vec<ValidationIssue>,
    /// Whether it does not pass: one issue that is no warning is enough
    /// ([`ValidationIssue::is_warning`]).
    pub invalid: bool,
    /// Whether its copies do not all hold the same.
    pub copies_differ: bool,
}

impl Validation {
    /// What `validate` finds of `artifacts`, read from a catalog that met
    /// `failures` on the way.
    pub fn of(artifacts: &[&Artifact], failures: &[DiscoveryFailure]) -> Validation {
        let artifacts: Vec<Checked> = artifacts
            .iter()
            .map(|artifact| {
                let issues = artifact.validate();
                Checked {
                    kind: artifact.kind(),
                    name: artifact.name().to_owned(),
                    path: artifact.path().to_path_buf(),
                    project: artifact.project().to_path_buf(),
                    invalid: issues.iter().any(|issue| !issue.is_warning()),
                    issues,
                    copies_differ: artifact.copies_differ(),
                }
            })
            .collect();
        Validation {
            invalid: artifacts.iter().filter(|checked| checked.invalid).count(),
            unreadable: failures
                .iter()
                .filter(|failure| failure.is_unreadable())
                .count(),
            differing: artifacts
                .iter()
                .filter(|checked| checked.copies_differ)
                .count(),
            artifacts,
        }
    }

    /// Whether everything checked passes: nothing invalid, nothing that
    /// would not read, and no copies that differ.
    pub fn passes(&self) -> bool {
        self.invalid == 0 && self.unreadable == 0 && self.differing == 0
    }
}

#[cfg(test)]
#[path = "reading_test.rs"]
mod tests;
