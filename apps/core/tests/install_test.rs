//! Putting a gathered skill into a mind project, and taking it back out.

mod common;

use std::fs;
use std::path::Path;

use common::{repository, skill, url};
use mindflayer_core::gather::{self, Request};
use mindflayer_core::install::{self, Installed, Removed, Standing};
use mindflayer_core::ledger::Ledger;
use mindflayer_core::{Directories, FlayerWorkspace, Kind, MindProject};
use tempfile::TempDir;

/// A workspace with one project in it, and a shelf holding `names`.
fn ready(names: &[&str]) -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();

    let files: Vec<(String, String)> = names
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                skill(name, &format!("What {name} does")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, body)| (path.as_str(), body.as_str()))
        .collect();
    let source = repository(&borrowed);

    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    // The clone lives in the shelf, so the temporary can go.
    drop(source);
    (dir, workspace, project, ledger)
}

fn candidate<'a>(candidates: &'a [install::Candidate], name: &str) -> &'a install::Candidate {
    candidates
        .iter()
        .find(|candidate| candidate.name() == name)
        .unwrap_or_else(|| panic!("no candidate named {name}"))
}

fn installed_at(project: &MindProject, name: &str) -> std::path::PathBuf {
    project.directory_for(Kind::Skill).join(name)
}

#[test]
fn a_disowned_copy_is_left_to_whoever_has_it_now() {
    let (dir, _workspace, project, ledger) = ready(&["deploy"]);
    let mut workspace = FlayerWorkspace::open(dir.path()).unwrap();
    workspace.link(&project).unwrap();
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        candidate(&candidates, "deploy"),
    )
    .unwrap();

    assert!(install::disown(&project, Kind::Skill, "deploy").unwrap());

    // Still there, and now it is somebody's: shown, never overwritten or
    // deleted by unticking it.
    let after = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(candidate(&after, "deploy").standing, Standing::Foreign);
    assert_eq!(
        install::uninstall(&workspace, &ledger, &project, Kind::Skill, "deploy").unwrap(),
        Removed::Foreign {
            name: String::from("deploy")
        }
    );
    assert!(installed_at(&project, "deploy").is_dir());
    assert!(
        !install::disown(&project, Kind::Skill, "deploy").unwrap(),
        "nothing left to drop"
    );
}

#[test]
fn disowning_asks_only_a_workspace_that_manages_the_project_and_creates_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();

    assert!(!install::disown(&project, Kind::Skill, "deploy").unwrap());
    workspace.link(&project).unwrap();
    assert!(!install::disown(&project, Kind::Skill, "deploy").unwrap());

    // A workspace that never installed anything has no ledger, and looking
    // must not give it one.
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn the_shelf_is_what_a_project_can_be_offered() {
    let (_dir, workspace, project, ledger) = ready(&["deploy", "commit-style"]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.standing == Standing::Absent));
}

#[test]
fn installing_copies_the_skill_into_the_directory_the_project_named() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    // The project keeps its skills where its agents look, not where we assume.
    let (project, _) = MindProject::init_with(
        &root,
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    )
    .unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    assert!(matches!(outcome, Installed::Added { .. }), "{outcome:?}");
    assert!(root.join(".claude/skills/deploy/SKILL.md").is_file());
}

#[test]
fn a_skills_whole_directory_arrives_with_it() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/scripts/run.sh", "echo hi\n"),
    ]);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    assert!(installed_at(&project, "deploy")
        .join("scripts")
        .join("run.sh")
        .is_file());
}

#[test]
fn a_second_survey_knows_what_is_already_there() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(again[0].standing, Standing::Installed);
    assert!(again[0].standing.present());
    assert!(again[0].standing.ours());
}

#[test]
fn installing_the_same_skill_twice_changes_nothing() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(&workspace, &ledger, &project, &again[0]).unwrap();

    assert!(
        matches!(outcome, Installed::Unchanged { .. }),
        "{outcome:?}"
    );
}

#[test]
fn uninstalling_takes_it_back_out() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    let outcome = install::uninstall(&workspace, &ledger, &project, Kind::Skill, "deploy").unwrap();

    assert!(matches!(outcome, Removed::Removed { .. }), "{outcome:?}");
    assert!(!installed_at(&project, "deploy").exists());
    let after = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(after[0].standing, Standing::Absent);
}

// ---------------------------------------------------------------------------
// What was not installed by Mindflayer is not Mindflayer's to touch
// ---------------------------------------------------------------------------

/// A skill somebody wrote by hand, of a name the shelf also offers.
fn write_by_hand(project: &MindProject, name: &str, description: &str) {
    let dir = installed_at(project, name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), skill(name, description)).unwrap();
}

#[test]
fn a_hand_written_skill_is_seen_as_present_but_not_ours() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates[0].standing, Standing::Foreign);
    assert!(candidates[0].standing.present(), "the box is ticked");
    assert!(!candidates[0].standing.ours(), "but it is not ours to move");
}

#[test]
fn installing_does_not_overwrite_a_hand_written_skill() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    assert!(matches!(outcome, Installed::Foreign { .. }), "{outcome:?}");
    let kept = fs::read_to_string(installed_at(&project, "deploy").join("SKILL.md")).unwrap();
    assert!(kept.contains("Written by a person"), "{kept}");
}

#[test]
fn uninstalling_does_not_delete_a_hand_written_skill() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let outcome = install::uninstall(&workspace, &ledger, &project, Kind::Skill, "deploy").unwrap();

    assert!(matches!(outcome, Removed::Foreign { .. }), "{outcome:?}");
    assert!(
        installed_at(&project, "deploy").join("SKILL.md").is_file(),
        "somebody's work was deleted"
    );
}

#[test]
fn uninstalling_something_that_is_not_there_says_so() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);

    let outcome = install::uninstall(&workspace, &ledger, &project, Kind::Skill, "deploy").unwrap();

    assert!(matches!(outcome, Removed::Absent { .. }), "{outcome:?}");
}

// ---------------------------------------------------------------------------
// Several projects, and the log
// ---------------------------------------------------------------------------

#[test]
fn two_projects_each_keep_their_own_answer() {
    let (dir, workspace, one, ledger) = ready(&["deploy"]);
    let other_root = dir.path().join("tanukeys");
    fs::create_dir(&other_root).unwrap();
    let (two, _) = MindProject::init(&other_root).unwrap();

    let candidates = install::survey(&workspace, &ledger, &one, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &one, &candidates[0]).unwrap();

    assert_eq!(
        install::survey(&workspace, &ledger, &one, Kind::Skill).unwrap()[0].standing,
        Standing::Installed
    );
    assert_eq!(
        install::survey(&workspace, &ledger, &two, Kind::Skill).unwrap()[0].standing,
        Standing::Absent,
        "installing into one project must not claim the other"
    );
}

#[test]
fn the_log_records_both_directions() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();
    install::uninstall(&workspace, &ledger, &project, Kind::Skill, "deploy").unwrap();

    let history = ledger.history(10).unwrap();
    let actions: Vec<&str> = history.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(actions[0], "uninstall");
    assert_eq!(actions[1], "install");
    assert!(history[0].detail.as_deref().unwrap().contains("deploy"));
}

#[test]
fn a_shelf_entry_whose_files_have_gone_is_named_in_the_error() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let install::Offer::Shelf(gathered) = &candidates[0].offer else {
        panic!("a gathered skill is offered from the shelf");
    };
    fs::remove_dir_all(workspace.root().join(&gathered.path)).unwrap();

    let error = install::install(&workspace, &ledger, &project, &candidates[0]).unwrap_err();

    assert!(error.to_string().contains("deploy"), "{error}");
}

#[test]
fn the_folder_is_named_after_the_skill_rather_than_after_its_shelf_folder() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        candidate(&candidates, "deploy"),
    )
    .unwrap();

    // Inside a project a skill's directory has to match its declared name:
    // that is what `validate` checks and what an agent uses to find it.
    let installed = installed_at(&project, "deploy");
    assert!(installed.is_dir());
    assert!(Path::new(&installed).join("SKILL.md").is_file());
}

// ---------------------------------------------------------------------------
// The workspace's own skills, offered beside the shelf
// ---------------------------------------------------------------------------

/// `ready`, with the project linked and a skill of the workspace's own.
fn with_own(shelf: &[&str], own: &[&str]) -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let (dir, _, project, ledger) = ready(shelf);
    let mut workspace = FlayerWorkspace::open(dir.path()).unwrap();
    workspace.link(&project).unwrap();
    for name in own {
        mindflayer_core::create(&workspace.own(), Kind::Skill, name, &format!("Our {name}"))
            .unwrap();
    }
    (dir, workspace, project, ledger)
}

#[test]
fn the_workspace_offers_its_own_skills_first_and_installs_them_like_any_other() {
    let (_dir, workspace, project, ledger) = with_own(&["deploy"], &["commit-style"]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    let offered: Vec<(&str, String)> = candidates
        .iter()
        .map(|candidate| (candidate.name(), candidate.origin()))
        .collect();
    assert_eq!(offered[0], ("commit-style", String::from("workspace")));
    assert_eq!(offered.len(), 2);

    install::install(&workspace, &ledger, &project, &candidates[0]).unwrap();

    assert!(installed_at(&project, "commit-style")
        .join("SKILL.md")
        .is_file());
    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(again[0].standing, Standing::Installed);
}

#[test]
fn one_name_offered_twice_is_installed_only_from_where_it_is_asked_for() {
    let (_dir, workspace, project, ledger) = with_own(&["deploy"], &["deploy"]);

    let twice =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap_err();
    let ours = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        Some("workspace"),
    )
    .unwrap();
    let nothing =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "absent", None).unwrap_err();

    assert!(
        matches!(twice, install::InstallError::OfferedTwice { .. }),
        "{twice}"
    );
    assert!(twice.to_string().contains("--from"), "{twice}");
    assert_eq!(ours.origin(), "workspace");
    assert!(
        matches!(nothing, install::InstallError::NotOffered { .. }),
        "{nothing}"
    );
}
