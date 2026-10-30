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
//!   its folder: SQLite makes `mindflayer.db-wal` and `mindflayer.db-shm`
//!   beside it to read it and takes them away after, so a watch on the
//!   folder would wake for every snapshot it set off, without end. A write
//!   reaches the file itself when its log is checkpointed, at the latest as
//!   the last connection to it closes, which is when each command's does.
//! - **Each path says what it is of** ([`Scope`]): the workspace, for its
//!   marker, its ledger, its library, its shelf and what it loads, or a
//!   project, by its entry, so a front end that rebuilds part of what it
//!   shows can tell which part.
//! - **Asked again after every change.** A skill installed is a new folder
//!   to watch, and a project linked a set of them, so a front end asks for
//!   the list again once something on it changed rather than keep one the
//!   disk has moved past; `flayer.toml` changing, a project linked or
//!   unlinked, is one such change.

use std::path::{Path, PathBuf};

use crate::agent::Agent;
use crate::catalog::Catalog;
use crate::claude_rules;
use crate::kind::Kind;
use crate::paths;
use crate::template;
use crate::workspace::{
    FlayerWorkspace, MarkerFile, MindProject, FLAYER_CONFIG, FLAYER_DIR, MIND_DIR,
};

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
/// knows. A read, as the snapshot is: nothing is made or changed, and what
/// cannot be listed is left out, as discovery reports it.
pub fn watched(workspace: &FlayerWorkspace, home: Option<&Path>) -> Vec<Watched> {
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
            Err(_) => list.add(&at_home, marker_of_workspace(&loaded.root)),
        }
    }
    if let Some(home) = home {
        list.add(&at_home, marker_of_workspace(home));
    }

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

/// Where a workspace at `root` keeps its marker.
fn marker_of_workspace(root: &Path) -> PathBuf {
    root.join(FLAYER_DIR).join(FLAYER_CONFIG)
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

    /// Everything of `project`'s a snapshot reads: its marker, whichever
    /// file it is in, the files every agent reads in it, the folders it
    /// keeps each kind in and those its agents read them from, the files
    /// Claude is given its rules in, and its templates.
    ///
    /// Every agent's files, not only those of the agents it names: its
    /// rules' block in `AGENTS.md` and the import in `CLAUDE.md` are read
    /// whichever it names, and a few files more are no cost against a
    /// change missed.
    fn project(&mut self, scope: &Scope, project: &MindProject, workspace: &FlayerWorkspace) {
        self.markers(scope, project.root());
        for agent in Agent::ALL {
            for file in agent.reads_files() {
                self.add(scope, project.root().join(file));
            }
        }
        self.folders(scope, project);
        // Filed by the rule's route, folders and all, as rules are kept.
        let claude = project.root().join(claude_rules::folder());
        for folder in Catalog::looked_in(&claude, Kind::Rule) {
            self.add(scope, folder);
        }
        self.templates(scope, workspace, project);
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
    fn templates(&mut self, scope: &Scope, workspace: &FlayerWorkspace, holder: &MindProject) {
        for folder in template::looked_in(Some(workspace), holder) {
            self.add(scope, folder);
        }
    }
}
