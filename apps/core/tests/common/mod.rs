//! Building a real git repository for a test, without the network and without
//! shelling out to `git`.
//!
//! Shared by the gather and install suites: both need a source to take skills
//! from, and a fixture that fetches would fail for reasons that have nothing
//! to do with the code.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

use gix::objs::tree::{Entry, EntryKind};
use mindflayer_core::FlayerWorkspace;
use tempfile::TempDir;

/// A git repository holding exactly these files, with one commit.
pub fn repository(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("create a temporary directory");
    write_files(dir.path(), files);
    commit(dir.path());
    dir
}

pub fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (path, contents) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("a file has a parent")).unwrap();
        fs::write(file, contents).unwrap();
    }
}

/// Commit the whole working tree, without needing a git identity configured.
///
/// The commit object is written by hand and the branch file is pointed at it,
/// which is all a clone needs and asks nothing of the machine's git config.
pub fn commit(root: &Path) {
    // Opened rather than initialized when it is already a repository, so a
    // fixture can commit a second time and stand for a source that moved on.
    let repo = if root.join(".git").is_dir() {
        gix::open(root).expect("open the repository")
    } else {
        gix::init(root).expect("initialize a repository")
    };
    let parents: Vec<gix::ObjectId> = repo
        .head_id()
        .map(|id| vec![id.detach()])
        .unwrap_or_default();
    let tree = write_tree(&repo, root);
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
    let id = repo
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

pub fn write_tree(repo: &gix::Repository, dir: &Path) -> gix::ObjectId {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let (kind, oid) = if path.is_dir() {
            (EntryKind::Tree, write_tree(repo, &path))
        } else {
            (
                EntryKind::Blob,
                repo.write_blob(fs::read(&path).unwrap()).unwrap().detach(),
            )
        };
        entries.push(Entry {
            mode: kind.into(),
            filename: name.to_string_lossy().as_bytes().into(),
            oid,
        });
    }
    // Git requires a tree's entries in its own order, and `Entry` sorts that
    // way, so writing an unsorted one would produce an object git rejects.
    entries.sort();
    repo.write_object(&gix::objs::Tree { entries })
        .unwrap()
        .detach()
}

pub fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
}

/// A flayer workspace with nothing gathered yet.
pub fn workspace() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    (dir, workspace)
}

/// The URL of a repository on disk. A path is one, for the local transport.
pub fn url(repo: &TempDir) -> String {
    repo.path().to_string_lossy().into_owned()
}
