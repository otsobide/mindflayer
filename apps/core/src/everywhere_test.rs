//! Unit tests for `everywhere.rs`, kept beside it rather than inside it.
//!
//! The integration suite removes the workspace's own from projects; these
//! hold what a plan says of the copies it found, and that a removal that
//! cannot be planned says the condition of the error it carries.

use super::*;

use tempfile::TempDir;

use crate::agent::Agent;
use crate::kind::Kind;
use crate::mother::random;
use crate::mother::{ProjectMother, SkillMother, WorkspaceMother};
use crate::settings::Share;
use crate::value::ArtifactName;

fn copy(project: &MindProject, standing: Standing, step: Step) -> ProjectCopy {
    ProjectCopy {
        project: project.clone(),
        standing,
        step,
        done: None,
    }
}

fn fs_write(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn target() -> Target {
    Target {
        kind: Kind::Skill,
        name: ArtifactName::from("deploy"),
        path: PathBuf::from("/w/.mindflayer/skills/deploy"),
        artifact: None,
    }
}

/// Somebody's own of the name is none of Mindflayer's, and removing the
/// library copy leaves nothing of it orphaned; every copy installed from
/// it, edited or not, is.
#[test]
fn the_copies_left_orphaned_are_every_one_installed_from_it_and_never_somebodys() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &[], Share::Shared);
    for _ in 0..50 {
        let steps: Vec<Step> = (0..random::between(0, 6))
            .map(|n| match random::between(0, 2) {
                0 => Step::Uninstall,
                1 => Step::Leave(vec![PathBuf::from(format!("/p/{n}"))]),
                _ => Step::Somebodys(PathBuf::from(format!("/p/{n}"))),
            })
            .collect();
        let plan = Plan {
            target: target(),
            copies: steps
                .iter()
                .map(|step| copy(&project, Standing::Current, step.clone()))
                .collect(),
            unread: Vec::new(),
        };

        let orphaned: Vec<&Step> = plan.orphaned().iter().map(|copy| &copy.step).collect();

        let expected: Vec<&Step> = steps
            .iter()
            .filter(|step| !matches!(step, Step::Somebodys(_)))
            .collect();
        assert_eq!(orphaned, expected, "{steps:?}");
    }
}

#[test]
fn a_plan_of_no_copy_leaves_nothing_orphaned() {
    let plan = Plan {
        target: target(),
        copies: Vec::new(),
        unread: Vec::new(),
    };

    assert!(plan.orphaned().is_empty());
}

/// Every condition is the one it carries, so it has no code of its own and
/// the panel tells a missing artifact by `not_found` whatever met it.
#[test]
fn removing_everywhere_has_no_code_of_its_own_and_says_what_it_carries() {
    assert!(EverywhereError::CODES.is_empty());

    let lifecycle = EverywhereError::from(LifecycleError::NotFound {
        typed: String::from("deploy"),
    });
    let ledger = EverywhereError::from(LedgerError::Unmade {
        path: PathBuf::from("/w/.mindflayer/mindflayer.db"),
    });

    assert_eq!(lifecycle.code(), "not_found");
    assert_eq!(lifecycle.to_string(), "nothing named `deploy` here");
    assert_eq!(ledger.code(), "ledger_unmade");
}

#[test]
fn a_name_the_workspace_s_own_do_not_hold_cannot_be_planned() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let refused = plan(&workspace, &Reference::parse("deploy")).unwrap_err();

    assert_eq!(refused.code(), "not_found");
}

/// A plan is a read: the ledger is not made for it, and nothing it found
/// is taken out.
#[test]
fn planning_with_no_project_linked_finds_no_copy_and_makes_no_ledger() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    let folder = library.join("deploy");
    fs_write(
        &folder.join("SKILL.md"),
        &SkillMother::create("deploy", "Ship it"),
    );

    let planned = plan(&workspace, &Reference::parse("skill:deploy")).unwrap();

    assert!(planned.copies.is_empty(), "{:?}", planned.copies);
    assert!(planned.unread.is_empty());
    assert_eq!(planned.target.path, folder);
    assert!(folder.join("SKILL.md").is_file());
    assert!(!workspace.ledger_path().exists());
}

/// A project nothing was installed into holds nothing of the name to say,
/// and one somebody keeps a copy of the name in by hand is said, as theirs.
#[test]
fn somebody_s_own_of_the_name_in_a_linked_project_is_planned_to_be_left() {
    let (dir, mut workspace) = WorkspaceMother::empty();
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    fs_write(
        &library.join("deploy").join("SKILL.md"),
        &SkillMother::create("deploy", "Ship it"),
    );
    let project = ProjectMother::create(&dir.path().join("app"), &[Agent::Claude], Share::Shared);
    workspace.link(&project).unwrap();
    let theirs = project.root().join(".claude").join("skills").join("deploy");
    fs_write(
        &theirs.join("SKILL.md"),
        &SkillMother::create("deploy", "Theirs"),
    );
    let quiet = ProjectMother::create(&dir.path().join("quiet"), &[Agent::Claude], Share::Shared);
    workspace.link(&quiet).unwrap();

    let planned = plan(&workspace, &Reference::parse("deploy")).unwrap();

    assert_eq!(planned.copies.len(), 1, "{:?}", planned.copies);
    assert_eq!(planned.copies[0].project.root(), project.root());
    assert_eq!(planned.copies[0].standing, Standing::Foreign);
    assert_eq!(planned.copies[0].step, Step::Somebodys(theirs));
    assert!(planned.orphaned().is_empty());
}
