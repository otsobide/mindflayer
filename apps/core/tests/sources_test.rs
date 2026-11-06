//! A gathered source's own changes against a real workspace and real
//! repositories: gathering some again by name, or every one.

mod common;

use std::fs;

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::gather::{self, GatherError, Request, Watch};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::sources::{self, Regathered, SourceError, Which};
use mindweaver_core::WeaverWorkspace;
use tempfile::TempDir;

/// Two repositories gathered into a workspace, `acme` offering `deploy` and
/// `review`, `other` offering `pdf-forms`, under those names.
fn two_gathered() -> (TempDir, WeaverWorkspace, Ledger, TempDir, TempDir) {
    let acme = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);
    let other = repository(&[(
        "skills/pdf-forms/SKILL.md",
        &skill("pdf-forms", "Fill forms"),
    )]);
    let (dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    for (repository, name) in [(&acme, "acme"), (&other, "other")] {
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(repository)).named(Some(name)),
        )
        .unwrap();
    }
    (dir, workspace, ledger, acme, other)
}

/// A source's name, and the names of what gathering it again added,
/// updated and removed.
type Moved = (String, Vec<String>, Vec<String>, Vec<String>);

/// What each gather again added, updated and removed, by the source's name.
fn moved(updated: &[Regathered]) -> Vec<Moved> {
    let names = |harvested: &[gather::Harvested]| -> Vec<String> {
        harvested.iter().map(|h| h.name.to_string()).collect()
    };
    updated
        .iter()
        .map(|regathered| {
            let report = regathered.outcome.as_ref().expect("gathered again");
            (
                regathered.source.name.to_string(),
                names(&report.added),
                names(&report.updated),
                names(&report.removed),
            )
        })
        .collect()
}

#[test]
fn every_source_is_gathered_again_and_says_what_moved() {
    let (_dir, workspace, ledger, acme, other) = two_gathered();
    write_files(
        acme.path(),
        &[(
            "skills/deploy/SKILL.md",
            &skill("deploy", "Ship it, carefully"),
        )],
    );
    fs::remove_dir_all(acme.path().join("skills").join("review")).unwrap();
    commit(acme.path());
    write_files(
        other.path(),
        &[("skills/sign/SKILL.md", &skill("sign", "Sign it"))],
    );
    commit(other.path());

    let updated = sources::update(&workspace, &ledger, Which::Every, &Watch::new()).unwrap();

    let mut said = moved(&updated);
    said.sort();
    assert_eq!(
        said,
        [
            (
                String::from("acme"),
                vec![],
                vec![String::from("deploy")],
                vec![String::from("review")]
            ),
            (
                String::from("other"),
                vec![String::from("sign")],
                vec![],
                vec![]
            ),
        ]
    );
    let shelf: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| format!("{}:{}", gathered.source.name, gathered.name))
        .collect();
    assert_eq!(shelf, ["acme:deploy", "other:pdf-forms", "other:sign"]);
}

#[test]
fn only_the_sources_named_are_gathered_again_each_once() {
    let (_dir, workspace, ledger, acme, other) = two_gathered();
    for repository in [&acme, &other] {
        write_files(
            repository.path(),
            &[("skills/new/SKILL.md", &skill("new", "New"))],
        );
        commit(repository.path());
    }
    let names = [String::from("other"), String::from("other")];

    let updated =
        sources::update(&workspace, &ledger, Which::Named(&names), &Watch::new()).unwrap();

    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].source.name.as_str(), "other");
    let shelf: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .filter(|gathered| gathered.name.as_str() == "new")
        .map(|gathered| gathered.source.name.to_string())
        .collect();
    assert_eq!(shelf, ["other"]);
}

#[test]
fn a_name_no_source_is_called_gathers_nothing() {
    let (_dir, workspace, ledger, _acme, _other) = two_gathered();
    let logged = ledger.history(100).unwrap().len();
    let names = [String::from("other"), String::from("nobody")];

    let error =
        sources::update(&workspace, &ledger, Which::Named(&names), &Watch::new()).unwrap_err();

    assert!(
        matches!(&error, SourceError::NotFound { name } if name == "nobody"),
        "{error:?}"
    );
    assert_eq!(
        ledger.history(100).unwrap().len(),
        logged,
        "no gather was run"
    );
}

#[test]
fn a_pinned_source_is_gathered_again_at_its_pin() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let pin = gix::open(source.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).pinned(Some(&pin)),
    )
    .unwrap();
    write_files(
        source.path(),
        &[("skills/deploy/SKILL.md", &skill("deploy", "Moved on"))],
    );
    commit(source.path());

    let updated = sources::update(&workspace, &ledger, Which::Every, &Watch::new()).unwrap();

    let report = updated[0].outcome.as_ref().unwrap();
    assert_eq!(report.revision.as_deref(), Some(pin.as_str()));
    assert_eq!(report.unchanged.len(), 1, "{report:?}");
    assert_eq!(
        ledger.sources().unwrap()[0].pin.as_deref(),
        Some(pin.as_str())
    );
    assert_eq!(
        ledger.gathered().unwrap()[0].summary.as_deref(),
        Some("Ship it")
    );
}

#[test]
fn a_source_that_cannot_be_gathered_again_is_said_and_the_rest_are_gathered() {
    let (_dir, workspace, ledger, acme, other) = two_gathered();
    let acme_url = url(&acme);
    drop(acme);
    write_files(
        other.path(),
        &[("skills/sign/SKILL.md", &skill("sign", "Sign it"))],
    );
    commit(other.path());

    let updated = sources::update(&workspace, &ledger, Which::Every, &Watch::new()).unwrap();

    let failed: Vec<&Regathered> = updated.iter().filter(|r| r.outcome.is_err()).collect();
    assert_eq!(failed.len(), 1, "{updated:?}");
    assert_eq!(failed[0].source.url.as_str(), acme_url);
    let gathered = updated
        .iter()
        .find(|r| r.source.name.as_str() == "other")
        .unwrap();
    assert_eq!(gathered.outcome.as_ref().unwrap().added.len(), 1);
    // What the one that failed supplied is on the shelf as it was.
    assert_eq!(
        ledger
            .gathered()
            .unwrap()
            .iter()
            .filter(|gathered| gathered.source.name.as_str() == "acme")
            .count(),
        2
    );
}

#[test]
fn once_one_is_cancelled_the_rest_are_not_begun_and_said_cancelled() {
    let (_dir, workspace, ledger, _acme, _other) = two_gathered();
    let logged = ledger.history(100).unwrap().len();
    let watch = Watch::new();
    watch.cancel();

    let updated = sources::update(&workspace, &ledger, Which::Every, &watch).unwrap();

    assert_eq!(updated.len(), 2);
    assert!(
        updated
            .iter()
            .all(|r| matches!(r.outcome, Err(GatherError::Cancelled))),
        "{updated:?}"
    );
    assert_eq!(ledger.history(100).unwrap().len(), logged);
    assert_eq!(ledger.gathered().unwrap().len(), 3);
}
