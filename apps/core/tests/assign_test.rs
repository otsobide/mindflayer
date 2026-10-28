//! One artifact installed into several projects at once, or taken back out
//! of them (#49): each project's part what one install there does, a project
//! named twice reached once, and one that refuses said of it while the rest
//! go ahead.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::skill;
use mindflayer_core::assign::{self, Install, Offered, Part};
use mindflayer_core::install::{Edits, Installed, Removed};
use mindflayer_core::{
    create, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, Reference, Share,
};
use tempfile::TempDir;

/// A workspace with a skill of its own, `deploy`, linking a project for
/// each of `names`, below it, each marked by settings naming every agent.
fn given(names: &[&str]) -> (TempDir, FlayerWorkspace, Vec<MindProject>) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let mut projects = Vec::new();
    for name in names {
        projects.push(linked(&mut workspace, &dir.path().join(name), &Agent::ALL));
    }
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    (dir, workspace, projects)
}

/// A project at `root`, naming `agents`, shared, and linked.
fn linked(workspace: &mut FlayerWorkspace, root: &Path, agents: &[Agent]) -> MindProject {
    fs::create_dir(root).unwrap();
    let (project, _) =
        MindProject::init_settings(root, agents, Share::Shared, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    project
}

/// Where each agent reads `name` in `project`.
fn copies(project: &MindProject, name: &str) -> Vec<PathBuf> {
    [".agents/skills", ".claude/skills"]
        .iter()
        .map(|folder| project.root().join(folder).join(name))
        .collect()
}

fn installing(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    typed: &str,
) -> Vec<Part<Install>> {
    assign::install(
        workspace,
        projects,
        &Reference::parse(typed),
        None,
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap()
    .parts
}

/// What each part's install came to, by project name, or the refusal.
fn installed(parts: &[Part<Install>]) -> Vec<(String, String)> {
    parts
        .iter()
        .map(|part| {
            let said = match &part.done {
                Ok(Offered {
                    installed: Ok(done),
                    ..
                }) => match &done.installed {
                    Installed::Added { .. } => String::from("added"),
                    Installed::Updated { .. } => String::from("updated"),
                    Installed::Unchanged { .. } => String::from("unchanged"),
                    Installed::Foreign { .. } => String::from("foreign"),
                    Installed::Edited { .. } => String::from("edited"),
                },
                Ok(Offered {
                    installed: Err(error),
                    ..
                })
                | Err(error) => format!("refused: {error}"),
            };
            (part.project.name().to_owned(), said)
        })
        .collect()
}

#[test]
fn a_skill_goes_into_every_project_asked_but_one_holding_somebodys_copy_of_it() {
    let (_dir, workspace, projects) = given(&["alpha", "beta", "gamma"]);
    let theirs = &copies(&projects[1], "deploy")[0];
    fs::create_dir_all(theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();

    let parts = installing(&workspace, &projects, "deploy");

    assert_eq!(
        installed(&parts),
        [
            (String::from("alpha"), String::from("added")),
            (String::from("beta"), String::from("foreign")),
            (String::from("gamma"), String::from("added")),
        ]
    );
    for project in [&projects[0], &projects[2]] {
        for copy in copies(project, "deploy") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    assert!(fs::read_to_string(theirs.join("SKILL.md"))
        .unwrap()
        .contains("Mine"));
    assert!(!copies(&projects[1], "deploy")[1].exists());
}

#[test]
fn a_project_named_twice_is_installed_into_once() {
    let (dir, workspace, projects) = given(&["alpha"]);
    let again = MindProject::open(dir.path().join("alpha/../alpha")).unwrap();

    let parts = installing(&workspace, &[projects[0].clone(), again], "deploy");

    assert_eq!(
        installed(&parts),
        [(String::from("alpha"), String::from("added"))]
    );
}

#[test]
fn a_project_that_cannot_take_the_kind_is_refused_and_the_rest_go_ahead() {
    let (dir, mut workspace, mut projects) = given(&["alpha", "gamma"]);
    projects.insert(1, linked(&mut workspace, &dir.path().join("beta"), &[]));

    let parts = installing(&workspace, &projects, "deploy");

    assert_eq!(
        installed(&parts),
        [
            (String::from("alpha"), String::from("added")),
            (
                String::from("beta"),
                String::from(
                    "refused: beta names no agent in its settings that skills are installed \
                     for, so none can be installed into it"
                )
            ),
            (String::from("gamma"), String::from("added")),
        ]
    );
}

#[test]
fn an_uninstall_takes_it_out_where_it_is_and_says_where_it_is_not() {
    let (_dir, workspace, projects) = given(&["alpha", "beta", "gamma"]);
    installing(&workspace, &projects[..2], "deploy");

    let parts = assign::uninstall(
        &workspace,
        &projects,
        &Reference::parse("skill:deploy"),
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap()
    .parts;

    let taken: Vec<(&str, &str)> = parts
        .iter()
        .map(|part| {
            let said = match &part.done {
                Ok((Kind::Skill, Removed::Removed { .. })) => "removed",
                Ok((Kind::Skill, Removed::Absent { .. })) => "absent",
                other => panic!("{other:?}"),
            };
            (part.project.name(), said)
        })
        .collect();
    assert_eq!(
        taken,
        [
            ("alpha", "removed"),
            ("beta", "removed"),
            ("gamma", "absent")
        ]
    );
    for project in &projects {
        for copy in copies(project, "deploy") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
}

#[test]
fn a_batch_of_no_project_makes_no_ledger() {
    let (_dir, workspace, _projects) = given(&["alpha"]);
    assert!(!workspace.ledger_path().exists());

    let parts = installing(&workspace, &[], "deploy");

    assert!(parts.is_empty());
    assert!(!workspace.ledger_path().exists());
}
