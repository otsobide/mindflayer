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
//! Each key is written only when it holds something: a `cwd` or a
//! `bearer_token_env_var` has no empty value to write, and an empty `env`
//! would be a table of its own holding nothing. A definition still has one
//! entry, and one canonical form, the form whose digest the ledger will
//! keep (#93), its members sorted, so an entry Codex or a person writes
//! again in another layout is the one Mindweaver wrote.
//!
//! The file is TOML, read up to [`MAX_BYTES`] and edited through
//! toml_edit, and made with `create_new`, `.codex` with it, when it is not
//! there yet. It is never one Codex reads for every project: a project
//! whose `.codex/config.toml` is inside an agent's own folder, one at the
//! user's home while `$CODEX_HOME` is `~/.codex`, is refused
//! ([`CodexMcpError::UserScope`]), as an install's copy is there.
//!
//! Codex merges a project's servers over the user's own, those of
//! `$CODEX_HOME/config.toml`, key by key, so a server of the name there and
//! the project's would be read as one made of both. A name the user's
//! config holds already is refused ([`CodexMcpError::Collision`]), read
//! from the Codex home [`AgentHomes`] gives and nowhere else. Refused
//! rather than warned of, though which it should be is what #18's
//! `codex-mcp-collision` would decide, and that was never run: a merge that
//! breaks more than the one name, the whole configuration or the other
//! servers, is the outcome a refusal is right for, and until a run says
//! the merge leaves the rest working, a refusal is the side to be wrong
//! on. With no Codex home given there is no user's config to read, and
//! nothing collides, as in a home holding no `config.toml`.
//!
//! Codex reads a project's `.codex/config.toml` only in a project the
//! user's config marks trusted, `[projects."<path>"] trust_level =
//! "trusted"`, so an entry written into one it does not is written all the
//! same, and said ([`Warning::Untrusted`]): trusting a project is the
//! user's to do, and Codex asks them. The path Mindweaver looks up is the
//! project's root as it is handed and as its links resolve to, either one
//! marked trusted counting; which path Codex keys a project by (the folder
//! it starts in, the repository's root above it) is docs-only, so a project
//! trusted under another is warned of, which costs a warning. With no Codex
//! home given nothing marks it trusted, and that is said.
//!
//! Nothing here knows of the ledger. Whether an entry of the name is
//! Mindweaver's to replace or take out, and whether Mindweaver made the
//! file, are the caller's, as its record says (#93, #94); and the caller
//! holds the workspace's lock, as one calling [`ConfigFile`] does.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::{self, ConfigFile, Edit, EntryError, FileError, Format, Value, Written};
use crate::install;
use crate::mcp::{self, Inexpressible, McpDefinition, Transport, TransportType};
use crate::value::{ArtifactName, InputName, VariableName};

/// Where Codex reads a project's servers, from its root.
pub const FILE: &str = ".codex/config.toml";

/// The table in [`FILE`] that holds every server by its name.
pub const SERVERS: &str = "mcp_servers";

/// The user's own config in Codex's home, the layer a project's [`FILE`]
/// is merged over.
pub const USER_FILE: &str = "config.toml";

/// The most a [`FILE`] is read for: 2 MiB, as Claude's `.mcp.json` is.
///
/// A project's servers and the settings beside them come to a few KiB, so
/// a file past this is not one somebody keeps them in, and it would be
/// held in memory whole, as text, as a tree of every byte and as what an
/// edit is checked against.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

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

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The project's [`FILE`].
    pub file: PathBuf,
    /// [`Written::Created`] when the file was not there and is made now,
    /// which the caller keeps, to hand back to [`remove`] as `created`.
    pub written: Written,
    pub entry: Entry,
    /// What is said of the project beside what is said of its entry: that
    /// Codex may not read the file at all.
    pub warnings: Vec<Warning>,
}

/// Write `definition`'s entry into the [`FILE`] at `root`, a project's
/// root, in place of one of the same name or after the servers already
/// there, and make the file, and `.codex`, when it is not there yet.
///
/// The entry is made first ([`entry`]), so a definition refused reads and
/// writes nothing. A file inside one of `homes` is refused before it is
/// read, and so is a name the user's config in the Codex home `homes`
/// gives already holds a server under. The file is read up to [`MAX_BYTES`] and written through
/// [`ConfigFile`], which keeps every byte outside the entry, makes a new
/// file with `create_new` and leaves one that already holds the entry as
/// it is.
pub fn render(
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<Rendered, CodexMcpError> {
    let entry = entry(definition)?;
    let file = config(root)?;
    if let Some((file, home, reader)) = install::in_agent_home(homes, &[file.path()]) {
        return Err(CodexMcpError::UserScope { file, home, reader });
    }
    let key = [SERVERS, entry.name.as_str()];
    let user = UserConfig::read(homes)?;
    if let Some(user) = &user {
        if user.get(&key)?.is_some() {
            return Err(CodexMcpError::Collision {
                name: entry.name.to_string(),
                config: user.path.clone(),
            });
        }
    }
    let trusted = match &user {
        Some(user) => user.trusts(root)?,
        None => false,
    };
    let edit = match file.get(&key)? {
        None => Edit::Insert(entry.value.clone()),
        Some(_) => Edit::Replace(entry.value.clone()),
    };
    let written = file.edit(&key, &edit)?;
    let mut warnings = Vec::new();
    if !trusted {
        warnings.push(Warning::Untrusted {
            name: entry.name.to_string(),
            file: file.path(),
            project: root.to_path_buf(),
            config: homes.codex_home.as_ref().map(|home| home.join(USER_FILE)),
        });
    }
    Ok(Rendered {
        file: file.path(),
        written,
        entry,
        warnings,
    })
}

/// Take the entry keyed `name` out of the [`FILE`] at `root`, every other
/// byte as it was, and take the file away with it when `created`, that
/// Mindweaver made it, and nothing else is left in it: no other server,
/// nothing beside [`SERVERS`], and no comment ([`holds_only`]).
///
/// [`Written::Removed`] when the file went, [`Written::Changed`] when the
/// entry did and the file stays, and [`Written::Unchanged`] when there was
/// no such entry, nor a file to take. A file Mindweaver made that somebody
/// has written in since is theirs too, and stays. `.codex` stays whatever
/// is left in it: it is Codex's own folder, which may hold what else Codex
/// keeps for the project, and which folder Mindweaver made is no record
/// anybody keeps.
pub fn remove(root: &Path, name: &str, created: bool) -> Result<Written, CodexMcpError> {
    let file = config(root)?;
    // Taken whole when the entry is all it holds, rather than written
    // without it first only to be taken away.
    if created {
        let taken = file.take(|text, _| holds_only(text, name))?;
        if taken == Written::Removed {
            return Ok(taken);
        }
    }
    let key = [SERVERS, name];
    if file.get(&key)?.is_none() {
        return Ok(Written::Unchanged);
    }
    Ok(file.edit(&key, &Edit::Remove)?)
}

/// Whether `text` holds the entry keyed `name` and nothing else, or not
/// even that: what taking the entry out of it leaves is blank.
///
/// Asked of the text, where `claude_mcp` asks the document
/// ([`config_entry::holds_only`]), since TOML has comments and the
/// document holds none: one somebody wrote in the file, above the entry or
/// anywhere else outside it, is something, and taking the entry out keeps
/// it. What the entry's own lines hold goes with it, as it does when the
/// file stays.
fn holds_only(text: &str, name: &str) -> bool {
    match config_entry::edit(Format::Toml, text, &[SERVERS, name], &Edit::Remove) {
        Ok(left) => left.trim().is_empty(),
        Err(EntryError::Missing { .. }) => text.trim().is_empty(),
        // Nothing else an edit refuses is a file holding the entry alone.
        Err(_) => false,
    }
}

/// The project's [`FILE`], read up to [`MAX_BYTES`].
fn config(root: &Path) -> Result<ConfigFile, CodexMcpError> {
    Ok(ConfigFile::new(root, FILE, Format::Toml)?.capped(MAX_BYTES))
}

/// The user's [`USER_FILE`] in the Codex home, as text: only read, never
/// written.
struct UserConfig {
    path: PathBuf,
    text: String,
}

impl UserConfig {
    /// The user's config in the Codex home `homes` gives, read through a
    /// link and up to [`MAX_BYTES`]; `None` when no home is given, or it
    /// holds no such file.
    ///
    /// Through a link, where a project's [`FILE`] is refused one: it is
    /// read and never written, and a user's config kept in a repository of
    /// their settings is most often linked into the home. Refused when it
    /// does not read as TOML, as Codex refuses it: whether it names a
    /// server cannot be told of a file that does not parse.
    fn read(homes: &AgentHomes) -> Result<Option<Self>, CodexMcpError> {
        let Some(home) = &homes.codex_home else {
            return Ok(None);
        };
        let path = home.join(USER_FILE);
        let failed = |source| FileError::Read {
            path: path.clone(),
            source,
        };
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(failed(source).into()),
        };
        let mut bytes = Vec::new();
        // One byte past the cap is enough to know it is past.
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(failed)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(FileError::TooLarge {
                path,
                cap: MAX_BYTES,
            }
            .into());
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(FileError::NotText { path }.into());
        };
        Ok(Some(UserConfig { path, text }))
    }

    /// Whether the user's config marks the project at `root` trusted,
    /// under its root as it is handed or as its links resolve to: the
    /// folder a program starts in is reported to it with its links
    /// resolved, and a project is often reached through one.
    fn trusts(&self, root: &Path) -> Result<bool, CodexMcpError> {
        let mut roots = vec![root.to_path_buf()];
        roots.extend(install::resolved(root).filter(|resolved| resolved != root));
        for root in &roots {
            // A path no TOML key can spell is none a project is marked by.
            let Some(key) = root.to_str() else {
                continue;
            };
            let level = self.get(&["projects", key, "trust_level"])?;
            if level == Some(Value::from("trusted")) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// What the user's config holds at `key`.
    fn get(&self, key: &[&str]) -> Result<Option<Value>, CodexMcpError> {
        config_entry::get(Format::Toml, &self.text, key).map_err(|source| {
            FileError::Entry {
                path: self.path.clone(),
                source,
            }
            .into()
        })
    }
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
    /// The project is not one the user's config marks trusted, and Codex
    /// reads its `.codex/config.toml` only in one it does: written all the
    /// same, since trusting a project is the user's to do, and Codex asks
    /// them. `config` is the user's config looked in, none when no Codex
    /// home was given.
    #[error("`{name}` is written into {file}, which Codex reads only in a project it trusts, and {}", unmarked(.project, .config.as_deref()))]
    Untrusted {
        name: String,
        file: PathBuf,
        project: PathBuf,
        config: Option<PathBuf>,
    },
}

codes!(Warning {
    Self::Untrusted { .. } => "codex_mcp_untrusted",
} carried {
    Self::Dropped { shape, .. } => shape,
});

/// Where nothing says `project` is trusted, for [`Warning::Untrusted`].
fn unmarked(project: &Path, config: Option<&Path>) -> String {
    match config {
        Some(config) => format!(
            "{} does not mark {} `trusted`",
            config.display(),
            project.display()
        ),
        None => format!(
            "no Codex home was given to say whether it trusts {}",
            project.display()
        ),
    }
}

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
    #[error(
        "{file} is inside {home}, which {reader} reads for every project, so no server is \
         written into it: Mindweaver gives a server to one project at a time"
    )]
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error(
        "`{name}` cannot be given to Codex: {config} holds a server `{name}` already, and Codex \
         reads a project's servers over the user's key by key, so the two would be read as one \
         server made of both"
    )]
    Collision { name: String, config: PathBuf },
    #[error(transparent)]
    File(#[from] FileError),
}

codes!(CodexMcpError {
    Self::Unfit { .. } => "codex_mcp_unfit",
    Self::InputUnread { .. } => "codex_mcp_input_unread",
    Self::InputInText { .. } => "codex_mcp_input_in_text",
    Self::UserScope { .. } => "codex_mcp_user_scope",
    Self::Collision { .. } => "codex_mcp_collision",
} carried {
    Self::Inexpressible { shape, .. } => shape,
    Self::File(error) => error,
});

#[cfg(test)]
#[path = "codex_mcp_test.rs"]
mod tests;
