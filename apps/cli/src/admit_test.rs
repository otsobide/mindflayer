//! Unit tests for `admit.rs`: what of a task from outside reaches a line, and
//! in what words.

use std::fs;
use std::path::Path;

use mindflayer_core::{Kind, MindProject};
use tempfile::TempDir;

use super::*;
use crate::task::{shell, Mark, MindsChange};

/// A workspace at `work` linking `alpha` under it and `beside` next to it,
/// and loading `library`, next to it too: nothing above any of them, so
/// `mind` run in `beside` consults no workspace at all.
fn fixture() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("work");
    let (mut workspace, _) = FlayerWorkspace::init(&root).unwrap();
    for project in [root.join("alpha"), dir.path().join("beside")] {
        let (project, _) = MindProject::init(project).unwrap();
        workspace.link(&project).unwrap();
    }
    let (library, _) = FlayerWorkspace::init(dir.path().join("library")).unwrap();
    workspace.load(&library).unwrap();
    (dir, FlayerWorkspace::open(&root).unwrap())
}

/// `task` admitted against `workspace`, looked for from nowhere: no home,
/// nothing named or activated.
fn admitted(workspace: &FlayerWorkspace, task: &Task) -> Result<Task, PolicyError> {
    Admission {
        workspace,
        lookup: &Lookup::default(),
    }
    .task(task)
}

/// The lines `task`, once admitted, is, as a shell shows them.
fn lines(workspace: &FlayerWorkspace, task: &Task) -> Vec<String> {
    admitted(workspace, task)
        .unwrap()
        .commands()
        .iter()
        .map(|line| shell(line))
        .collect()
}

fn code(result: Result<Task, PolicyError>) -> &'static str {
    result.expect_err("it was admitted").code()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn add(holder: Holder) -> Task {
    Task::Add {
        holder,
        kind: Kind::Skill,
        name: String::from("deploy"),
        description: String::from("Ship it"),
        template: None,
    }
}

#[test]
fn a_project_held_by_its_entry_is_named_to_the_command_by_its_folder() {
    let (_dir, workspace) = fixture();
    let alpha = workspace.root().join("alpha");

    let admitted = admitted(&workspace, &add(Holder::Project(String::from("alpha")))).unwrap();

    assert_eq!(admitted, add(Holder::Project(text(&alpha))));
    assert_eq!(
        lines(&workspace, &add(Holder::Project(String::from("alpha")))),
        [format!(
            "mind -C {} add skill deploy 'Ship it'",
            text(&alpha)
        )]
    );
}

/// The browser's word for which workspace `mind` consults is not taken: the
/// backend asks, as the TUI does for its rows.
#[test]
fn whether_a_line_says_which_workspace_is_decided_here_not_sent() {
    let (dir, workspace) = fixture();
    let alpha = text(&workspace.root().join("alpha"));
    let beside = text(&dir.path().join("beside"));

    for sent in [
        Holder::Project(String::from("alpha")),
        Holder::Elsewhere(String::from("alpha")),
    ] {
        assert_eq!(
            admitted(&workspace, &add(sent)).unwrap(),
            add(Holder::Project(alpha.clone()))
        );
    }
    for sent in [
        Holder::Project(String::from("../beside")),
        Holder::Elsewhere(String::from("../beside")),
    ] {
        assert_eq!(
            admitted(&workspace, &add(sent)).unwrap(),
            add(Holder::Elsewhere(beside.clone()))
        );
    }
    assert_eq!(
        lines(&workspace, &add(Holder::Project(String::from("../beside")))),
        [format!("mind -W . -C {beside} add skill deploy 'Ship it'")]
    );
}

#[test]
fn the_workspaces_own_stay_the_workspaces() {
    let (_dir, workspace) = fixture();

    assert_eq!(
        admitted(&workspace, &add(Holder::Workspace)).unwrap(),
        add(Holder::Workspace)
    );
}

#[test]
fn a_holder_the_registry_does_not_hold_is_refused() {
    let (dir, workspace) = fixture();
    let absolute = text(&workspace.root().join("alpha"));
    let outside = text(&dir.path().join("beside"));

    for entry in [
        "beta",
        "-C",
        "--home=/etc",
        "./alpha",
        "../alpha",
        "alpha/..",
        "/etc",
        &absolute,
        &outside,
    ] {
        for holder in [
            Holder::Project(entry.to_owned()),
            Holder::Elsewhere(entry.to_owned()),
        ] {
            assert_eq!(code(admitted(&workspace, &add(holder))), "not_registered");
        }
    }
}

#[test]
fn each_project_an_install_names_is_named_by_its_folder() {
    let (dir, workspace) = fixture();
    let install = Task::Install {
        projects: vec![String::from("alpha"), String::from("../beside")],
        kind: Kind::Skill,
        name: String::from("deploy"),
        from: Some(String::from("--yes")),
    };

    assert_eq!(
        lines(&workspace, &install),
        [format!(
            "flayer install -p {} -p {} --from=--yes skill:deploy",
            text(&workspace.root().join("alpha")),
            text(&dir.path().join("beside"))
        )]
    );
    let unknown = Task::Adopt {
        project: String::from("gamma"),
        kind: Kind::Skill,
        name: String::from("deploy"),
    };
    assert_eq!(code(admitted(&workspace, &unknown)), "not_registered");
}

#[test]
fn an_entry_a_command_takes_goes_to_it_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();
    let new = text(&dir.path().join("moved"));

    assert_eq!(
        lines(
            &workspace,
            &Task::Unlink {
                entry: String::from("../beside")
            }
        ),
        ["flayer unlink ../beside"]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Relink {
                old: String::from("alpha"),
                new: new.clone(),
            }
        ),
        [format!("flayer relink alpha {new}")]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Unload {
                entry: String::from("../library")
            }
        ),
        ["flayer unload ../library"]
    );
    assert_eq!(
        code(admitted(
            &workspace,
            &Task::Unlink {
                entry: String::from("-rf")
            }
        )),
        "not_registered"
    );
    assert_eq!(
        code(admitted(
            &workspace,
            &Task::Unload {
                entry: String::from("../beside")
            }
        )),
        "not_loaded"
    );
}

#[test]
fn a_folder_not_linked_yet_goes_to_the_command_as_given() {
    let (dir, workspace) = fixture();
    let fresh = dir.path().join("fresh");
    fs::create_dir(&fresh).unwrap();
    let fresh = text(&fresh);

    assert_eq!(
        lines(
            &workspace,
            &Task::InitAndLink {
                path: fresh.clone()
            }
        ),
        [
            format!("mind -C {fresh} init"),
            format!("flayer link {fresh}")
        ]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Load {
                path: fresh.clone()
            }
        ),
        [format!("flayer load {fresh}")]
    );
}

#[test]
fn a_folder_that_is_not_absolute_normalized_and_outside_any_workspace_folder_is_refused() {
    let (_dir, workspace) = fixture();
    let flayer = text(&workspace.flayer_dir());
    let climbing = format!("{}{}..", text(workspace.root()), std::path::MAIN_SEPARATOR);

    for (path, expected) in [
        ("beta", "path_relative"),
        ("-x", "path_relative"),
        ("file:///etc", "path_url"),
        (climbing.as_str(), "path_unnormalized"),
        (flayer.as_str(), "path_in_mindflayer"),
    ] {
        for task in [
            Task::Link {
                path: path.to_owned(),
            },
            Task::InitAndLink {
                path: path.to_owned(),
            },
            Task::Load {
                path: path.to_owned(),
            },
            Task::LoadHere {
                path: path.to_owned(),
            },
            Task::Relink {
                old: String::from("alpha"),
                new: path.to_owned(),
            },
            Task::Minds {
                init: vec![path.to_owned()],
                link: vec![path.to_owned()],
                unlink: Vec::new(),
            },
            Task::Minds {
                init: Vec::new(),
                link: vec![path.to_owned()],
                unlink: Vec::new(),
            },
            marks(&[(path, MindsChange::Link)]),
            marks(&[(path, MindsChange::InitAndLink)]),
        ] {
            assert_eq!(code(admitted(&workspace, &task)), expected, "{task:?}");
        }
    }
}

/// The panel's marks, each a path and what to do to it.
fn marks(marked: &[(&str, MindsChange)]) -> Task {
    Task::Marks {
        marks: marked
            .iter()
            .map(|(path, change)| Mark {
                path: (*path).to_owned(),
                change: *change,
            })
            .collect(),
    }
}

#[test]
fn marks_on_folders_go_to_the_minds_screen_s_lines_as_given() {
    let (dir, workspace) = fixture();
    let fresh = dir.path().join("fresh");
    fs::create_dir(&fresh).unwrap();
    let fresh = text(&fresh);
    let beside = text(&dir.path().join("beside"));
    let other = text(&dir.path().join("other"));

    assert_eq!(
        lines(
            &workspace,
            &marks(&[
                (&other, MindsChange::Link),
                (&fresh, MindsChange::InitAndLink),
                (&beside, MindsChange::Unlink),
            ])
        ),
        [
            format!("mind -C {fresh} init"),
            format!("flayer link {other} {fresh}"),
            format!("flayer unlink {beside}"),
        ]
    );
    let library = text(&dir.path().join("library"));
    assert_eq!(
        code(admitted(
            &workspace,
            &marks(&[(&library, MindsChange::Unlink)])
        )),
        "not_registered"
    );
}

#[test]
fn a_mark_to_unlink_or_unload_names_a_folder_an_entry_points_at() {
    let (dir, workspace) = fixture();
    let beside = text(&dir.path().join("beside"));
    let library = text(&dir.path().join("library"));

    let minds = Task::Minds {
        init: Vec::new(),
        link: Vec::new(),
        unlink: vec![beside.clone()],
    };
    assert_eq!(
        lines(&workspace, &minds),
        [format!("flayer unlink {beside}")]
    );
    let wrong = Task::Minds {
        init: Vec::new(),
        link: Vec::new(),
        unlink: vec![library.clone()],
    };
    assert_eq!(code(admitted(&workspace, &wrong)), "not_registered");
    assert_eq!(
        lines(
            &workspace,
            &Task::UnloadHere {
                path: library.clone()
            }
        ),
        [format!("mind unload {library}")]
    );
    assert_eq!(
        code(admitted(&workspace, &Task::UnloadHere { path: beside })),
        "not_loaded"
    );
}

#[test]
fn a_batch_is_held_to_it_change_by_change() {
    let (_dir, workspace) = fixture();
    let change = |project: &str| Task::Uninstall {
        projects: vec![project.to_owned()],
        kind: Kind::Skill,
        name: String::from("deploy"),
    };

    let batch = Task::ApplyInstall {
        changes: vec![change("alpha"), change("nobody")],
    };
    assert_eq!(code(admitted(&workspace, &batch)), "not_registered");
    let batch = Task::ApplyInstall {
        changes: vec![change("alpha")],
    };
    assert_eq!(
        admitted(&workspace, &batch).unwrap(),
        Task::ApplyInstall {
            changes: vec![change(&text(&workspace.root().join("alpha")))]
        }
    );
}

/// Free text is the command's to judge, and the rule that no value is read
/// as a flag already keeps it a value: the policy is about what a value
/// names on the disk.
#[test]
fn what_names_nothing_on_the_disk_is_left_as_it_was_sent() {
    let (_dir, workspace) = fixture();
    let task = Task::Add {
        holder: Holder::Workspace,
        kind: Kind::Rule,
        name: String::from("--yes"),
        description: String::from("- the fast way"),
        template: Some(String::from("-rf")),
    };

    assert_eq!(admitted(&workspace, &task).unwrap(), task);
}

#[test]
fn nothing_is_admitted_against_a_workspace_that_links_nothing() {
    let dir = TempDir::new().unwrap();
    let workspace = FlayerWorkspace::unmade(dir.path()).unwrap();

    assert_eq!(
        code(admitted(
            &workspace,
            &add(Holder::Project(String::from(".")))
        )),
        "not_registered"
    );
    assert!(!dir.path().join(mindflayer_core::FLAYER_DIR).exists());
}
