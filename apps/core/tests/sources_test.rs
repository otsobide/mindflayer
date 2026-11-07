//! A gathered source's own changes against a real workspace and real
//! repositories: gathering some again by name, or every one, and removing
//! one with what it put on the shelf.

mod common;

use std::fs;

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::assign;
use mindweaver_core::gather::{self, GatherError, Request, Watch};
use mindweaver_core::install::{Confirmed, Edits};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::marketplace;
use mindweaver_core::sources::{self, Again, Regathered, SourceError, Which};
use mindweaver_core::{
    Agent, AgentHomes, Directories, Kind, MindProject, Reference, Share, WeaverWorkspace,
};
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

/// What gathering a source on its own again came to, which every source
/// here is.
fn again(regathered: &Regathered) -> Result<&gather::Report, &GatherError> {
    match &regathered.outcome {
        Again::Gathered(outcome) => outcome.as_ref(),
        Again::Marketplace(_) => panic!("{regathered:?} is no source gathered on its own"),
    }
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
            let report = again(regathered).expect("gathered again");
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

    let report = again(&updated[0]).unwrap();
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

    let failed: Vec<&Regathered> = updated.iter().filter(|r| again(r).is_err()).collect();
    assert_eq!(failed.len(), 1, "{updated:?}");
    assert_eq!(failed[0].source.url.as_str(), acme_url);
    let gathered = updated
        .iter()
        .find(|r| r.source.name.as_str() == "other")
        .unwrap();
    assert_eq!(again(gathered).unwrap().added.len(), 1);
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
            .all(|r| matches!(again(r), Err(GatherError::Cancelled))),
        "{updated:?}"
    );
    assert_eq!(ledger.history(100).unwrap().len(), logged);
    assert_eq!(ledger.gathered().unwrap().len(), 3);
}

// ---------------------------------------------------------------------------
// Removing one
// ---------------------------------------------------------------------------

/// `two_gathered`, with a project, `collapse`, linked below the workspace
/// and given `deploy` from `acme`.
fn installed_from_acme() -> (
    TempDir,
    WeaverWorkspace,
    Ledger,
    MindProject,
    TempDir,
    TempDir,
) {
    let (dir, mut workspace, ledger, acme, other) = two_gathered();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    let parts = assign::install(
        &workspace,
        std::slice::from_ref(&project),
        &Reference::parse("deploy"),
        Some("acme"),
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
        Confirmed::Yes,
    )
    .unwrap()
    .parts;
    assert!(parts.iter().all(|part| part
        .done
        .as_ref()
        .is_ok_and(|offered| offered.installed.is_ok())));
    (dir, workspace, ledger, project, acme, other)
}

/// The folders a source called `name` has: its clone and its shelf.
fn folders(workspace: &WeaverWorkspace, ledger: &Ledger, name: &str) -> Vec<std::path::PathBuf> {
    let source = sources::named(ledger, name).unwrap();
    vec![
        workspace.cache_dir().join(&source.directory),
        workspace.gathered_dir(Kind::Skill).join(&source.directory),
    ]
}

#[test]
fn a_removal_planned_says_its_folders_and_what_was_installed_from_it_and_writes_nothing() {
    let (_dir, workspace, ledger, project, _acme, _other) = installed_from_acme();
    let expected = folders(&workspace, &ledger, "acme");

    let removal = sources::plan_removal(&workspace, &ledger, "acme").unwrap();

    assert_eq!(removal.source.name.as_str(), "acme");
    assert_eq!(removal.artifacts, 2);
    assert_eq!(removal.folders, expected);
    let installed: Vec<(String, Kind, String)> = removal
        .installed
        .iter()
        .map(|from| {
            (
                from.project.name().to_owned(),
                from.kind,
                from.name.to_string(),
            )
        })
        .collect();
    assert_eq!(
        installed,
        [(
            project.name().to_owned(),
            Kind::Skill,
            String::from("deploy")
        )]
    );
    assert!(removal.unread.is_empty());
    assert!(expected.iter().all(|folder| folder.is_dir()));
    assert_eq!(ledger.sources().unwrap().len(), 2);
}

#[test]
fn a_removal_takes_its_folders_and_rows_and_leaves_what_was_installed_recorded() {
    let (_dir, workspace, ledger, project, _acme, _other) = installed_from_acme();
    let gone = folders(&workspace, &ledger, "acme");
    let kept = folders(&workspace, &ledger, "other");
    let source = sources::named(&ledger, "acme").unwrap();

    let removal = sources::remove(&workspace, &ledger, "acme").unwrap();

    assert_eq!(removal.folders, gone);
    assert!(gone.iter().all(|folder| !folder.exists()), "{gone:?}");
    assert!(kept.iter().all(|folder| folder.is_dir()), "{kept:?}");
    let names: Vec<String> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| source.name.to_string())
        .collect();
    assert_eq!(names, ["other"]);
    assert!(ledger
        .gathered()
        .unwrap()
        .iter()
        .all(|gathered| gathered.source.name.as_str() == "other"));
    // Installed from it, recorded still, where it came from said, its
    // copies where they were.
    let record = ledger
        .installation(project.name(), Kind::Skill, "deploy")
        .unwrap()
        .expect("still recorded");
    assert_eq!(record.source_id, None);
    assert_eq!(record.origin, Some(source.origin()));
    assert!(project
        .root()
        .join(".claude/skills/deploy/SKILL.md")
        .is_file());
}

/// A source of rules (#111) is removed as one of skills is: its clone and
/// its folder on the rules' shelf go, and a rule installed from it stays,
/// recorded, its origin said and its source gone.
#[test]
fn a_source_of_rules_is_removed_with_its_shelf_and_leaves_what_was_installed_from_it() {
    let rules = repository(&[("rules/git/no-force-push.md", "Never force-push.\n")]);
    let (dir, mut workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&rules))
            .from_subdirectory("rules")
            .gathering(Kind::Rule)
            .named(Some("team-rules")),
    )
    .unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    let parts = assign::install(
        &workspace,
        std::slice::from_ref(&project),
        &Reference::parse("git/no-force-push"),
        Some("team-rules"),
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
        Confirmed::Yes,
    )
    .unwrap()
    .parts;
    assert!(parts.iter().all(|part| part
        .done
        .as_ref()
        .is_ok_and(|offered| offered.installed.is_ok())));
    let source = sources::named(&ledger, "team-rules").unwrap();
    let shelf = workspace.gathered_dir(Kind::Rule).join(&source.directory);

    let planned = sources::plan_removal(&workspace, &ledger, "team-rules").unwrap();

    assert_eq!(
        planned.folders,
        [workspace.cache_dir().join(&source.directory), shelf.clone()]
    );
    assert_eq!(planned.artifacts, 1);
    let installed: Vec<(Kind, String)> = planned
        .installed
        .iter()
        .map(|from| (from.kind, from.name.to_string()))
        .collect();
    assert_eq!(installed, [(Kind::Rule, String::from("git/no-force-push"))]);

    sources::remove(&workspace, &ledger, "team-rules").unwrap();

    assert!(!shelf.exists());
    assert!(ledger.sources().unwrap().is_empty());
    let record = ledger
        .installation(project.name(), Kind::Rule, "git/no-force-push")
        .unwrap()
        .expect("still recorded");
    assert_eq!(record.source_id, None);
    assert_eq!(record.origin, Some(source.origin()));
    assert!(fs::read_to_string(project.root().join("AGENTS.md"))
        .unwrap()
        .contains("Never force-push."));
}

#[test]
fn a_folder_on_the_shelf_no_source_is_called_is_never_removed() {
    let (_dir, workspace, ledger, _acme, _other) = two_gathered();
    let stray = workspace.gathered_dir(Kind::Skill).join("stray");
    fs::create_dir_all(stray.join("notes")).unwrap();

    let error = sources::remove(&workspace, &ledger, "stray").unwrap_err();

    assert!(
        matches!(&error, SourceError::NotFound { name } if name == "stray"),
        "{error:?}"
    );
    assert!(stray.join("notes").is_dir());
    assert_eq!(ledger.sources().unwrap().len(), 2);
}

#[test]
fn a_source_recorded_with_a_folder_that_is_not_its_own_is_refused_and_nothing_removed() {
    let (dir, workspace, ledger, _acme, _other) = two_gathered();
    // What no gather records: a folder name that climbs out of the shelf,
    // onto a folder somebody keeps beside the workspace.
    let outside = dir.path().join("keep");
    fs::create_dir_all(outside.join("notes")).unwrap();
    for directory in ["../../keep", "..", ".", ""] {
        rusqlite::Connection::open(workspace.ledger_path())
            .unwrap()
            .execute(
                "UPDATE sources SET directory = ?1 WHERE name = 'acme'",
                [directory],
            )
            .unwrap();

        let planned = sources::plan_removal(&workspace, &ledger, "acme").unwrap_err();
        let error = sources::remove(&workspace, &ledger, "acme").unwrap_err();

        assert_eq!(planned.code(), "source_folder_not_its_own", "{directory}");
        assert_eq!(error.code(), "source_folder_not_its_own", "{directory}");
        assert_eq!(
            error.to_string(),
            format!(
                "the source `acme` is recorded with the folder `{directory}`, which is no one \
                 folder of the shelf or the cache, so nothing is removed"
            )
        );
        assert!(outside.join("notes").is_dir(), "{directory}");
        assert!(workspace.cache_dir().is_dir() && workspace.gathered_dir(Kind::Skill).is_dir());
        assert_eq!(ledger.sources().unwrap().len(), 2, "{directory}");
    }
}

// Unix only: a folder's write permission is what is taken away, and Windows
// keeps none on a folder.
#[cfg(unix)]
#[test]
fn a_shelf_folder_that_will_not_go_keeps_the_source_and_its_rows() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, workspace, ledger, _acme, _other) = two_gathered();
    let [clone, shelf] = <[_; 2]>::try_from(folders(&workspace, &ledger, "acme")).unwrap();
    let held = workspace.gathered_dir(Kind::Skill);
    fs::set_permissions(&held, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = held.join(".probe");
    let writable = fs::write(&probe, "").is_ok();
    let _ = fs::remove_file(&probe);

    let error = sources::remove(&workspace, &ledger, "acme");
    fs::set_permissions(&held, fs::Permissions::from_mode(0o755)).unwrap();
    if writable {
        eprintln!("skipped: {} can be written all the same", held.display());
        return;
    }

    let error = error.unwrap_err();
    assert_eq!(error.code(), "source_folder_unremoved");
    assert!(
        error
            .to_string()
            .starts_with(&format!("{}: ", shelf.display())),
        "{error}"
    );
    assert!(
        !clone.exists(),
        "the clone, which every gather makes again, went first"
    );
    assert!(shelf.is_dir());
    assert_eq!(ledger.sources().unwrap().len(), 2);
    assert_eq!(ledger.gathered().unwrap().len(), 3);
}

// ---------------------------------------------------------------------------
// A marketplace and its plugins (#103)
// ---------------------------------------------------------------------------

/// A marketplace's repository, `m`, listing `a` and `b`, each a plugin of
/// its own repository's, gathered whole into a workspace, beside `acme`, a
/// source gathered on its own.
fn marketplace_gathered() -> (TempDir, WeaverWorkspace, Ledger, TempDir, TempDir) {
    let market = repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{"name": "m", "plugins": [
                {"name": "a", "source": "./a"},
                {"name": "b", "source": "./b"}
            ]}"#,
        ),
        ("a/skills/one/SKILL.md", &skill("one", "First")),
        ("b/skills/two/SKILL.md", &skill("two", "Second")),
    ]);
    let acme = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    marketplace::gather(
        &workspace,
        &ledger,
        &marketplace::Request::new(Request::git(url(&market)), marketplace::Pick::All),
    )
    .unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&acme)).named(Some("acme")),
    )
    .unwrap();
    (dir, workspace, ledger, market, acme)
}

/// What gathering a marketplace again came to.
fn again_of(regathered: &Regathered) -> &marketplace::Report {
    match &regathered.outcome {
        Again::Marketplace(Ok(report)) => report,
        other => panic!("{other:?} is no marketplace gathered again"),
    }
}

#[test]
fn a_marketplace_gathered_again_gathers_each_plugin_of_it_again_from_its_catalog() {
    let (_dir, workspace, ledger, market, _acme) = marketplace_gathered();
    write_files(
        market.path(),
        &[
            (
                ".claude-plugin/marketplace.json",
                r#"{"name": "m", "plugins": [{"name": "a", "source": "./a"}]}"#,
            ),
            ("a/skills/one/SKILL.md", &skill("one", "First, again")),
        ],
    );
    commit(market.path());

    let names = [String::from("m")];
    let updated =
        sources::update(&workspace, &ledger, Which::Named(&names), &Watch::new()).unwrap();

    assert_eq!(updated.len(), 1);
    let report = again_of(&updated[0]);
    let children: Vec<(&str, usize)> = report
        .children
        .iter()
        .map(|child| {
            (
                child.entry.as_str(),
                child.outcome.as_ref().unwrap().updated.len(),
            )
        })
        .collect();
    assert_eq!(children, [("a", 1)]);
    // The one it lists no more is said, and kept as it was.
    let failures: Vec<(Option<&str>, &str)> = report
        .catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(failures, [(Some("b"), "marketplace_entry_unlisted")]);
    assert!(sources::named(&ledger, "m.b").is_ok());
    assert!(ledger
        .gathered()
        .unwrap()
        .iter()
        .any(|gathered| gathered.name == "two"));
}

#[test]
fn every_source_gathered_again_gathers_a_marketplace_once_with_its_plugins() {
    let (_dir, workspace, ledger, _market, _acme) = marketplace_gathered();

    let updated = sources::update(&workspace, &ledger, Which::Every, &Watch::new()).unwrap();

    let names: Vec<&str> = updated
        .iter()
        .map(|regathered| regathered.source.name.as_str())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"m") && names.contains(&"acme"), "{names:?}");
    let market = updated
        .iter()
        .find(|regathered| regathered.source.name == "m")
        .unwrap();
    let entries: Vec<&str> = again_of(market)
        .children
        .iter()
        .map(|child| child.entry.as_str())
        .collect();
    assert_eq!(entries, ["a", "b"]);
}

#[test]
fn a_plugin_gathered_again_by_its_name_is_gathered_through_its_marketplace_alone() {
    let (_dir, workspace, ledger, _market, _acme) = marketplace_gathered();

    let names = [String::from("m.b")];
    let updated =
        sources::update(&workspace, &ledger, Which::Named(&names), &Watch::new()).unwrap();

    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].source.name, "m.b");
    let entries: Vec<&str> = again_of(&updated[0])
        .children
        .iter()
        .map(|child| child.entry.as_str())
        .collect();
    assert_eq!(entries, ["b"]);

    // Named beside its marketplace, it is its marketplace's to gather, once.
    let names = [String::from("m.a"), String::from("m")];
    let updated =
        sources::update(&workspace, &ledger, Which::Named(&names), &Watch::new()).unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].source.name, "m");
}

#[test]
fn the_sources_are_listed_each_plugin_under_its_marketplace() {
    let (_dir, _workspace, ledger, _market, _acme) = marketplace_gathered();

    let listed: Vec<(String, Option<String>, Option<String>)> = ledger
        .source_listing()
        .unwrap()
        .into_iter()
        .map(|it| {
            (
                it.source.name.to_string(),
                it.parent.map(|parent| parent.to_string()),
                it.entry,
            )
        })
        .collect();

    let at = listed.iter().position(|(name, _, _)| name == "m").unwrap();
    assert_eq!(
        listed[at..at + 3],
        [
            (String::from("m"), None, None),
            (
                String::from("m.a"),
                Some(String::from("m")),
                Some(String::from("a"))
            ),
            (
                String::from("m.b"),
                Some(String::from("m")),
                Some(String::from("b"))
            ),
        ]
    );
    assert_eq!(listed.len(), 4);
}

#[test]
fn removing_a_marketplace_says_and_takes_each_plugin_of_it_and_their_folders() {
    let (dir, mut workspace, ledger, _market, _acme) = marketplace_gathered();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    assign::install(
        &workspace,
        std::slice::from_ref(&project),
        &Reference::parse("one"),
        Some("m.a"),
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
        Confirmed::Yes,
    )
    .unwrap();
    let market = sources::named(&ledger, "m").unwrap();
    let a = sources::named(&ledger, "m.a").unwrap();
    let clone = workspace.cache_dir().join(&market.directory);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&a.directory);

    let planned = sources::plan_removal(&workspace, &ledger, "m").unwrap();

    let plugins: Vec<&str> = planned
        .children
        .iter()
        .map(|child| child.source.name.as_str())
        .collect();
    assert_eq!(plugins, ["m.a", "m.b"]);
    assert_eq!(planned.folders, std::slice::from_ref(&clone));
    assert_eq!(planned.children[0].folders, std::slice::from_ref(&shelf));
    assert_eq!(planned.children[0].installed.len(), 1);
    assert!(clone.is_dir() && shelf.is_dir());

    let removed = sources::remove(&workspace, &ledger, "m").unwrap();

    assert_eq!(removed.children.len(), 2);
    assert!(!clone.exists() && !shelf.exists());
    let names: Vec<String> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| source.name.to_string())
        .collect();
    assert_eq!(names, ["acme"]);
    let record = ledger
        .installation(project.name(), Kind::Skill, "one")
        .unwrap()
        .expect("still recorded");
    assert_eq!(record.source_id, None);
}

#[test]
fn removing_one_plugin_of_a_marketplace_leaves_the_marketplace_and_its_clone() {
    let (_dir, workspace, ledger, _market, _acme) = marketplace_gathered();
    let market = sources::named(&ledger, "m").unwrap();

    sources::remove(&workspace, &ledger, "m.a").unwrap();

    assert!(sources::named(&ledger, "m").is_ok());
    assert!(sources::named(&ledger, "m.b").is_ok());
    assert!(workspace.cache_dir().join(&market.directory).is_dir());
}
