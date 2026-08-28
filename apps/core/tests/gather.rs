//! Gathering skills out of a real git repository, against a real workspace.

mod common;

use std::fs;

use common::{commit, repository, skill, url, workspace, write_files};
use mindflayer_core::gather::{self, GatherError, Request};
use mindflayer_core::ledger::{Action, Ledger, SourceKind};
use mindflayer_core::{FlayerWorkspace, Kind, MindProject};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn gathering_puts_a_repositorys_skills_on_the_workspace_shelf() {
    let source = repository(&[
        (
            "skills/commit-style/SKILL.md",
            &skill("commit-style", "How we commit"),
        ),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("README.md", "not a skill"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 2, "{:?}", report.added);
    assert!(report.updated.is_empty() && report.unchanged.is_empty());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.revision.is_some(), "a clone resolves a commit");

    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert!(shelf.join("commit-style").join("SKILL.md").is_file());
    assert!(shelf.join("deploy").join("SKILL.md").is_file());
    // Only the skills folder was harvested.
    assert!(!shelf.join("README.md").exists());
}

#[test]
fn a_skills_whole_directory_travels_with_it() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/scripts/run.sh", "echo hi\n"),
        ("skills/deploy/reference.md", "detail\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let deploy = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join("deploy");
    assert!(deploy.join("scripts").join("run.sh").is_file());
    assert!(deploy.join("reference.md").is_file());
}

#[test]
fn gathering_the_same_repository_again_changes_nothing() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));

    gather::gather(&workspace, &ledger, &request).unwrap();
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(again.unchanged.len(), 1);
    assert!(again.added.is_empty() && again.updated.is_empty());
    // One shelf entry, not two: the second gather is the same source.
    assert_eq!(ledger.gathered().unwrap().len(), 1);
}

#[test]
fn a_skill_that_changed_upstream_is_reported_as_updated() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    gather::gather(&workspace, &ledger, &request).unwrap();

    write_files(
        source.path(),
        &[(
            "skills/deploy/SKILL.md",
            &skill("deploy", "Ship it, carefully"),
        )],
    );
    commit(source.path());

    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(again.updated.len(), 1, "{again:?}");
    assert_eq!(
        ledger.gathered().unwrap()[0].summary.as_deref(),
        Some("Ship it, carefully")
    );
}

#[test]
fn a_file_the_source_deleted_does_not_survive_on_the_shelf() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/old.md", "leftover\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    gather::gather(&workspace, &ledger, &request).unwrap();

    fs::remove_file(source.path().join("skills/deploy/old.md")).unwrap();
    commit(source.path());
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    let deploy = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join("deploy");
    assert!(deploy.join("SKILL.md").is_file());
    assert!(!deploy.join("old.md").exists());
}

#[test]
fn two_repositories_offering_one_name_both_survive() {
    let first = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "From the first"))]);
    let second = repository(&[(
        "skills/deploy/SKILL.md",
        &skill("deploy", "From the second"),
    )]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let one = gather::gather(&workspace, &ledger, &Request::git(url(&first))).unwrap();
    let two = gather::gather(&workspace, &ledger, &Request::git(url(&second))).unwrap();

    assert_ne!(
        one.directory, two.directory,
        "each source gets its own shelf"
    );
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert!(gathered.iter().all(|entry| entry.name == "deploy"));
    let summaries: Vec<Option<&str>> = gathered.iter().map(|g| g.summary.as_deref()).collect();
    assert!(summaries.contains(&Some("From the first")));
    assert!(summaries.contains(&Some("From the second")));
}

#[test]
fn one_unreadable_skill_does_not_cost_the_ones_beside_it() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter at all\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 1);
    assert_eq!(report.added[0].name, "good");
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].to_string().contains("front matter"));
}

#[test]
fn a_directory_without_a_manifest_is_not_a_skill() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/shared/logo.png", "not a skill\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 1);
    assert!(report.failures.is_empty());
}

#[test]
fn another_folder_can_be_named_instead_of_skills() {
    let source = repository(&[("agents/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git(url(&source)).from_subdirectory("agents");
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(report.added.len(), 1);
}

#[test]
fn a_folder_the_source_does_not_have_is_named_in_the_error() {
    let source = repository(&[("README.md", "nothing to gather\n")]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap_err();

    assert!(matches!(error, GatherError::NoSuchFolder { .. }), "{error}");
    assert!(error.to_string().contains("skills"));
}

#[test]
fn rules_cannot_be_gathered_yet_and_say_so() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let mut request = Request::git(url(&source));
    request.kind = Kind::Rule;
    let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(error, GatherError::NotGatherable { .. }),
        "{error}"
    );
}

#[test]
fn the_log_records_a_gather_and_the_source_it_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let address = url(&source);

    gather::gather(&workspace, &ledger, &Request::git(address.clone())).unwrap();

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, Action::Gather.slug());
    assert_eq!(history[0].target.as_deref(), Some(address.as_str()));
    assert_eq!(history[0].outcome, "ok");

    let recorded = ledger
        .source(SourceKind::Git, &address, None, "skills")
        .unwrap()
        .expect("the source was registered");
    assert!(recorded.revision.is_some());
}

#[test]
fn a_gather_that_fails_is_logged_as_a_failure() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git("/does/not/exist/anywhere");
    assert!(gather::gather(&workspace, &ledger, &request).is_err());

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].outcome, "failed");
    assert!(history[0].detail.is_some());
}

#[test]
fn gathering_writes_nothing_into_a_mind_project() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    // A workspace that is also a project, which is the arrangement where a
    // gather leaking into `.mind` would be easiest to miss.
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    let ledger = workspace.ledger().unwrap();

    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let skills = project.directory_for(Kind::Skill);
    assert!(skills.is_dir(), "init made it");
    assert_eq!(
        fs::read_dir(&skills).unwrap().count(),
        0,
        "gathering fills the workspace shelf and stops there"
    );
}

#[test]
fn the_clone_is_kept_so_a_gather_can_be_looked_at_afterwards() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let clone = workspace.cache_dir().join(&report.directory);
    assert!(clone
        .join("skills")
        .join("deploy")
        .join("SKILL.md")
        .is_file());
}

#[test]
fn the_ledger_lands_beside_the_marker_file() {
    let (dir, workspace) = workspace();
    let ledger = Ledger::open(workspace.ledger_path()).unwrap();
    drop(ledger);

    assert!(dir
        .path()
        .join(".mindflayer")
        .join("mindflayer.db")
        .is_file());
}
