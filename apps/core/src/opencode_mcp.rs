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
//! ([`Warning::FolderInV1`]). V2's `disabled`, which #92 gives a program's
//! entry, is never written: a definition has no switch for it, and the
//! least entry leaves it out. Nor do OpenCode's extras give it ([`keys`]),
//! since in V1's shape it would stand beside the `enabled` written there,
//! the two shapes' switch mixed in one entry.
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
//! which V2's shape does not take, text OpenCode would fill in where no
//! input stands, `{env:` or `{file:` written as it is (`text_substituted`),
//! and extras over a key either shape may write refuse the entry.
//!
//! The file is the first of `opencode.json`, `opencode.jsonc`,
//! `.opencode/opencode.json` and `.opencode/opencode.jsonc` there is at the
//! project's root ([`FILES`]), or `opencode.json`, made with `create_new`,
//! when none is. Each is read as JSONC ([`Format::Jsonc`]), as OpenCode
//! reads every one of them, up to [`MAX_BYTES`], and its comments and
//! trailing commas are kept with every other byte outside the entry. The
//! file goes with its last server only when the caller says Mindweaver made
//! it and nothing else is left in it, a comment somebody wrote included.
//! It is never one OpenCode reads for every project: a project whose file
//! is inside an agent's own folder, one whose root is `~/.config/opencode`
//! while that is OpenCode's, is refused ([`OpenCodeMcpError::UserScope`]),
//! as `codex_mcp` refuses one and an install's copy is refused there.
//!
//! Nothing here knows of the ledger. Whether an entry of the name is
//! Mindweaver's to replace or take out, and whether Mindweaver made the
//! file, are the caller's, as its record says (#93, #94); and the caller
//! holds the workspace's lock, as one calling [`ConfigFile`] does.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::artifact::{self, issue_list, ValidationIssue};
use crate::code::codes;
use crate::config_entry::{self, ConfigFile, Edit, EntryError, FileError, Format, Value, Written};
use crate::hash;
use crate::install;
use crate::mcp::{self, Inexpressible, McpDefinition, Runs, Transport, TransportType};
use crate::value::{ArtifactName, ContentHash, InputName, VariableName};

/// Where OpenCode reads a project's config from its root, in the order one
/// is chosen: the first of them there is the one written into.
pub const FILES: [&str; 4] = [
    "opencode.json",
    "opencode.jsonc",
    ".opencode/opencode.json",
    ".opencode/opencode.jsonc",
];

/// The one of [`FILES`] made when none of them is there.
pub const FILE: &str = FILES[0];

/// Where OpenCode reads its global config, in its own folder, every one of
/// them there is merged under a project's config key by key (docs/agents.md,
/// docs-only): what doctor reads for a server of an installed one's name,
/// never written.
pub const USER_FILES: [&str; 3] = ["config.json", "opencode.json", "opencode.jsonc"];

/// The most a file of [`FILES`] is read for: 2 MiB, as Claude's
/// `.mcp.json` is.
///
/// A project's servers and the settings beside them come to a few KiB, so
/// a file past this is not one somebody keeps them in, and it would be
/// held in memory whole, as text, as a tree of every byte and as what an
/// edit is checked against.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

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

    /// The entry's fingerprint, [`hash::fingerprint`] of its
    /// [canonical form](Self::canonical): what the ledger keeps of it once
    /// it is written, and what one an agent's file holds is told by.
    pub fn fingerprint(&self) -> ContentHash {
        hash::fingerprint(&self.canonical())
    }

    /// What the agent runs or reaches, read off the entry as it is written,
    /// its `command`, the program and its arguments in one array, or its `url`: what an install says it gives the agent before it writes
    /// anything, never spelled again from the definition.
    pub fn runs(&self) -> Runs {
        Runs::of_line(&self.value).expect("an entry is written with a program or an address")
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
/// the address only when it holds something, and V2's `disabled` after
/// them, which is never written.
///
/// What [`McpDefinition::inexpressible`] holds OpenCode's extras to, every
/// one of them whether or not this definition or this file's shape writes
/// it, so that what `show` says of a definition turns neither on which keys
/// it fills nor on which file it is written into
/// ([`Inexpressible::ExtraRepeated`]). `disabled` among them, V2's word for
/// the switch V1's shape is written with as `enabled`, for an address as
/// for a program, since V1's writes `enabled` for both: an extra giving it
/// would stand beside `enabled` in an entry written in V1's, the two shapes
/// mixed in one entry, and what it said would turn on the file's shape.
pub(crate) const fn keys(kind: TransportType) -> &'static [&'static str] {
    match kind {
        TransportType::Stdio => &[
            "type",
            "command",
            "enabled",
            "environment",
            "cwd",
            "disabled",
        ],
        TransportType::Http | TransportType::Sse => {
            &["type", "url", "enabled", "headers", "disabled"]
        }
    }
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The one of [`FILES`] written into.
    pub file: PathBuf,
    /// [`Written::Created`] when none of [`FILES`] was there and [`FILE`]
    /// is made now, which the caller keeps, to hand back to [`remove`] as
    /// `created`.
    pub written: Written,
    pub entry: Entry,
}

/// Write `definition`'s entry into the first of [`FILES`] there is at
/// `root`, a project's root, in place of one of the same name or after the
/// servers already there, and make [`FILE`] when none of them is there yet.
///
/// The entry is made first ([`entry`]), so a definition refused reads and
/// writes nothing: what refuses one refuses it in either shape. A file
/// inside one of `homes` is refused before it is read. It is
/// written in the shape a server of the name stands in, where it stands,
/// rather than beside itself in the other ([`standing`]), and a new one in
/// the shape the file keeps its servers in ([`followed`]). The file is read
/// as JSONC, as OpenCode reads every one of them, up to [`MAX_BYTES`], and
/// written through [`ConfigFile`], which keeps every byte outside the
/// entry, comments and trailing commas included, makes a new file with
/// `create_new` and leaves one that already holds the entry as it is.
pub fn render(
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<Rendered, OpenCodeMcpError> {
    let (file, given, standing) = prepared(root, definition, homes)?;
    let key = given.shape.key(given.name.as_str());
    let edit = match standing {
        None => Edit::Insert(given.value.clone()),
        Some(_) => Edit::Replace(given.value.clone()),
    };
    let written = file.edit(&key, &edit)?;
    Ok(Rendered {
        file: file.path(),
        written,
        entry: given,
    })
}

/// What [`render`] would write, with nothing written: for an install to
/// say what OpenCode would be given before it writes anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// The one of [`FILES`] it would be written into.
    pub file: PathBuf,
    /// The entry, in the shape that file would be given it in.
    pub entry: Entry,
}

/// What [`render`] would write into the first of [`FILES`] there is at
/// `root`, read and refused as [`render`] reads and refuses it, in the
/// shape it would write, with nothing written: one place deciding both, so
/// what an install says OpenCode will be given is what it then writes.
pub fn planned(
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<Planned, OpenCodeMcpError> {
    let (file, entry, _) = prepared(root, definition, homes)?;
    Ok(Planned {
        file: file.path(),
        entry,
    })
}

/// The file [`render`] writes into, the entry it writes there in the shape
/// it is written in, and the shape a server of the name stands in there
/// already, each refusal asked in the order [`render`] asks it: what it
/// and [`planned`] share.
fn prepared(
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<(ConfigFile, Entry, Option<Shape>), OpenCodeMcpError> {
    let checked = entry(definition, Shape::V2)?;
    let (file, _) = config(root)?;
    if let Some((file, home, reader)) = install::in_agent_home(homes, &[file.path()]) {
        return Err(OpenCodeMcpError::UserScope { file, home, reader });
    }
    let standing = standing(&file, checked.name.as_str())?;
    let given = match standing.map_or_else(|| followed(&file), Ok)? {
        Shape::V2 => checked,
        Shape::V1 => entry(definition, Shape::V1)?,
    };
    Ok((file, given, standing))
}

/// Take the server keyed `name` out of the first of [`FILES`] there is at
/// `root`, in whichever shape it stands, every other byte as it was, and
/// take the file away with it when `created`, that Mindweaver made it, and
/// nothing else is left in it: no other server, nothing beside them, and
/// no comment ([`holds_only`]).
///
/// [`Written::Removed`] when the file went, [`Written::Changed`] when the
/// server did and the file stays, and [`Written::Unchanged`] when there was
/// no such server, nor a file to take. A file Mindweaver made that somebody
/// has written in since is theirs too, and stays; and only [`FILE`] is ever
/// one Mindweaver made, so another of [`FILES`] chosen in its place, once it
/// is gone, stays whatever it holds. `.opencode` is never taken away.
pub fn remove(root: &Path, name: &str, created: bool) -> Result<Written, OpenCodeMcpError> {
    remove_from(root, chosen(root)?, name, created)
}

/// [`remove`], from OpenCode's config file at `route`, one of [`FILES`],
/// rather than from the first of them there is: for a server recorded in
/// the file it was written into, which another of the four may have come
/// before since, as [`servers`] reads one where its record says. Taken out
/// of the first instead, it would be a server of the name somebody keeps
/// there, and the one written left behind.
///
/// A route that is none of [`FILES`] holds no server OpenCode reads, so
/// nothing is taken out of it ([`Written::Unchanged`]), as [`servers`]
/// finds none an entry could be told by there.
pub fn remove_from(
    root: &Path,
    route: &str,
    name: &str,
    created: bool,
) -> Result<Written, OpenCodeMcpError> {
    if !FILES.contains(&route) {
        return Ok(Written::Unchanged);
    }
    let file = ConfigFile::new(root, route, Format::Jsonc)?.capped(MAX_BYTES);
    let standing = standing(&file, name)?;
    // Taken whole when the server is all it holds, rather than written
    // without it first only to be taken away.
    if created && route == FILE {
        let key = standing.unwrap_or(Shape::V2).key(name);
        let taken = file.take(|text, _| holds_only(text, &key))?;
        if taken == Written::Removed {
            return Ok(taken);
        }
    }
    let Some(shape) = standing else {
        return Ok(Written::Unchanged);
    };
    Ok(file.edit(&shape.key(name), &Edit::Remove)?)
}

/// Every server OpenCode's config file at `route`, one of [`FILES`], in
/// the project at `root` holds, by its key, with the shape it stands in,
/// V2's under `mcp.servers` first and then V1's under `mcp`, each in the
/// order the file holds them: none when there is no file there.
///
/// Read as [`render`] reads a file to write into it, as JSONC and up to
/// [`MAX_BYTES`], so a file it would refuse to write into is refused here
/// too, with the same code, and so is one holding a server in both shapes
/// ([`OpenCodeMcpError::Twice`]), which `render` and `remove` refuse:
/// which of the two OpenCode reads cannot be told. `route` rather than the
/// file [`render`] picks, so a server is read where its record says it was
/// written, whichever of the four comes first now.
pub fn servers(root: &Path, route: &str) -> Result<Vec<(String, Shape, Value)>, OpenCodeMcpError> {
    let file = ConfigFile::new(root, route, Format::Jsonc)?.capped(MAX_BYTES);
    let Some(Value::Object(members)) = file.get(&[MCP])? else {
        return Ok(Vec::new());
    };
    let mut servers = Vec::new();
    let mut v1 = Vec::new();
    for (key, value) in members {
        match (key.as_str(), value) {
            (SERVERS, Value::Object(v2)) => {
                servers.extend(v2.into_iter().map(|(name, value)| (name, Shape::V2, value)))
            }
            (key, value) if !held(key) => v1.push((key.to_owned(), Shape::V1, value)),
            _ => {}
        }
    }
    if let Some((name, ..)) = v1
        .iter()
        .find(|(name, ..)| servers.iter().any(|(other, ..)| other == name))
    {
        return Err(OpenCodeMcpError::Twice {
            path: file.path(),
            name: name.clone(),
        });
    }
    servers.extend(v1);
    Ok(servers)
}

/// The first of OpenCode's global config files ([`USER_FILES`]) in the
/// folders `homes` gives it ([`AgentHomes::opencode_dirs`]) that holds a
/// server keyed `name`, in either shape: none when none does, or no folder
/// is given.
///
/// Read through a link and up to [`MAX_BYTES`], as JSONC, as Codex's own
/// config is read ([`config_entry::read_through`]): only read, and a
/// user's config is most often linked into the home from a repository of
/// their settings.
pub(crate) fn held_by_user(
    homes: &AgentHomes,
    name: &str,
) -> Result<Option<PathBuf>, OpenCodeMcpError> {
    for path in homes
        .opencode_dirs()
        .into_iter()
        .flat_map(|folder| USER_FILES.map(|file| folder.join(file)))
    {
        let Some(text) = config_entry::read_through(&path, MAX_BYTES)? else {
            continue;
        };
        let holds = |key: &[&str]| {
            config_entry::get(Format::Jsonc, &text, key).map_err(|source| FileError::Entry {
                path: path.clone(),
                source,
            })
        };
        let v1 = !held(name) && holds(&Shape::V1.key(name))?.is_some();
        if v1 || holds(&Shape::V2.key(name))?.is_some() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// The shape a server not in `file` yet is written in: V1's when `file`
/// keeps a server in V1's shape already, so that its servers keep one
/// shape (D-12), and V2's otherwise.
fn followed(file: &ConfigFile) -> Result<Shape, OpenCodeMcpError> {
    let v1 = match file.get(&[MCP])? {
        Some(Value::Object(members)) => members.iter().any(|(key, _)| !held(key)),
        _ => false,
    };
    Ok(if v1 { Shape::V1 } else { Shape::V2 })
}

/// Which shape a server keyed `name` stands in in `file`, `None` when it is
/// in neither, and refused when it is in both, since which of the two
/// OpenCode reads cannot be told.
///
/// A key OpenCode's `mcp` holds itself is never a server of V1's: `servers`
/// is where V2's are, which taking out as one server would take every one
/// of them.
fn standing(file: &ConfigFile, name: &str) -> Result<Option<Shape>, OpenCodeMcpError> {
    let v2 = file.get(&Shape::V2.key(name))?.is_some();
    let v1 = !held(name) && file.get(&Shape::V1.key(name))?.is_some();
    match (v2, v1) {
        (true, true) => Err(OpenCodeMcpError::Twice {
            path: file.path(),
            name: name.to_owned(),
        }),
        (true, false) => Ok(Some(Shape::V2)),
        (false, true) => Ok(Some(Shape::V1)),
        (false, false) => Ok(None),
    }
}

/// Whether `key` is one OpenCode's `mcp` holds itself beside the servers its
/// V1 shape keys there (`servers`, `timeout`), as it spells it: the names a
/// server is refused for OpenCode ([`mcp::name`]'s), which keep no server of
/// V1's.
fn held(key: &str) -> bool {
    mcp::name::reserved(Agent::OpenCode).contains(&key)
}

/// The first of [`FILES`] there is at `root`, or [`FILE`] when none is,
/// read as JSONC up to [`MAX_BYTES`], beside its route.
///
/// JSONC whichever it is, `.json` too: OpenCode reads each of them with
/// one JSONC parser, comments and trailing commas and all (its config
/// docs), so a `.json` holding a comment is a file it loads, which strict
/// JSON, as Claude's `.mcp.json` is read, would refuse. A file made is
/// strict JSON all the same, since nothing written into it is a comment.
fn config(root: &Path) -> Result<(ConfigFile, &'static str), OpenCodeMcpError> {
    let route = chosen(root)?;
    Ok((
        ConfigFile::new(root, route, Format::Jsonc)?.capped(MAX_BYTES),
        route,
    ))
}

/// The first of [`FILES`] there is at `root`, as the issue (#92) orders
/// them, or [`FILE`] when none is: what [`Agent::mcp_file`] says of
/// OpenCode, so the file an entry is recorded in is the one written into.
///
/// Whatever is there counts, a link or a folder too, so that the file's
/// route refuses it ([`ConfigFile`]) rather than a server being written
/// into another beside it. A `.opencode` that is no folder holds none of
/// them, as OpenCode finds none in it.
pub(crate) fn chosen(root: &Path) -> Result<&'static str, FileError> {
    for route in FILES {
        let path = root.join(route);
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(route),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) => {}
            Err(source) => return Err(FileError::Read { path, source }),
        }
    }
    Ok(FILE)
}

/// Whether `text` holds the server at `key` and nothing else, or not even
/// that: what taking it out of the text leaves is the objects on the way
/// to it, empty, and no comment.
///
/// Asked of what is left, as `codex_mcp` asks its TOML, rather than of the
/// document alone, as `claude_mcp` asks its strict JSON
/// ([`config_entry::holds_only`]), since JSONC has comments and the
/// document holds none: one somebody wrote above the server or anywhere
/// else outside it is something, and taking the server out keeps it
/// ([`config_entry::jsonc_commented`]). What the server's own lines hold
/// goes with it, as it does when the file stays.
fn holds_only(text: &str, key: &[&str]) -> bool {
    let left = match config_entry::edit(Format::Jsonc, text, key, &Edit::Remove) {
        Ok(left) => left,
        Err(EntryError::Missing { .. }) => text.to_owned(),
        // Nothing else an edit refuses is a file holding the server alone.
        Err(_) => return false,
    };
    !config_entry::jsonc_commented(&left)
        && config_entry::document(Format::Jsonc, &left)
            .is_ok_and(|document| config_entry::holds_only(&document, key))
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
    #[error(
        "{path}: `{name}` is a server both under `mcp.servers` and under `mcp`, OpenCode's V2 \
         and V1 shapes of one, so which of the two is meant cannot be told, and the file was \
         left as it is"
    )]
    Twice { path: PathBuf, name: String },
    #[error(
        "{file} is inside {home}, which {reader} reads for every project, so no server is \
         written into it: Mindweaver gives a server to one project at a time"
    )]
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error(transparent)]
    File(#[from] FileError),
}

codes!(OpenCodeMcpError {
    Self::Unfit { .. } => "opencode_mcp_unfit",
    Self::InputUnread { .. } => "opencode_mcp_input_unread",
    Self::Twice { .. } => "opencode_mcp_twice",
    Self::UserScope { .. } => "opencode_mcp_user_scope",
} carried {
    Self::Inexpressible { shape, .. } => shape,
    Self::File(error) => error,
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
