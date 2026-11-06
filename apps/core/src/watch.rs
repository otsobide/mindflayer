//! What a front end watches to know when what it shows of a workspace is
//! out of date (#74): every file and folder a snapshot of it reads that a
//! terminal, the TUI or an editor can change, each named on its own.
//!
//! Core's, as every path rule is: which files say what a project holds, and
//! which folders its agents read, are facts the server must not learn, so
//! the panel's backend hands this list over as it is, and a desktop shell
//! would hand over the same one.
//!
//! - **Named, never a tree.** Each path is watched on its own: a file for
//!   its own changes, a folder for what is directly in it, and nothing
//!   further down. Never a project's root, which would wake for every file
//!   of the code beside it, and never a `.git`, a build's output or what a
//!   skill keeps in its own folders; a recursive watch costs one of the
//!   system's watches for every folder under it, and a repository with a
//!   `node_modules` runs out of them. The folders named are those discovery
//!   looks in ([`Catalog::looked_in`]), so what is listed is what is read.
//! - **What is not there yet is named all the same**: a project's
//!   `.claude/skills` before its first install, a marker before `init`.
//!   Watching the nearest folder above it that is there, for its being
//!   made, is the watcher's, since it is the same for any path.
//! - **Nothing a read changes.** The ledger is named by its file, never by
//!   its folder: SQLite makes `mindweaver.db-wal` and `mindweaver.db-shm`
//!   beside it to read it and takes them away after, so a watch on the
//!   folder would wake for every snapshot it set off, without end. A write
//!   reaches the file itself when its log is checkpointed, at the latest as
//!   the last connection to it closes, which is when each command's does.
//! - **Each path says what it is of** ([`Scope`]): the workspace, for its
//!   marker, its ledger, its library, its shelf and what it loads, or a
//!   project, by its entry, so a front end that rebuilds part of what it
//!   shows can tell which part.
//! - **The agents' own folders too**, those doctor reads for every project
//!   ([`crate::doctor`]): Codex's config, OpenCode's global one, Claude's
//!   user memory file and each folder of skills an agent reads at user
//!   scope, as [`AgentHomes`] names them, under the workspace's scope, as
//!   the default workspace's marker is: they are no holder's, and a
//!   snapshot reads them for every project.
//! - **Asked again after every change.** A skill installed is a new folder
//!   to watch, and a project linked a set of them, so a front end asks for
//!   the list again once something on it changed rather than keep one the
//!   disk has moved past; `weaver.toml` changing, a project linked or
//!   unlinked, is one such change.

use std::path::{Path, PathBuf};

use crate::agent::{Agent, AgentHomes};
use crate::catalog::Catalog;
use crate::claude_md::CLAUDE_MD;
use crate::claude_rules;
use crate::codex_mcp;
use crate::kind::Kind;
use crate::names::Names;
use crate::opencode_mcp;
use crate::paths;
use crate::template;
use crate::workspace::{MarkerFile, MindProject, WeaverWorkspace, MIND_DIR};

/// One path to watch, and what a change to it is a change to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watched {
    /// A file, watched for its own changes, or a folder, watched for what
    /// is directly in it; either may not be there yet.
    pub path: PathBuf,
    pub scope: Scope,
}

/// What a watched path belongs to.
///
/// The holders a front end reads an artifact from, the workspace's own or a
/// project by its entry, so one that keeps what it read of an artifact can
/// tell whether a change reached it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// The workspace: its marker, its ledger, its library and its shelf,
    /// what it loads, and the default workspace's marker, whose `known`
    /// the snapshot lists.
    Workspace,
    /// A project the workspace links, by its entry as the marker spells it
    /// and the snapshot lists it.
    Project(String),
}

/// Every path whose change means a snapshot of `workspace` is out of date,
/// the workspace's first and then each project's in the registry's order,
/// each path once for each scope it is of.
///
/// `home` is the one [`Snapshot::of`](crate::view::Snapshot::of) is handed,
/// whose default workspace's marker says which workspaces this machine
/// knows, and `homes` the agents' own folders its doctor is told of. A
/// read, as the snapshot is: nothing is made or changed, and what cannot be
/// listed is left out, as discovery reports it.
pub fn watched(
    workspace: &WeaverWorkspace,
    home: Option<&Path>,
    homes: &AgentHomes,
) -> Vec<Watched> {
    let mut list = List::default();
    let at_home = Scope::Workspace;

    list.add(&at_home, workspace.config_path());
    list.add(&at_home, workspace.ledger_path());
    // The shelf is listed from the ledger, but a gather writes its folders
    // first, so naming them says so before the ledger's log is checkpointed.
    for kind in Kind::ALL {
        list.add(&at_home, workspace.gathered_dir(kind));
    }
    let own = workspace.own();
    list.folders(&at_home, &own);
    list.templates(&at_home, workspace, &own);
    for loaded in workspace.loads() {
        match loaded.workspace {
            Ok(source) => {
                list.add(&at_home, source.config_path());
                list.folders(&at_home, &source.own());
            }
            // What would bring it back.
            Err(_) => list.workspace_markers(&at_home, &loaded.root),
        }
    }
    if let Some(home) = home {
        list.workspace_markers(&at_home, home);
    }
    list.agent_homes(&at_home, homes);

    for member in workspace.members() {
        let scope = Scope::Project(paths::as_route(&member.entry));
        match &member.project {
            Ok(project) => list.project(&scope, project, workspace),
            // What would make it a project again.
            Err(_) => list.markers(&scope, &member.root),
        }
    }
    list.0
}

/// The list being put together, each path once for each scope.
#[derive(Default)]
struct List(Vec<Watched>);

impl List {
    fn add(&mut self, scope: &Scope, path: PathBuf) {
        let watched = Watched {
            path,
            scope: scope.clone(),
        };
        if !self.0.contains(&watched) {
            self.0.push(watched);
        }
    }

    /// Each file a workspace's marker can be in, under `root`: the one
    /// Mindweaver writes and the one a Mindflayer build wrote, either of
    /// which makes it a workspace, and a move from the one to the other
    /// changes.
    fn workspace_markers(&mut self, scope: &Scope, root: &Path) {
        for names in Names::ALL {
            self.add(
                scope,
                root.join(names.workspace_dir())
                    .join(names.workspace_config()),
            );
        }
    }

    /// Everything of `project`'s a snapshot reads: its marker, whichever
    /// file it is in, the files every agent reads in it, the folders it
    /// keeps each kind in and those its agents read them from, the files
    /// Claude is given its rules in, and its templates.
    ///
    /// Every agent's files, not only those of the agents it names: its
    /// rules' block in `AGENTS.md` and the import in `CLAUDE.md` are read
    /// whichever it names, and a few files more are no cost against a
    /// change missed.
    fn project(&mut self, scope: &Scope, project: &MindProject, workspace: &WeaverWorkspace) {
        self.markers(scope, project.root());
        for agent in Agent::ALL {
            for file in agent.reads_files() {
                self.add(scope, project.root().join(file));
            }
        }
        self.folders(scope, project);
        // Every folder an agent reads skills from, whoever put them there,
        // which doctor compares, and the scratch folder whose stages it
        // says.
        for agent in Agent::ALL {
            for folder in agent.reads(Kind::Skill) {
                for looked in Catalog::looked_in(&project.root().join(folder), Kind::Skill) {
                    self.add(scope, looked);
                }
            }
        }
        self.add(scope, project.scratch_dir());
        // Filed by the rule's route, folders and all, as rules are kept, in
        // the folder a Mindflayer build filed them in too, where those it
        // recorded stay until they are moved.
        for names in Names::ALL {
            let claude = project.root().join(claude_rules::folder_under(names));
            for folder in Catalog::looked_in(&claude, Kind::Rule) {
                self.add(scope, folder);
            }
        }
        self.templates(scope, workspace, project);
    }

    /// What doctor reads in each agent's own folder for every project:
    /// Codex's config, OpenCode's global config files, Claude's user memory
    /// file, which it leaves out of what keeps Claude from `AGENTS.md`, and
    /// each folder of skills an agent reads at user scope, with each skill
    /// in it. None of a folder `homes` does not name.
    fn agent_homes(&mut self, scope: &Scope, homes: &AgentHomes) {
        if let Some(home) = &homes.codex_home {
            self.add(scope, home.join(codex_mcp::USER_FILE));
        }
        if let Some(folder) = &homes.opencode_config_dir {
            for file in opencode_mcp::USER_FILES {
                self.add(scope, folder.join(file));
            }
        }
        if let Some(folder) = &homes.claude_config_dir {
            self.add(scope, folder.join(CLAUDE_MD));
        }
        for agent in Agent::ALL {
            for folder in homes.skills_read_by(agent) {
                for looked in Catalog::looked_in(&folder, Kind::Skill) {
                    self.add(scope, looked);
                }
            }
        }
    }

    /// Each file a project's marker can be in, under `root`: whichever is
    /// there, and the others, which a migration or a project made there
    /// would write.
    fn markers(&mut self, scope: &Scope, root: &Path) {
        let mind = root.join(MIND_DIR);
        for file in [
            MarkerFile::SettingsToml,
            MarkerFile::SettingsJson,
            MarkerFile::MindToml,
        ] {
            self.add(scope, mind.join(file.name()));
        }
    }

    /// Every folder discovery looks in for each kind `holder` keeps.
    fn folders(&mut self, scope: &Scope, holder: &MindProject) {
        for kind in Kind::ALL {
            for folder in Catalog::folders(holder, kind) {
                for looked in Catalog::looked_in(&folder, kind) {
                    self.add(scope, looked);
                }
            }
        }
    }

    /// Every folder the templates `holder` is offered are looked for in.
    fn templates(&mut self, scope: &Scope, workspace: &WeaverWorkspace, holder: &MindProject) {
        for folder in template::looked_in(Some(workspace), holder) {
            self.add(scope, folder);
        }
    }
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
