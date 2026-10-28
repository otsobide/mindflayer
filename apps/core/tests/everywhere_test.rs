//! One of the workspace's own removed from the workspace and from every
//! project it links that has it (#50): what is unchanged since it was
//! installed taken out, what was changed since and what is somebody's left
//! and said, and the library copy removed after them all.

mod common;

use std::fs;
use std::path::PathBuf;

use common::skill;
use mindflayer_core::assign;
use mindflayer_core::everywhere::{self, Plan, Step};
use mindflayer_core::install::{Edits, Removed};
use mindflayer_core::{
    create, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, Reference, Share,
    Standing,
};
use tempfile::TempDir;

/// A workspace with a skill of its own, `deploy`, linking a project for
/// each of `names`, below it, each marked by settings naming every agent.
fn given(names: &[&str]) -> (TempDir, FlayerWorkspace, Vec<MindProject>) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let mut projects = Vec::new();
    for name in names {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        let (project, _) =
            MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
                .unwrap();
        workspace.link(&project).unwrap();
        projects.push(project);
    }
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    (dir, workspace, projects)
}

/// Where each agent reads `name` in `project`.
fn copies(project: &MindProject, name: &str) -> Vec<PathBuf> {
    [".agents/skills", ".claude/skills"]
        .iter()
        .map(|folder| project.root().join(folder).join(name))
        .collect()
}

/// The workspace's own `deploy`, in its library.
fn library(workspace: &FlayerWorkspace) -> PathBuf {
    workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy")
}

fn install(workspace: &FlayerWorkspace, projects: &[MindProject], typed: &str) {
    let parts = assign::install(
        workspace,
        projects,
        &Reference::parse(typed),
        None,
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap()
    .parts;
    for part in parts {
        assert!(
            part.done
                .as_ref()
                .is_ok_and(|offered| offered.installed.is_ok()),
            "{:?}",
            part.done
        );
    }
}

/// Each project's copy in the plan, by its name, with how it stands and
/// what is done with it.
fn said(plan: &Plan) -> Vec<(String, Standing, Step)> {
    plan.copies
        .iter()
        .map(|copy| {
            (
                copy.project.name().to_owned(),
                copy.standing,
                copy.step.clone(),
            )
        })
        .collect()
}

/// Projects whose copies of `deploy` stand each as their name says, and
/// one, `bare`, that holds none: `current` installed after the library copy
/// changed, `outdated` before, `edited` with a copy changed by hand, and
/// `theirs` holding somebody's own of that name.
fn every_kind_of_copy() -> (TempDir, FlayerWorkspace, Vec<MindProject>) {
    let (dir, workspace, projects) = given(&["bare", "current", "edited", "outdated", "theirs"]);
    install(&workspace, &projects[2..4], "deploy");
    fs::write(
        library(&workspace).join("SKILL.md"),
        skill("deploy", "Ship it, then check"),
    )
    .unwrap();
    install(&workspace, &projects[1..2], "deploy");
    fs::write(
        copies(&projects[2], "deploy")[1].join("SKILL.md"),
        skill("deploy", "Ship it my way"),
    )
    .unwrap();
    let theirs = &copies(&projects[4], "deploy")[1];
    fs::create_dir_all(theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();
    (dir, workspace, projects)
}

#[test]
fn the_plan_says_each_project_copy_and_how_it_stands() {
    let (_dir, workspace, projects) = every_kind_of_copy();

    let plan = everywhere::plan(&workspace, &Reference::parse("deploy")).unwrap();

    assert_eq!(plan.target.path, library(&workspace));
    assert_eq!(
        said(&plan),
        [
            (String::from("current"), Standing::Current, Step::Uninstall),
            (
                String::from("edited"),
                Standing::Edited,
                Step::Leave(vec![copies(&projects[2], "deploy")[1].clone()])
            ),
            (
                String::from("outdated"),
                Standing::Outdated,
                Step::Uninstall
            ),
            (
                String::from("theirs"),
                Standing::Foreign,
                Step::Somebodys(copies(&projects[4], "deploy")[1].clone())
            ),
        ]
    );
    // Somebody's is no copy of it, so removing it alone orphans the rest.
    let orphaned: Vec<&str> = plan
        .orphaned()
        .iter()
        .map(|copy| copy.project.name())
        .collect();
    assert_eq!(orphaned, ["current", "edited", "outdated"]);
}

#[test]
fn the_plan_writes_nothing() {
    let (_dir, workspace, projects) = given(&["alpha"]);
    let plan = everywhere::plan(&workspace, &Reference::parse("deploy")).unwrap();
    assert!(plan.copies.is_empty());
    assert!(!workspace.ledger_path().exists(), "made a ledger to read");

    install(&workspace, &projects, "deploy");
    let before = fs::read(workspace.ledger_path()).unwrap();
    everywhere::plan(&workspace, &Reference::parse("deploy")).unwrap();

    assert_eq!(fs::read(workspace.ledger_path()).unwrap(), before);
    assert!(library(&workspace).join("SKILL.md").is_file());
    for copy in copies(&projects[0], "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn removing_it_everywhere_takes_out_what_is_unchanged_and_leaves_the_rest() {
    let (_dir, workspace, projects) = every_kind_of_copy();

    let applied = everywhere::apply(
        &workspace,
        &Reference::parse("deploy"),
        &AgentHomes::default(),
    )
    .unwrap();

    applied.removed.unwrap();
    assert!(!library(&workspace).exists());
    let done: Vec<(&str, bool)> = applied
        .plan
        .copies
        .iter()
        .map(|copy| {
            let removed = matches!(copy.done, Some(Ok(Removed::Removed { .. })));
            (copy.project.name(), removed)
        })
        .collect();
    assert_eq!(
        done,
        [
            ("current", true),
            ("edited", false),
            ("outdated", true),
            ("theirs", false)
        ]
    );
    for project in [&projects[1], &projects[3]] {
        for copy in copies(project, "deploy") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
    // Every copy of the edited one stays, and is still recorded: orphaned
    // now, and somebody's work until an uninstall is forced.
    for copy in copies(&projects[2], "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    let ledger = workspace.ledger_for_reading().unwrap();
    let standings: Vec<(String, Standing)> =
        mindflayer_core::install::standings(&workspace, &ledger, &projects[2], Kind::Skill)
            .unwrap();
    assert_eq!(standings, [(String::from("deploy"), Standing::Edited)]);
    assert!(
        fs::read_to_string(copies(&projects[4], "deploy")[1].join("SKILL.md"))
            .unwrap()
            .contains("Mine")
    );
}

#[test]
fn a_copy_installed_from_somewhere_else_is_left_out_of_it() {
    let (dir, mut workspace, projects) = given(&["alpha", "beta"]);
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    let (loaded, _) = FlayerWorkspace::init(&team).unwrap();
    create(&loaded.own(), Kind::Skill, "deploy", "Their way").unwrap();
    workspace.load(&loaded).unwrap();
    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    install_from(&workspace, &projects[..1], "workspace");
    install_from(&workspace, &projects[1..], "load:team");

    let plan = everywhere::plan(&workspace, &Reference::parse("deploy")).unwrap();

    assert_eq!(
        said(&plan),
        [(String::from("alpha"), Standing::Current, Step::Uninstall)]
    );
}

fn install_from(workspace: &FlayerWorkspace, projects: &[MindProject], from: &str) {
    let parts = assign::install(
        workspace,
        projects,
        &Reference::parse("deploy"),
        Some(from),
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap()
    .parts;
    for part in parts {
        assert!(
            part.done
                .as_ref()
                .is_ok_and(|offered| offered.installed.is_ok()),
            "{:?}",
            part.done
        );
    }
}

#[test]
fn a_rule_goes_from_the_block_of_every_project_that_received_it() {
    let (_dir, workspace, projects) = given(&["alpha"]);
    create(
        &workspace.own(),
        Kind::Rule,
        "git/no-force-push",
        "Never force-push",
    )
    .unwrap();
    install(&workspace, &projects, "rule:git/no-force-push");
    let block = || fs::read_to_string(projects[0].root().join("AGENTS.md")).unwrap_or_default();
    assert!(block().contains("git/no-force-push"), "{}", block());

    let applied = everywhere::apply(
        &workspace,
        &Reference::parse("rule:git/no-force-push"),
        &AgentHomes::default(),
    )
    .unwrap();

    applied.removed.unwrap();
    assert!(matches!(
        applied.plan.copies[0].done,
        Some(Ok(Removed::Removed { .. }))
    ));
    assert!(!block().contains("git/no-force-push"), "{}", block());
    assert!(!workspace
        .own()
        .directory_for(Kind::Rule)
        .unwrap()
        .join("git/no-force-push.md")
        .exists());
}

#[test]
fn a_project_artifact_is_not_found_among_the_workspaces_own() {
    let (dir, mut workspace, _) = given(&[]);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    let skills = Directories::default().with(Kind::Skill, "skills");
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &skills).unwrap();
    workspace.link(&project).unwrap();
    create(&project, Kind::Skill, "notes", "Mine").unwrap();

    let refused = everywhere::plan(&workspace, &Reference::parse("notes")).unwrap_err();

    assert!(
        refused.to_string().contains("nothing named `notes`"),
        "{refused}"
    );
    assert!(root.join("skills/notes/SKILL.md").is_file());
}
