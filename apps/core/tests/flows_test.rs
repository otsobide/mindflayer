//! Whole flows through core, each built by its mothers and run over names
//! and agents drawn afresh: gathering, installing, editing, updating and
//! taking out, end to end.
//!
//! Each step has a suite of its own; what these hold is that the steps still
//! fit together, which is what breaks when a function one of them reuses
//! changes for the sake of another. Every flow ends where it started, so a
//! step that leaves something behind, or takes what was not its, fails here.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::gather::{self, Request};
use mindweaver_core::install::{self, Edits, Installed, Removed, Standing};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::mother::random;
use mindweaver_core::mother::{GitRepositoryMother, ProjectMother, ShelfMother, SkillMother};
use mindweaver_core::update;
use mindweaver_core::{Agent, AgentHomes, FlayerWorkspace, Kind, MindProject, Share, MIND_DIR};

const ROUNDS: usize = 8;

/// Some agents, at least one, so a project has somewhere to install to.
fn agents() -> Vec<Agent> {
    let mut agents = random::some_of(&Agent::ALL);
    if agents.is_empty() {
        agents.push(random::pick(&Agent::ALL));
    }
    agents
}

/// Names of skills, each different.
fn names() -> Vec<String> {
    let drawn: BTreeSet<String> = (0..random::between(1, 4))
        .map(|_| random::kebab())
        .collect();
    drawn.into_iter().collect()
}

/// Every file under `root` but those in `.mind`, which is Mindflayer's own,
/// with what it holds: what a project's people see of it.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == MIND_DIR) {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else {
                found.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    found
}

/// A workspace that gathered `names`, and a project it links, for `agents`.
fn ready(
    names: &[String],
    agents: &[Agent],
    share: Share,
) -> (tempfile::TempDir, FlayerWorkspace, Ledger, MindProject) {
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let (dir, mut workspace, ledger) = ShelfMother::gathered(&borrowed);
    let root = dir.path().join("app");
    fs::create_dir(&root).unwrap();
    if share == Share::Local {
        gix::init(&root).unwrap();
    }
    let project = ProjectMother::create(&root, agents, share);
    workspace.link(&project).unwrap();
    (dir, workspace, ledger, project)
}

fn install_all(workspace: &FlayerWorkspace, ledger: &Ledger, project: &MindProject) {
    for candidate in install::survey(workspace, ledger, project, Kind::Skill).unwrap() {
        let installed = install::install(
            workspace,
            ledger,
            project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap()
        .installed;
        assert!(
            matches!(installed, Installed::Added { .. }),
            "{installed:?}"
        );
    }
}

fn standings(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
) -> Vec<(String, Standing)> {
    install::standings(workspace, ledger, project, Kind::Skill).unwrap()
}

/// Every place `name` has a copy in `project`, one for each folder its
/// agents read.
fn copies(project: &MindProject, name: &str) -> Vec<PathBuf> {
    install::targets(project, Kind::Skill)
        .unwrap()
        .into_iter()
        .map(|target| project.root().join(target.folder).join(name))
        .collect()
}

#[test]
fn what_is_installed_and_taken_out_again_leaves_the_project_as_it_was() {
    for _ in 0..ROUNDS {
        let (names, agents) = (names(), agents());
        let share = random::pick(&[Share::Local, Share::Shared]);
        let (_dir, workspace, ledger, project) = ready(&names, &agents, share);
        fs::write(project.root().join("README.md"), random::text(80)).unwrap();
        let before = files(project.root());
        let exclude = project.root().join(".git/info/exclude");
        let excluded_before = fs::read(&exclude).ok();

        install_all(&workspace, &ledger, &project);
        let installed: Vec<String> = standings(&workspace, &ledger, &project)
            .into_iter()
            .map(|(name, standing)| {
                assert_eq!(standing, Standing::Current, "{name} for {agents:?}");
                name
            })
            .collect();
        assert_eq!(installed, names, "for {agents:?}");
        for name in &names {
            for copy in copies(&project, name) {
                assert!(copy.join("SKILL.md").is_file(), "{copy:?} for {agents:?}");
            }
        }

        for name in &names {
            let removed = install::uninstall(
                &workspace,
                &ledger,
                &project,
                Kind::Skill,
                name,
                &AgentHomes::default(),
            )
            .unwrap();
            assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
        }

        assert!(standings(&workspace, &ledger, &project).is_empty());
        assert_eq!(
            files(project.root()),
            before,
            "{names:?} for {agents:?}, {share:?}"
        );
        assert_eq!(
            fs::read(&exclude).ok(),
            excluded_before,
            "{names:?}, {share:?}"
        );
    }
}

#[test]
fn an_edited_copy_is_kept_until_it_is_forced_out() {
    for _ in 0..ROUNDS {
        let (names, agents) = (names(), agents());
        let (_dir, workspace, ledger, project) = ready(&names, &agents, Share::Shared);
        install_all(&workspace, &ledger, &project);
        let name = random::pick(&names);
        let edited = random::pick(&copies(&project, &name)).join("SKILL.md");
        fs::write(&edited, SkillMother::create(&name, "Edited by hand")).unwrap();

        let stood: BTreeMap<String, Standing> = standings(&workspace, &ledger, &project)
            .into_iter()
            .collect();
        assert_eq!(stood[&name], Standing::Edited, "{edited:?}");
        assert!(
            stood
                .iter()
                .filter(|(other, _)| **other != name)
                .all(|(_, standing)| *standing == Standing::Current),
            "{stood:?}"
        );

        let kept = install::uninstall(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            &name,
            &AgentHomes::default(),
        )
        .unwrap();
        assert!(matches!(kept, Removed::Edited { .. }), "{kept:?}");
        assert!(edited.is_file(), "an edited copy taken out");
        for copy in copies(&project, &name) {
            assert!(
                copy.join("SKILL.md").is_file(),
                "{copy:?} taken out with the edited one"
            );
        }

        let forced = install::uninstall_with(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            &name,
            &AgentHomes::default(),
            Edits::Forced,
        )
        .unwrap();
        assert!(matches!(forced, Removed::Removed { .. }), "{forced:?}");
        for copy in copies(&project, &name) {
            assert!(!copy.exists(), "{copy:?} left after a forced uninstall");
        }
    }
}

#[test]
fn a_source_that_moved_on_outdates_its_copies_and_an_update_brings_them_current() {
    for _ in 0..ROUNDS {
        let (name, agents) = (random::kebab(), agents());
        let (dir, workspace) = mindweaver_core::mother::WorkspaceMother::empty();
        let source = GitRepositoryMother::with_skills(&[name.as_str()]);
        let url = GitRepositoryMother::url(&source);
        let ledger = workspace.ledger().unwrap();
        gather::gather(&workspace, &ledger, &Request::git(&url)).unwrap();
        let root = dir.path().join("app");
        let project = ProjectMother::create(&root, &agents, Share::Shared);
        let mut workspace = workspace;
        workspace.link(&project).unwrap();
        install_all(&workspace, &ledger, &project);

        let moved_on = SkillMother::create(&name, &SkillMother::description());
        GitRepositoryMother::write(
            source.path(),
            &[(&format!("skills/{name}/SKILL.md"), &moved_on)],
        );
        GitRepositoryMother::commit(source.path());
        gather::gather(&workspace, &ledger, &Request::git(&url)).unwrap();
        assert_eq!(
            standings(&workspace, &ledger, &project),
            [(name.clone(), Standing::Outdated)],
            "for {agents:?}"
        );

        drop(ledger);
        update::apply(
            &workspace,
            std::slice::from_ref(&project),
            &[],
            Edits::Kept,
            &AgentHomes::default(),
        )
        .unwrap();

        let ledger = workspace.ledger().unwrap();
        assert_eq!(
            standings(&workspace, &ledger, &project),
            [(name.clone(), Standing::Current)]
        );
        for copy in copies(&project, &name) {
            assert_eq!(
                fs::read_to_string(copy.join("SKILL.md")).unwrap(),
                moved_on,
                "{copy:?}"
            );
        }
    }
}

#[test]
fn a_copy_taken_away_by_hand_stands_missing_and_an_update_puts_it_back() {
    for _ in 0..ROUNDS {
        let (names, agents) = (names(), agents());
        let (_dir, workspace, ledger, project) = ready(&names, &agents, Share::Shared);
        install_all(&workspace, &ledger, &project);
        let name = random::pick(&names);
        let gone = random::pick(&copies(&project, &name));
        fs::remove_dir_all(&gone).unwrap();

        let stood: BTreeMap<String, Standing> = standings(&workspace, &ledger, &project)
            .into_iter()
            .collect();
        assert_eq!(stood[&name], Standing::Missing, "{gone:?}");

        drop(ledger);
        update::apply(
            &workspace,
            std::slice::from_ref(&project),
            &[],
            Edits::Kept,
            &AgentHomes::default(),
        )
        .unwrap();

        assert!(gone.join("SKILL.md").is_file(), "{gone:?} not put back");
        let ledger = workspace.ledger().unwrap();
        assert!(standings(&workspace, &ledger, &project)
            .iter()
            .all(|(_, standing)| *standing == Standing::Current));
    }
}

#[test]
fn a_folder_somebody_keeps_where_a_copy_would_go_refuses_the_install_and_is_left_alone() {
    for _ in 0..ROUNDS {
        let (names, agents) = (names(), agents());
        let (_dir, workspace, ledger, project) = ready(&names, &agents, Share::Shared);
        let name = random::pick(&names);
        let theirs = random::pick(&copies(&project, &name));
        let text = random::text(60);
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("SKILL.md"), &text).unwrap();
        let before = files(project.root());

        let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.name() == name.as_str())
            .unwrap();
        assert_eq!(candidate.standing, Standing::Foreign, "{theirs:?}");
        let refused = install::install(
            &workspace,
            &ledger,
            &project,
            candidate,
            &AgentHomes::default(),
        )
        .unwrap()
        .installed;

        assert!(matches!(refused, Installed::Foreign { .. }), "{refused:?}");
        assert_eq!(files(project.root()), before, "the install wrote something");
        assert_eq!(fs::read_to_string(theirs.join("SKILL.md")).unwrap(), text);
    }
}
