//! Unit tests for `assign.rs`, kept beside it rather than inside it.
//!
//! The integration suite installs into and uninstalls from several projects
//! at once; these hold what the batch itself adds: each project once,
//! however it is spelled, in the order asked, and a batch of none that
//! opens nothing.

use super::*;

use std::path::PathBuf;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, WorkspaceMother};
use crate::settings::Share;

fn projects(dir: &TempDir, count: usize) -> Vec<MindProject> {
    (0..count)
        .map(|n| ProjectMother::create(&dir.path().join(format!("p{n}")), &[], Share::Shared))
        .collect()
}

fn roots(projects: &[&MindProject]) -> Vec<PathBuf> {
    projects
        .iter()
        .map(|project| project.root().to_path_buf())
        .collect()
}

#[test]
fn each_project_asked_once_is_kept_in_the_order_asked() {
    let dir = TempDir::new().unwrap();
    let mut asked = projects(&dir, random::between(1, 5));
    asked.reverse();

    let once = each(&asked);

    assert_eq!(roots(&once), roots(&asked.iter().collect::<Vec<_>>()));
}

#[test]
fn a_project_asked_twice_is_kept_once_where_it_was_first_asked() {
    let dir = TempDir::new().unwrap();
    let made = projects(&dir, 3);
    let asked = vec![
        made[1].clone(),
        made[0].clone(),
        made[1].clone(),
        made[2].clone(),
        made[0].clone(),
    ];

    let once = each(&asked);

    assert_eq!(
        roots(&once),
        [
            made[1].root().to_path_buf(),
            made[0].root().to_path_buf(),
            made[2].root().to_path_buf(),
        ]
    );
}

/// A name and a path, or a link and what it leads to, are one directory:
/// installing into it twice would say twice what was done once.
#[cfg(unix)]
#[test]
fn a_project_reached_through_a_link_is_the_project_it_leads_to() {
    let dir = TempDir::new().unwrap();
    let made = projects(&dir, 1);
    let link = dir.path().join("linked");
    std::os::unix::fs::symlink(made[0].root(), &link).unwrap();
    let through = MindProject::open(&link).unwrap();

    let asked = [made[0].clone(), through];

    let once = each(&asked);

    assert_eq!(roots(&once), [made[0].root().to_path_buf()]);
}

#[test]
fn a_batch_of_no_project_did_nothing_and_scrubbed_nothing() {
    let done: Assigned<()> = nothing();

    assert!(done.parts.is_empty());
    assert!(done.scrub.is_empty());
}

#[test]
fn a_batch_of_no_project_opens_no_ledger() {
    let (_dir, workspace) = WorkspaceMother::empty();

    assert!(opened(&workspace, &[]).unwrap().is_none());
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn a_batch_of_some_project_opens_the_ledger_to_write_once() {
    let (dir, workspace) = WorkspaceMother::empty();
    let made = projects(&dir, random::between(1, 3));

    let (_ledger, scrub) = opened(&workspace, &made).unwrap().expect("opened");

    assert!(scrub.is_empty());
    assert!(workspace.ledger_path().is_file());
}

#[test]
fn installing_into_no_project_and_taking_out_of_none_make_no_ledger() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let reference = Reference::parse("deploy");
    let homes = AgentHomes::default();

    let installed = install(
        &workspace,
        &[],
        &reference,
        None,
        workspace.root(),
        &homes,
        Edits::Kept,
        Confirmed::Yes,
    )
    .unwrap();
    let uninstalled = uninstall(&workspace, &[], &reference, &homes, Edits::Kept).unwrap();
    let orphaned = uninstall_orphaned(&workspace, &[], &homes, Edits::Kept).unwrap();

    assert!(installed.parts.is_empty());
    assert!(uninstalled.parts.is_empty());
    assert!(orphaned.parts.is_empty());
    assert!(!workspace.ledger_path().exists());
}

/// With no ledger nothing was ever installed, so each project's part says
/// so, empty, and no ledger is made to say it.
#[test]
fn looking_for_orphans_with_no_ledger_gives_each_project_an_empty_part() {
    let (dir, workspace) = WorkspaceMother::empty();
    let made = projects(&dir, random::between(1, 4));

    let done = uninstall_orphaned(&workspace, &made, &AgentHomes::default(), Edits::Kept).unwrap();

    assert_eq!(done.parts.len(), made.len());
    for part in &done.parts {
        assert!(
            matches!(&part.done, Ok(orphans) if orphans.is_empty()),
            "{:?}",
            part.done
        );
    }
    assert!(!workspace.ledger_path().exists());
}
