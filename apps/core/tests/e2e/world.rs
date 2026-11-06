//! The world an end-to-end test of core builds for itself, and how a front
//! end reaches core in it.
//!
//! A world is a temporary folder standing for `/home`, with `you` in it as
//! the home of the person a front end works for, as the CLI's end-to-end
//! suite has it, so a path a test spells, `~/Projects/api`, and every path
//! core answers with read back as `/home`, read as the reference's
//! (`docs/reference.md`).
//!
//! What a person or a git server makes is made directly. What Mindweaver
//! keeps is made only by core's own public calls, each the one a command
//! makes for that step (`mind init` is [`MindProject::init_asked`], `mindweaver
//! link` is [`WeaverWorkspace::link`], an install is [`assign::install`]),
//! with what a front end hands them: the home, the agents' own folders under
//! it, and no workspace named or activated unless a test is about that. Each
//! such step is held to go cleanly, as the CLI suite's `given` holds a
//! command to exit 0 with nothing on stderr.

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::assign;
use mindweaver_core::create::Start;
use mindweaver_core::doctor::Machine;
use mindweaver_core::install::{Confirmed, Edits, Installed};
use mindweaver_core::known;
use mindweaver_core::lifecycle;
use mindweaver_core::mother::GitRepositoryMother;
use mindweaver_core::resolve::{self, Inputs, Intent, ResolveError, Resolved};
use mindweaver_core::view::Snapshot;
use mindweaver_core::{
    Agent, AgentHomes, Directories, Kind, MindProject, Reference, Share, WeaverWorkspace,
};
use serde::Serialize;
use serde_json::Value;
use tempfile::TempDir;

/// What the world's root is read as in what core answers, and what a test
/// may spell it as.
const ROOT: &str = "/home";

/// The home's name under the root: `/home/you`, as in the reference.
const YOU: &str = "you";

/// A world of folders, repositories and Mindweaver's own files, built step by
/// step for one test and gone when it ends.
pub struct World {
    /// Kept for its lifetime: dropping it takes the world away.
    root: TempDir,
    /// The root with its links resolved, as core may answer it: on macOS
    /// the system's temporary folder is reached through `/var`, a link.
    resolved: PathBuf,
}

impl World {
    /// An empty world: a home with nothing in it, not even a workspace.
    pub fn empty() -> Self {
        let root = TempDir::new().expect("make the world's root");
        fs::create_dir(root.path().join(YOU)).expect("make the home");
        let resolved = root
            .path()
            .canonicalize()
            .expect("resolve the world's root");
        Self { root, resolved }
    }

    /// A workspace, `~/Projects`, linking each of `projects`: a git
    /// repository in it made a project as `mind init` makes one, then the
    /// workspace made and each linked as `mindweaver init` and `mindweaver link` do.
    pub fn projects(projects: &[&str]) -> Self {
        let world = Self::empty();
        for project in projects {
            let at = format!("~/Projects/{project}");
            world.repository(&at).init_project(&at);
        }
        world.folder("~/Projects").init_workspace("~/Projects");
        for project in projects {
            world.link("~/Projects", &format!("~/Projects/{project}"));
        }
        world
    }

    /// The home, `/home/you`, where it is on the disk.
    pub fn home(&self) -> PathBuf {
        self.root.path().join(YOU)
    }

    /// Where `spelled`, a path as the person types it (`~/x`, `/home/you/x`),
    /// is on the disk.
    pub fn at(&self, spelled: &str) -> PathBuf {
        if spelled == "~" {
            return self.home();
        }
        if let Some(rest) = spelled.strip_prefix("~/") {
            return self.home().join(rest);
        }
        if let Some(rest) = spelled.strip_prefix(ROOT) {
            if rest.is_empty() || rest.starts_with('/') {
                return self.root.path().join(rest.trim_start_matches('/'));
            }
        }
        panic!("`{spelled}` is not a path under the world's {ROOT}: spell it from ~ or {ROOT}")
    }

    // ---------------------------------------------------------------------
    // The world outside Mindweaver: what a person or a git server made.
    // ---------------------------------------------------------------------

    /// A folder at `at`, and the folders above it.
    pub fn folder(&self, at: &str) -> &Self {
        fs::create_dir_all(self.at(at)).expect("make the folder");
        self
    }

    /// A git repository at `at`, with nothing committed.
    pub fn repository(&self, at: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(&path).expect("make the repository's folder");
        gix::init(&path).expect("initialize a repository");
        self
    }

    /// A file at `at` holding `contents`, as an editor saves one.
    pub fn file(&self, at: &str, contents: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(path.parent().expect("a file has a folder")).unwrap();
        fs::write(path, contents).expect("write the file");
        self
    }

    /// A git repository at `at` holding `files` in one commit, or `files`
    /// committed on top of what it held: a source of skills, as a team's
    /// repository is, and that source moved on.
    pub fn source(&self, at: &str, files: &[(&str, &str)]) -> &Self {
        let path = self.at(at);
        GitRepositoryMother::write(&path, files);
        GitRepositoryMother::commit(&path);
        self
    }

    /// The commit the source at `at` is on, whole.
    pub fn head(&self, at: &str) -> String {
        let repository = gix::open(self.at(at)).expect("open the source");
        repository
            .head_id()
            .expect("a source has a commit")
            .to_string()
    }

    // ---------------------------------------------------------------------
    // What a test reads back from the disk.
    // ---------------------------------------------------------------------

    /// What the file at `at` holds.
    pub fn read(&self, at: &str) -> String {
        fs::read_to_string(self.at(at)).unwrap_or_else(|error| panic!("read {at}: {error}"))
    }

    /// Whether anything is at `at`.
    pub fn exists(&self, at: &str) -> bool {
        self.at(at).symlink_metadata().is_ok()
    }

    /// Every file under `at`, by its path from there with forward slashes,
    /// sorted.
    pub fn files(&self, at: &str) -> Vec<String> {
        fn walk(root: &Path, dir: &Path, into: &mut Vec<String>) {
            for entry in fs::read_dir(dir).expect("read the folder") {
                let path = entry.expect("read an entry").path();
                if path.is_dir() {
                    walk(root, &path, into);
                } else {
                    let relative = path.strip_prefix(root).expect("under the root");
                    let parts: Vec<String> = relative
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    into.push(parts.join("/"));
                }
            }
        }
        let root = self.at(at);
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        files.sort();
        files
    }

    /// The id the project at `at` was given, which `mind init` draws afresh.
    pub fn project_id(&self, at: &str) -> String {
        let settings = self.read(&format!("{at}/.mind/settings.toml"));
        settings
            .lines()
            .find_map(|line| line.strip_prefix("id = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or_else(|| panic!("no id in {at}/.mind/settings.toml"))
            .to_owned()
    }

    // ---------------------------------------------------------------------
    // What a front end hands core.
    // ---------------------------------------------------------------------

    /// The workspace at `at`, opened.
    pub fn workspace(&self, at: &str) -> WeaverWorkspace {
        WeaverWorkspace::open(self.at(at)).unwrap_or_else(|error| panic!("open {at}: {error}"))
    }

    /// The project at `at`, opened.
    pub fn project(&self, at: &str) -> MindProject {
        MindProject::open(self.at(at)).unwrap_or_else(|error| panic!("open {at}: {error}"))
    }

    /// The agents' own folders, where the CLI finds them with nothing set:
    /// `~/.claude`, `~/.codex`, `~/.config/opencode` and `~/.agents`.
    pub fn homes(&self) -> AgentHomes {
        AgentHomes::defaults_under(&self.home())
    }

    /// Which workspace a front end gets in `start` for `intent`, with
    /// `named` given with `-W` and `activated` as the shell's activation
    /// has it, each spelled from `~` or `/home`, and the home as the
    /// default's: [`resolve::resolve`], as every command asks it.
    pub fn resolve(
        &self,
        start: &str,
        intent: Intent,
        named: Option<&str>,
        activated: Option<&str>,
    ) -> Result<Option<Resolved>, ResolveError> {
        let start = self.at(start);
        let home = self.home();
        let named = named.map(|at| self.at(at));
        let activated = activated.map(|at| self.at(at));
        resolve::resolve(&Inputs {
            flag: named.as_deref(),
            activated: activated.as_deref(),
            start: &start,
            home: Some(&home),
            intent,
        })
    }

    /// The snapshot every front end reads the workspace a command in `at`
    /// would use through ([`Snapshot::read`]), as JSON, paths read as
    /// `/home`.
    pub fn snapshot(&self, at: &str) -> Value {
        let start = self.at(at);
        let home = self.home();
        let snapshot = Snapshot::read(
            &Inputs {
                flag: None,
                activated: None,
                start: &start,
                home: Some(&home),
                intent: Intent::Read,
            },
            &Machine::default(),
        )
        .expect("read the snapshot")
        .expect("a workspace to read");
        self.json(&snapshot)
    }

    // ---------------------------------------------------------------------
    // Mindweaver's own state, through core as the commands reach it.
    // ---------------------------------------------------------------------

    /// `mindweaver init` in `at`: the workspace made, and remembered by the
    /// default one in the home when that is made already, which it never
    /// makes ([`known::remember`]).
    pub fn init_workspace(&self, at: &str) -> &Self {
        let (workspace, _) = WeaverWorkspace::init(self.at(at))
            .unwrap_or_else(|error| panic!("mindweaver init in {at}: {error}"));
        known::remember(Some(&self.home()), &workspace)
            .unwrap_or_else(|error| panic!("remember {at}: {error}"));
        self
    }

    /// `mind init` in `at`: a project for every agent, kept out of git, its
    /// folders where core puts them when none is asked for.
    pub fn init_project(&self, at: &str) -> &Self {
        self.init_project_for(at, &Agent::ALL)
    }

    /// `mind init --agents <agents>` in `at`.
    pub fn init_project_for(&self, at: &str, agents: &[Agent]) -> &Self {
        MindProject::init_asked(self.at(at), agents, Share::Local, &Directories::default())
            .unwrap_or_else(|error| panic!("mind init in {at}: {error}"));
        self
    }

    /// A project at `at` as a Mindweaver from before settings made one,
    /// marked by a `.mind/mind.toml` naming its folders and nothing else,
    /// which core's `MindProject::init` still writes for such a test.
    pub fn older_project(&self, at: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(&path).expect("make the project's folder");
        MindProject::init(&path).unwrap_or_else(|error| panic!("mark {at}: {error}"));
        self
    }

    /// `mindweaver link` of the project at `project`, in the workspace at
    /// `workspace`.
    pub fn link(&self, workspace: &str, project: &str) -> &Self {
        let mut opened = self.workspace(workspace);
        opened
            .link(&self.project(project))
            .unwrap_or_else(|error| panic!("link {project} into {workspace}: {error}"));
        self
    }

    /// `mindweaver add`, or `mind add` in a project the workspace links: `kind`
    /// `name`, said to do `description`, made by `holder`, the workspace at
    /// `workspace` itself for its library.
    pub fn add(
        &self,
        workspace: &str,
        holder: &str,
        kind: Kind,
        name: &str,
        description: &str,
    ) -> &Self {
        let opened = self.workspace(workspace);
        let holding = if holder == workspace {
            opened.own()
        } else {
            self.project(holder)
        };
        lifecycle::add(
            Some(&opened),
            &holding,
            kind,
            name,
            description,
            Start::Scratch,
        )
        .unwrap_or_else(|error| panic!("add {name} to {holder}: {error}"));
        self
    }

    /// `mindweaver install -p <project> <typed>` in the workspace at
    /// `workspace`, held to have written every copy with nothing left out.
    pub fn install(&self, workspace: &str, project: &str, typed: &str) -> &Self {
        let opened = self.workspace(workspace);
        let assigned = assign::install(
            &opened,
            &[self.project(project)],
            &Reference::parse(typed),
            None,
            &self.at(workspace),
            &self.homes(),
            Edits::Kept,
            Confirmed::Yes,
        )
        .unwrap_or_else(|error| panic!("open the ledger of {workspace}: {error}"));
        for part in assigned.parts {
            let offered = part
                .done
                .unwrap_or_else(|error| panic!("install {typed} into {project}: {error}"));
            let installation = offered
                .installed
                .unwrap_or_else(|error| panic!("install {typed} into {project}: {error}"));
            assert!(
                matches!(installation.installed, Installed::Added { .. })
                    && installation.left_out.is_empty()
                    && installation.swept.is_empty(),
                "installing {typed} into {project} did not go cleanly: {installation:?}"
            );
        }
        self
    }

    // ---------------------------------------------------------------------
    // What core answered, as a front end reads it.
    // ---------------------------------------------------------------------

    /// `value` as the JSON a front end is handed of it, every path under
    /// the root read as `/home`.
    pub fn json(&self, value: &impl Serialize) -> Value {
        let text = serde_json::to_string(value).expect("what core answers serializes");
        serde_json::from_str(&self.said(text)).unwrap()
    }

    /// `text`, something core said, every path under the root read as
    /// `/home`.
    pub fn said(&self, text: impl Display) -> String {
        let text = text.to_string();
        // The resolved spelling first: where the temporary folder is a
        // link, it is the longer one, and holds the other.
        text.replace(&*self.resolved.to_string_lossy(), ROOT)
            .replace(&*self.root.path().to_string_lossy(), ROOT)
    }

    /// `path`, one core answered with, as the person spells it.
    pub fn spelled(&self, path: &Path) -> String {
        self.said(path.display())
    }
}

/// What `value` holds at `pointer`, as a front end picks a part of what it
/// reads, or a failure naming the pointer.
pub fn at<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value
        .pointer(pointer)
        .unwrap_or_else(|| panic!("nothing at {pointer} in {value:#}"))
}
