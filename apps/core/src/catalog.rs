//! Every skill held by a set of mind projects.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::skill::{Skill, SkillError, SKILL_FILE};
use crate::workspace::MindProject;

/// The skills discovered across some mind projects, and what went wrong.
///
/// Failures are collected rather than returned, because one unreadable file
/// must not hide the forty skills next to it: a front end lists what it found
/// and reports the rest.
#[derive(Debug, Default)]
pub struct Catalog {
    skills: Vec<Skill>,
    failures: Vec<DiscoveryFailure>,
}

impl Catalog {
    /// Load the skills held by every given project.
    ///
    /// A project whose `.mind/skills` does not exist yet is not a failure: it
    /// is a project nobody has added a skill to.
    pub fn discover(projects: &[MindProject]) -> Self {
        let mut catalog = Catalog::default();

        for project in projects {
            let skills_dir = project.skills_dir();
            match skill_directories(&skills_dir) {
                Ok(directories) => {
                    for directory in directories {
                        match Skill::load(directory, project.root()) {
                            Ok(skill) => catalog.skills.push(skill),
                            Err(error) => catalog.failures.push(error.into()),
                        }
                    }
                }
                Err(DirectoryScan::Absent) => {}
                Err(DirectoryScan::Failed(source)) => {
                    catalog.failures.push(DiscoveryFailure::SkillsDirectory {
                        path: skills_dir,
                        source,
                    });
                }
            }
        }

        // Sorting here rather than in each front end is what makes two runs,
        // and two front ends, agree on an order. Name first: it is what the
        // reader is scanning for.
        catalog
            .skills
            .sort_by(|a, b| a.name().cmp(b.name()).then(a.project.cmp(&b.project)));
        catalog.failures.sort_by(|a, b| a.path().cmp(b.path()));
        catalog
    }

    /// The skills found, ordered by name and then by project.
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// What could not be read, ordered by path.
    pub fn failures(&self) -> &[DiscoveryFailure] {
        &self.failures
    }

    /// Whether anything at all was found.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Every skill declaring `name`.
    ///
    /// More than one is possible and is not by itself an error: the same name
    /// in two projects is ordinary in a workspace that manages both. Which one
    /// wins is a decision for whoever is orchestrating them, so both are
    /// returned and the caller says so.
    pub fn find(&self, name: &str) -> Vec<&Skill> {
        self.skills
            .iter()
            .filter(|skill| skill.name() == name)
            .collect()
    }

    /// The skills held by one project.
    pub fn in_project<'a>(&'a self, root: &'a Path) -> impl Iterator<Item = &'a Skill> {
        self.skills
            .iter()
            .filter(move |skill| skill.project == root)
    }
}

/// Why a skill, or a whole skills folder, could not be read.
#[derive(Debug, Error)]
pub enum DiscoveryFailure {
    #[error("{path}: cannot be listed: {source}")]
    SkillsDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Skill(#[from] SkillError),
}

impl DiscoveryFailure {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            DiscoveryFailure::SkillsDirectory { path, .. } => path,
            DiscoveryFailure::Skill(error) => error.path(),
        }
    }
}

/// Outcome of listing a skills folder that is not "here are the directories".
enum DirectoryScan {
    /// The folder is not there, which is ordinary.
    Absent,
    /// The folder is there and could not be read, which is not.
    Failed(io::Error),
}

/// The immediate subdirectories of `dir` that hold a `SKILL.md`, sorted.
///
/// Only the immediate ones: a skill's own directory is free to contain
/// scripts, references and assets, and none of those are skills.
fn skill_directories(dir: &Path) -> Result<Vec<PathBuf>, DirectoryScan> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(DirectoryScan::Absent),
        Err(error) => return Err(DirectoryScan::Failed(error)),
    };

    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(DirectoryScan::Failed)?;
        let path = entry.path();
        // A directory without a SKILL.md is not a broken skill, it is not a
        // skill: shared assets and stray dotfolders both land here.
        if path.join(SKILL_FILE).is_file() {
            directories.push(path);
        }
    }
    directories.sort();
    Ok(directories)
}
