//! How what an MCP server's installation wrote stands, an entry an agent
//! (D-04, D-10), and what a project's agents' config files hold that
//! nothing records, which the installed view lists as foreign (D-23).
//!
//! An entry carries no mark of its own, so it is told by three things: the
//! ledger's record of what was written ([`EntryTarget`], with its
//! fingerprint), what the agent's file holds under the server's name, by
//! its fingerprint ([`crate::hash::fingerprint`] of its canonical form, as
//! the renderer's own entry is fingerprinted), and what the place it came
//! from offers now, rendered for that agent as its renderer would render
//! it. Neither the file alone nor the offer alone tells it: an edited entry
//! can be what is offered now, and an untouched one can be behind it.
//!
//! It stands in the words a copy does ([`Standing`]), said in the same
//! order, edited first, then orphaned, missing and outdated, and each means
//! of an entry what it means of a copy. What differs is how each is told:
//!
//! - **Foreign** by the server's name, where a copy is told by its path: an
//!   entry the agent's file holds under a name nothing records an entry of
//!   in that file for that agent. A file is shared by every server of the
//!   project, so its path tells no entry from another.
//! - **Edited** by the fingerprint of what the name holds, where a copy is
//!   told by its tree's hash: the same whatever a file does to the entry's
//!   layout, changed by any value.
//! - **Missing** when the file the record names holds nothing under the
//!   name, or is not there, or cannot be read, which is said beside it: as
//!   a block that cannot be read holds none of a project's rules, whose next
//!   render says why.
//! - **Outdated** when what the place it came from offers now is given to
//!   the agent as an entry other than the one written, by its fingerprint,
//!   or as none: so an entry can be outdated for one agent and current for
//!   another, when only what one agent is given has changed.
//!
//! Every file is read as its renderer reads it to write, so one it would
//! refuse to write into is refused here too, with the renderer's own code.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use thiserror::Error;

use super::standing::{Found, Looked};
use super::{entry_for_root, offered_for, offers, Offer, Standing};
use crate::agent::Agent;
use crate::catalog::DiscoveryFailure;
use crate::claude_mcp::{self, ClaudeMcpError};
use crate::code::codes;
use crate::codex_mcp::{self, CodexMcpError};
use crate::kind::Kind;
use crate::ledger::{EntryTarget, InstallRecord, Ledger, LedgerError};
use crate::mcp::McpDefinition;
use crate::opencode_mcp::{self, OpenCodeMcpError, Shape};
use crate::value::{ArtifactName, ContentHash, Origin};
use crate::workspace::{MindProject, WeaverWorkspace};

/// Why an agent's config file could not be read for the MCP servers it
/// holds: what its renderer says, as it would say it refusing to write into
/// the file, with the renderer's code.
#[derive(Debug, Error)]
pub enum ConfigUnread {
    #[error(transparent)]
    Claude(#[from] ClaudeMcpError),
    #[error(transparent)]
    Codex(#[from] CodexMcpError),
    #[error(transparent)]
    OpenCode(#[from] OpenCodeMcpError),
}

codes!(ConfigUnread {} carried {
    Self::Claude(error) => error,
    Self::Codex(error) => error,
    Self::OpenCode(error) => error,
});

/// One server an agent's config file holds.
#[derive(Debug, Clone)]
struct Held {
    /// The key it is under.
    name: String,
    /// The fingerprint of what the key holds.
    fingerprint: ContentHash,
    /// For OpenCode, the shape it stands in, which what is offered is
    /// rendered in to be compared with it; none for the other agents.
    shape: Option<Shape>,
}

/// What `agent`'s config file at `route`, in the project at `root`, holds
/// of MCP servers, each by its key, as its renderer reads the file: none
/// when there is no file there, or `route` names no file the agent reads
/// its servers from, which can be only a record somebody wrote by hand.
fn held(root: &Path, agent: Agent, route: &str) -> Result<Vec<Held>, ConfigUnread> {
    // Matched on the agent with every one named in the last arm, so an
    // agent added says how its file is read before it compiles.
    let servers: Vec<(String, Option<Shape>, crate::config_entry::Value)> = match agent {
        Agent::Claude if route == claude_mcp::FILE => claude_mcp::servers(root)?
            .into_iter()
            .map(|(name, value)| (name, None, value))
            .collect(),
        Agent::Codex if route == codex_mcp::FILE => codex_mcp::servers(root)?
            .into_iter()
            .map(|(name, value)| (name, None, value))
            .collect(),
        Agent::OpenCode if opencode_mcp::FILES.contains(&route) => {
            opencode_mcp::servers(root, route)?
                .into_iter()
                .map(|(name, shape, value)| (name, Some(shape), value))
                .collect()
        }
        Agent::Claude | Agent::Codex | Agent::OpenCode => Vec::new(),
    };
    Ok(servers
        .into_iter()
        .map(|(name, shape, value)| Held {
            fingerprint: value.fingerprint(),
            name,
            shape,
        })
        .collect())
}

/// Each config file read for a project, once each, by agent and route.
struct Files<'a> {
    root: &'a Path,
    read: BTreeMap<(Agent, String), Result<Vec<Held>, ConfigUnread>>,
}

impl<'a> Files<'a> {
    fn new(root: &'a Path) -> Self {
        Files {
            root,
            read: BTreeMap::new(),
        }
    }

    /// What `agent`'s file at `route` holds, read the first time it is
    /// asked for: none it can be told by when it could not be read.
    fn of(&mut self, agent: Agent, route: &str) -> Option<&[Held]> {
        let root = self.root;
        self.read
            .entry((agent, route.to_owned()))
            .or_insert_with(|| held(root, agent, route))
            .as_deref()
            .ok()
    }

    /// The server `name` in `agent`'s file at `route`, if it holds one.
    fn server(&mut self, agent: Agent, route: &str, name: &str) -> Option<Held> {
        self.of(agent, route)?
            .iter()
            .find(|held| held.name == name)
            .cloned()
    }
}

/// What the place an installation came from offers now, for one agent.
enum Offered {
    /// Nothing: it no longer offers the server.
    Nothing,
    /// The fingerprint of the entry each offer of it is given as, none for
    /// one the agent would be given no entry of.
    Entries(Vec<Option<ContentHash>>),
}

/// What each of `offered`, the places a record's server is offered from
/// now, gives `agent` as its entry, in OpenCode's `shape` when the agent's
/// file keeps the server in one.
fn rendered(
    workspace: &WeaverWorkspace,
    offered: &[&Offer],
    agent: Agent,
    shape: Option<Shape>,
) -> Offered {
    if offered.is_empty() {
        return Offered::Nothing;
    }
    Offered::Entries(
        offered
            .iter()
            .map(|offer| fingerprint_of(&offer.source(workspace), agent, shape))
            .collect(),
    )
}

/// The fingerprint of the entry `agent` is given of the definition at
/// `path`, in OpenCode's `shape` (V2's when none is said, the one written
/// into a file that keeps none of V1's): none when the file does not read
/// as a definition, or the agent would be given no entry of it, which an
/// install of it is then refused for.
fn fingerprint_of(path: &Path, agent: Agent, shape: Option<Shape>) -> Option<ContentHash> {
    let definition = McpDefinition::parse(&fs::read_to_string(path).ok()?).ok()?;
    match agent {
        Agent::Claude => claude_mcp::entry(&definition)
            .ok()
            .map(|entry| entry.fingerprint()),
        Agent::Codex => codex_mcp::entry(&definition)
            .ok()
            .map(|entry| entry.fingerprint()),
        Agent::OpenCode => opencode_mcp::entry(&definition, shape.unwrap_or(Shape::V2))
            .ok()
            .map(|entry| entry.fingerprint()),
    }
}

/// How one entry stands, from the fingerprint the ledger recorded writing
/// (`written`, none when nothing records an entry of the name there), the
/// one the agent's file holds under the name (`held`, none when it holds
/// nothing there) and what the place it came from offers now.
fn of_entry(
    written: Option<&ContentHash>,
    held: Option<&ContentHash>,
    offered: &Offered,
) -> Standing {
    let Some(written) = written else {
        return match held {
            None => Standing::Absent,
            Some(_) => Standing::Foreign,
        };
    };
    if held.is_some_and(|held| held != written) {
        return Standing::Edited;
    }
    let Offered::Entries(entries) = offered else {
        return Standing::Orphaned;
    };
    if held.is_none() {
        return Standing::Missing;
    }
    if entries.iter().flatten().any(|offered| offered == written) {
        Standing::Current
    } else {
        Standing::Outdated
    }
}

/// How an installation stands from how each of its entries does, and
/// whether the place it came from offers it still: the first true of
/// edited, orphaned, missing and outdated, as for a copy.
fn of_installation(entries: &[Standing], offered: bool) -> Standing {
    let any = |standing| entries.contains(&standing);
    if any(Standing::Edited) {
        Standing::Edited
    } else if !offered {
        Standing::Orphaned
    } else if any(Standing::Missing) {
        Standing::Missing
    } else if any(Standing::Outdated) {
        Standing::Outdated
    } else {
        Standing::Current
    }
}

/// Each entry `record`, an MCP server's installation in `project`, wrote,
/// as it is found where its record says, and how the installation stands,
/// given `offered`, where what it installed is offered now: what
/// [`super::standings`] says of it, and an update would act on.
///
/// An entry is untouched while the file holds the fingerprint written,
/// edited while it holds another, and missing while it holds none, or the
/// file cannot be read.
pub(crate) fn told(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    record: &InstallRecord,
    offered: &[&Offer],
) -> (Vec<Looked>, Standing) {
    let mut files = Files::new(project.root());
    let mut looked = Vec::new();
    let mut standings = Vec::new();
    for entry in &record.entries {
        let held = files.server(entry.agent, &entry.file, &entry.name);
        let found = match &held {
            None => Found::Missing,
            Some(held) if held.fingerprint == entry.fingerprint => Found::Untouched,
            Some(_) => Found::Edited,
        };
        looked.push(Looked {
            path: project.root().join(&entry.file),
            found,
        });
        let shape = held.as_ref().and_then(|held| held.shape);
        standings.push(of_entry(
            Some(&entry.fingerprint),
            held.as_ref().map(|held| &held.fingerprint),
            &rendered(workspace, offered, entry.agent, shape),
        ));
    }
    (looked, of_installation(&standings, !offered.is_empty()))
}

/// One entry of an agent's config file, as the installed view lists it
/// (D-23): one an installation recorded, as it stands, or one nothing
/// records, foreign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub agent: Agent,
    /// The file, as a route from the project, `/` separated.
    pub file: String,
    /// The key it is under, the server's name.
    pub name: ArtifactName,
    pub standing: Standing,
    /// Whether the file holds an entry under the name, which an orphaned
    /// one may or may not.
    pub held: bool,
    /// Where what was installed came from; none for a foreign entry.
    pub origin: Option<Origin>,
}

/// Every entry of `project`'s agents' config files, and every one `records`
/// recorded, as the installed view lists them (D-23), by agent in report
/// order, then name, then file, with each file that could not be read.
///
/// `records` are the MCP installations `workspace`'s ledger records in the
/// project and `offers` every offer of an MCP server there, which what they
/// installed is held to; with no workspace there is none of either. Each of
/// `agents`, the project's own, has its file read for what nothing records,
/// which is foreign; a recorded entry is looked for in the file its record
/// names, whichever agent it is for. A file is read once, however many
/// entries are in it, and one that cannot be read is said once.
pub(crate) fn listed(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    records: &[InstallRecord],
    offers: &[Offer],
    agents: &[Agent],
) -> (Vec<Listed>, Vec<DiscoveryFailure>) {
    let root = project.root();
    let mut files = Files::new(root);
    let mut failures = Vec::new();
    let mut listed = Vec::new();
    for record in records {
        let offered = offered_for(record, offers);
        for entry in &record.entries {
            let held = files.server(entry.agent, &entry.file, &entry.name);
            let shape = held.as_ref().and_then(|held| held.shape);
            let offered = match workspace {
                Some(workspace) => rendered(workspace, &offered, entry.agent, shape),
                None => Offered::Nothing,
            };
            listed.push(Listed {
                agent: entry.agent,
                file: entry.file.clone(),
                name: entry.name.clone(),
                standing: of_entry(
                    Some(&entry.fingerprint),
                    held.as_ref().map(|held| &held.fingerprint),
                    &offered,
                ),
                held: held.is_some(),
                origin: record.origin.clone(),
            });
        }
    }
    for &agent in agents {
        let route = match agent.mcp_file(root) {
            Ok(route) => route,
            // Only OpenCode's is looked for, and only a folder that cannot
            // be read fails it.
            Err(error) => {
                failures.push(DiscoveryFailure::Config {
                    path: root.to_path_buf(),
                    unread: Box::new(OpenCodeMcpError::from(error).into()),
                });
                continue;
            }
        };
        let recorded = |name: &str| {
            records.iter().any(|record| {
                record
                    .entries
                    .iter()
                    .any(|entry| entry.agent == agent && entry.file == route && entry.name == name)
            })
        };
        let foreign: Vec<String> = files
            .of(agent, route)
            .unwrap_or_default()
            .iter()
            .map(|held| held.name.clone())
            .filter(|name| !recorded(name))
            .collect();
        listed.extend(foreign.into_iter().map(|name| Listed {
            agent,
            file: route.to_owned(),
            name: ArtifactName::from(name),
            standing: Standing::Foreign,
            held: true,
            origin: None,
        }));
    }
    listed.sort_by(|a, b| (a.agent, &a.name, &a.file).cmp(&(b.agent, &b.name, &b.file)));
    failures.extend(files.read.into_iter().filter_map(|((_, route), read)| {
        read.err().map(|unread| DiscoveryFailure::Config {
            path: root.join(route),
            unread: Box::new(unread),
        })
    }));
    (listed, failures)
}

/// How a server's entry stands for one agent, or why that agent's file
/// could not be read to tell.
pub type ForAgent = (Agent, Result<Standing, ConfigUnread>);

/// How the MCP server `name` stands in `project` for each of its agents, in
/// report order: what an install of it into each agent's config file would
/// meet there, against what `workspace`'s ledger records of it and what is
/// offered now.
///
/// Absent where the agent's file holds nothing by the name and nothing
/// records an entry of it, foreign where the file holds one nothing
/// records, and otherwise as its recorded entry stands, looked for in the
/// file its record names. An agent whose file cannot be read, or not be
/// found, is said so, the others told all the same. Only the ledger can
/// fail it as a whole.
pub fn entry_standings(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
) -> Result<Vec<ForAgent>, LedgerError> {
    let root = project.root();
    let mut agents = project
        .settings()
        .map(|settings| settings.agents.clone())
        .unwrap_or_default();
    agents.sort();
    agents.dedup();
    let record = ledger.installation(&entry_for_root(workspace, root), Kind::Mcp, name)?;
    let offers = match &record {
        Some(_) => offers(workspace, ledger, Kind::Mcp)?,
        None => Vec::new(),
    };
    let offered = record
        .as_ref()
        .map(|record| offered_for(record, &offers))
        .unwrap_or_default();
    let mut told = Vec::new();
    for agent in agents {
        let entry: Option<&EntryTarget> = record
            .iter()
            .flat_map(|record| &record.entries)
            .find(|entry| entry.agent == agent);
        let route = match entry {
            Some(entry) => entry.file.clone(),
            None => match agent.mcp_file(root) {
                Ok(route) => route.to_owned(),
                Err(error) => {
                    told.push((agent, Err(OpenCodeMcpError::from(error).into())));
                    continue;
                }
            },
        };
        let held = match held(root, agent, &route) {
            Ok(held) => held.into_iter().find(|held| held.name == name),
            Err(unread) => {
                told.push((agent, Err(unread)));
                continue;
            }
        };
        let shape = held.as_ref().and_then(|held| held.shape);
        told.push((
            agent,
            Ok(of_entry(
                entry.map(|entry| &entry.fingerprint),
                held.as_ref().map(|held| &held.fingerprint),
                &rendered(workspace, &offered, agent, shape),
            )),
        ));
    }
    Ok(told)
}

#[cfg(test)]
#[path = "entries_test.rs"]
mod tests;
