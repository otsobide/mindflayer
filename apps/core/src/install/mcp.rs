//! Installing an MCP server into a project, and taking it back out (D-10).
//!
//! A server is no copy: it reaches each agent the project names as one
//! entry in that agent's own config file ([`Agent::mcp_file`]), written by
//! the agent's renderer ([`crate::claude_mcp`], [`crate::codex_mcp`],
//! [`crate::opencode_mcp`]), every byte outside it kept. It is one
//! installation a project, recorded once with an entry target for each file
//! it wrote ([`EntryTarget`]), each by the fingerprint of what was written
//! there: an entry carries no mark of its own, so that record is the one
//! thing that makes it Mindweaver's.
//!
//! What an install writes it writes into every file it reaches or into
//! none: a file that cannot be written puts back each one written before
//! it, and nothing is recorded. An agent whose renderer refuses the
//! definition (a shape it cannot express, an input read from no variable)
//! is reached by none of it and said with the refusal's code, and the
//! others are written all the same, as a rule about some files only reaches
//! Claude alone: refusing every agent for one would keep a server from the
//! agents that can run it over one that cannot. So is an agent whose file
//! git tracks in a local project that does not set `edit_tracked` (D-02),
//! as Claude's `CLAUDE.md` is left alone there, and Codex in a project it
//! does not trust, whose entry is written all the same, since trusting a
//! project is the user's to do (#91).
//!
//! What is somebody's is never written over: a server of the name in a file
//! the install writes into, which nothing records, refuses the whole
//! install, as a copy nothing records does, and so does an entry changed
//! since it was written, unless the install is forced ([`Edits::Forced`]).
//! In a local project a config file the install makes is kept out of git,
//! an entry in the project's group of the exclude block written before the
//! file lands, and the ledger notes that Mindweaver made it (`created`),
//! for the file to go with its last server and only then. One somebody had
//! is theirs to commit.
//!
//! An uninstall goes by the record: each entry it names is taken out of the
//! file it names, and nothing else, every other server and byte there kept,
//! all of them or none, as an install writes them. An entry changed since
//! it was written refuses it unless forced, as a copy does, and forcing
//! never reaches an entry nothing records; one in a file git tracks in a
//! local project without `edit_tracked` refuses it too, since taking it out
//! is an edit of a team's file. A file Mindweaver made goes with its last
//! server, as its renderer decides, and its exclude entry with it.

use std::fs;
use std::path::{Path, PathBuf};

use super::entries::{self, ConfigUnread};
use super::rules::{self, Withheld};
use super::{
    edited, local, offered_for, project_entry, user_scope, Candidate, Edits, InstallError,
    Installation, Installed, Offer, Removed, Standing,
};
use crate::agent::{Agent, AgentHomes};
use crate::artifact::{self, ArtifactError};
use crate::claude_mcp::{self, ClaudeMcpError};
use crate::code::Said;
use crate::codex_mcp::{self, CodexMcpError};
use crate::config_entry::Written;
use crate::copy;
use crate::exclude;
use crate::kind::Kind;
use crate::ledger::{Action, EntryTarget, InstallRecord, Ledger, Outcome};
use crate::mcp::{McpDefinition, Runs};
use crate::opencode_mcp::{self, OpenCodeMcpError, Shape};
use crate::paths;
use crate::value::{ArtifactName, ContentHash, VariableName};
use crate::workspace::{replace_bytes_unresolved, MindProject, WeaverWorkspace, WorkspaceError};

/// What installing an MCP server did, agent by agent
/// ([`Installation::entries`]), or, for one asked of without a yes, would do
/// ([`preview`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Each entry written, or found holding what it would be written as,
    /// an agent each, in report order.
    pub given: Vec<Given>,
    /// Each agent the project names that the server does not reach, and
    /// why, in report order: one its renderer refuses the definition, one
    /// whose file is not Mindweaver's to edit, and Codex in a project it
    /// does not trust, which is given its entry all the same.
    pub unreached: Vec<Unreached>,
    /// Each variable an agent given it reads where it runs, for an input of
    /// the server's, in the order the definition declares them, each once:
    /// what somebody sets there before an agent starts it, since no file
    /// holds a value (D-11). Every input's, a secret's or not: a value an
    /// entry refers to is read from its variable alike.
    pub variables: Vec<Variable>,
}

/// One entry an install gave an agent, or would give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    pub agent: Agent,
    /// The agent's config file.
    pub file: PathBuf,
    /// What became of the file: made, changed, or left as it was when it
    /// held the entry already. None in a [`preview`], which writes nothing.
    pub written: Option<Written>,
    /// What the agent runs or reaches, read off the entry its renderer
    /// writes ([`Runs`]).
    pub runs: Runs,
    /// What its renderer said of the entry, each by its code: what it is
    /// written without, and a transport the agent calls deprecated.
    pub warnings: Vec<Said>,
}

/// A variable an agent reads for one of a server's inputs where it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: VariableName,
    /// Whether an input read from it is a secret.
    pub secret: bool,
}

/// An agent the project names that an MCP server does not reach, and why,
/// by the code of what says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreached {
    pub agent: Agent,
    pub why: Said,
}

/// The agents `project` names, each once, in report order: those a server
/// is given an entry for. None for a project a `mind.toml` marks, which
/// names no agent.
pub(super) fn agents(project: &MindProject) -> Vec<Agent> {
    let mut agents = project
        .settings()
        .map(|settings| settings.agents.clone())
        .unwrap_or_default();
    agents.sort();
    agents.dedup();
    agents
}

/// Refuse a project that names no agent: a server is an entry in the
/// config file of each agent it names, and there is none to write into.
pub(crate) fn receives(project: &MindProject) -> Result<(), InstallError> {
    if agents(project).is_empty() {
        return Err(InstallError::NoTarget {
            project: project.name().to_owned(),
            kind: Kind::Mcp,
        });
    }
    Ok(())
}

/// `offers`, each with how it stands in `project`: while the ledger records
/// it, as its entries are told ([`entries::told`]); otherwise somebody's
/// when the config file of an agent the project names holds a server of its
/// name, and absent when none does. A file that cannot be read holds
/// nothing that can be told, and an install reading it says why.
pub(crate) fn survey(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    offers: &[Offer],
) -> Result<Vec<Candidate>, InstallError> {
    let records = ledger.records(&project_entry(workspace, project), Kind::Mcp)?;
    let root = project.root();
    let held: Vec<entries::Held> = agents(project)
        .into_iter()
        .filter_map(|agent| entries::held(root, agent, agent.mcp_file(root).ok()?).ok())
        .flatten()
        .collect();
    // Once a name: every offer of one name stands as the one installation
    // the project has of it, whichever of them it came from.
    let mut said: Vec<(ArtifactName, Standing)> = Vec::new();
    let mut candidates = Vec::with_capacity(offers.len());
    for offer in offers {
        let name = offer.name();
        let standing = match said.iter().find(|(seen, _)| seen == name) {
            Some((_, standing)) => *standing,
            None => {
                let standing = match records.iter().find(|record| record.name == *name) {
                    Some(record) => {
                        entries::told(workspace, project, record, &offered_for(record, offers)).1
                    }
                    None if held.iter().any(|held| held.name == name.as_str()) => Standing::Foreign,
                    None => Standing::Absent,
                };
                said.push((name.clone(), standing));
                standing
            }
        };
        candidates.push(Candidate {
            offer: offer.clone(),
            standing,
        });
    }
    Ok(candidates)
}

/// Install `candidate`, an MCP server, into `project`, holding the
/// workspace's lock until its record is written, as an install of a skill
/// holds it: two installs of one name at once would otherwise each record
/// the entries they wrote, and the files hold whichever came last.
pub(crate) fn install(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    receives(project)?;
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    install_held(workspace, ledger, project, candidate, homes, edits)
}

/// One agent the install writes an entry for.
struct Reach {
    agent: Agent,
    /// Its config file, as a route from the project ([`Agent::mcp_file`]).
    route: &'static str,
    file: PathBuf,
    /// Whether the file holds a server of the name already, which the
    /// ledger records: what tells an entry added from one replaced.
    held: bool,
}

/// Who an install of a server gives what, decided before anything is
/// written ([`decide`]): what a [`preview`] says and an install writes.
struct Decided {
    name: ArtifactName,
    definition: McpDefinition,
    /// The installation the ledger records already, when it does.
    before: Option<InstallRecord>,
    /// Each agent whose entry is written, in report order.
    reached: Vec<Reach>,
    unreached: Vec<Unreached>,
    /// Each agent whose renderer refuses the definition, of those
    /// unreached.
    refused: Vec<Agent>,
}

/// Who an install of `candidate` into `project` gives what, every file read
/// and nothing written: the agents whose renderers take the definition,
/// less those whose file is not Mindweaver's to edit, each file read for a
/// server of the name somebody keeps there and for an entry changed since
/// it was written. Somebody's, or one edited and not forced, is the
/// [`Installed`] refusal the install answers with, nothing written.
fn decide(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Result<Decided, Installed>, InstallError> {
    receives(project)?;
    let root = project.root();
    let name = candidate.name().to_owned();
    let definition = definition(workspace, candidate)?;
    let entry = project_entry(workspace, project);
    let before = ledger.installation(&entry, Kind::Mcp, &name)?;
    let recorded = |agent: Agent, route: &str| {
        before
            .iter()
            .flat_map(|record| &record.entries)
            .any(|entry| entry.agent == agent && entry.file == route)
    };

    let mut unreached: Vec<Unreached> = Vec::new();
    let mut refused: Vec<Agent> = Vec::new();
    let mut reached: Vec<Reach> = Vec::new();
    for agent in agents(project) {
        if let Err(refusal) = entry_for(agent, &definition) {
            unreached.push(Unreached {
                agent,
                why: Said::of(refusal.code(), &refusal),
            });
            refused.push(agent);
            continue;
        }
        let route = agent
            .mcp_file(root)
            .map_err(|error| config(OpenCodeMcpError::from(error).into()))?;
        let file = root.join(route);
        if let Some(why) = rules::withheld_from(project, Path::new(route)) {
            unreached.push(withheld(agent, &file, &why));
            continue;
        }
        reached.push(Reach {
            agent,
            route,
            file,
            held: false,
        });
    }
    if reached.is_empty() {
        return Err(unreached_by_all(&name, project, unreached));
    }
    // Into one of the agents' own folders, a project at the user's home, is
    // the user-scope install D-22 leaves for later, as a copy there is.
    let files: Vec<PathBuf> = reached.iter().map(|reach| reach.file.clone()).collect();
    if let Some(error) = user_scope(homes, &files) {
        return Err(error);
    }
    // Somebody's server of the name in a file this writes into refuses the
    // whole install, nothing written in any: it is theirs, as a copy
    // nothing records is, though the other agents read other files.
    for reach in &mut reached {
        reach.held = entries::held(root, reach.agent, reach.route)
            .map_err(config)?
            .iter()
            .any(|held| held.name == name.as_str());
        if reach.held && !recorded(reach.agent, reach.route) {
            return Ok(Err(Installed::Foreign {
                name,
                path: reach.file.clone(),
            }));
        }
    }
    // And an entry changed since it was written is theirs as much, unless
    // the install is forced: wherever the record has it, as for a copy.
    let edited = edited(workspace, ledger, project, Kind::Mcp, &name)?;
    if edits == Edits::Kept && !edited.is_empty() {
        return Ok(Err(Installed::Edited {
            name,
            paths: edited,
        }));
    }
    Ok(Ok(Decided {
        name,
        definition,
        before,
        reached,
        unreached,
        refused,
    }))
}

/// What installing `candidate`, an MCP server, into `project` would give
/// each agent the project names, decided as the install decides it
/// ([`decide`]) and each entry made as its renderer would make it there
/// ([`planned`]), with nothing written and nothing recorded: what a front
/// end shows before it asks whether to write a server that does not come
/// from the workspace's own library ([`super::asks_first`]).
///
/// [`Installed::Unconfirmed`] with what each agent would run and why each
/// not reached is not, or the refusal the install would meet: somebody's
/// server of the name, an entry edited since and not forced, a file that
/// cannot be read, or no agent reached at all. A read: no lock is taken,
/// as a survey takes none.
pub fn preview(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let decided = match decide(workspace, ledger, project, candidate, homes, edits)? {
        Ok(decided) => decided,
        Err(refused) => return Ok(refused.into()),
    };
    let root = project.root();
    let Decided {
        name,
        definition,
        reached,
        mut unreached,
        ..
    } = decided;
    let mut given: Vec<Given> = Vec::new();
    for reach in &reached {
        let done = match planned(reach.agent, root, &definition, homes) {
            Ok(done) => done,
            Err(refusal) if refuses_agent(&refusal) => {
                unreached.push(Unreached {
                    agent: reach.agent,
                    why: Said::of(refusal.code(), &refusal),
                });
                continue;
            }
            Err(error) => return Err(config(error)),
        };
        unreached.extend(done.unread.into_iter().map(|why| Unreached {
            agent: reach.agent,
            why,
        }));
        given.push(Given {
            agent: reach.agent,
            file: reach.file.clone(),
            written: None,
            runs: done.runs,
            warnings: done.warnings,
        });
    }
    unreached.sort_by_key(|unreached| unreached.agent);
    if given.is_empty() {
        return Err(unreached_by_all(&name, project, unreached));
    }
    let paths = given.iter().map(|given| given.file.clone()).collect();
    Ok(Installation {
        entries: Some(Rendered {
            given,
            unreached,
            variables: variables(&definition),
        }),
        ..Installation::from(Installed::Unconfirmed { name, paths })
    })
}

/// [`install`], with the workspace's lock held already, as
/// [`super::install_held`] is for a skill: for an update, which writes
/// several installations under one hold of it.
///
/// Who is given what is decided before anything is written ([`decide`]),
/// as a [`preview`] decides it. Then each is written in report order, and the first that fails puts back the files
/// written before it, as they were, with the folder made on the way, and
/// leaves the record as it was.
///
/// An agent its renderer now refuses loses the entry an earlier install
/// recorded for it, taken out of its file as an uninstall takes it, and
/// out of the record: left, the agent would go on running what the library
/// no longer gives it, outdated for good, since no install can write it
/// again. One whose file is not Mindweaver's to edit keeps it, recorded
/// still, since nothing here may take it out, and dropping its record
/// would make it nobody's to take out.
pub(crate) fn install_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let Decided {
        name,
        definition,
        before,
        reached,
        mut unreached,
        mut refused,
    } = match decide(workspace, ledger, project, candidate, homes, edits)? {
        Ok(decided) => decided,
        Err(refused) => return Ok(refused.into()),
    };
    let root = project.root();
    let entry = project_entry(workspace, project);

    // Out of git before anything lands, each file an entry makes in a local
    // project, as an install's copies are, and only those: one somebody
    // had, git tracking it or not, is theirs to commit.
    let key = local(project);
    let making: Vec<exclude::Entry> = reached
        .iter()
        .filter(|reach| fs::symlink_metadata(&reach.file).is_err())
        .filter(|reach| !exclude::tracks_file(root, Path::new(reach.route)).unwrap_or(false))
        .map(|reach| exclude::Entry::file(reach.route))
        .collect();
    let excluded = match key.filter(|_| !making.is_empty()) {
        Some(key) => Some(exclude::add(root, key, &making)?),
        None => None,
    };
    // A line naming a file that is not there would keep whatever somebody
    // writes there later out of git without a word.
    let unexclude = |made: &[&str]| {
        let left: Vec<exclude::Entry> = making
            .iter()
            .filter(|entry| !made.iter().any(|route| entry.path() == Path::new(route)))
            .cloned()
            .collect();
        if let Some(key) = key.filter(|_| !left.is_empty()) {
            let _ = exclude::remove(root, key, &left);
        }
    };

    let mut written: Vec<Before> = Vec::new();
    let mut given: Vec<Given> = Vec::new();
    let mut changes: Vec<copy::Change> = Vec::new();
    let mut targets: Vec<EntryTarget> = Vec::new();
    let mut made: Vec<&str> = Vec::new();
    for reach in &reached {
        let was = Before::of(root, reach.route);
        let done = match render(reach.agent, root, &definition, homes) {
            Ok(done) => done,
            // Refused the agent alone, its file neither read nor written.
            Err(refusal) if refuses_agent(&refusal) => {
                unreached.push(Unreached {
                    agent: reach.agent,
                    why: Said::of(refusal.code(), &refusal),
                });
                refused.push(reach.agent);
                continue;
            }
            Err(error) => {
                // Its own file is as it was, a write taking away what it
                // made, but for the folder it made on the way.
                if let Some(folder) = &was.folder {
                    let _ = fs::remove_dir(folder);
                }
                put_back(&written);
                unexclude(&[]);
                return Err(config(error));
            }
        };
        changes.push(match done.written {
            Written::Unchanged => copy::Change::Unchanged,
            Written::Created | Written::Changed | Written::Removed if reach.held => {
                copy::Change::Updated
            }
            Written::Created | Written::Changed | Written::Removed => copy::Change::Added,
        });
        if done.written == Written::Created {
            made.push(reach.route);
        }
        unreached.extend(done.unread.into_iter().map(|why| Unreached {
            agent: reach.agent,
            why,
        }));
        targets.push(EntryTarget {
            agent: reach.agent,
            file: reach.route.to_owned(),
            name: name.clone(),
            fingerprint: done.fingerprint,
        });
        given.push(Given {
            agent: reach.agent,
            file: reach.file.clone(),
            written: Some(done.written),
            runs: done.runs,
            warnings: done.warnings,
        });
        written.push(was);
    }
    unreached.sort_by_key(|unreached| unreached.agent);
    if given.is_empty() {
        unexclude(&[]);
        return Err(unreached_by_all(&name, project, unreached));
    }

    // What an earlier install wrote for an agent refused now, taken out as
    // an uninstall takes it, while its file may be edited: each holds what
    // was recorded, or the install is forced, since an edited one refused
    // it above. A file Mindweaver made that goes with it, or went by hand,
    // is forgotten as an uninstall forgets it.
    let mut dropped: Vec<Agent> = Vec::new();
    let mut unmade: Vec<&str> = Vec::new();
    for left in before.iter().flat_map(|record| &record.entries) {
        if !refused.contains(&left.agent)
            || rules::withheld_from(project, Path::new(&left.file)).is_some()
        {
            continue;
        }
        let was = Before::of(root, &left.file);
        let (created, taken) = match taken_out(ledger, &entry, root, left) {
            Ok(taken) => taken,
            Err(error) => {
                put_back(&written);
                unexclude(&[]);
                return Err(error);
            }
        };
        if taken != Written::Unchanged {
            changes.push(copy::Change::Updated);
        }
        if created && fs::symlink_metadata(&was.path).is_err() {
            unmade.push(&left.file);
        }
        dropped.push(left.agent);
        written.push(was);
    }

    // Each entry written, and each an earlier install recorded for an agent
    // this one gave none and took none from, which stays what that install
    // wrote.
    let mut entries = targets;
    entries.extend(
        before
            .iter()
            .flat_map(|record| &record.entries)
            .filter(|entry| !given.iter().any(|given| given.agent == entry.agent))
            .filter(|entry| !dropped.contains(&entry.agent))
            .cloned(),
    );
    entries.sort_by_key(|entry| entry.agent);
    // A source to point at only for what was gathered, as for a skill.
    let source_id = match &candidate.offer {
        Offer::Shelf(gathered) => Some(gathered.source.id),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    };
    let record = InstallRecord {
        kind: Kind::Mcp,
        name: name.clone(),
        source_id,
        origin: Some(candidate.origin()),
        content_hash: None,
        targets: Vec::new(),
        entries,
    };
    let recorded = ledger.installed(&entry, &record).and_then(|()| {
        made.iter()
            .try_for_each(|route| ledger.set_created(&entry, route, true))?;
        unmade
            .iter()
            .try_for_each(|route| ledger.set_created(&entry, route, false))
    });
    // A record that cannot be written leaves the files with entries nobody
    // records, which no install or uninstall would touch again: they are
    // put back, and the record with them.
    if let Err(error) = recorded {
        put_back(&written);
        unexclude(&[]);
        for route in &made {
            let _ = ledger.set_created(&entry, route, false);
        }
        for route in &unmade {
            let _ = ledger.set_created(&entry, route, true);
        }
        let _ = match &before {
            Some(before) => ledger.installed(&entry, before),
            None => ledger.uninstalled(&entry, Kind::Mcp, &name).map(drop),
        };
        return Err(error.into());
    }
    unexclude(&made);
    // After the files go, as an uninstall takes a line out.
    let lines: Vec<exclude::Entry> = unmade.iter().map(exclude::Entry::file).collect();
    if let Some(key) = key.filter(|_| !lines.is_empty()) {
        let _ = exclude::remove(root, key, &lines);
    }
    let detail = format!("{name} into {}", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    // One change for the installation, as for a skill's copies: added when
    // every entry is new, unchanged when none changed, updated otherwise.
    let paths = given.iter().map(|given| given.file.clone()).collect();
    let installed = if changes
        .iter()
        .all(|change| *change == copy::Change::Unchanged)
    {
        Installed::Unchanged {
            name,
            paths,
            unhashed: None,
        }
    } else if changes.iter().all(|change| *change == copy::Change::Added) {
        Installed::Added {
            name,
            paths,
            unhashed: None,
        }
    } else {
        Installed::Updated {
            name,
            paths,
            unhashed: None,
        }
    };
    Ok(Installation {
        excluded,
        entries: Some(Rendered {
            given,
            unreached,
            variables: variables(&definition),
        }),
        ..Installation::from(installed)
    })
}

/// Take the MCP server `name` back out of `project`: each entry its
/// installation recorded, from the file the record names, and the record
/// with them.
///
/// Holds the workspace's lock from before the record is read until it is
/// dropped, for the reason an install holds it. Only what the ledger
/// records is taken out: with no record, a server of the name in the file
/// of an agent the project names is somebody's, and said
/// ([`Removed::Foreign`]). An entry changed since it was written refuses
/// the whole uninstall, every entry left and the record kept
/// ([`Removed::Edited`]), unless `edits` forces it; an entry no longer in
/// its file has nothing to take out, and is forgotten with the rest, and so
/// is a file Mindweaver made that is no longer there, its mark and its
/// exclude line with it.
///
/// What can refuse is asked before anything is taken out: a file that
/// cannot be read, and one git tracks in a local project without
/// `edit_tracked` ([`InstallError::EntryWithheld`]), which a removal would
/// edit as a write would. Then each file is changed in report order, and
/// the first that fails puts back the files changed before it and keeps
/// the record.
pub(crate) fn uninstall(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
    edits: Edits,
) -> Result<Removed, InstallError> {
    let root = project.root();
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, Kind::Mcp, name)? else {
        return Ok(match somebodys(project, name) {
            Some(path) => Removed::Foreign {
                name: ArtifactName::from(name),
                path,
            },
            None => Removed::Absent {
                name: ArtifactName::from(name),
            },
        });
    };
    if edits == Edits::Kept {
        let edited = edited(workspace, ledger, project, Kind::Mcp, name)?;
        if !edited.is_empty() {
            return Ok(Removed::Edited {
                name: ArtifactName::from(name),
                paths: edited,
            });
        }
    }
    let mut taking: Vec<&EntryTarget> = Vec::new();
    let mut unmade: Vec<&str> = Vec::new();
    for target in &record.entries {
        // A file Mindweaver made and somebody took away is forgotten with
        // the record, as one its renderer takes away is: its exclude line
        // would keep whatever is written there later out of git without a
        // word, and its mark would have a later uninstall take that file
        // away as Mindweaver's. Whichever of them another server's record
        // still names, the install that puts that server back makes the
        // file, and so marks it and keeps it out again. A file still there
        // keeps both, whatever was taken out of it: it is still the one
        // Mindweaver made, to go with the last server in it.
        if fs::symlink_metadata(root.join(&target.file)).is_err() {
            if ledger.created(&entry, &target.file)? {
                unmade.push(&target.file);
            }
            continue;
        }
        let holds = entries::held(root, target.agent, &target.file)
            .map_err(config)?
            .iter()
            .any(|held| held.name == name);
        if !holds {
            continue;
        }
        if let Some(why) = rules::withheld_from(project, Path::new(&target.file)) {
            return Err(InstallError::EntryWithheld {
                file: root.join(&target.file),
                name: name.to_owned(),
                why,
            });
        }
        taking.push(target);
    }

    let mut changed: Vec<Before> = Vec::new();
    let mut gone: Vec<&str> = Vec::new();
    for target in taking {
        let was = Before::of(root, &target.file);
        let created = ledger.created(&entry, &target.file)?;
        match remove(target.agent, root, &target.file, name, created) {
            Ok(written) => {
                if written == Written::Removed {
                    gone.push(&target.file);
                }
                changed.push(was);
            }
            Err(error) => {
                put_back(&changed);
                return Err(config(error));
            }
        }
    }
    gone.extend(unmade);
    let forgotten = ledger.uninstalled(&entry, Kind::Mcp, name).and_then(|_| {
        gone.iter()
            .try_for_each(|route| ledger.set_created(&entry, route, false))
    });
    // A record kept beside entries gone would say they are missing; one
    // dropped beside entries put back would leave them nobody's.
    if let Err(error) = forgotten {
        put_back(&changed);
        let _ = ledger.installed(&entry, &record);
        for route in &gone {
            let _ = ledger.set_created(&entry, route, true);
        }
        return Err(error.into());
    }
    let detail = format!("{name} from {}", project.name());
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;

    // After the files go, as a copy's line goes after the copy; and said
    // rather than raised when the block cannot be edited, since they are
    // gone and so is their record.
    let lines: Vec<exclude::Entry> = gone.iter().map(exclude::Entry::file).collect();
    let still_excluded = local(project)
        .filter(|_| !lines.is_empty())
        .and_then(|key| exclude::remove(root, key, &lines).err())
        .map(|error| error.to_string());
    Ok(Removed::Removed {
        name: ArtifactName::from(name),
        paths: record
            .entries
            .iter()
            .map(|target| root.join(&target.file))
            .collect(),
        still_excluded,
        rendered: None,
    })
}

/// Where somebody's server `name` is in `project` while the ledger records
/// none there: the file of the first agent the project names that holds a
/// server of the name, as an install of it would meet it. A file that
/// cannot be read holds nothing that can be told.
pub(crate) fn somebodys(project: &MindProject, name: &str) -> Option<PathBuf> {
    let root = project.root();
    agents(project).into_iter().find_map(|agent| {
        let route = agent.mcp_file(root).ok()?;
        entries::held(root, agent, route)
            .ok()?
            .iter()
            .any(|held| held.name == name)
            .then(|| root.join(route))
    })
}

/// Take `left`, an entry an earlier install recorded in the project the
/// ledger names `project`, at `root`, out of its file as an uninstall takes
/// it: only while the file holds a server of its name. Whether Mindweaver
/// made the file, and what became of it.
fn taken_out(
    ledger: &Ledger,
    project: &str,
    root: &Path,
    left: &EntryTarget,
) -> Result<(bool, Written), InstallError> {
    let created = ledger.created(project, &left.file)?;
    let holds = entries::held(root, left.agent, &left.file)
        .map_err(config)?
        .iter()
        .any(|held| held.name == left.name.as_str());
    if !holds {
        return Ok((created, Written::Unchanged));
    }
    let written =
        remove(left.agent, root, &left.file, left.name.as_str(), created).map_err(config)?;
    Ok((created, written))
}

/// Take the server `name` out of `agent`'s config file at `route` in the
/// project at `root`, through the agent's renderer, and the file with it
/// when `created` and nothing else is left in it.
fn remove(
    agent: Agent,
    root: &Path,
    route: &str,
    name: &str,
    created: bool,
) -> Result<Written, ConfigUnread> {
    // Matched on the agent, so one added says how its entry is taken out
    // before it compiles. Claude's and Codex's files are one each, the one
    // the record names, as `entries::held` read it; OpenCode's is taken out
    // of the one the record names, whichever of the four comes first now.
    Ok(match agent {
        Agent::Claude => claude_mcp::remove(root, name, created)?,
        Agent::Codex => codex_mcp::remove(root, name, created)?,
        Agent::OpenCode => opencode_mcp::remove_from(root, route, name, created)?,
    })
}

/// The definition `candidate` offers, read where it is offered and held to
/// what `validate` calls invalid, as a skill is held to its rules before it
/// is copied: a token written as it is goes into no agent's file.
///
/// Followed through a link only inside where it is offered from, as a rule
/// is ([`rules::inside`]): what it leads to would be a command every agent
/// in the project runs.
fn definition(
    workspace: &WeaverWorkspace,
    candidate: &Candidate,
) -> Result<McpDefinition, InstallError> {
    let name = candidate.name();
    let from = candidate.source(workspace);
    if !from.is_file() {
        return Err(match &candidate.offer {
            Offer::Loaded { entry, .. } => InstallError::Gone {
                name: name.to_string(),
                path: from,
                workspace: paths::normalize(&workspace.root().join(entry)),
            },
            Offer::Workspace { .. } | Offer::Shelf(_) => InstallError::Missing {
                name: name.to_string(),
                path: from,
                workspace: workspace.root().to_path_buf(),
            },
        });
    }
    rules::inside(&from, &rules::within(workspace, &candidate.offer, &from))?;
    let text = fs::read_to_string(&from).map_err(|source| ArtifactError::Read {
        path: from.clone(),
        source,
    })?;
    let definition = McpDefinition::parse(&text).map_err(|source| ArtifactError::Definition {
        path: from.clone(),
        source: Box::new(source),
    })?;
    let issues = artifact::mcp_strict_issues(&definition, name, name, &[]);
    if !issues.is_empty() {
        return Err(InstallError::Unfit {
            name: name.to_string(),
            issues,
        });
    }
    Ok(definition)
}

/// Whether `agent` is given an entry of `definition` at all, asked of its
/// renderer before any file is read, and why not when it is not.
fn entry_for(agent: Agent, definition: &McpDefinition) -> Result<(), ConfigUnread> {
    // Matched on the agent, so one added says how its entry is made before
    // it compiles.
    match agent {
        Agent::Claude => drop(claude_mcp::entry(definition)?),
        Agent::Codex => drop(codex_mcp::entry(definition)?),
        // V2's, which a file keeping V1's refuses nothing more of: what
        // refuses one shape refuses the other.
        Agent::OpenCode => drop(opencode_mcp::entry(definition, Shape::V2)?),
    }
    Ok(())
}

/// What one agent's renderer did, or would do.
struct Done {
    written: Written,
    fingerprint: ContentHash,
    runs: Runs,
    warnings: Vec<Said>,
    /// What is said of the project beside the entry, that the agent may not
    /// read the file at all: Codex's, in a project it does not trust.
    unread: Vec<Said>,
}

/// What one agent's renderer would write and say, with nothing written.
struct Planned {
    runs: Runs,
    warnings: Vec<Said>,
    /// As [`Done::unread`].
    unread: Vec<Said>,
}

/// Each of `warnings`, by its code.
fn said<W: std::fmt::Display>(warnings: &[W], code: impl Fn(&W) -> &'static str) -> Vec<Said> {
    warnings
        .iter()
        .map(|warning| Said::of(code(warning), warning))
        .collect()
}

/// Write `definition`'s entry into `agent`'s config file in the project at
/// `root`, through the agent's renderer.
fn render(
    agent: Agent,
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<Done, ConfigUnread> {
    // Matched on the agent, so one added says how its entry is written
    // before it compiles.
    Ok(match agent {
        Agent::Claude => {
            let rendered = claude_mcp::render(root, definition)?;
            Done {
                written: rendered.written,
                fingerprint: rendered.entry.fingerprint(),
                runs: rendered.entry.runs(),
                warnings: said(&rendered.entry.warnings, claude_mcp::Warning::code),
                unread: Vec::new(),
            }
        }
        Agent::Codex => {
            let rendered = codex_mcp::render(root, definition, homes)?;
            Done {
                written: rendered.written,
                fingerprint: rendered.entry.fingerprint(),
                runs: rendered.entry.runs(),
                warnings: said(&rendered.entry.warnings, codex_mcp::Warning::code),
                unread: said(&rendered.warnings, codex_mcp::Warning::code),
            }
        }
        Agent::OpenCode => {
            let rendered = opencode_mcp::render(root, definition, homes)?;
            Done {
                written: rendered.written,
                fingerprint: rendered.entry.fingerprint(),
                runs: rendered.entry.runs(),
                warnings: said(&rendered.entry.warnings, opencode_mcp::Warning::code),
                unread: Vec::new(),
            }
        }
    })
}

/// What [`render`] would write into `agent`'s config file in the project at
/// `root`, and say, through the renderer's own plan of it, with nothing
/// written. Claude's is its entry, all its render decides beyond whether
/// the file holds the name already, which [`decide`] has read it for.
fn planned(
    agent: Agent,
    root: &Path,
    definition: &McpDefinition,
    homes: &AgentHomes,
) -> Result<Planned, ConfigUnread> {
    // Matched on the agent, so one added says how its entry is planned
    // before it compiles.
    Ok(match agent {
        Agent::Claude => {
            let entry = claude_mcp::entry(definition)?;
            Planned {
                runs: entry.runs(),
                warnings: said(&entry.warnings, claude_mcp::Warning::code),
                unread: Vec::new(),
            }
        }
        Agent::Codex => {
            let planned = codex_mcp::planned(root, definition, homes)?;
            Planned {
                runs: planned.entry.runs(),
                warnings: said(&planned.entry.warnings, codex_mcp::Warning::code),
                unread: said(&planned.warnings, codex_mcp::Warning::code),
            }
        }
        Agent::OpenCode => {
            let planned = opencode_mcp::planned(root, definition, homes)?;
            Planned {
                runs: planned.entry.runs(),
                warnings: said(&planned.entry.warnings, opencode_mcp::Warning::code),
                unread: Vec::new(),
            }
        }
    })
}

/// Each variable `definition`'s inputs are read from, in the order they are
/// declared, each once, a secret when any input read from it is.
fn variables(definition: &McpDefinition) -> Vec<Variable> {
    let mut variables: Vec<Variable> = Vec::new();
    for input in &definition.inputs {
        let Some(name) = &input.env_var else {
            continue;
        };
        match variables.iter_mut().find(|seen| seen.name == *name) {
            Some(seen) => seen.secret |= input.secret,
            None => variables.push(Variable {
                name: name.clone(),
                secret: input.secret,
            }),
        }
    }
    variables
}

/// Whether `error` is a renderer refusing its agent the definition, the
/// file neither read nor written: what leaves that agent unreached rather
/// than failing the install. Codex's collision with a server of the name in
/// the user's own config is the one met only as the entry is written, that
/// config being read then; the rest are asked first ([`entry_for`]).
///
/// Matched on every variant with no wildcard, so a refusal added to a
/// renderer says which it is before it compiles.
fn refuses_agent(error: &ConfigUnread) -> bool {
    match error {
        ConfigUnread::Claude(error) => match error {
            ClaudeMcpError::Unfit { .. }
            | ClaudeMcpError::Inexpressible { .. }
            | ClaudeMcpError::InputUnread { .. } => true,
            ClaudeMcpError::NotStrict { .. } | ClaudeMcpError::File(_) => false,
        },
        ConfigUnread::Codex(error) => match error {
            CodexMcpError::Unfit { .. }
            | CodexMcpError::Inexpressible { .. }
            | CodexMcpError::InputUnread { .. }
            | CodexMcpError::InputInText { .. }
            | CodexMcpError::Collision { .. } => true,
            CodexMcpError::UserScope { .. } | CodexMcpError::File(_) => false,
        },
        ConfigUnread::OpenCode(error) => match error {
            OpenCodeMcpError::Unfit { .. }
            | OpenCodeMcpError::Inexpressible { .. }
            | OpenCodeMcpError::InputUnread { .. } => true,
            OpenCodeMcpError::Twice { .. }
            | OpenCodeMcpError::UserScope { .. }
            | OpenCodeMcpError::File(_) => false,
        },
    }
}

/// `agent` unreached for its `file`, which git tracks in a local project
/// that does not set `edit_tracked`, or may: by the code a block of rules
/// withheld from such a file is said with, the condition being the one.
fn withheld(agent: Agent, file: &Path, why: &Withheld) -> Unreached {
    let said = format!(
        "{}: {why}, so no MCP server is written into it",
        file.display()
    );
    Unreached {
        agent,
        why: Said::of(why.code(), &said),
    }
}

/// The refusal of an install that reaches no agent the project names.
fn unreached_by_all(
    name: &ArtifactName,
    project: &MindProject,
    why: Vec<Unreached>,
) -> InstallError {
    InstallError::Unreached {
        name: name.to_string(),
        project: project.name().to_owned(),
        why,
    }
}

/// `error`, an agent's config file refused or failing, as an install's.
fn config(error: ConfigUnread) -> InstallError {
    InstallError::Config(Box::new(error))
}

/// A config file as it was before an entry was written into it, for
/// putting it back when a later one fails.
struct Before {
    path: PathBuf,
    /// Whether anything was at its place.
    there: bool,
    /// What it held, when it could be read.
    bytes: Option<Vec<u8>>,
    /// The folder on its route that was not there either, which the
    /// renderer makes with the file: `.codex`.
    folder: Option<PathBuf>,
}

impl Before {
    fn of(root: &Path, route: &str) -> Before {
        let path = root.join(route);
        let folder = Path::new(route)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| root.join(parent))
            .filter(|folder| fs::symlink_metadata(folder).is_err());
        Before {
            there: fs::symlink_metadata(&path).is_ok(),
            bytes: fs::read(&path).ok(),
            path,
            folder,
        }
    }

    /// Put the file back as it was: what it held, or nothing, with the
    /// folder made for it while it is empty. Best effort, as an install
    /// that failed takes its exclude lines back: what cannot be put back is
    /// no reason to hide why the install failed.
    fn restore(&self) {
        match (&self.bytes, self.there) {
            (Some(bytes), _) => {
                if fs::read(&self.path).ok().as_ref() != Some(bytes) {
                    let _ = replace_bytes_unresolved(&self.path, bytes);
                }
            }
            (None, false) => {
                let _ = fs::remove_file(&self.path);
                if let Some(folder) = &self.folder {
                    let _ = fs::remove_dir(folder);
                }
            }
            // There and unread: nothing a renderer wrote, since it reads
            // a file before it writes one.
            (None, true) => {}
        }
    }
}

/// Put back each of `written`, last first.
fn put_back(written: &[Before]) {
    for before in written.iter().rev() {
        before.restore();
    }
}
