//! A project's marker from version 3 on: `.mind/settings.toml`, and before
//! [`SETTINGS_TOML_VERSION`] the `.mind/settings.json` it replaced, holding
//! what the hub needs to know about the project and nothing else.
//!
//! Which agents read it, whether what is installed into it is shared with the
//! team or kept to this checkout (D-02), whether a team file git tracks may
//! be edited, where the project authors artifacts of its own (D-06), and,
//! when not in the folders its agents read, where what is installed into it
//! lands (the owner's amendment to D-07), beside an id that names it for
//! good and the name it is shown by (D-09).
//! These describe the files physically in the project, and several
//! workspaces may link one project, so they live in the project rather than
//! in any one workspace's `flayer.toml`, where two workspaces could disagree
//! about one project's files.
//!
//! TOML, as `flayer.toml` is, the owner's choice (D-06, amended): both
//! markers are written from a template whose comments say what each key
//! does, and edited through toml_edit, which keeps every byte an edit does
//! not touch, those comments included. JSON, which the settings were
//! written in first, has no comments to say it with, and was edited a
//! third way, through `config_entry`, which is for the agents' own config
//! files. A `settings.json` is still read the way it was written, versions
//! 3 and 4 alike, and changed by nothing but the migration that moves it to
//! a `settings.toml` (`mind migrate`): read through the same key-at-a-time
//! reader, so a key each holds is held to the same rules in both.
//!
//! A key this build does not know is carried rather than refused: read past,
//! and kept by an edit, where it was. The format grows, and a project
//! configured by a newer Mindflayer is still a project, as it is for unknown
//! front matter. A key that changes what an older build would do with the
//! project is the exception, and comes with a version of its own, which
//! that build refuses: `targets` came with [`TARGETS_VERSION`], which a
//! `settings.json` holds only while it sets them, and a `settings.toml`, which
//! no build from before them reads, holds them at its own version.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};

use crate::agent::Agent;
use crate::config_entry::{self, Format, Value};
use crate::kind::Kind;
use crate::paths;
use crate::workspace::{
    check_version, toml_string, Directories, Marker, MarkerFile, WorkspaceError, MIND_DIR,
    SETTINGS_TOML_VERSION, SETTINGS_VERSION, TARGETS_VERSION,
};

/// The marker file of a project from [`SETTINGS_TOML_VERSION`] on, inside
/// its `.mind`: what `mind init` writes.
pub const SETTINGS_FILE: &str = "settings.toml";

/// The marker file of a version 3 or 4 project, inside its `.mind`: read as
/// it was written, and moved to a [`SETTINGS_FILE`] by `mind migrate`.
pub const SETTINGS_JSON_FILE: &str = "settings.json";

/// The contents of a project's settings file, `.mind/settings.toml` or the
/// `.mind/settings.json` before it.
///
/// Every key the format defines, and no other: what a newer build added is
/// left in the file, and an edit leaves it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The layout version the file was written with, which says which file
    /// it is in: [`SETTINGS_TOML_VERSION`] for a `settings.toml`; for a
    /// `settings.json`, [`SETTINGS_VERSION`], or [`TARGETS_VERSION`] for one
    /// that says where installs land. One this build does not know is
    /// refused.
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
    /// Where what is installed into the project lands, for a kind it names,
    /// in place of the folders each agent reads (`TargetTable::standard`).
    /// A kind it leaves out, every kind in a new project, goes where its
    /// agents read.
    pub targets: Targets,
}

impl Settings {
    /// A new project's settings, at [`SETTINGS_TOML_VERSION`], with an id
    /// of its own and `edit_tracked` off: the one setting nobody chooses at
    /// the start, and the one that edits a team's files.
    pub fn new(
        name: impl Into<String>,
        agents: Vec<Agent>,
        share: Share,
        directories: Directories,
    ) -> Self {
        Settings {
            version: SETTINGS_TOML_VERSION,
            id: ProjectId::generate(),
            name: name.into(),
            agents,
            share,
            edit_tracked: false,
            directories,
            targets: Targets::default(),
        }
    }

    /// Read the settings file at `path`, in the grammar its name says
    /// ([`grammar`]).
    ///
    /// Refused when it does not parse (a `settings.json` that is not strict
    /// JSON included), when a key the format defines is missing or holds the
    /// wrong thing (but for `targets` in a version 3 file, read past then as
    /// before them), and when its version is not one its file holds: a newer
    /// one as written by a newer Mindflayer, any other as another file's
    /// ([`MarkerFile::of_version`]), a `settings.json` holding 3 or 4 and a
    /// `settings.toml` [`SETTINGS_TOML_VERSION`]. A key this build does not
    /// know is read past.
    pub fn read(path: &Path) -> Result<Self, WorkspaceError> {
        let text = std::fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        parse(path, &text)
    }

    /// The text of a new `settings.toml` holding these, written from a
    /// template rather than serialized, as `flayer.toml` is: its comments say
    /// what each key does, in the file somebody opens to change one, and an
    /// edit keeps them ([`MindProject::change_settings`]). Version
    /// [`SETTINGS_TOML_VERSION`] whatever these say, the one version that
    /// file holds.
    ///
    /// Every folder `directories` names is written out, the kinds this
    /// build knows first, as `mind.toml` spelled out its defaults; a
    /// `[targets]` table only when it names a kind, since leaving it out
    /// says the same. Each path is spelled with `/`, so the file reads the
    /// same on every platform, and one that is not UTF-8 is refused; each
    /// key is quoted when TOML cannot hold it bare ([`toml_key`]).
    ///
    /// [`MindProject::change_settings`]: crate::MindProject::change_settings
    pub fn text(&self) -> Result<String, WorkspaceError> {
        let agents: Vec<String> = self
            .agents
            .iter()
            .map(|agent| toml_string(agent.name()))
            .collect();
        let mut text = format!(
            "# Mindflayer mind project: what Mindflayer needs to know about the project\n\
             # this {MIND_DIR} is in, and nothing else. No agent reads it.\n\
             #\n\
             # `name` is what the project is shown as, and `agents` the agents that work\n\
             # in it (claude, codex, opencode): what is installed into the project is\n\
             # copied where each of them reads. `mind agents` changes them.\n\
             #\n\
             # `share` is \"local\" to keep {MIND_DIR}, and what is installed, out of git\n\
             # for this checkout alone, or \"shared\" to leave both for the team to\n\
             # commit. `edit_tracked` lets a file git tracks, such as AGENTS.md, be\n\
             # edited to reach an agent in a local project. `mind share` changes both.\n\
             #\n\
             # `directories` says where the project keeps the skills and rules it writes\n\
             # itself, relative to the directory holding {MIND_DIR}, to commit with its\n\
             # code; a kind left out has no folder. A `[targets]` table, when there,\n\
             # names the folders a kind is installed in instead of where the agents\n\
             # read: `mind targets` writes it.\n\
             #\n\
             # `id` names the project for good, wherever it moves, and `version` is how\n\
             # this file is laid out: change neither.\n\
             {VERSION} = {SETTINGS_TOML_VERSION}\n\
             {ID} = {}\n\
             {NAME} = {}\n\
             {AGENTS} = [{}]\n\
             {SHARE} = {}\n\
             {EDIT_TRACKED} = {}\n\
             \n\
             [{DIRECTORIES}]\n",
            toml_string(self.id.as_str()),
            toml_string(&self.name),
            agents.join(", "),
            toml_string(self.share.name()),
            self.edit_tracked,
        );
        for (key, directory) in self.directories.entries() {
            text.push_str(&format!("{} = {}\n", toml_key(key), toml_path(directory)?));
        }
        if !self.targets.is_empty() {
            text.push_str(&format!("\n[{TARGETS}]\n"));
            for (key, folders) in self.targets.entries() {
                let folders = folders
                    .iter()
                    .map(|folder| toml_path(folder))
                    .collect::<Result<Vec<String>, WorkspaceError>>()?;
                text.push_str(&format!("{} = [{}]\n", toml_key(key), folders.join(", ")));
            }
        }
        Ok(text)
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

/// Where a project's settings install each kind they name, in place of the
/// folders its agents read: `"targets": {"skills": [".skills"]}` sends every
/// skill installed into the project to `.skills`, for every agent it names.
///
/// Keyed by the kind's folder name, as [`Directories`] is, and a key this
/// build does not know is carried for the same reason. Each folder is as the
/// file spells it, relative to the project's root: whether an install may
/// use it is the project's to say when it hands its targets on
/// ([`MindProject::targets`](crate::MindProject::targets)), so a folder
/// written by hand that leads out of the project is read, said, and used for
/// nothing, rather than refusing every other thing the file says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Targets(BTreeMap<String, Vec<PathBuf>>);

impl Targets {
    /// The folders `kind` is installed in, if the settings name any.
    pub fn get(&self, kind: Kind) -> Option<&[PathBuf]> {
        self.0.get(kind.folder()).map(Vec::as_slice)
    }

    /// Say that `kind` is installed in `folders`.
    pub fn with(mut self, kind: Kind, folders: Vec<PathBuf>) -> Self {
        self.0.insert(kind.folder().to_owned(), folders);
        self
    }

    /// Say nothing of where `kind` is installed, so it goes where the
    /// project's agents read.
    pub fn without(mut self, kind: Kind) -> Self {
        self.0.remove(kind.folder());
        self
    }

    /// Whether no kind is named, which is every kind installed where the
    /// agents read.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Say where whatever `key` names is installed, a key this build may not
    /// know included.
    fn with_key(mut self, key: &str, folders: Vec<PathBuf>) -> Self {
        self.0.insert(key.to_owned(), folders);
        self
    }

    /// Every entry, the kinds this build knows first, in report order, then
    /// any other key in name order, as [`Directories`] lists its own.
    pub(crate) fn entries(&self) -> Vec<(&str, &[PathBuf])> {
        let known = Kind::ALL
            .into_iter()
            .filter_map(|kind| self.0.get_key_value(kind.folder()));
        let other = self
            .0
            .iter()
            .filter(|(key, _)| Kind::ALL.iter().all(|kind| kind.folder() != key.as_str()));
        known
            .chain(other)
            .map(|(key, folders)| (key.as_str(), folders.as_slice()))
            .collect()
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

/// As its text, the one spelling settings hold it in and an exclude group
/// names it by, rather than as a struct around it.
impl Serialize for ProjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `path` as a TOML string spelled with `/`, refused when it is not UTF-8.
fn toml_path(path: &Path) -> Result<String, WorkspaceError> {
    config_path(path).map(|written| toml_string(&written))
}

/// `key` as a TOML key, quoted when it is not a bare one, as toml_edit
/// writes a key an edit adds ([`edited`]), so the two spell a key alike.
///
/// Quoted only when it has to be, rather than always, so the kinds this
/// build knows read `skills = ...` as the template's comments do. Not left
/// bare: a key this build does not know is carried from a file that quoted
/// it, `"my.kind"` in a `settings.json` or a `mind.toml`, and written bare
/// would be a dotted key, or no key at all, in a file no command could read.
fn toml_key(key: &str) -> String {
    toml_edit::Key::new(key).display_repr().into_owned()
}

const VERSION: &str = "version";
const ID: &str = "id";
const NAME: &str = "name";
const AGENTS: &str = "agents";
const SHARE: &str = "share";
const EDIT_TRACKED: &str = "edit_tracked";
const DIRECTORIES: &str = "directories";
const TARGETS: &str = "targets";

/// The grammar the settings file at `path` is written in, which its name
/// says: JSON for a `settings.json`, TOML for a `settings.toml`.
///
/// By the name, unlike an agent's config file (`config_entry`, where the
/// caller says): each settings file was only ever written in one grammar,
/// and its name is how the project tells which it is.
pub(crate) fn grammar(path: &Path) -> Format {
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        Format::Json
    } else {
        Format::Toml
    }
}

/// The settings `text` holds, read from the file at `path`, which errors
/// name, in the grammar its name says ([`grammar`]).
///
/// Either grammar is read into one tree and held to the keys a key at a
/// time, so a `settings.toml` and a `settings.json` holding the same are
/// read alike, and refused alike, each refusal naming the key.
///
/// The version is read first and held to this build before anything else,
/// so a file a newer Mindflayer wrote is refused for being newer, whatever
/// else it holds, rather than for a key that version gave a new meaning.
/// Then to the versions its file holds: one another file holds is
/// malformed rather than guessed at, since what it would mean here is
/// nobody's to say.
pub(crate) fn parse(path: &Path, text: &str) -> Result<Settings, WorkspaceError> {
    let malformed = |detail: String| WorkspaceError::Malformed {
        path: path.to_path_buf(),
        detail,
    };
    let grammar = grammar(path);
    let document =
        config_entry::document(grammar, text).map_err(|error| malformed(error.to_string()))?;
    let Value::Object(members) = &document else {
        return Err(malformed("it holds no object".to_owned()));
    };
    let members = Members::new(members, grammar);

    let version = members.version().map_err(malformed)?;
    check_version(path, Marker::Project, version)?;
    let (file, held) = match grammar {
        Format::Toml => (
            MarkerFile::SettingsToml,
            format!("version {SETTINGS_TOML_VERSION}"),
        ),
        _ => (
            MarkerFile::SettingsJson,
            format!("version {SETTINGS_VERSION} or {TARGETS_VERSION}"),
        ),
    };
    let written = MarkerFile::of_version(version);
    if written != file {
        return Err(malformed(format!(
            "version {version} is a {}'s, and a {} is {held}",
            written.name(),
            file.name()
        )));
    }
    members.settings(version).map_err(malformed)
}

/// `text`, the `settings.toml` at `path` holding `before`, changed to hold
/// `wanted` through toml_edit, as `flayer.toml` is edited, with every byte
/// that is not a changed value as it was.
///
/// Only the keys whose values differ are touched, `directories` and
/// `targets` an entry at a time: a key this build does not know, the order
/// the keys are written in, the comments above a key and after its value on
/// its line, and a value that did not change all stay as the file had them,
/// so the template's comments and a file somebody laid out by hand are kept.
/// A changed value is written as toml_edit writes a new one, where the old
/// one stood. A key the file left out goes in after the others in its table,
/// and only when what it holds is not what leaving it out reads as:
/// `edit_tracked` off is not written in by an edit that changes something
/// else.
///
/// The version and the id are refused rather than changed: a version moves
/// with a migration, which writes the file its version describes, and an id
/// names the project for good. A `settings.toml` is [`SETTINGS_TOML_VERSION`]
/// whatever it sets, so no edit moves it, `targets` included.
///
/// `targets` emptied is taken out whole, its `[targets]` header included,
/// since leaving it out is what a file that installs where its agents read
/// says; but for a comment somebody wrote above the header, which keeps the
/// header, empty, since an empty table says the same and their words stay.
/// What is taken out of a table takes the comments on its own lines with it.
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
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| refused(error.to_string()))?;
    let root = document.as_table_mut();

    if wanted.name != before.name {
        set(root, NAME, wanted.name.as_str().into());
    }
    if wanted.agents != before.agents {
        let agents: toml_edit::Array = wanted.agents.iter().map(|agent| agent.name()).collect();
        set(root, AGENTS, agents.into());
    }
    if wanted.share != before.share {
        set(root, SHARE, wanted.share.name().into());
    }
    if wanted.edit_tracked != before.edit_tracked
        && (wanted.edit_tracked || root.contains_key(EDIT_TRACKED))
    {
        set(root, EDIT_TRACKED, wanted.edit_tracked.into());
    }

    if wanted.directories != before.directories {
        edit_entries(
            path,
            root,
            DIRECTORIES,
            &before.directories.entries(),
            &wanted.directories.entries(),
            |directory| Ok(config_path(directory)?.into()),
        )?;
    }
    if wanted.targets != before.targets {
        if wanted.targets.is_empty() {
            take_out(root, TARGETS);
        } else {
            edit_entries(
                path,
                root,
                TARGETS,
                &before.targets.entries(),
                &wanted.targets.entries(),
                |folders| {
                    let written = folders
                        .iter()
                        .map(|folder| config_path(folder))
                        .collect::<Result<toml_edit::Array, WorkspaceError>>()?;
                    Ok(written.into())
                },
            )?;
        }
    }
    Ok(document.to_string())
}

/// `text`, the `settings.toml` at `path`, holding `id` in place of the id
/// it holds, through toml_edit as [`edited`] writes a changed value, with
/// every other byte as it was.
///
/// Apart from [`edited`], which refuses to change an id: a change to a
/// project's settings never moves the id that names it for good, and giving
/// it another is an act of its own, for a copy made with its `.mind`
/// (`mind init --new-id`, [`crate::relink::new_id`]).
pub(crate) fn renewed(path: &Path, text: &str, id: &ProjectId) -> Result<String, WorkspaceError> {
    let mut document: toml_edit::DocumentMut =
        text.parse()
            .map_err(|error: toml_edit::TomlError| WorkspaceError::Edit {
                path: path.to_path_buf(),
                detail: error.to_string(),
            })?;
    set(document.as_table_mut(), ID, id.as_str().into());
    Ok(document.to_string())
}

/// `value` written at `key` of `table`, in place of what is there, keeping
/// the spacing around it and a comment after it on its line; after the
/// last key when there is none.
fn set(table: &mut dyn toml_edit::TableLike, key: &str, mut value: toml_edit::Value) {
    match table.get_mut(key) {
        Some(toml_edit::Item::Value(old)) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        Some(other) => *other = toml_edit::Item::Value(value),
        None => {
            table.insert(key, toml_edit::Item::Value(value));
        }
    }
}

/// The table at `key` of `root` changed from holding `was` to holding
/// `now`, each entry written by `write`, an entry at a time: one the change
/// leaves alone, a key this build does not know included, stays where it
/// is, and one it adds goes after the others. A table the file does not
/// have is made, as a `[key]` table after the others.
fn edit_entries<V: PartialEq + ?Sized>(
    path: &Path,
    root: &mut toml_edit::Table,
    key: &str,
    was: &[(&str, &V)],
    now: &[(&str, &V)],
    write: impl Fn(&V) -> Result<toml_edit::Value, WorkspaceError>,
) -> Result<(), WorkspaceError> {
    let table = root
        .entry(key)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| WorkspaceError::Edit {
            path: path.to_path_buf(),
            detail: format!("`{key}` is not a table"),
        })?;
    for (name, value) in now {
        if !was.iter().any(|(old, held)| old == name && held == value) {
            set(table, name, write(value)?);
        }
    }
    for (name, _) in was {
        if !now.iter().any(|(new, _)| new == name) {
            table.remove(name);
        }
    }
    Ok(())
}

/// The whole of `key` taken out of `root`, but for a table somebody wrote a
/// comment above, which is emptied and kept with its comment.
fn take_out(root: &mut toml_edit::Table, key: &str) {
    let commented = match root.get(key) {
        Some(toml_edit::Item::Table(table)) => table
            .decor()
            .prefix()
            .and_then(toml_edit::RawString::as_str)
            .is_some_and(|prefix| prefix.contains('#')),
        _ => false,
    };
    match root.get_mut(key) {
        Some(toml_edit::Item::Table(table)) if commented => table.clear(),
        _ => {
            root.remove(key);
        }
    }
}

/// `path` spelled with `/`, as a settings file holds it, refused when it is
/// not UTF-8.
fn config_path(path: &Path) -> Result<String, WorkspaceError> {
    paths::to_config_string(path).ok_or_else(|| WorkspaceError::NonUtf8Path {
        path: path.to_path_buf(),
    })
}

/// The members of a settings file's object, read a key at a time.
struct Members<'a> {
    members: &'a [(String, Value)],
    /// What the grammar calls a group of keys, for a refusal to say: "an
    /// object" in JSON, "a table" in TOML.
    group: &'static str,
}

impl<'a> Members<'a> {
    fn new(members: &'a [(String, Value)], grammar: Format) -> Self {
        let group = match grammar {
            Format::Toml => "a table",
            _ => "an object",
        };
        Members { members, group }
    }

    /// The members of `entries`, an object inside this one.
    fn inside(&self, entries: &'a [(String, Value)]) -> Self {
        Members {
            members: entries,
            group: self.group,
        }
    }
    fn settings(&self, version: u32) -> Result<Settings, String> {
        Ok(Settings {
            version,
            id: self.id()?,
            name: self.string(NAME)?.to_owned(),
            agents: self.agents()?,
            share: self.share()?,
            edit_tracked: self.edit_tracked()?,
            directories: self.directories()?,
            targets: self.targets(version)?,
        })
    }

    /// The member named `key`, refused when the file writes it twice: which
    /// one a reader keeps is that reader's, so neither is guessed at.
    fn get(&self, key: &str) -> Result<Option<&'a Value>, String> {
        let mut named = self.members.iter().filter(|(name, _)| name == key);
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
            Some(_) => return Err(format!("`{DIRECTORIES}` is not {}", self.group)),
        };
        let entries = self.inside(entries);
        let mut directories = Directories::default();
        for (key, _) in entries.members {
            let Value::String(directory) = entries.required(key)? else {
                return Err(format!("`{DIRECTORIES}.{key}` is not a string"));
            };
            directories = directories.with_key(key, directory);
        }
        Ok(directories)
    }

    /// None when the file leaves it out, which is every kind installed where
    /// the project's agents read. Read whatever the version says, a version
    /// 3 `settings.json` given `targets` by hand included: somebody wrote
    /// them to be used, and the only reader that could miss them is a build
    /// from before them. Nothing this build does leaves one so: such a file
    /// is changed by nothing, and `mind migrate` moves what it holds,
    /// `targets` included, to a `settings.toml` at its own version, which
    /// no build from before them reads.
    ///
    /// A kind named with no folder is refused rather than read as installed
    /// nowhere: leaving the kind out is how a file says the standard
    /// folders, and a project that installs a kind nowhere is one an install
    /// can only refuse. Refused only from [`TARGETS_VERSION`], the version
    /// that defines the key: in a version 3 file, which every build before
    /// it opened with the key read past, one that does not read as targets
    /// is read past still, rather than closing a project those builds
    /// opened. The move carries only what reads, so such targets stay
    /// where they were written, in the `settings.json` it keeps.
    fn targets(&self, version: u32) -> Result<Targets, String> {
        match self.strict_targets() {
            Err(_) if version < TARGETS_VERSION => Ok(Targets::default()),
            read => read,
        }
    }

    /// `targets`, held to what the key holds.
    fn strict_targets(&self) -> Result<Targets, String> {
        let entries = match self.get(TARGETS)? {
            None => return Ok(Targets::default()),
            Some(Value::Object(entries)) => entries,
            Some(_) => return Err(format!("`{TARGETS}` is not {}", self.group)),
        };
        let entries = self.inside(entries);
        let mut targets = Targets::default();
        for (key, _) in entries.members {
            let not_a_list = || format!("`{TARGETS}.{key}` is not a list of folders");
            let Value::Array(folders) = entries.required(key)? else {
                return Err(not_a_list());
            };
            if folders.is_empty() {
                return Err(format!(
                    "`{TARGETS}.{key}` names no folder; leave `{key}` out to install them \
                     where the project's agents read"
                ));
            }
            let folders = folders
                .iter()
                .map(|folder| match folder {
                    Value::String(folder) => Ok(PathBuf::from(folder)),
                    _ => Err(not_a_list()),
                })
                .collect::<Result<Vec<PathBuf>, String>>()?;
            targets = targets.with_key(key, folders);
        }
        Ok(targets)
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
