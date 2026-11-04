//! What the integration suites share, each helper the mother it was moved
//! into (`mindweaver_core::mother`), kept here under the name every suite
//! already calls it by.
//!
//! Building a real git repository for a test, without the network and without
//! shelling out to `git`, is shared by the gather and install suites: both
//! need a source to take skills from, and a fixture that fetches would fail
//! for reasons that have nothing to do with the code.

#![allow(dead_code)]

use std::path::Path;

use mindweaver_core::mother::{GitRepositoryMother, SkillMother, WorkspaceMother};
use mindweaver_core::{Directories, FlayerWorkspace, Kind};
use tempfile::TempDir;

/// A git repository holding exactly these files, with one commit.
pub fn repository(files: &[(&str, &str)]) -> TempDir {
    GitRepositoryMother::create(files)
}

pub fn write_files(root: &Path, files: &[(&str, &str)]) {
    GitRepositoryMother::write(root, files);
}

/// Commit the whole working tree, without needing a git identity configured.
pub fn commit(root: &Path) {
    GitRepositoryMother::commit(root);
}

pub fn write_tree(repo: &gix::Repository, dir: &Path) -> gix::ObjectId {
    GitRepositoryMother::tree(repo, dir)
}

/// Put every file under `work_tree` in its repository's index, as `git add
/// -A` would, so git tracks them, committed or not.
pub fn indexed(work_tree: &Path) {
    GitRepositoryMother::index(work_tree);
}

pub fn skill(name: &str, description: &str) -> String {
    SkillMother::create(name, description)
}

/// A flayer workspace with nothing gathered yet.
pub fn workspace() -> (TempDir, FlayerWorkspace) {
    WorkspaceMother::empty()
}

/// Where a workspace keeps its own when told to keep them beside its
/// projects: for a test about a workspace sharing a folder with a project,
/// rather than about where a new one keeps them.
pub fn beside() -> Directories {
    Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules")
}

/// The URL of a repository on disk. A path is one, for the local transport.
pub fn url(repo: &TempDir) -> String {
    GitRepositoryMother::url(repo)
}
