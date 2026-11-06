//! A Claude Code plugin marketplace (#103): a repository whose
//! `.claude-plugin/marketplace.json` lists plugins, each in a folder of the
//! marketplace's own repository or in a repository of its own.
//!
//! Its catalog is read here, an entry at a time ([`Catalog`]). It is JSON
//! somebody else wrote, so nothing in it is trusted: an entry that cannot be
//! read, names a folder outside the repository it is in, or is a plugin no
//! gather makes ([`EntryFailure`]) is said on its own, and the rest of the
//! catalog is read all the same; a key it does not know is passed over;
//! nothing it names is ever run, a `command` source and a `headersHelper`
//! among them; and every string it holds reaches a front end as the text it
//! is, for the front end to make printable as it does a gather's.
//!
//! A marketplace is a parent source with one child per entry gathered
//! (D-13): what is listed is gathered only when somebody picks it.

use std::borrow::Cow;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jsonc_parser::{JsonArray, JsonObject, JsonValue, ParseOptions};
use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

use crate::code::codes;
use crate::copy;
use crate::paths;
use crate::url::{fetched, is_remote, redacted};

/// Where a marketplace's catalog is, from the folder the marketplace is in:
/// the root of its repository, unless a tree URL or `--path` names another.
pub const CATALOG: &str = ".claude-plugin/marketplace.json";

/// How much of a catalog is read: more than any marketplace lists (the
/// largest known, claude-community, lists some two thousand plugins in
/// well under a megabyte), and never a file somebody made large enough to
/// be a reason not to finish.
pub const CATALOG_CAP: u64 = 16 * 1024 * 1024;

/// What a catalog says, as read: its own name, how many entries it lists,
/// each that can be gathered, in its order, and why each other cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// The name the catalog gives the marketplace, as written: what the
    /// source is called unless somebody names it.
    pub name: Option<String>,
    /// How many entries its `plugins` list holds, read or not.
    pub listed: usize,
    /// Each entry that can be gathered, in the order it lists them.
    pub entries: Vec<Entry>,
    /// Each entry that cannot, with why, in the same order.
    pub failures: Vec<EntryFailure>,
}

/// One plugin a catalog lists, which a gather can make a child source of.
///
/// What it serializes to is a contract, as a report's is: each field by its
/// name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Its name, as the catalog writes it: what `--plugin` takes for it.
    pub name: String,
    /// The one line the catalog says of it.
    pub description: Option<String>,
    /// Where it is.
    pub plugin: Plugin,
    /// The skills it names, each a folder from its plugin's root, when it
    /// names them: those are its skills, and no others. `None` when it names
    /// none, and its plugin's `skills/` and a `SKILL.md` at its root are.
    pub skills: Option<Vec<String>>,
    /// Its licence, as an SPDX identifier the catalog gives: a skill's own
    /// for one that says none.
    pub license: Option<String>,
    /// What else of a plugin it declares in the catalog itself, which no
    /// gather takes.
    pub declares: Vec<Holding>,
}

/// Where an entry's plugin is: in the marketplace's own repository, or in a
/// repository of its own, at a branch and a commit when the catalog names
/// them.
///
/// Its fields are untrusted text: a `repo` is held to `owner/repo`, a URL to
/// another machine's ([`is_remote`]), and a folder to its repository, so a
/// catalog reaches nothing on this machine but what its own repository
/// holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plugin {
    /// `./x`, `.`, or a bare name under the catalog's `metadata.pluginRoot`:
    /// a folder of the marketplace's repository, by its route from the
    /// marketplace's folder, empty for that folder itself.
    Relative { route: String },
    /// `{"source": "github", "repo": "owner/repo"}`: a GitHub repository.
    Github {
        repo: String,
        reference: Option<String>,
        sha: Option<String>,
    },
    /// `{"source": "url", "url": ...}`: a git repository at a URL.
    Url {
        url: String,
        reference: Option<String>,
        sha: Option<String>,
    },
    /// `{"source": "git-subdir", "url": ..., "path": ...}`: a folder of a
    /// git repository at a URL.
    GitSubdir {
        url: String,
        path: String,
        reference: Option<String>,
        sha: Option<String>,
    },
}

impl Plugin {
    /// The kind of source it is, as the catalog spells it, `relative` for a
    /// folder of the marketplace's own repository.
    pub fn kind(&self) -> &'static str {
        match self {
            Plugin::Relative { .. } => "relative",
            Plugin::Github { .. } => "github",
            Plugin::Url { .. } => "url",
            Plugin::GitSubdir { .. } => "git-subdir",
        }
    }

    /// The repository it is fetched from, for one of its own:
    /// `https://github.com/<repo>` for a GitHub one, its URL otherwise.
    pub fn url(&self) -> Option<Cow<'_, str>> {
        match self {
            Plugin::Relative { .. } => None,
            Plugin::Github { repo, .. } => Some(fetched(repo)),
            Plugin::Url { url, .. } | Plugin::GitSubdir { url, .. } => Some(Cow::Borrowed(url)),
        }
    }

    /// Its folder in its repository: a relative one's route from the
    /// marketplace's folder, a `git-subdir`'s path, empty for the root.
    pub fn path(&self) -> &str {
        match self {
            Plugin::Relative { route } => route,
            Plugin::GitSubdir { path, .. } => path,
            Plugin::Github { .. } | Plugin::Url { .. } => "",
        }
    }

    /// The branch or tag the catalog names, if any.
    pub fn reference(&self) -> Option<&str> {
        match self {
            Plugin::Relative { .. } => None,
            Plugin::Github { reference, .. }
            | Plugin::Url { reference, .. }
            | Plugin::GitSubdir { reference, .. } => reference.as_deref(),
        }
    }

    /// The commit the catalog pins it to, if any, as written.
    pub fn sha(&self) -> Option<&str> {
        match self {
            Plugin::Relative { .. } => None,
            Plugin::Github { sha, .. }
            | Plugin::Url { sha, .. }
            | Plugin::GitSubdir { sha, .. } => sha.as_deref(),
        }
    }
}

/// As an object holding its kind, the word the catalog spells it with, the
/// repository it is fetched from, only through [`redacted`], its folder in
/// it, and its branch and commit.
///
/// By hand rather than derived: a derive would tag it with the variant's
/// Rust name, where the catalog's own word is the one a person reads in it.
impl Serialize for Plugin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut plugin = serializer.serialize_struct("Plugin", 5)?;
        plugin.serialize_field("kind", self.kind())?;
        plugin.serialize_field("url", &self.url().map(|url| redacted(&url).into_owned()))?;
        plugin.serialize_field("path", self.path())?;
        plugin.serialize_field("reference", &self.reference())?;
        plugin.serialize_field("sha", &self.sha())?;
        plugin.end()
    }
}

/// What a plugin holds beside its skills, which a gather reports and never
/// takes: none of it is a skill, and each is something an agent runs or
/// reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Holding {
    Hooks,
    Agents,
    McpServers,
}

impl Holding {
    /// How a report says it.
    pub const fn words(self) -> &'static str {
        match self {
            Holding::Hooks => "hooks",
            Holding::Agents => "agents",
            Holding::McpServers => "MCP servers",
        }
    }

    /// The key a catalog's entry declares it under, and the file or folder
    /// of a plugin it is kept in.
    const fn declared(self) -> (&'static str, &'static str) {
        match self {
            Holding::Hooks => ("hooks", "hooks"),
            Holding::Agents => ("agents", "agents"),
            Holding::McpServers => ("mcpServers", ".mcp.json"),
        }
    }

    const ALL: [Holding; 3] = [Holding::Hooks, Holding::Agents, Holding::McpServers];

    /// What the plugin at `root` holds of them, as files or folders where a
    /// plugin keeps each: looked at, never read or followed.
    pub fn held_at(root: &Path) -> Vec<Holding> {
        Holding::ALL
            .into_iter()
            .filter(|holding| root.join(holding.declared().1).symlink_metadata().is_ok())
            .collect()
    }
}

/// By the key a catalog declares it under, the word its format gives it,
/// as a plugin's kind is the catalog's own word.
impl Serialize for Holding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.declared().0)
    }
}

impl fmt::Display for Holding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.words())
    }
}

/// The archives a `url` source can name, by how their URL ends: a plugin
/// packed into one is not a git repository, and is not gathered.
const ARCHIVES: [&str; 8] = [
    ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".tar.zst",
];

/// What a catalog is read with: JSON, as Claude Code reads it, and nothing
/// jsonc-parser's default would add to it. Its tree, which stops at a depth
/// of nesting a catalog made deep enough to overflow the stack would pass,
/// rather than one parsed into types, so an entry that does not fit costs
/// only itself.
fn options() -> ParseOptions {
    ParseOptions {
        allow_comments: false,
        allow_trailing_commas: false,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}

impl Catalog {
    /// Read the catalog of the marketplace in `folder` of the repository
    /// cloned at `clone` ([`CATALOG`]), and hold each relative entry to a
    /// folder the repository holds: one that is not there, or that leads
    /// out of the clone through a link, is a failure of its own.
    ///
    /// The catalog itself is held to the clone the same way: one that is a
    /// link leading out of it, or in a folder that does, is refused rather
    /// than read, since a repository can link it to any file on this
    /// machine (`marketplace_catalog_outside`).
    pub fn read(clone: &Path, folder: &str) -> Result<Catalog, MarketplaceError> {
        let path = clone.join(folder).join(CATALOG);
        let failed = |source| MarketplaceError::Read {
            path: path.clone(),
            source,
        };
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(MarketplaceError::CatalogMissing {
                    path: route_in(clone, &path),
                })
            }
            Err(error) => return Err(failed(error)),
        }
        let bound = fs::canonicalize(clone).map_err(failed)?;
        let (resolved, inside) =
            copy::followed(&path, std::slice::from_ref(&bound)).map_err(failed)?;
        if !inside {
            return Err(MarketplaceError::CatalogOutside {
                path: route_in(clone, &path),
                to: resolved,
            });
        }
        let mut bytes = Vec::new();
        let file = fs::File::open(&resolved).map_err(failed)?;
        io::Read::read_to_end(&mut io::Read::take(file, CATALOG_CAP + 1), &mut bytes)
            .map_err(failed)?;
        if bytes.len() as u64 > CATALOG_CAP {
            return Err(MarketplaceError::CatalogTooLarge {
                path: route_in(clone, &path),
            });
        }
        let text = String::from_utf8(bytes).map_err(|_| MarketplaceError::CatalogUnreadable {
            path: route_in(clone, &path),
            reason: String::from("it is not UTF-8"),
        })?;
        let catalog =
            Catalog::parse(&text).map_err(|reason| MarketplaceError::CatalogUnreadable {
                path: route_in(clone, &path),
                reason,
            })?;
        Ok(catalog.held_to(clone, folder, &bound))
    }

    /// What `text`, a catalog, says, each entry read on its own: or, when
    /// it is not a catalog at all, why, which is no JSON object with a
    /// `plugins` list.
    ///
    /// A key it does not know is passed over, as Claude Code passes it
    /// over. Two entries of one name are neither listed, each a failure
    /// (`marketplace_entry_duplicate`): which one `--plugin` would mean is
    /// not a guess to make, as two skills of one name in one source are
    /// neither gathered.
    pub fn parse(text: &str) -> Result<Catalog, String> {
        let value = jsonc_parser::parse_to_value(text, &options())
            .map_err(|error| format!("it is not JSON: {error}"))?;
        let Some(JsonValue::Object(root)) = value else {
            return Err(String::from("it is not a JSON object"));
        };
        let Some(plugins) = root.get_array("plugins") else {
            return Err(String::from("it has no `plugins` list"));
        };
        let plugin_root = root
            .get_object("metadata")
            .and_then(|metadata| metadata.get_string("pluginRoot"))
            .map(|root| root.to_string());
        let mut read: Vec<Result<Entry, EntryFailure>> = plugins
            .iter()
            .enumerate()
            .map(|(at, value)| entry(at, value, plugin_root.as_deref()))
            .collect();
        let names: Vec<Option<String>> = read
            .iter()
            .map(|read| match read {
                Ok(entry) => Some(entry.name.clone()),
                Err(failure) => failure.name().map(str::to_owned),
            })
            .collect();
        for (at, each) in read.iter_mut().enumerate() {
            let Some(name) = &names[at] else { continue };
            let again = names
                .iter()
                .enumerate()
                .any(|(other, its)| other != at && its.as_ref() == Some(name));
            if again {
                *each = Err(EntryFailure::Duplicate { name: name.clone() });
            }
        }
        let mut catalog = Catalog {
            name: root.get_string("name").map(|name| name.to_string()),
            listed: plugins.len(),
            entries: Vec::new(),
            failures: Vec::new(),
        };
        for each in read {
            match each {
                Ok(entry) => catalog.entries.push(entry),
                Err(failure) => catalog.failures.push(failure),
            }
        }
        Ok(catalog)
    }

    /// The same catalog, each relative entry that names no folder of the
    /// repository cloned at `clone`, its folder `folder` being the
    /// marketplace's, among the failures: one that is not there, and one
    /// leading out of the clone (`bound`, resolved) through a link.
    fn held_to(mut self, clone: &Path, folder: &str, bound: &Path) -> Catalog {
        let mut kept = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            let Plugin::Relative { route } = &entry.plugin else {
                kept.push(entry);
                continue;
            };
            let at = clone.join(folder).join(route);
            let shown = paths::as_route(&paths::normalize(&Path::new(folder).join(route)));
            match copy::followed(&at, std::slice::from_ref(&bound.to_path_buf())) {
                Ok((_, false)) => self.failures.push(EntryFailure::Outside {
                    name: entry.name,
                    path: displayed(&shown),
                }),
                Ok((resolved, true)) if resolved.is_dir() => kept.push(entry),
                Ok(_) | Err(_) => self.failures.push(EntryFailure::Missing {
                    name: entry.name,
                    path: displayed(&shown),
                }),
            }
        }
        self.entries = kept;
        self
    }

    /// The entry called `name`, if it lists one that can be gathered.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Whether it lists `name` at all, as an entry that can be gathered or
    /// as one that cannot.
    pub fn lists(&self, name: &str) -> bool {
        self.entry(name).is_some()
            || self
                .failures
                .iter()
                .any(|failure| failure.name() == Some(name))
    }
}

/// A route as a person reads one, `.` for the root.
fn displayed(route: &str) -> String {
    if route.is_empty() {
        String::from(".")
    } else {
        route.to_owned()
    }
}

/// Where `path` is in the repository cloned at `clone`, for an error to
/// name: the clone's own folder is the workspace's, and named for its own
/// reasons.
fn route_in(clone: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(clone)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

/// Why an entry is not one a gather makes a child of: a shape it does not
/// have, said with where it is in the list, or a failure of its own.
enum Unfit {
    Unreadable(&'static str),
    Failure(EntryFailure),
}

/// The entry at `at` in a catalog's `plugins` list, `plugin_root` being
/// what its `metadata.pluginRoot` names, or why it cannot be gathered.
fn entry(
    at: usize,
    value: &JsonValue<'_>,
    plugin_root: Option<&str>,
) -> Result<Entry, EntryFailure> {
    let unreadable = |name: Option<&str>, reason: &str| EntryFailure::Unreadable {
        at,
        name: name.map(str::to_owned),
        reason: reason.to_owned(),
    };
    let JsonValue::Object(object) = value else {
        return Err(unreadable(None, "it is not a JSON object"));
    };
    let name = match object.get("name") {
        Some(JsonValue::String(name)) if !name.trim().is_empty() => name.to_string(),
        _ => return Err(unreadable(None, "it has no name")),
    };
    let named = Some(name.as_str());
    let plugin = match object.get("source") {
        Some(JsonValue::String(path)) => relative(&name, path, plugin_root)?,
        Some(JsonValue::Object(source)) => match elsewhere(&name, source) {
            Ok(plugin) => plugin,
            Err(Unfit::Unreadable(reason)) => return Err(unreadable(named, reason)),
            Err(Unfit::Failure(failure)) => return Err(failure),
        },
        Some(_) => {
            return Err(unreadable(
                named,
                "its `source` is neither a path nor an object",
            ))
        }
        None => return Err(unreadable(named, "it has no `source`")),
    };
    let skills = match object.get("skills") {
        None | Some(JsonValue::Null) => None,
        Some(JsonValue::String(path)) => Some(vec![path.to_string()]),
        Some(JsonValue::Array(paths)) => Some(
            strings(paths).ok_or_else(|| unreadable(named, "its `skills` are not all paths"))?,
        ),
        Some(_) => return Err(unreadable(named, "its `skills` are not paths")),
    };
    Ok(Entry {
        description: text(object, "description"),
        license: text(object, "license"),
        declares: Holding::ALL
            .into_iter()
            .filter(|holding| {
                object
                    .get(holding.declared().0)
                    .is_some_and(|value| !matches!(value, JsonValue::Null))
            })
            .collect(),
        name,
        plugin,
        skills,
    })
}

/// The string `object` holds under `key`, when it holds one there that is
/// not blank.
fn text(object: &JsonObject<'_>, key: &str) -> Option<String> {
    object
        .get_string(key)
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Each string in `values`, or `None` when one is something else.
fn strings(values: &JsonArray<'_>) -> Option<Vec<String>> {
    values
        .iter()
        .map(|value| match value {
            JsonValue::String(text) => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

/// A relative entry's plugin: `path` as its route from the marketplace's
/// folder, held to that folder lexically, as `--path` is held to a
/// repository ([`crate::gather`]).
///
/// `.` and a path starting `./` are read from the marketplace's folder; a
/// bare name from `plugin_root`, when the catalog's `metadata.pluginRoot`
/// names one, as Claude Code reads it, and from the marketplace's folder
/// otherwise. Absolute, or climbing out (`..`), it is refused
/// (`marketplace_entry_outside`).
fn relative(name: &str, path: &str, plugin_root: Option<&str>) -> Result<Plugin, EntryFailure> {
    let bare = !(path == "."
        || path == ".."
        || path.starts_with("./")
        || path.starts_with("../")
        || path.starts_with(".\\")
        || path.starts_with("..\\")
        || crate::gather::outside(path));
    let joined = match plugin_root.filter(|_| bare) {
        Some(root) => Path::new(root).join(path),
        None => PathBuf::from(path),
    };
    if crate::gather::outside(&joined.to_string_lossy()) {
        return Err(EntryFailure::Outside {
            name: name.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(Plugin::Relative {
        route: paths::as_route(&paths::normalize(&joined)),
    })
}

/// The plugin a source object names, `source` being an entry's `source`: a
/// git repository of its own, or why it is no plugin a gather makes.
fn elsewhere(name: &str, source: &JsonObject<'_>) -> Result<Plugin, Unfit> {
    let failed = |failure: EntryFailure| Unfit::Failure(failure);
    let Some(kind) = source.get_string("source") else {
        return Err(Unfit::Unreadable("its `source` names no kind"));
    };
    let reference = text(source, "ref");
    let sha = text(source, "sha");
    let url = text(source, "url");
    match kind.as_ref() {
        "github" => {
            let repo = text(source, "repo").ok_or(Unfit::Unreadable("it names no `repo`"))?;
            // `owner/repo` and nothing else: `fetched` reads anything else as
            // the path it is, which would make a catalog's `github` entry a
            // folder on this machine.
            if matches!(fetched(&repo), Cow::Borrowed(_)) {
                return Err(Unfit::Unreadable("its `repo` is not `owner/repo`"));
            }
            Ok(Plugin::Github {
                repo,
                reference,
                sha,
            })
        }
        "url" => {
            let url = url.ok_or(Unfit::Unreadable("it names no `url`"))?;
            if is_archive(&url) {
                return Err(failed(EntryFailure::Archive {
                    name: name.to_owned(),
                    url: redacted(&url).into_owned(),
                }));
            }
            remote(name, &url).map_err(failed)?;
            Ok(Plugin::Url {
                url,
                reference,
                sha,
            })
        }
        "git-subdir" => {
            let url = url.ok_or(Unfit::Unreadable("it names no `url`"))?;
            let path = text(source, "path").ok_or(Unfit::Unreadable("it names no `path`"))?;
            remote(name, &url).map_err(failed)?;
            if crate::gather::outside(&path) {
                return Err(failed(EntryFailure::Outside {
                    name: name.to_owned(),
                    path,
                }));
            }
            Ok(Plugin::GitSubdir {
                url,
                path: paths::as_route(&paths::normalize(Path::new(&path))),
                reference,
                sha,
            })
        }
        "npm" => Err(failed(EntryFailure::Npm {
            name: name.to_owned(),
            package: text(source, "package").unwrap_or_default(),
        })),
        "archive" => Err(failed(EntryFailure::Archive {
            name: name.to_owned(),
            url: redacted(&url.unwrap_or_default()).into_owned(),
        })),
        "command" => Err(failed(EntryFailure::Command {
            name: name.to_owned(),
        })),
        other => Err(failed(EntryFailure::Unknown {
            name: name.to_owned(),
            kind: other.to_owned(),
        })),
    }
}

/// `url`, when it names another machine's repository, or why the entry
/// called `name` that names it is refused.
fn remote(name: &str, url: &str) -> Result<(), EntryFailure> {
    if is_remote(url) {
        Ok(())
    } else {
        Err(EntryFailure::Local {
            name: name.to_owned(),
            url: redacted(url).into_owned(),
        })
    }
}

/// Whether `url` names an archive rather than a repository, by how it ends
/// before any query, in any case.
fn is_archive(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    ARCHIVES.iter().any(|ending| path.ends_with(ending))
}

/// Why one entry of a catalog cannot be gathered, as opposed to the catalog
/// not being one at all ([`MarketplaceError`]): the rest of the catalog is
/// listed, and gathered, all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EntryFailure {
    /// It is not an object with a name and a source, or holds a key the
    /// catalog's format gives a shape it does not have.
    #[error("plugins[{at}]{}: {reason}, so it is not listed", .name.as_deref().map(|name| format!(" (`{name}`)")).unwrap_or_default())]
    Unreadable {
        /// Where it is in the `plugins` list, from 0.
        at: usize,
        name: Option<String>,
        reason: String,
    },
    /// Another entry has its name.
    #[error("`{name}`: the catalog lists more than one plugin of this name, so none is gathered")]
    Duplicate { name: String },
    /// Its folder is absolute, climbs out of the folder it is read from, or
    /// leads out of the repository through a link.
    #[error("`{name}`: `{path}` is no folder of the marketplace's repository: a plugin's folder never climbs out of it, nor leads out of it through a link")]
    Outside { name: String, path: String },
    /// Its folder is not in the marketplace's repository.
    #[error("`{name}`: the marketplace's repository has no `{path}` folder")]
    Missing { name: String, path: String },
    /// It names a repository on this machine ([`is_remote`]).
    #[error("`{name}`: {url} is a folder on this machine, which a catalog does not reach: gather it with `mindweaver gather git` if you mean it")]
    Local { name: String, url: String },
    /// An npm package, which no gather makes a plugin of.
    #[error("`{name}`: the npm package `{package}`, which is not gathered")]
    Npm { name: String, package: String },
    /// An archive, which no gather unpacks.
    #[error("`{name}`: an archive{}, which is not gathered", if .url.is_empty() { String::new() } else { format!(" at {}", .url) })]
    Archive { name: String, url: String },
    /// A plugin made by running a command, which nothing here runs.
    #[error(
        "`{name}`: its plugin is made by running a command, which is never run, so it is refused"
    )]
    Command { name: String },
    /// A kind of source this build does not know.
    #[error("`{name}`: a `{kind}` source, which this build does not know, so it is not gathered")]
    Unknown { name: String, kind: String },
}

codes!(EntryFailure {
    Self::Unreadable { .. } => "marketplace_entry_unreadable",
    Self::Duplicate { .. } => "marketplace_entry_duplicate",
    Self::Outside { .. } => "marketplace_entry_outside",
    Self::Missing { .. } => "marketplace_entry_missing",
    Self::Local { .. } => "marketplace_entry_local",
    Self::Npm { .. } => "marketplace_npm_not_gathered",
    Self::Archive { .. } => "marketplace_archive_not_gathered",
    Self::Command { .. } => "marketplace_command_refused",
    Self::Unknown { .. } => "marketplace_source_unknown",
});

impl EntryFailure {
    /// The name of the entry it is about, when it has one.
    pub fn name(&self) -> Option<&str> {
        match self {
            EntryFailure::Unreadable { name, .. } => name.as_deref(),
            EntryFailure::Duplicate { name }
            | EntryFailure::Outside { name, .. }
            | EntryFailure::Missing { name, .. }
            | EntryFailure::Local { name, .. }
            | EntryFailure::Npm { name, .. }
            | EntryFailure::Archive { name, .. }
            | EntryFailure::Command { name }
            | EntryFailure::Unknown { name, .. } => Some(name),
        }
    }
}

/// Why a marketplace could not be read at all, as opposed to one entry of
/// it ([`EntryFailure`]).
#[derive(Debug, Error)]
pub enum MarketplaceError {
    /// The repository holds no catalog where the marketplace is.
    #[error("{}: no such file, so this is no Claude Code marketplace", .path.display())]
    CatalogMissing { path: PathBuf },
    /// The catalog is a link leading out of the repository, or in a folder
    /// that does.
    #[error("{}: links to {}, outside the repository, so it is not read", .path.display(), .to.display())]
    CatalogOutside { path: PathBuf, to: PathBuf },
    /// The catalog is larger than [`CATALOG_CAP`].
    #[error("{}: larger than {} MiB, so it is not read", .path.display(), CATALOG_CAP / 1024 / 1024)]
    CatalogTooLarge { path: PathBuf },
    /// The catalog is no JSON object with a `plugins` list.
    #[error("{}: not a marketplace's catalog: {reason}", .path.display())]
    CatalogUnreadable { path: PathBuf, reason: String },
    #[error("{}: cannot be read: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(MarketplaceError {
    Self::CatalogMissing { .. } => "marketplace_catalog_missing",
    Self::CatalogOutside { .. } => "marketplace_catalog_outside",
    Self::CatalogTooLarge { .. } => "marketplace_catalog_too_large",
    Self::CatalogUnreadable { .. } => "marketplace_catalog_unreadable",
    Self::Read { .. } => "marketplace_read_failed",
});

#[cfg(test)]
#[path = "marketplace_test.rs"]
mod tests;
