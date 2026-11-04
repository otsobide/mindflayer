//! Mothers of what lives on the disk: a workspace, a project, a git
//! repository to gather from, and a workspace with a shelf already filled.
//!
//! Each lives in a temporary folder the caller holds, and goes when it is
//! dropped, so no test reads or writes outside the folder it was given. The
//! git repository is written in process with `gix`, with no network and no
//! `git` binary, and asks nothing of the machine's git configuration.

use std::fs;
use std::path::Path;

use gix::objs::tree::{Entry, EntryKind};
use tempfile::TempDir;

use crate::agent::Agent;
use crate::gather::{self, Request};
use crate::ledger::Ledger;
use crate::mother::artifact::SkillMother;
use crate::mother::random;
use crate::settings::Share;
use crate::workspace::{Directories, FlayerWorkspace, MindProject};

/// Makes flayer workspaces.
pub struct WorkspaceMother;

impl WorkspaceMother {
    /// A workspace with nothing linked, loaded or gathered, keeping its own
    /// in its library.
    pub fn empty() -> (TempDir, FlayerWorkspace) {
        let dir = TempDir::new().expect("make a temporary folder");
        let (workspace, _) = FlayerWorkspace::init(dir.path()).expect("make a workspace");
        (dir, workspace)
    }

    /// A workspace in `root`, made if it is not yet one.
    pub fn create(root: &Path) -> FlayerWorkspace {
        fs::create_dir_all(root).expect("make the workspace's folder");
        FlayerWorkspace::init(root).expect("make a workspace").0
    }
}

/// Makes mind projects.
pub struct ProjectMother;

impl ProjectMother {
    /// A project in a folder of `parent` named by a word, marked by its
    /// settings, for some agents, either share mode.
    pub fn random(parent: &Path) -> MindProject {
        let agents = random::some_of(&Agent::ALL);
        let share = random::pick(&[Share::Local, Share::Shared]);
        Self::create(&parent.join(random::word()), &agents, share)
    }

    /// The project `root` is, marked by its settings as `mind init` marks
    /// one, for `agents`.
    pub fn create(root: &Path, agents: &[Agent], share: Share) -> MindProject {
        fs::create_dir_all(root).expect("make the project's folder");
        MindProject::init_asked(root, agents, share, &Directories::default())
            .expect("make a project")
            .0
    }

    /// The project `root` is, marked by a `mind.toml` as a version 2 `mind
    /// init` did, so a copy is installed into the folder it authors in.
    pub fn older(root: &Path) -> MindProject {
        fs::create_dir_all(root).expect("make the project's folder");
        MindProject::init(root).expect("make a project").0
    }

    /// An older project in `workspace`'s folder named `name`, linked to it.
    pub fn linked(workspace: &mut FlayerWorkspace, name: &str) -> MindProject {
        let project = Self::older(&workspace.root().join(name));
        workspace.link(&project).expect("link the project");
        project
    }
}

/// Makes git repositories on the disk, each with one commit, to gather from.
pub struct GitRepositoryMother;

impl GitRepositoryMother {
    /// A repository holding exactly `files`, each a path and its contents.
    pub fn create(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("make a temporary folder");
        Self::write(dir.path(), files);
        Self::commit(dir.path());
        dir
    }

    /// A repository holding a skill for each of `names`, under `skills/`.
    pub fn with_skills(names: &[&str]) -> TempDir {
        let files: Vec<(String, String)> = names
            .iter()
            .map(|name| {
                (
                    format!("skills/{name}/SKILL.md"),
                    SkillMother::create(name, &format!("What {name} does")),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        Self::create(&borrowed)
    }

    /// The URL a gather is handed for a repository on the disk: its path, for
    /// the local transport.
    pub fn url(repository: &TempDir) -> String {
        repository.path().to_string_lossy().into_owned()
    }

    /// Write `files` under `root`, making the folders they are in.
    pub fn write(root: &Path, files: &[(&str, &str)]) {
        for (path, contents) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().expect("a file has a parent")).unwrap();
            fs::write(file, contents).unwrap();
        }
    }

    /// Commit the whole working tree, without a git identity configured.
    ///
    /// The commit object is written by hand and the branch file pointed at
    /// it, which is all a clone needs. Opened rather than initialized when it
    /// is already a repository, so a fixture can commit a second time and
    /// stand for a source that moved on.
    pub fn commit(root: &Path) {
        let repository = if root.join(".git").is_dir() {
            gix::open(root).expect("open the repository")
        } else {
            gix::init(root).expect("initialize a repository")
        };
        let parents: Vec<gix::ObjectId> = repository
            .head_id()
            .map(|id| vec![id.detach()])
            .unwrap_or_default();
        let tree = Self::tree(&repository, root);
        let who = gix::actor::Signature {
            name: "Fixture".into(),
            email: "fixture@example.com".into(),
            time: gix::date::Time::new(0, 0),
        };
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author: who.clone(),
            committer: who,
            encoding: None,
            message: "fixture".into(),
            extra_headers: Vec::new(),
        };
        let id = repository
            .write_object(&commit)
            .expect("write the commit")
            .detach();

        let head = fs::read_to_string(root.join(".git").join("HEAD")).unwrap();
        let branch = head
            .trim()
            .strip_prefix("ref: ")
            .expect("a fresh repository has a symbolic HEAD");
        let reference = root.join(".git").join(branch);
        fs::create_dir_all(reference.parent().unwrap()).unwrap();
        fs::write(reference, format!("{id}\n")).unwrap();
    }

    /// Write `dir`, all but `.git`, as a tree of `repository`'s.
    pub fn tree(repository: &gix::Repository, dir: &Path) -> gix::ObjectId {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            let path = entry.path();
            let (kind, oid) = if path.is_dir() {
                (EntryKind::Tree, Self::tree(repository, &path))
            } else {
                (
                    EntryKind::Blob,
                    repository
                        .write_blob(fs::read(&path).unwrap())
                        .unwrap()
                        .detach(),
                )
            };
            entries.push(Entry {
                mode: kind.into(),
                filename: name.to_string_lossy().as_bytes().into(),
                oid,
            });
        }
        // Git requires a tree's entries in its own order, which `Entry`
        // sorts by: an unsorted one is an object git rejects.
        entries.sort();
        repository
            .write_object(&gix::objs::Tree { entries })
            .unwrap()
            .detach()
    }

    /// Put every file under `work_tree` in its repository's index, as `git
    /// add -A` would, so git tracks them, committed or not.
    pub fn index(work_tree: &Path) {
        let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
        let tree = Self::tree(&repository, work_tree);
        let mut index = repository.index_from_tree(&tree).unwrap();
        index.write(Default::default()).unwrap();
    }
}

/// Makes workspaces whose shelf already holds what was gathered.
pub struct ShelfMother;

impl ShelfMother {
    /// A workspace that gathered a skill for each of `names` from one
    /// repository, and its ledger. The repository is gone once the gather
    /// took what it holds, as a clone keeps it on the shelf.
    pub fn gathered(names: &[&str]) -> (TempDir, FlayerWorkspace, Ledger) {
        let (dir, workspace) = WorkspaceMother::empty();
        let source = GitRepositoryMother::with_skills(names);
        let ledger = workspace.ledger().expect("open the ledger");
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(GitRepositoryMother::url(&source)),
        )
        .expect("gather the skills");
        (dir, workspace, ledger)
    }
}
