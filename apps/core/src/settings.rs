//! A v3 project's marker: `.mind/settings.json`, holding what the hub needs
//! to know about the project and nothing else.
//!
//! Which agents read it, whether what is installed into it is shared with the
//! team or kept to this checkout (D-02), whether a team file git tracks may
//! be edited, and where the project authors artifacts of its own (D-06),
//! beside an id that names it for good and the name it is shown by (D-09).
//! These describe the files physically in the project, and several
//! workspaces may link one project, so they live in the project rather than
//! in any one workspace's `flayer.toml`, where two workspaces could disagree
//! about one project's files.
//!
//! JSON rather than the TOML `mind.toml` is written in, the owner's choice
//! (D-06): a file this small explains nothing a comment would have to, and
//! every tool and editor reads and writes JSON. Strict JSON, not JSONC, for
//! the same reason: `jq`, Python's `json` and a browser's `JSON.parse` take
//! it as it is, and an edit (`config_entry`, held to RFC 8259) refuses what
//! they would refuse.
//!
//! A key this build does not know is carried rather than refused: read past,
//! and kept by an edit, where it was. The format grows (a project's own
//! install targets, #40, are the next key), and a project configured by a
//! newer Mindflayer is still a project, as it is for unknown front matter.

use std::fmt;
use std::path::Path;

use crate::agent::Agent;
use crate::config_entry::{self, Edit, Format, Value};
use crate::paths;
use crate::workspace::{check_version, Directories, Marker, WorkspaceError, SETTINGS_VERSION};

/// The marker file of a v3 project, inside its `.mind`.
pub const SETTINGS_FILE: &str = "settings.json";

/// The contents of `.mind/settings.json`.
///
/// Every key the format defines, and no other: what a newer build added is
/// left in the file, and an edit leaves it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The layout version the file was written with:
    /// [`SETTINGS_VERSION`] or, refused, one this build does not know.
    pub version: u32,
    /// What names the project whatever it is called and wherever it is.
    pub id: ProjectId,
    /// A human name for the project. Defaults to its directory's name.
    pub name: String,
    /// The agents that work in the project, in the order the file lists
    /// them: where each kind is installed for each is the target table's to
    /// say (`TargetTable`), which serves an agent named twice once.
    pub agents: Vec<Agent>,
    /// Whether what is installed into the project is the team's or this
    /// checkout's.
    pub share: Share,
    /// Whether a team file that git tracks (`AGENTS.md`, `CLAUDE.md`, an MCP
    /// config) may be edited to reach an agent in a [local](Share::Local)
    /// project. Off unless the file says so.
    pub edit_tracked: bool,
    /// Where the project authors artifacts of its own, relative to its root.
    /// A kind it does not name has no such folder: never a default, so a
    /// kind added later is not given a folder at the root nobody asked for.
    pub directories: Directories,
}

impl Settings {
    /// A new project's settings, at [`SETTINGS_VERSION`], with an id of its
    /// own and `edit_tracked` off: the one setting nobody chooses at the
    /// start, and the one that edits a team's files.
    pub fn new(
        name: impl Into<String>,
        agents: Vec<Agent>,
        share: Share,
        directories: Directories,
    ) -> Self {
        Settings {
            version: SETTINGS_VERSION,
            id: ProjectId::generate(),
            name: name.into(),
            agents,
            share,
            edit_tracked: false,
            directories,
        }
    }

    /// Read the settings file at `path`.
    ///
    /// Refused when it is not strict JSON, when a key the format defines is
    /// missing or holds the wrong thing, and when its version is not
    /// [`SETTINGS_VERSION`]: a newer one as written by a newer Mindflayer, an
    /// older one as no settings file at all, since versions 1 and 2 are
    /// `mind.toml`'s. A key this build does not know is read past.
    pub fn read(path: &Path) -> Result<Self, WorkspaceError> {
        let text = std::fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        parse(path, &text)
    }

    /// The text of a new settings file holding these: pretty-printed, two
    /// spaces a level, ending in a newline, the keys in the order the format
    /// lists them.
    ///
    /// Written as `config_entry` lays out what an edit inserts, so a file
    /// written new and one an edit has changed since read alike. Refused for
    /// a directory that is not UTF-8, which JSON cannot hold.
    pub fn text(&self) -> Result<String, WorkspaceError> {
        let members = self
            .members()?
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value));
        Ok(config_entry::json_text(&Value::Object(members.collect()))
            .expect("settings hold nothing JSON cannot write"))
    }

    /// Each key the format defines, with what these settings hold for it, in
    /// the order a new file writes them.
    fn members(&self) -> Result<Vec<(&'static str, Value)>, WorkspaceError> {
        Ok(vec![
            (VERSION, Value::Integer(i64::from(self.version))),
            (ID, Value::from(self.id.as_str())),
            (NAME, Value::from(self.name.as_str())),
            (
                AGENTS,
                Value::array(self.agents.iter().map(|agent| agent.name())),
            ),
            (SHARE, Value::from(self.share.name())),
            (EDIT_TRACKED, Value::Bool(self.edit_tracked)),
            (DIRECTORIES, Value::Object(self.directory_members()?)),
        ])
    }

    /// The entries of `directories`, the kinds this build knows first, in
    /// report order, then any other key in name order, each a path spelled
    /// with `/` so the file reads the same on every platform.
    fn directory_members(&self) -> Result<Vec<(String, Value)>, WorkspaceError> {
        self.directories
            .entries()
            .into_iter()
            .map(|(key, directory)| {
                let written = paths::to_config_string(directory).ok_or_else(|| {
                    WorkspaceError::NonUtf8Path {
                        path: directory.to_path_buf(),
                    }
                })?;
                Ok((key.to_owned(), Value::from(written)))
            })
            .collect()
    }
}

/// Whether what is installed into a project reaches the team (D-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Share {
    /// Installed copies and entries stay out of git, through a block of
    /// `info/exclude` an install keeps, and `.mind` itself does through a
    /// `.gitignore` of its own (D-01). What a new project is.
    Local,
    /// Written for the team to commit: nothing is kept out of git, the
    /// project's settings included.
    Shared,
}

impl Share {
    /// Both, in the order a front end lists them.
    pub const ALL: [Share; 2] = [Share::Local, Share::Shared];

    /// How the settings file spells it.
    pub const fn name(self) -> &'static str {
        match self {
            Share::Local => "local",
            Share::Shared => "shared",
        }
    }

    /// The one spelled `word`, as the settings file and a command line
    /// spell it.
    pub fn named(word: &str) -> Option<Share> {
        Share::ALL.into_iter().find(|share| share.name() == word)
    }
}

impl fmt::Display for Share {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A project's id: 128 random bits, written as 32 lowercase hexadecimal
/// digits.
///
/// What names a project when its name and its path are no use: both change,
/// a name by hand and a path when the repository moves, and several
/// workspaces may know one project by different routes. Random rather than
/// derived from either, so that a copy of a project made with `cp -r` is the
/// only way two projects share one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(String);

impl ProjectId {
    /// The number of hexadecimal digits an id is written with.
    pub const LEN: usize = 32;

    /// A new id, drawn from the operating system through `std`'s hasher keys
    /// as a stage's name is (`copy::random`), rather than from a dependency
    /// kept for it.
    pub fn generate() -> Self {
        ProjectId(format!(
            "{:016x}{:016x}",
            crate::copy::random(),
            crate::copy::random()
        ))
    }

    /// The id as written, when `text` is one: exactly [`LEN`](Self::LEN)
    /// lowercase hexadecimal digits, one spelling for one id.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.len() == Self::LEN
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        digits.then(|| ProjectId(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

const VERSION: &str = "version";
const ID: &str = "id";
const NAME: &str = "name";
const AGENTS: &str = "agents";
const SHARE: &str = "share";
const EDIT_TRACKED: &str = "edit_tracked";
const DIRECTORIES: &str = "directories";

/// The settings `text` holds, read from the file at `path`, which errors name.
///
/// The version is read first and held to this build before anything else,
/// so a file a newer Mindflayer wrote is refused for being newer, whatever
/// else it holds, rather than for a key that version gave a new meaning.
pub(crate) fn parse(path: &Path, text: &str) -> Result<Settings, WorkspaceError> {
    let malformed = |detail: String| WorkspaceError::Malformed {
        path: path.to_path_buf(),
        detail,
    };
    let document =
        config_entry::document(Format::Json, text).map_err(|error| malformed(error.to_string()))?;
    let Value::Object(members) = &document else {
        return Err(malformed("it holds no object".to_owned()));
    };
    let members = Members(members);

    let version = members.version().map_err(malformed)?;
    check_version(path, Marker::Project, version)?;
    if version < SETTINGS_VERSION {
        return Err(malformed(format!(
            "version {version} is a {}'s, and a settings file is version {SETTINGS_VERSION}",
            crate::workspace::MIND_CONFIG
        )));
    }
    members.settings(version).map_err(malformed)
}

/// `text`, the settings file at `path` holding `before`, changed to hold
/// `wanted`, with every byte that is not a changed value as it was.
///
/// Only the keys whose values differ are touched, each as one entry
/// (`config_entry::edit`), and `directories` an entry at a time: a key this
/// build does not know, the order the keys are written in, the file's
/// indentation and a value that did not change all stay as the file had
/// them, so a file somebody edited by hand is not laid out again. A key the
/// file left out goes in after the others, and only when what it holds is
/// not what leaving it out reads as: `edit_tracked` off is not written in
/// by an edit that changes something else.
///
/// The version and the id are refused rather than changed: a version moves
/// with a migration, which writes the file its version describes, and an id
/// names the project for good.
pub(crate) fn edited(
    path: &Path,
    text: &str,
    before: &Settings,
    wanted: &Settings,
) -> Result<String, WorkspaceError> {
    let refused = |detail: String| WorkspaceError::Edit {
        path: path.to_path_buf(),
        detail,
    };
    if wanted.version != before.version {
        return Err(refused(format!(
            "`{VERSION}` changes with a migration, never with an edit"
        )));
    }
    if wanted.id != before.id {
        return Err(refused(format!(
            "`{ID}` names the project for good, so no edit changes it"
        )));
    }

    let mut edits: Vec<(Vec<String>, Edit)> = Vec::new();
    for ((key, was), (_, now)) in before.members()?.into_iter().zip(wanted.members()?) {
        if was == now {
            continue;
        }
        if let (Value::Object(was), Value::Object(now)) = (&was, &now) {
            // `directories`, the one object: an entry at a time, so one the
            // change leaves alone, a key this build does not know included,
            // stays where it is.
            for (name, value) in now {
                let edit = match was.iter().find(|(old, _)| old == name) {
                    Some((_, old)) if old == value => continue,
                    Some(_) => Edit::Replace(value.clone()),
                    None => Edit::Insert(value.clone()),
                };
                edits.push((vec![key.to_owned(), name.clone()], edit));
            }
            for (name, _) in was {
                if !now.iter().any(|(new, _)| new == name) {
                    edits.push((vec![key.to_owned(), name.clone()], Edit::Remove));
                }
            }
            continue;
        }
        let present = config_entry::get(Format::Json, text, &[key])
            .map_err(|error| refused(error.to_string()))?
            .is_some();
        let edit = if present {
            Edit::Replace(now)
        } else {
            Edit::Insert(now)
        };
        edits.push((vec![key.to_owned()], edit));
    }

    let mut text = text.to_owned();
    for (key, edit) in edits {
        let key: Vec<&str> = key.iter().map(String::as_str).collect();
        text = config_entry::edit(Format::Json, &text, &key, &edit)
            .map_err(|error| refused(error.to_string()))?;
    }
    Ok(text)
}

/// The members of a settings file's object, read a key at a time.
struct Members<'a>(&'a [(String, Value)]);

impl<'a> Members<'a> {
    fn settings(&self, version: u32) -> Result<Settings, String> {
        Ok(Settings {
            version,
            id: self.id()?,
            name: self.string(NAME)?.to_owned(),
            agents: self.agents()?,
            share: self.share()?,
            edit_tracked: self.edit_tracked()?,
            directories: self.directories()?,
        })
    }

    /// The member named `key`, refused when the file writes it twice: which
    /// one a reader keeps is that reader's, so neither is guessed at.
    fn get(&self, key: &str) -> Result<Option<&'a Value>, String> {
        let mut named = self.0.iter().filter(|(name, _)| name == key);
        match (named.next(), named.next()) {
            (None, _) => Ok(None),
            (Some((_, value)), None) => Ok(Some(value)),
            (Some(_), Some(_)) => Err(format!(
                "`{key}` is written more than once, so which one is meant cannot be told"
            )),
        }
    }

    fn required(&self, key: &str) -> Result<&'a Value, String> {
        self.get(key)?.ok_or_else(|| format!("`{key}` is missing"))
    }

    fn version(&self) -> Result<u32, String> {
        match self.required(VERSION)? {
            Value::Integer(version) => u32::try_from(*version).ok(),
            _ => None,
        }
        .ok_or_else(|| format!("`{VERSION}` is not a version number"))
    }

    fn id(&self) -> Result<ProjectId, String> {
        match self.required(ID)? {
            Value::String(id) => ProjectId::parse(id),
            _ => None,
        }
        .ok_or_else(|| {
            format!(
                "`{ID}` is not {} lowercase hexadecimal digits",
                ProjectId::LEN
            )
        })
    }

    fn string(&self, key: &str) -> Result<&'a str, String> {
        match self.required(key)? {
            Value::String(text) => Ok(text),
            _ => Err(format!("`{key}` is not a string")),
        }
    }

    fn agents(&self) -> Result<Vec<Agent>, String> {
        let Value::Array(names) = self.required(AGENTS)? else {
            return Err(format!("`{AGENTS}` is not a list of agent names"));
        };
        names
            .iter()
            .map(|name| match name {
                Value::String(name) => name
                    .parse::<Agent>()
                    .map_err(|error| format!("`{AGENTS}`: {error}")),
                _ => Err(format!("`{AGENTS}` is not a list of agent names")),
            })
            .collect()
    }

    fn share(&self) -> Result<Share, String> {
        match self.required(SHARE)? {
            Value::String(word) => Share::named(word),
            _ => None,
        }
        .ok_or_else(|| {
            format!(
                "`{SHARE}` is neither \"{}\" nor \"{}\"",
                Share::Local,
                Share::Shared
            )
        })
    }

    /// Off when the file leaves it out: the safe answer, which edits no
    /// team's file.
    fn edit_tracked(&self) -> Result<bool, String> {
        match self.get(EDIT_TRACKED)? {
            None => Ok(false),
            Some(Value::Bool(edit)) => Ok(*edit),
            Some(_) => Err(format!("`{EDIT_TRACKED}` is neither true nor false")),
        }
    }

    /// None when the file leaves it out, which is a project authoring
    /// nothing of its own.
    fn directories(&self) -> Result<Directories, String> {
        let entries = match self.get(DIRECTORIES)? {
            None => return Ok(Directories::default()),
            Some(Value::Object(entries)) => entries,
            Some(_) => return Err(format!("`{DIRECTORIES}` is not an object")),
        };
        let entries = Members(entries);
        let mut directories = Directories::default();
        for (key, _) in entries.0 {
            let Value::String(directory) = entries.required(key)? else {
                return Err(format!("`{DIRECTORIES}.{key}` is not a string"));
            };
            directories = directories.with_key(key, directory);
        }
        Ok(directories)
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
