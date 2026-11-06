//! A plugin's own manifest (#104): what a plugin a marketplace lists says of
//! itself, in one of the three formats a plugin is written in, Codex's,
//! Claude Code's and the vendor-neutral Agent Plugins format's, and what
//! makes a folder a package in the last, which a gather of a repository
//! reads too.
//!
//! It is JSON somebody else wrote, read as a catalog is
//! ([`crate::marketplace::Catalog`]): as JSON alone, to [`MANIFEST_CAP`],
//! never through a link out of its clone, and a key it does not know passed
//! over. What it names is read and never run, and nothing it points at is
//! imported: its MCP servers are said, as a catalog's are
//! ([`Holding`]), and its app connectors not looked at.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jsonc_parser::JsonValue;

use crate::copy;
use crate::marketplace::{options, strings, text, Dialect, Holding};

/// How much of a manifest is read: far more than any plugin writes, which
/// is a few hundred bytes, and never a file somebody made large enough to
/// be a reason not to finish.
pub const MANIFEST_CAP: u64 = 1024 * 1024;

/// What an Agent Plugins manifest's `$schema` names, compared without case,
/// hyphens and underscores: an agent plugin's schema.
///
/// Matched within the URL rather than held to one spelling of it, since no
/// copy of the format's specification was at hand when this was written
/// (docs/architecture.md, open questions): a root `plugin.json` is a
/// manifest of the format only when it says it is, which keeps any other
/// tool's `plugin.json` from making a repository a package.
pub const AGENT_PLUGINS_SCHEMA: &str = "agentplugin";

/// The format a plugin's manifest is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `.codex-plugin/plugin.json`, as openai/plugins writes each.
    Codex,
    /// A `plugin.json` at the plugin's root whose `$schema` names the
    /// Agent Plugins format's ([`AGENT_PLUGINS_SCHEMA`]).
    AgentPlugins,
    /// `.claude-plugin/plugin.json`.
    Claude,
}

impl Format {
    /// Where a plugin keeps a manifest in this format, from its root.
    pub const fn file(self) -> &'static str {
        match self {
            Format::Codex => ".codex-plugin/plugin.json",
            Format::AgentPlugins => "plugin.json",
            Format::Claude => ".claude-plugin/plugin.json",
        }
    }

    /// The order a plugin a catalog in `dialect` lists is looked in for its
    /// manifest: the format of the agent the catalog is for first, as that
    /// agent reads the plugin, then the vendor-neutral one, then the other
    /// agent's.
    pub const fn read_for(dialect: Dialect) -> [Format; 3] {
        match dialect {
            Dialect::Claude => [Format::Claude, Format::AgentPlugins, Format::Codex],
            Dialect::Codex => [Format::Codex, Format::AgentPlugins, Format::Claude],
        }
    }
}

/// What a plugin's manifest says of it, as far as a gather reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format: Format,
    /// The folders its skills are in, from its root, as it names each under
    /// `skills`: searched beside its `skills/`. Empty for the Agent Plugins
    /// format, whose skills are its `skills/<child>` alone.
    pub skills: Vec<String>,
    /// Its licence, as it gives it: a skill's own for one that says none.
    pub license: Option<String>,
    /// What else of a plugin it declares, which no gather takes.
    pub declares: Vec<Holding>,
}

/// Why a manifest that is there cannot be read: said, and the plugin
/// searched as if it had none, since one broken file is no reason not to
/// take the skills beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    pub path: PathBuf,
    pub reason: String,
}

impl Manifest {
    /// The manifest of the plugin at `root`, in the repository whose clone
    /// resolves to `bound`: the first of `formats` it holds, or `None` when
    /// it holds none.
    ///
    /// A root `plugin.json` that is not one of the Agent Plugins format, by
    /// its `$schema`, or cannot be read as one, is no manifest and passed
    /// over, since the name is anybody's; a Codex or a Claude Code one,
    /// whose place says what it is, is the plugin's manifest whatever it
    /// holds, and one that cannot be read is said ([`Unreadable`]).
    pub fn read(
        root: &Path,
        bound: &Path,
        formats: &[Format],
    ) -> Result<Option<Manifest>, Unreadable> {
        for format in formats {
            match read(root, bound, *format) {
                Ok(Some(manifest)) => return Ok(Some(manifest)),
                Ok(None) => {}
                Err(_) if *format == Format::AgentPlugins => {}
                Err(reason) => {
                    return Err(Unreadable {
                        path: root.join(format.file()),
                        reason,
                    })
                }
            }
        }
        Ok(None)
    }
}

/// Whether the folder at `folder`, in the repository whose clone resolves
/// to `bound`, is a package in the Agent Plugins format: its `plugin.json`
/// says so. Its skills are then its `skills/<child>` alone, never deeper
/// (`gather::candidates`).
pub fn is_package(folder: &Path, bound: &Path) -> bool {
    matches!(
        Manifest::read(folder, bound, &[Format::AgentPlugins]),
        Ok(Some(_))
    )
}

/// The manifest in `format` of the plugin at `root`, `None` when nothing is
/// there or, for the Agent Plugins format, when what is there does not say
/// it is one; or why what is there cannot be read.
fn read(root: &Path, bound: &Path, format: Format) -> Result<Option<Manifest>, String> {
    let path = &root.join(format.file());
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    let (resolved, inside) = copy::followed(path, std::slice::from_ref(&bound.to_path_buf()))
        .map_err(|error| error.to_string())?;
    if !inside {
        return Err(format!(
            "it links to {}, outside the repository",
            resolved.display()
        ));
    }
    let mut bytes = Vec::new();
    let file = fs::File::open(&resolved).map_err(|error| error.to_string())?;
    io::Read::read_to_end(&mut io::Read::take(file, MANIFEST_CAP + 1), &mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MANIFEST_CAP {
        return Err(format!("it is larger than {} KiB", MANIFEST_CAP / 1024));
    }
    let text_of = String::from_utf8(bytes).map_err(|_| String::from("it is not UTF-8"))?;
    let value = jsonc_parser::parse_to_value(&text_of, &options())
        .map_err(|error| format!("it is not JSON: {error}"))?;
    let Some(JsonValue::Object(object)) = value else {
        return Err(String::from("it is not a JSON object"));
    };
    if format == Format::AgentPlugins {
        let says = object.get_string("$schema").is_some_and(|schema| {
            schema
                .chars()
                .filter(|c| !matches!(c, '-' | '_'))
                .collect::<String>()
                .to_lowercase()
                .contains(AGENT_PLUGINS_SCHEMA)
        });
        if !says {
            return Ok(None);
        }
    }
    let skills = match (format, object.get("skills")) {
        (Format::AgentPlugins, _) | (_, None | Some(JsonValue::Null)) => Vec::new(),
        (_, Some(JsonValue::String(folder))) => vec![folder.to_string()],
        (_, Some(JsonValue::Array(folders))) => {
            strings(folders).ok_or_else(|| String::from("its `skills` are not all paths"))?
        }
        (_, Some(_)) => return Err(String::from("its `skills` are not paths")),
    };
    Ok(Some(Manifest {
        format,
        skills,
        license: text(&object, "license"),
        declares: Holding::declared_in(&object, Some(root)),
    }))
}

#[cfg(test)]
#[path = "plugin_test.rs"]
mod tests;
