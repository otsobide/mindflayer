//! One artifact installed into several projects at once, or taken back out
//! of them (#49): each project's part what one install there does, a project
//! named twice reached once, and one that refuses said of it while the rest
//! go ahead. And what nothing offers any more taken out of them (#50).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::skill;
use mindflayer_core::assign::{self, Install, Offered, Part};
use mindflayer_core::install::{self, Edits, Installed, Removed};
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

/// What `uninstall_orphaned` took out of each project, and what it left,
/// by kind and name.
fn orphans_taken(parts: &[Part<assign::Orphans>]) -> Vec<(String, Vec<(String, &'static str)>)> {
    parts
        .iter()
        .map(|part| {
            let orphans = part.done.as_ref().unwrap();
            let said = orphans
                .iter()
                .map(|orphan| {
                    let what = match &orphan.removed {
                        Ok(Removed::Removed { .. }) => "removed",
                        Ok(Removed::Edited { .. }) => "edited",
                        other => panic!("{other:?}"),
                    };
                    (format!("{}:{}", orphan.kind.slug(), orphan.name), what)
                })
                .collect();
            (part.project.name().to_owned(), said)
        })
        .collect()
}

fn orphans_out(
    workspace: &FlayerWorkspace,
    projects: &[MindProject],
    edits: Edits,
) -> Vec<Part<assign::Orphans>> {
    assign::uninstall_orphaned(workspace, projects, &AgentHomes::default(), edits)
        .unwrap()
        .parts
}

#[test]
fn what_nothing_offers_any_more_is_taken_out_and_the_rest_stays() {
    let (_dir, workspace, projects) = given(&["alpha", "beta"]);
    create(&workspace.own(), Kind::Skill, "ship", "Ship it too").unwrap();
    installing(&workspace, &projects, "deploy");
    installing(&workspace, &projects, "ship");
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::remove_dir_all(library.join("deploy")).unwrap();

    let parts = orphans_out(&workspace, &projects, Edits::Kept);

    let removed = vec![(String::from("skill:deploy"), "removed")];
    assert_eq!(
        orphans_taken(&parts),
        [
            (String::from("alpha"), removed.clone()),
            (String::from("beta"), removed)
        ]
    );
    for project in &projects {
        for copy in copies(project, "deploy") {
            assert!(!copy.exists(), "{}", copy.display());
        }
        for copy in copies(project, "ship") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    // Run again, there is nothing left to take out.
    let again = orphans_out(&workspace, &projects, Edits::Kept);
    assert!(orphans_taken(&again)
        .iter()
        .all(|(_, said)| said.is_empty()));
}

#[test]
fn an_orphan_changed_since_it_was_installed_is_left_until_it_is_forced() {
    let (_dir, workspace, projects) = given(&["alpha"]);
    installing(&workspace, &projects, "deploy");
    let edited = copies(&projects[0], "deploy")[1].join("SKILL.md");
    fs::write(&edited, skill("deploy", "My way")).unwrap();
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::remove_dir_all(library.join("deploy")).unwrap();

    let kept = orphans_out(&workspace, &projects, Edits::Kept);
    assert_eq!(
        orphans_taken(&kept),
        [(
            String::from("alpha"),
            vec![(String::from("skill:deploy"), "edited")]
        )]
    );
    assert!(edited.is_file());

    let forced = orphans_out(&workspace, &projects, Edits::Forced);
    assert_eq!(
        orphans_taken(&forced),
        [(
            String::from("alpha"),
            vec![(String::from("skill:deploy"), "removed")]
        )]
    );
    assert!(!edited.exists());
}

#[test]
fn a_copy_from_a_loaded_workspace_that_cannot_be_opened_is_not_an_orphan() {
    let (dir, mut workspace, projects) = given(&["alpha"]);
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    let (loaded, _) = FlayerWorkspace::init(&team).unwrap();
    create(&loaded.own(), Kind::Skill, "review", "Review it").unwrap();
    workspace.load(&loaded).unwrap();
    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    installing(&workspace, &projects, "review");
    // Away, as a drive unmounted is: what it offers cannot be told.
    fs::rename(&team, dir.path().join("away")).unwrap();

    let parts = orphans_out(&workspace, &projects, Edits::Kept);

    assert_eq!(orphans_taken(&parts), [(String::from("alpha"), vec![])]);
    for copy in copies(&projects[0], "review") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn a_copy_of_one_of_the_workspaces_own_that_cannot_be_read_is_not_an_orphan() {
    let (_dir, workspace, projects) = given(&["alpha"]);
    installing(&workspace, &projects, "deploy");
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    // Still there, and somebody's to mend: it offers nothing while its front
    // matter does not parse.
    let manifest = library.join("deploy/SKILL.md");
    fs::write(
        &manifest,
        "---\nname: deploy\ndescription: [unclosed\n---\n",
    )
    .unwrap();
    assert!(!install::own_failures(&workspace, &[Kind::Skill]).is_empty());

    let parts = orphans_out(&workspace, &projects, Edits::Forced);

    assert_eq!(orphans_taken(&parts), [(String::from("alpha"), vec![])]);
    for copy in copies(&projects[0], "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn a_copy_from_a_loaded_workspace_whose_artifact_cannot_be_read_is_not_an_orphan() {
    let (dir, mut workspace, projects) = given(&["alpha"]);
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    let (loaded, _) = FlayerWorkspace::init(&team).unwrap();
    create(&loaded.own(), Kind::Skill, "review", "Review it").unwrap();
    workspace.load(&loaded).unwrap();
    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    installing(&workspace, &projects, "review");
    let manifest = loaded
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("review/SKILL.md");
    fs::write(
        &manifest,
        "---\nname: review\ndescription: [unclosed\n---\n",
    )
    .unwrap();

    let parts = orphans_out(&workspace, &projects, Edits::Forced);

    assert_eq!(orphans_taken(&parts), [(String::from("alpha"), vec![])]);
    for copy in copies(&projects[0], "review") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn looking_for_orphans_where_nothing_was_ever_installed_makes_no_ledger() {
    let (_dir, workspace, projects) = given(&["alpha"]);

    let parts = orphans_out(&workspace, &projects, Edits::Kept);

    assert_eq!(orphans_taken(&parts), [(String::from("alpha"), vec![])]);
    assert!(!workspace.ledger_path().exists());
}
