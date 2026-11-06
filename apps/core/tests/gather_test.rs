//! Gathering skills out of a real git repository, against a real workspace.

mod common;

use std::fs;
use std::sync::{Arc, Mutex};

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::gather::{self, GatherError, Progress, Request, Search, Warning, Watch};
use mindweaver_core::ledger::{Action, Ledger, SourceKind};
use mindweaver_core::marketplace::Dialect;
use mindweaver_core::value::SourceName;
use mindweaver_core::{Kind, MindProject, ValidationIssue, WeaverWorkspace, DEFAULT_SUBDIRECTORY};
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
fn a_skill_s_claude_plugin_folder_is_left_off_the_shelf_and_a_gather_again_is_unchanged() {
    let source = repository(&[
        (
            "plugins/ops/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        (
            "plugins/ops/skills/deploy/.claude-plugin/plugin.json",
            r#"{"name": "deploy", "hooks": "hooks.json"}"#,
        ),
        (
            "plugins/ops/skills/deploy/scripts/.claude-plugin/example.json",
            "{}",
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));

    let first = gather::gather(&workspace, &ledger, &request).unwrap();
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    let deploy = workspace
        .gathered_dir(Kind::Skill)
        .join(&first.directory)
        .join("deploy");
    assert!(deploy.join("SKILL.md").is_file());
    assert!(!deploy.join(".claude-plugin").exists(), "only at its top");
    assert!(deploy.join("scripts/.claude-plugin/example.json").is_file());
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
    // The hash of what the shelf holds, which an install's copy is too.
    assert_eq!(
        ledger.gathered().unwrap()[0].content_hash,
        Some(mindweaver_core::hash::tree(&deploy).unwrap())
    );
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
fn a_source_recorded_with_a_folder_that_is_not_its_own_is_refused_and_nothing_fetched_into_it() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    gather::gather(&workspace, &ledger, &request).unwrap();

    // What no gather records: the cache's own folder, and the cache itself,
    // which a fetch would replace whole.
    for directory in ["..", "."] {
        rusqlite::Connection::open(workspace.ledger_path())
            .unwrap()
            .execute("UPDATE sources SET directory = ?1", [directory])
            .unwrap();

        let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

        assert_eq!(error.code(), "source_folder_not_its_own", "{directory}");
        assert!(
            matches!(&error, GatherError::FolderNotItsOwn { directory: said, .. } if said == directory),
            "{error:?}"
        );
        assert!(workspace.ledger_path().is_file(), "{directory}");
        assert!(workspace.config_path().is_file(), "{directory}");
        assert!(workspace.cache_dir().is_dir(), "{directory}");
        assert_eq!(ledger.gathered().unwrap().len(), 1, "{directory}");
    }
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
fn a_skill_its_source_no_longer_holds_is_taken_off_the_shelf_and_said_removed() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();
    let review = workspace
        .gathered_dir(Kind::Skill)
        .join(&first.directory)
        .join("review");
    assert!(review.join("SKILL.md").is_file());

    fs::remove_dir_all(source.path().join("skills").join("review")).unwrap();
    commit(source.path());
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    let removed: Vec<&str> = again.removed.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(removed, ["review"], "{again:?}");
    assert_eq!(again.removed[0].path, review);
    assert_eq!(again.removed[0].summary.as_deref(), Some("Read a change"));
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
    assert_eq!(
        again.total(),
        1,
        "what is removed is on the shelf no longer"
    );
    assert!(!review.exists(), "its copy is off the shelf");
    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.name.to_string())
        .collect();
    assert_eq!(names, ["deploy"]);
    assert_eq!(
        ledger.history(1).unwrap()[0].detail.as_deref(),
        Some("0 added, 0 updated, 1 unchanged, 1 removed, 0 failed")
    );
}

#[test]
fn a_skill_whose_folder_no_longer_reads_is_kept_as_it_was_and_said() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();

    write_files(
        source.path(),
        &[("skills/deploy/SKILL.md", "no front matter\n")],
    );
    commit(source.path());
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert!(again.removed.is_empty(), "{again:?}");
    assert_eq!(again.failures.len(), 1, "{again:?}");
    let copy = workspace
        .gathered_dir(Kind::Skill)
        .join(&first.directory)
        .join("deploy")
        .join("SKILL.md");
    assert_eq!(
        fs::read_to_string(copy).unwrap(),
        skill("deploy", "Ship it")
    );
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].revision, first.revision, "taken when it read");
}

#[test]
fn a_skill_that_renamed_itself_is_removed_by_its_old_name_and_keeps_its_folder() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();

    write_files(
        source.path(),
        &[("skills/deploy/SKILL.md", &skill("ship", "Ship it"))],
    );
    commit(source.path());
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    // Updated, as its folder is, which is what the shelf files it by.
    let updated: Vec<&str> = again.updated.iter().map(|h| h.name.as_str()).collect();
    let removed: Vec<&str> = again.removed.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        (updated, removed),
        (vec!["ship"], vec!["deploy"]),
        "{again:?}"
    );
    let folder = workspace
        .gathered_dir(Kind::Skill)
        .join(&first.directory)
        .join("deploy");
    assert_eq!(
        fs::read_to_string(folder.join("SKILL.md")).unwrap(),
        skill("ship", "Ship it"),
        "the folder holds the name it took"
    );
    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.name.to_string())
        .collect();
    assert_eq!(names, ["ship"]);
}

#[test]
fn a_skill_whose_folder_was_renamed_upstream_leaves_nothing_in_its_old_folder() {
    let source = repository(&[("skills/deploy-skill/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    assert!(shelf.join("deploy-skill").join("SKILL.md").is_file());

    fs::rename(
        source.path().join("skills").join("deploy-skill"),
        source.path().join("skills").join("deploy"),
    )
    .unwrap();
    commit(source.path());
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    // Still supplied, under its name, so nothing is removed.
    assert!(again.removed.is_empty(), "{again:?}");
    assert!(again.failures.is_empty(), "{again:?}");
    assert!(shelf.join("deploy").join("SKILL.md").is_file());
    assert!(
        !shelf.join("deploy-skill").exists(),
        "its old folder is off the shelf"
    );
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    assert_eq!(
        workspace.root().join(&gathered[0].path),
        shelf.join("deploy"),
        "its row names the folder it was taken into"
    );
}

#[test]
fn a_cancelled_gather_takes_nothing_off_the_shelf() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    fs::remove_dir_all(source.path().join("skills").join("review")).unwrap();
    commit(source.path());
    // As late as a cancel is honoured: every folder read, nothing placed.
    let watch = cancelled_at(
        |progress| matches!(progress, Progress::Harvesting { done, total } if done == total),
    );

    let cancelled = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    );

    assert!(
        matches!(cancelled, Err(GatherError::Cancelled)),
        "{cancelled:?}"
    );
    assert_eq!(ledger.gathered().unwrap().len(), 2);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    assert!(shelf.join("review").join("SKILL.md").is_file());
}

/// The commit `repository`'s HEAD is at.
fn head_of(repository: &TempDir) -> String {
    gix::open(repository.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string()
}

/// A repository whose `deploy` said `Ship it` at its first commit and
/// `Ship it, carefully` at its second, and both commits, oldest first.
fn moved_on() -> (TempDir, String, String) {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let first = head_of(&source);
    write_files(
        source.path(),
        &[(
            "skills/deploy/SKILL.md",
            &skill("deploy", "Ship it, carefully"),
        )],
    );
    commit(source.path());
    let second = head_of(&source);
    (source, first, second)
}

#[test]
fn a_pinned_gather_takes_that_commit_and_its_source_keeps_the_pin_beside_its_branch() {
    let (source, first, _) = moved_on();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    // In capitals, as a commit copied from elsewhere may be, and the same
    // commit for it.
    let request = Request::git(url(&source)).pinned(Some(first.to_uppercase()));
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(report.revision.as_deref(), Some(first.as_str()));
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered[0].summary.as_deref(), Some("Ship it"));
    assert_eq!(gathered[0].revision.as_deref(), Some(first.as_str()));
    let sources = ledger.sources().unwrap();
    assert_eq!(sources[0].pin.as_deref(), Some(first.as_str()));
    assert_eq!(sources[0].reference, None);
    assert_eq!(sources[0].revision.as_deref(), Some(first.as_str()));
}

#[test]
fn moving_a_sources_pin_or_dropping_it_updates_that_one_source() {
    let (source, first, second) = moved_on();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let gathered_at = |pin: Option<&str>| {
        gather::gather(&workspace, &ledger, &Request::git(url(&source)).pinned(pin)).unwrap()
    };

    let pinned = gathered_at(Some(&first));
    for (pin, revision, summary) in [
        (Some(second.as_str()), &second, "Ship it, carefully"),
        (Some(first.as_str()), &first, "Ship it"),
        (None, &second, "Ship it, carefully"),
    ] {
        let report = gathered_at(pin);

        assert_eq!(report.directory, pinned.directory, "{pin:?}");
        assert_eq!(report.name, pinned.name, "{pin:?}");
        let sources = ledger.sources().unwrap();
        assert_eq!(sources.len(), 1, "{pin:?}: {sources:#?}");
        assert_eq!(sources[0].pin.as_deref(), pin);
        assert_eq!(
            sources[0].revision.as_deref(),
            Some(revision.as_str()),
            "{pin:?}"
        );
        let gathered = ledger.gathered().unwrap();
        assert_eq!(gathered.len(), 1, "{pin:?}");
        assert_eq!(gathered[0].summary.as_deref(), Some(summary), "{pin:?}");
    }
}

#[test]
fn a_pin_its_remote_will_not_hand_over_nor_its_branch_reach_is_refused_and_changes_nothing() {
    let (source, _, second) = moved_on();
    let elsewhere = repository(&[("README.md", "another repository\n")]);
    let foreign = head_of(&elsewhere);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let before = (ledger.sources().unwrap(), ledger.gathered().unwrap());

    let refused = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).pinned(Some(&foreign)),
    )
    .unwrap_err();

    assert_eq!(refused.code(), "git_pin_unreachable", "{refused}");
    assert!(refused.to_string().contains(&foreign), "{refused}");
    assert_eq!(
        (ledger.sources().unwrap(), ledger.gathered().unwrap()),
        before
    );
    assert_eq!(before.0[0].revision.as_deref(), Some(second.as_str()));
    assert_eq!(before.0[0].pin, None);
    let logged = &ledger.history(1).unwrap()[0];
    assert_eq!(logged.outcome, "failed");
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
fn a_skill_that_breaks_the_rules_is_gathered_with_a_warning() {
    // Lenient, as the Agent Skills client guide advises for skills written
    // for other agents: each problem is said, and nothing is left behind.
    let source = repository(&[
        ("skills/a--b/SKILL.md", &skill("a--b", "Doubled")),
        (
            "skills/nested/SKILL.md",
            "---\nname: nested\ndescription: Nested metadata\nmetadata:\n  owner:\n    team: ops\n---\n",
        ),
        ("skills/fine/SKILL.md", &skill("fine", "Nothing wrong")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let names: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["a--b", "fine", "nested"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert_eq!(
        report.warnings,
        vec![
            Warning {
                path: shelf.join("a--b").join("SKILL.md"),
                issue: ValidationIssue::NameNotKebabCase {
                    segment: "a--b".into()
                },
            },
            Warning {
                path: shelf.join("nested").join("SKILL.md"),
                issue: ValidationIssue::MetadataNotAString {
                    key: "owner".into()
                },
            },
        ]
    );
    // What is ignored is not rewritten: the shelf holds the file as written.
    // Line endings are left out of the comparison because the checkout, not
    // Mindweaver, decides them: on Windows git's autocrlf turns them into CRLF.
    let copied = fs::read_to_string(shelf.join("nested").join("SKILL.md")).unwrap();
    let copied = copied.replace("\r\n", "\n");
    assert!(copied.contains("  owner:\n    team: ops\n"), "{copied}");
}

#[test]
fn a_compatibility_written_as_a_list_is_gathered_with_a_warning() {
    // A list of agents is what an author writes when the specification's
    // prose string does not occur to them; the skill is still a skill.
    let source = repository(&[(
        "skills/deploy/SKILL.md",
        "---\nname: deploy\ndescription: Ship it\ncompatibility: [claude-code, codex]\n---\n",
    )]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let names: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["deploy"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert_eq!(
        report.warnings,
        vec![Warning {
            path: shelf.join("deploy").join("SKILL.md"),
            issue: ValidationIssue::CompatibilityNotAString,
        }]
    );
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

/// What `request` gathered into a new workspace, each skill by its name
/// and where it was found in the repository, held to have failed nowhere.
fn found(request: &Request) -> Vec<(String, String)> {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let report = gather::gather(&workspace, &ledger, request).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| (gathered.name.to_string(), gathered.upstream_path))
        .collect()
}

/// `pairs` as [`found`] gives them.
fn named(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, path)| ((*name).to_owned(), (*path).to_owned()))
        .collect()
}

#[test]
fn skills_grouped_in_folders_of_their_own_are_found_in_each() {
    let source = repository(&[
        ("skills/team/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/team/deploy/run.sh", "exit 0\n"),
        (
            "skills/.curated/review/SKILL.md",
            &skill("review", "Read a change"),
        ),
        (
            "skills/.system/notes/SKILL.md",
            &skill("notes", "Keep notes"),
        ),
        ("skills/audit/SKILL.md", &skill("audit", "Audit it")),
    ]);

    let gathered = found(&Request::git(url(&source)));

    assert_eq!(
        gathered,
        named(&[
            ("audit", "skills/audit"),
            ("deploy", "skills/team/deploy"),
            ("notes", "skills/.system/notes"),
            ("review", "skills/.curated/review"),
        ])
    );
}

/// Every layout the tools that install skills from a repository share, a
/// repository of its own each: where the skill's folder is in it.
const LAYOUTS: [&str; 8] = [
    "skills/deploy",
    "skills/team/deploy",
    "deploy",
    ".agents/skills/deploy",
    ".claude/skills/deploy",
    "skills/.curated/deploy",
    "skills/.system/deploy",
    "plugins/ops/skills/deploy",
];

#[test]
fn a_skill_is_found_in_every_layout_the_tools_share_with_nothing_named() {
    for layout in LAYOUTS {
        let manifest = format!("{layout}/SKILL.md");
        let script = format!("{layout}/scripts/run.sh");
        let source = repository(&[
            (&manifest, &skill("deploy", "Ship it")),
            (&script, "exit 0\n"),
            ("README.md", "not a skill\n"),
        ]);

        let gathered = found(&Request::git(url(&source)));

        assert_eq!(gathered, named(&[("deploy", layout)]), "{layout}");
    }
}

#[test]
fn a_repository_holding_every_layout_at_once_offers_each_skill_found() {
    let files: Vec<(String, String)> = LAYOUTS
        .iter()
        .enumerate()
        .map(|(at, layout)| {
            let name = format!("skill-{at}");
            let folder = layout.replace("deploy", &name);
            (format!("{folder}/SKILL.md"), skill(&name, "Do it"))
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let source = repository(&borrowed);

    let gathered = found(&Request::git(url(&source)));

    let mut expected: Vec<(String, String)> = LAYOUTS
        .iter()
        .enumerate()
        .map(|(at, layout)| {
            let name = format!("skill-{at}");
            (name.clone(), layout.replace("deploy", &name))
        })
        .collect();
    expected.sort();
    assert_eq!(gathered, expected);
}

/// A repository holding `files`, in a folder called `name` of its own, as
/// a clone of `owner/<name>` is: its URL ends in the name.
fn repository_named(name: &str, files: &[(&str, &str)]) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join(name);
    write_files(&root, files);
    commit(&root);
    let url = root.to_string_lossy().into_owned();
    (dir, url)
}

#[test]
fn a_skill_md_at_the_repository_s_root_makes_the_whole_of_it_one_skill_filed_by_its_name() {
    let (_source, address) = repository_named(
        "pdf-skill",
        &[
            ("SKILL.md", &skill("pdf-skill", "Read PDFs")),
            ("scripts/extract.py", "print(1)\n"),
            ("LICENSE", "MIT License\n"),
            // Inside the one skill, so a file of it, and nothing of its own.
            ("skills/forms/SKILL.md", &skill("forms", "Fill in forms")),
            (".claude-plugin/plugin.json", "{}"),
        ],
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(address.clone())).unwrap();

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let names: Vec<(&str, &str)> = report
        .added
        .iter()
        .map(|harvested| (harvested.name.as_str(), harvested.directory.as_str()))
        .collect();
    assert_eq!(names, [("pdf-skill", "pdf-skill")]);
    let skill = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join("pdf-skill");
    assert!(skill.join("scripts/extract.py").is_file());
    assert!(skill.join("skills/forms/SKILL.md").is_file());
    assert!(
        !skill.join(".git").exists(),
        "the clone's own is no part of it"
    );
    assert!(!skill.join(".claude-plugin").exists());
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    assert_eq!(gathered[0].upstream_path, ".");
    assert_eq!(gathered[0].license.as_deref(), Some("MIT License"));

    let again = gather::gather(&workspace, &ledger, &Request::git(address)).unwrap();
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
}

#[test]
fn a_root_skill_declaring_another_name_than_its_repository_s_is_gathered_with_a_warning() {
    let (_source, address) = repository_named("pdf-skill", &[("SKILL.md", &skill("pdf", "Read"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(address)).unwrap();

    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert_eq!(report.added.len(), 1, "{report:?}");
    assert_eq!(report.added[0].name.as_str(), "pdf");
    assert_eq!(
        report.warnings,
        [Warning {
            path: shelf.join("pdf-skill").join("SKILL.md"),
            issue: ValidationIssue::NameDirectoryMismatch {
                name: String::from("pdf"),
                directory: String::from("pdf-skill"),
            },
        }]
    );
}

#[test]
fn a_folder_named_that_holds_a_skill_md_is_that_one_skill() {
    let source = repository(&[
        ("skills/pdf/SKILL.md", &skill("pdf", "Read PDFs")),
        (
            "skills/pdf/forms/SKILL.md",
            &skill("forms", "Fill in forms"),
        ),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);

    let gathered = found(&Request::git(url(&source)).from_subdirectory("skills/pdf"));

    assert_eq!(gathered, named(&[("pdf", "skills/pdf")]));
}

/// Each failure of `report` that is a clash: the folder in the clone it
/// is of, by its route from the clone's root, the name it shares and
/// where the others are.
fn clashes(
    report: &gather::Report,
    workspace: &WeaverWorkspace,
) -> Vec<(String, String, Vec<String>)> {
    let clone = workspace.cache_dir().join(&report.directory);
    report
        .failures
        .iter()
        .map(|failure| match failure {
            gather::Failure::Clash { path, name, with } => {
                assert_eq!(failure.code(), "gathered_name_clash");
                let route = path.strip_prefix(&clone).unwrap();
                (
                    route.to_string_lossy().into_owned(),
                    name.clone(),
                    with.clone(),
                )
            }
            other => panic!("{other}"),
        })
        .collect()
}

#[test]
fn two_skills_of_one_folder_name_in_one_source_are_a_failure_for_both_and_the_rest_is_taken() {
    let source = repository(&[
        ("skills/pdf/SKILL.md", &skill("pdf", "Read PDFs")),
        (
            ".claude/skills/pdf/SKILL.md",
            &skill("pdf", "Read PDFs, ours"),
        ),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let added: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(added, ["review"]);
    assert_eq!(
        clashes(&report, &workspace),
        [
            (
                String::from(".claude/skills/pdf"),
                String::from("pdf"),
                vec![String::from("skills/pdf")]
            ),
            (
                String::from("skills/pdf"),
                String::from("pdf"),
                vec![String::from(".claude/skills/pdf")]
            ),
        ]
    );
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert!(!shelf.join("pdf").exists(), "neither is picked");
    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.name.to_string())
        .collect();
    assert_eq!(names, ["review"]);
}

#[test]
fn two_skills_declaring_one_name_in_one_source_are_a_failure_for_both() {
    let source = repository(&[
        ("skills/pdf-reader/SKILL.md", &skill("pdf", "Read PDFs")),
        ("skills/pdf-writer/SKILL.md", &skill("pdf", "Write PDFs")),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let added: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(added, ["review"]);
    assert_eq!(
        clashes(&report, &workspace),
        [
            (
                String::from("skills/pdf-reader"),
                String::from("pdf"),
                vec![String::from("skills/pdf-writer")]
            ),
            (
                String::from("skills/pdf-writer"),
                String::from("pdf"),
                vec![String::from("skills/pdf-reader")]
            ),
        ]
    );
}

#[test]
fn a_skill_an_earlier_gather_took_stays_as_it_was_once_another_of_its_name_clashes_with_it() {
    let source = repository(&[("skills/pdf/SKILL.md", &skill("pdf", "Read PDFs"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();

    write_files(
        source.path(),
        &[(".agents/skills/pdf/SKILL.md", &skill("pdf", "Another"))],
    );
    commit(source.path());
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(again.failures.len(), 2, "{:?}", again.failures);
    assert!(again.removed.is_empty(), "{again:?}");
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    assert_eq!(
        fs::read_to_string(shelf.join("pdf/SKILL.md")).unwrap(),
        skill("pdf", "Read PDFs")
    );
    assert_eq!(ledger.gathered().unwrap().len(), 1);
}

#[test]
fn a_repository_holding_no_skill_anywhere_gathers_nothing_and_is_no_failure() {
    let source = repository(&[("README.md", "nothing to gather\n")]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert!(report.is_empty(), "{report:?}");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

#[test]
fn a_skill_found_further_down_is_filed_on_the_shelf_under_its_folders_name() {
    let source = repository(&[
        ("skills/team/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/team/deploy/run.sh", "exit 0\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert!(shelf.join("deploy").join("run.sh").is_file());
    assert!(!shelf.join("team").exists());
    assert_eq!(report.added[0].directory, "deploy");
}

#[test]
fn a_folder_named_restricts_the_search_to_what_is_in_it() {
    let source = repository(&[
        (
            "plugins/ops/skills/audit/SKILL.md",
            &skill("audit", "Audit it"),
        ),
        (
            "plugins/web/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ]);

    let gathered = found(&Request::git(url(&source)).from_subdirectory("plugins/ops"));

    assert_eq!(gathered, named(&[("audit", "plugins/ops/skills/audit")]));
}

#[test]
fn a_skill_further_down_than_the_search_goes_or_in_a_package_s_folder_is_not_found() {
    let source = repository(&[
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
        (
            "skills/a/b/c/d/deep/SKILL.md",
            &skill("deep", "Too far down"),
        ),
        (
            "skills/tools/node_modules/vendored/SKILL.md",
            &skill("vendored", "A package's"),
        ),
    ]);

    let gathered = found(&Request::git(url(&source)));

    assert_eq!(gathered, named(&[("review", "skills/review")]));
}

#[test]
fn each_gathered_skill_records_its_folder_upstream_its_commit_and_the_hash_installs_use() {
    let source = repository(&[
        (
            "plugins/ops/skills/audit/SKILL.md",
            &skill("audit", "Audit it"),
        ),
        ("plugins/ops/skills/audit/check.sh", "exit 0\n"),
        (
            "plugins/ops/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git(url(&source)).from_subdirectory("./plugins/ops/skills/");
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let gathered = ledger.gathered().unwrap();
    let recorded: Vec<(&str, &str, Option<&str>)> = gathered
        .iter()
        .map(|artifact| {
            (
                artifact.name.as_str(),
                artifact.upstream_path.as_str(),
                artifact.revision.as_deref(),
            )
        })
        .collect();
    let revision = report.revision.as_deref();
    assert_eq!(
        recorded,
        [
            ("audit", "plugins/ops/skills/audit", revision),
            ("deploy", "plugins/ops/skills/deploy", revision),
        ]
    );
    // By the recipe an install hashes its copy by, of what the shelf holds.
    for artifact in &gathered {
        let shelved = workspace.root().join(&artifact.path);
        assert_eq!(
            artifact.content_hash,
            Some(mindweaver_core::hash::tree(&shelved).unwrap()),
            "{}",
            artifact.name
        );
    }
}

#[test]
fn a_gathered_skills_licence_is_its_manifests_else_its_licence_files_else_unknown() {
    let source = repository(&[
        (
            "skills/stated/SKILL.md",
            "---\nname: stated\ndescription: Says it\nlicense: Apache-2.0\n---\n",
        ),
        ("skills/stated/LICENSE", "MIT License\n"),
        ("skills/filed/SKILL.md", &skill("filed", "Keeps a file")),
        (
            "skills/filed/LICENSE.txt",
            "MIT License\n\nCopyright (c) Acme\n",
        ),
        ("skills/tagged/SKILL.md", &skill("tagged", "Tags a file")),
        (
            "skills/tagged/LICENSE.md",
            "# Licence\n\nSPDX-License-Identifier: MIT OR Apache-2.0\n",
        ),
        ("skills/unknown/SKILL.md", &skill("unknown", "Says nothing")),
        // The repository's licence is its own, not the skill's folder's.
        ("LICENSE", "GNU GENERAL PUBLIC LICENSE\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let licenses: Vec<(String, Option<String>)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|artifact| (artifact.name.to_string(), artifact.license))
        .collect();
    assert_eq!(
        licenses,
        [
            (String::from("filed"), Some(String::from("MIT License"))),
            (String::from("stated"), Some(String::from("Apache-2.0"))),
            (
                String::from("tagged"),
                Some(String::from("MIT OR Apache-2.0"))
            ),
            (String::from("unknown"), None),
        ]
    );
}

/// A plugin's search: its entry's `skills`, when it names them, and its
/// licence.
fn plugin(skills: Option<&[&str]>, license: Option<&str>) -> Search {
    Search::Plugin {
        skills: skills.map(|skills| skills.iter().map(|skill| (*skill).to_owned()).collect()),
        license: license.map(str::to_owned),
        dialect: Dialect::Claude,
    }
}

#[test]
fn a_plugin_naming_no_skills_offers_its_skills_folder_and_nothing_else_of_it() {
    let source = repository(&[
        (
            "plugins/ops/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        (
            "plugins/ops/skills/team/review/SKILL.md",
            &skill("review", "Look"),
        ),
        (
            "plugins/ops/agents/helper/SKILL.md",
            &skill("helper", "Not one"),
        ),
        ("plugins/ops/other/SKILL.md", &skill("other", "Not one")),
        ("skills/lint/SKILL.md", &skill("lint", "Another plugin's")),
    ]);

    let gathered = found(
        &Request::git(url(&source))
            .from_subdirectory("plugins/ops")
            .searching(plugin(None, None)),
    );

    assert_eq!(
        gathered,
        named(&[
            ("deploy", "plugins/ops/skills/deploy"),
            ("review", "plugins/ops/skills/team/review"),
        ])
    );
}

#[test]
fn a_plugin_with_a_skill_md_at_its_root_is_that_one_skill() {
    let source = repository(&[
        ("plugins/pdf/SKILL.md", &skill("pdf", "Read a PDF")),
        (
            "plugins/pdf/skills/inner/SKILL.md",
            &skill("inner", "Its own"),
        ),
    ]);

    let gathered = found(
        &Request::git(url(&source))
            .from_subdirectory("plugins/pdf")
            .searching(plugin(None, None)),
    );

    assert_eq!(gathered, named(&[("pdf", "plugins/pdf")]));
}

#[test]
fn the_skills_a_plugin_names_are_its_skills_and_no_others() {
    let source = repository(&[
        ("skills/xlsx/SKILL.md", &skill("xlsx", "Sheets")),
        ("skills/docx/SKILL.md", &skill("docx", "Documents")),
        ("skills/art/SKILL.md", &skill("art", "Another plugin's")),
        ("tools/pdf/SKILL.md", &skill("pdf", "Elsewhere")),
    ]);

    let gathered = found(&Request::git(url(&source)).searching(plugin(
        Some(&[
            "./skills/xlsx",
            "skills/docx/",
            "./tools/pdf",
            "./skills/xlsx",
        ]),
        None,
    )));

    assert_eq!(
        gathered,
        named(&[
            ("docx", "skills/docx"),
            ("pdf", "tools/pdf"),
            ("xlsx", "skills/xlsx"),
        ])
    );
}

#[test]
fn a_skill_a_plugin_names_that_is_no_skill_or_no_folder_of_the_repository_is_said_and_the_rest_taken(
) {
    let source = repository(&[
        ("skills/xlsx/SKILL.md", &skill("xlsx", "Sheets")),
        ("skills/empty/README.md", "no skill here\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).searching(plugin(
            Some(&[
                "./skills/xlsx",
                "./skills/empty",
                "./skills/gone",
                "../../etc",
                "/etc",
            ]),
            None,
        )),
    )
    .unwrap();

    assert_eq!(report.added.len(), 1, "{report:?}");
    let codes: Vec<&str> = report
        .failures
        .iter()
        .map(|failure| failure.code())
        .collect();
    assert_eq!(
        codes,
        [
            "gathered_listed_outside",
            "gathered_listed_outside",
            "gathered_listed_missing",
            "gathered_listed_missing",
        ],
        "{:?}",
        report.failures
    );
}

#[test]
fn a_plugins_licence_is_a_skills_that_says_none_after_its_manifest_and_its_licence_file() {
    let source = repository(&[
        (
            "skills/stated/SKILL.md",
            "---\nname: stated\ndescription: Says it\nlicense: Apache-2.0\n---\n",
        ),
        ("skills/filed/SKILL.md", &skill("filed", "Keeps a file")),
        ("skills/filed/LICENSE", "MIT License\n"),
        ("skills/silent/SKILL.md", &skill("silent", "Says nothing")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).searching(plugin(None, Some("BSD-3-Clause"))),
    )
    .unwrap();

    let licenses: Vec<(String, Option<String>)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|artifact| (artifact.name.to_string(), artifact.license))
        .collect();
    assert_eq!(
        licenses,
        [
            (String::from("filed"), Some(String::from("MIT License"))),
            (String::from("silent"), Some(String::from("BSD-3-Clause"))),
            (String::from("stated"), Some(String::from("Apache-2.0"))),
        ]
    );
}

/// A plugin's search as a catalog in `dialect` lists it, naming no skills.
fn listed_by(dialect: Dialect) -> Search {
    Search::Plugin {
        skills: None,
        license: None,
        dialect,
    }
}

#[test]
fn the_folders_a_plugins_manifest_names_are_searched_beside_its_skills_folder() {
    let source = repository(&[
        (
            "plugins/codex/.codex-plugin/plugin.json",
            r#"{"name": "codex", "skills": "./custom/"}"#,
        ),
        (
            "plugins/codex/skills/plain/SKILL.md",
            &skill("plain", "In skills/"),
        ),
        (
            "plugins/codex/custom/named/SKILL.md",
            &skill("named", "Where its manifest says"),
        ),
        (
            "plugins/codex/other/left/SKILL.md",
            &skill("left", "Nowhere it names"),
        ),
        (
            "plugins/claude/.claude-plugin/plugin.json",
            r#"{"name": "claude", "skills": ["./a", "./b/", "./a"]}"#,
        ),
        ("plugins/claude/a/one/SKILL.md", &skill("one", "In a")),
        ("plugins/claude/b/two/SKILL.md", &skill("two", "In b")),
    ]);

    let codex = found(
        &Request::git(url(&source))
            .from_subdirectory("plugins/codex")
            .searching(listed_by(Dialect::Codex)),
    );
    let claude = found(
        &Request::git(url(&source))
            .from_subdirectory("plugins/claude")
            .searching(listed_by(Dialect::Claude)),
    );

    assert_eq!(
        codex,
        named(&[
            ("named", "plugins/codex/custom/named"),
            ("plain", "plugins/codex/skills/plain"),
        ])
    );
    assert_eq!(
        claude,
        named(&[
            ("one", "plugins/claude/a/one"),
            ("two", "plugins/claude/b/two")
        ])
    );
}

#[test]
fn a_folder_a_plugins_manifest_names_out_of_the_repository_is_said_and_the_rest_taken() {
    let source = repository(&[
        (
            ".codex-plugin/plugin.json",
            r#"{"skills": ["../../etc", "/etc", "./skills"]}"#,
        ),
        ("skills/kept/SKILL.md", &skill("kept", "Kept")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).searching(listed_by(Dialect::Codex)),
    )
    .unwrap();

    assert_eq!(report.added.len(), 1, "{report:?}");
    let codes: Vec<&str> = report
        .failures
        .iter()
        .map(|failure| failure.code())
        .collect();
    assert_eq!(
        codes,
        ["gathered_manifest_outside", "gathered_manifest_outside"]
    );
}

#[test]
fn a_plugins_manifest_that_cannot_be_read_is_said_and_its_plugin_searched_as_if_it_had_none() {
    let source = repository(&[
        (".codex-plugin/plugin.json", "{\"name\": \"broken\""),
        ("skills/kept/SKILL.md", &skill("kept", "Kept")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).searching(listed_by(Dialect::Codex)),
    )
    .unwrap();

    assert_eq!(report.added.len(), 1, "{report:?}");
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].code(), "gathered_manifest_unreadable");
    assert!(report.failures[0]
        .path()
        .ends_with(".codex-plugin/plugin.json"));
}

#[test]
fn a_plugins_licence_is_its_manifests_before_its_catalogs() {
    let source = repository(&[
        (".claude-plugin/plugin.json", r#"{"license": "MIT"}"#),
        ("skills/silent/SKILL.md", &skill("silent", "Says nothing")),
        (
            "skills/stated/SKILL.md",
            "---\nname: stated\ndescription: Says it\nlicense: Apache-2.0\n---\n",
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).searching(plugin(None, Some("BSD-3-Clause"))),
    )
    .unwrap();

    let licenses: Vec<(String, Option<String>)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|artifact| (artifact.name.to_string(), artifact.license))
        .collect();
    assert_eq!(
        licenses,
        [
            (String::from("silent"), Some(String::from("MIT"))),
            (String::from("stated"), Some(String::from("Apache-2.0"))),
        ]
    );
}

#[test]
fn a_repository_gathered_whole_is_searched_as_it_was_whatever_agents_manifest_it_holds() {
    let source = repository(&[
        (".codex-plugin/plugin.json", r#"{"skills": "./nowhere"}"#),
        (".claude-plugin/plugin.json", r#"{"skills": 7}"#),
        ("skills/kept/SKILL.md", &skill("kept", "Kept")),
        ("tools/also/SKILL.md", &skill("also", "Found by its layout")),
    ]);

    let gathered = found(&Request::git(url(&source)));

    assert_eq!(
        gathered,
        named(&[("also", "tools/also"), ("kept", "skills/kept")])
    );
}

#[test]
fn a_folder_the_source_does_not_have_is_named_in_the_error() {
    let source = repository(&[("README.md", "nothing to gather\n")]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source)).from_subdirectory("skills");

    let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

    assert!(matches!(error, GatherError::NoSuchFolder { .. }), "{error}");
    assert!(error.to_string().contains("skills"));
}

#[test]
fn a_gather_that_finds_no_folder_leaves_the_revision_its_shelf_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source)).from_subdirectory("skills");
    gather::gather(&workspace, &ledger, &request).unwrap();
    let before = ledger
        .source(SourceKind::Git, &url(&source), None, "skills")
        .unwrap();

    fs::remove_dir_all(source.path().join("skills")).unwrap();
    write_files(source.path(), &[("README.md", "moved elsewhere\n")]);
    commit(source.path());
    let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

    assert!(matches!(error, GatherError::NoSuchFolder { .. }), "{error}");
    let after = ledger
        .source(SourceKind::Git, &url(&source), None, "skills")
        .unwrap();
    assert_eq!(
        after.map(|source| source.revision),
        before.map(|source| source.revision),
        "the shelf still holds what that revision gave it"
    );
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
        .source(SourceKind::Git, &address, None, DEFAULT_SUBDIRECTORY)
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
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    let ledger = workspace.ledger().unwrap();

    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let skills = project.directory_for(Kind::Skill).unwrap();
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
fn a_gather_reports_reading_every_folder_before_placing_any() {
    let source = repository(&[
        ("skills/broken/SKILL.md", "no front matter at all\n"),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let (watch, seen) = recording();

    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap();

    let seen = seen.lock().unwrap();
    // What `gix` counts while it fetches depends on how fast the clone is,
    // so only the steps every gather says are compared whole.
    let said: Vec<Progress> = seen
        .iter()
        .copied()
        .filter(|progress| {
            matches!(
                progress,
                Progress::Connecting
                    | Progress::CheckingOut { total: None, .. }
                    | Progress::Harvesting { .. }
                    | Progress::Placing { .. }
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            Progress::Connecting,
            Progress::CheckingOut {
                done: 0,
                total: None
            },
            Progress::Harvesting { done: 0, total: 2 },
            Progress::Harvesting { done: 1, total: 2 },
            Progress::Harvesting { done: 2, total: 2 },
            // The broken one was read, and is not placed.
            Progress::Placing { done: 0, total: 1 },
            Progress::Placing { done: 1, total: 1 },
        ]
    );
    // And what it does count is said while it fetches, never after.
    let reading = seen
        .iter()
        .position(|progress| matches!(progress, Progress::Harvesting { .. }))
        .unwrap();
    assert!(seen[reading..].iter().all(|progress| matches!(
        progress,
        Progress::Harvesting { .. } | Progress::Placing { .. }
    )));
}

#[test]
fn a_flag_set_before_checkout_leaves_no_shelf_change_and_no_ledger_rows() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch =
        cancelled_at(|progress| matches!(progress, Progress::CheckingOut { total: None, .. }));

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert!(!workspace.gathered_dir(Kind::Skill).exists(), "no shelf");
    assert!(ledger.gathered().unwrap().is_empty());
    assert!(ledger.history(10).unwrap().is_empty(), "nothing logged");
    assert_eq!(
        ledger
            .source(SourceKind::Git, &url(&source), None, DEFAULT_SUBDIRECTORY)
            .unwrap(),
        None,
        "the source it registered is forgotten"
    );
    // And the clone it had begun is gone with it.
    assert_eq!(fs::read_dir(workspace.cache_dir()).unwrap().count(), 0);
}

#[test]
fn a_cancelled_gather_of_a_known_source_leaves_its_shelf_and_rows_as_they_were() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    let before = fs::read_to_string(shelf.join("deploy/SKILL.md")).unwrap();
    let recorded = ledger
        .source(SourceKind::Git, &url(&source), None, DEFAULT_SUBDIRECTORY)
        .unwrap();

    write_files(
        source.path(),
        &[
            (
                "skills/deploy/SKILL.md",
                &skill("deploy", "Ship it, carefully"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Look first")),
        ],
    );
    commit(source.path());
    // As late as a cancel is honoured: every folder read, nothing placed.
    let watch = cancelled_at(
        |progress| matches!(progress, Progress::Harvesting { done, total } if done == total),
    );
    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert_eq!(
        fs::read_to_string(shelf.join("deploy/SKILL.md")).unwrap(),
        before
    );
    assert!(!shelf.join("review").exists());
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].summary.as_deref(), Some("Ship it"));
    assert_eq!(
        ledger
            .source(SourceKind::Git, &url(&source), None, DEFAULT_SUBDIRECTORY)
            .unwrap(),
        recorded,
        "the revision it was gathered at is the one it still has"
    );
    assert_eq!(ledger.history(10).unwrap().len(), 1, "the first gather");
}

#[test]
fn a_gather_cancelled_before_it_starts_fetches_nothing() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let (watch, seen) = recording();
    watch.cancel();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert!(seen.lock().unwrap().is_empty(), "not even a connection");
    assert!(!workspace.cache_dir().exists());
    assert!(ledger.history(10).unwrap().is_empty());
}

#[test]
fn a_cancel_once_placing_has_begun_comes_too_late_to_stop_it() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Look first")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch = cancelled_at(|progress| matches!(progress, Progress::Placing { done: 0, .. }));

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap();

    // Half a shelf would be worse than a whole one.
    assert_eq!(report.added.len(), 2);
    assert_eq!(ledger.gathered().unwrap().len(), 2);
}

#[test]
fn a_cancel_stops_a_gather_at_every_step_said_to_be_cancellable_and_no_other() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Look first")),
    ]);
    let (watch, seen) = recording();
    {
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).watched_by(watch),
        )
        .unwrap();
    }
    // Only the steps every gather says: `gix`'s counts depend on how fast
    // the clone is, so a cancel at one of them could not be made to land.
    let steps: Vec<Progress> = seen
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|progress| {
            !matches!(
                progress,
                Progress::Receiving { .. }
                    | Progress::Resolving { .. }
                    | Progress::CheckingOut { total: Some(_), .. }
            )
        })
        .collect();
    assert!(steps.iter().any(|step| !step.cancellable()), "{steps:?}");

    for step in steps {
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();
        let outcome = gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).watched_by(cancelled_at(move |progress| progress == step)),
        );

        assert_eq!(
            matches!(outcome, Err(GatherError::Cancelled)),
            step.cancellable(),
            "cancelled at {step:?}: {outcome:?}"
        );
    }
}

#[test]
fn the_ledger_lands_beside_the_marker_file() {
    let (dir, workspace) = workspace();
    let ledger = Ledger::open(workspace.ledger_path()).unwrap();
    drop(ledger);

    assert!(dir
        .path()
        .join(".mindweaver")
        .join("mindweaver.db")
        .is_file());
}

#[test]
fn a_url_with_a_token_in_it_is_refused_and_the_token_is_kept_nowhere() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git("https://me:ghp_t0ken@example.com/acme/skills"),
    )
    .unwrap_err();

    assert!(
        matches!(error, GatherError::CredentialsInUrl { .. }),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills: a password, a token or, over http(s), a user name in \
         the URL is refused, because gathering stores and shows the URL: gather it without one, \
         and configure a git credential helper that has it"
    );
    // Nothing was registered, and nothing was fetched into the cache.
    assert!(ledger
        .source(
            SourceKind::Git,
            "https://example.com/acme/skills",
            None,
            DEFAULT_SUBDIRECTORY
        )
        .unwrap()
        .is_none());
    assert!(!workspace.cache_dir().exists());
    // The attempt is logged, as every failed gather is, under the URL it was
    // refused for.
    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].target.as_deref(),
        Some("https://example.com/acme/skills")
    );
    // And the token is nowhere in the database file, whatever reads it.
    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(9).any(|bytes| bytes == b"ghp_t0ken"));
}

#[test]
fn a_password_cut_by_a_slash_is_refused_too_and_kept_nowhere() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    // No parser reads this as a URL with a password in it, which is why it
    // has to be caught before anything parses it.
    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git("https://me:p/t0ken@example.invalid/acme/skills"),
    )
    .unwrap_err();

    assert!(
        matches!(error, GatherError::CredentialsInUrl { .. }),
        "{error}"
    );
    assert!(!error.to_string().contains("t0ken"), "{error}");
    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(5).any(|bytes| bytes == b"t0ken"));
    assert!(!workspace.cache_dir().exists());
}

/// Leave in `workspace` what a Mindweaver from before the refusal left after
/// gathering from `url` with a token in it: the URL in the source the clone
/// in `directory` belongs to, in the log, and in that clone's config and
/// reflog.
fn left_by_an_older_build(workspace: &WeaverWorkspace, directory: &str, url: &str) {
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute("UPDATE sources SET url = ?1", [url])
        .unwrap();
    database
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (0, 'gather', ?1, 'failed', ?2)",
            [url, &format!("{url}: cannot be fetched")],
        )
        .unwrap();

    let git = workspace.cache_dir().join(directory).join(".git");
    let config = fs::read_to_string(git.join("config")).unwrap();
    let config: String = config
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("url = ") {
                format!("\turl = {url}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(git.join("config"), config).unwrap();
    let mut reflog = fs::read_to_string(git.join("logs").join("HEAD")).unwrap();
    reflog.push_str(&format!(
        "0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"
    ));
    fs::write(git.join("logs").join("HEAD"), reflog).unwrap();
}

/// Every file under `root` that reads as text, joined, for asserting on.
fn text_under(root: &std::path::Path) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            text.push_str(&text_under(&path));
        } else if let Ok(contents) = fs::read_to_string(&path) {
            text.push_str(&contents);
        }
    }
    text
}

#[test]
fn what_an_older_build_stored_with_a_token_is_scrubbed_once_from_the_ledger_and_the_clone() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    left_by_an_older_build(
        &workspace,
        &report.directory,
        "https://me:ghp_t0ken@example.com/acme/skills",
    );

    // Opened the way a command that writes opens it.
    let (ledger, scrub) = workspace.ledger_to_write().unwrap();

    assert_eq!(scrub.sources.len(), 1, "{:?}", scrub.sources);
    assert_eq!(scrub.sources[0].url, "https://example.com/acme/skills");
    assert_eq!(scrub.actions, 1);
    assert!(scrub.failures.is_empty(), "{:?}", scrub.failures);

    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(9).any(|bytes| bytes == b"ghp_t0ken"));
    let clone = text_under(&workspace.cache_dir().join(&report.directory).join(".git"));
    assert!(!clone.contains("ghp_t0ken"));
    assert!(clone.contains("url = https://example.com/acme/skills\n"));
    // What was gathered is still there, under the URL without the token.
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].source.url, "https://example.com/acme/skills");

    // Said once: the second time there is nothing left to say.
    assert!(workspace.ledger_to_write().unwrap().1.is_empty());
}

#[test]
fn one_url_scrubbed_out_of_several_sources_is_told_about_once() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    drop(ledger);
    // Two branches and another folder of one repository, all gathered with
    // the same token: one credential, to be given to a helper once.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(
            "INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen,
                                  last_seen, name, canonical_url, reference_key)
                 VALUES ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'main', 'skills',
                         'a', 0, 0, 'a', 'https://example.com/acme/skills', 'main'),
                        ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'dev', 'skills',
                         'b', 0, 0, 'b', 'https://example.com/acme/skills', 'dev'),
                        ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'main', 'more',
                         'c', 0, 0, 'c', 'https://example.com/acme/skills', 'main');",
        )
        .unwrap();

    let (ledger, scrub) = workspace.ledger_to_write().unwrap();

    let told: Vec<&str> = scrub
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(told, ["https://example.com/acme/skills"]);
    assert_eq!(ledger.tainted_sources().unwrap(), []);
}

/// Fetches the repository at `from` with gix whatever URL it is asked for,
/// as a forge would answer each spelling of one repository's: what a test
/// needs to gather `https://github.com/...` with no network.
#[derive(Debug)]
struct Forge {
    from: String,
    asked: Mutex<Vec<String>>,
}

impl mindweaver_core::Remote for Forge {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        pin: Option<&mindweaver_core::value::Revision>,
        into: &std::path::Path,
        watch: &Watch,
    ) -> Result<mindweaver_core::port::Cloned, mindweaver_core::port::GitError> {
        self.asked.lock().unwrap().push(url.to_owned());
        mindweaver_core::adapter::gix::Gix.fetch(&self.from, reference, pin, into, watch)
    }

    fn scrub(&self, clone: &std::path::Path) -> Result<bool, mindweaver_core::port::GitError> {
        mindweaver_core::adapter::gix::Gix.scrub(clone)
    }
}

#[test]
fn two_gathers_of_one_repository_by_two_spellings_update_one_source() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forge = Arc::new(Forge {
        from: url(&source),
        asked: Mutex::new(Vec::new()),
    });
    let gathered = |typed: &str| {
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(typed).through(Arc::clone(&forge) as Arc<dyn mindweaver_core::Remote>),
        )
        .unwrap()
    };

    let first = gathered("https://github.com/acme/skills");
    let second = gathered("git@github.com:Acme/skills.git");
    let third = gathered("acme/skills");

    // One source, one folder: the second gather found what the first took,
    // where the first put it, and so did the third.
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].name, "acme-skills");
    assert_eq!(sources[0].url, "https://github.com/acme/skills");
    assert_eq!(second.directory, first.directory);
    assert_eq!(third.directory, first.directory);
    assert_eq!(first.added.len(), 1, "{first:?}");
    assert_eq!(second.unchanged.len(), 1, "{second:?}");
    assert_eq!(third.unchanged.len(), 1, "{third:?}");
    assert_eq!(ledger.gathered().unwrap().len(), 1);
    // Each fetched as it was typed, `owner/repo` as the GitHub repository
    // it names.
    assert_eq!(
        *forge.asked.lock().unwrap(),
        [
            "https://github.com/acme/skills",
            "git@github.com:Acme/skills.git",
            "https://github.com/acme/skills",
        ]
    );
}

// Unix's alone: a Windows path is no file URL with `file://` before it.
#[cfg(unix)]
#[test]
fn two_gathers_of_a_folder_by_its_path_and_its_file_url_update_one_source() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let path = url(&source);

    gather::gather(&workspace, &ledger, &Request::git(path.clone())).unwrap();
    let again = gather::gather(
        &workspace,
        &ledger,
        &Request::git(format!("file://{path}/")),
    )
    .unwrap();

    assert_eq!(ledger.sources().unwrap().len(), 1);
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
    assert!(again.added.is_empty() && again.updated.is_empty());
}

#[test]
fn a_gather_names_its_source_as_asked_and_a_later_one_renames_it() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let first = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).named(Some("team-skills")),
    )
    .unwrap();
    let again = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let renamed = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).named(Some("Team.Skills_2")),
    )
    .unwrap();

    assert_eq!(first.name, "team-skills");
    // A gather that names none keeps the name the source has.
    assert_eq!(again.name, "team-skills");
    assert_eq!(renamed.name, "Team.Skills_2");
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].name, "Team.Skills_2");
    assert_eq!(renamed.directory, first.directory, "its folder stays");
}

#[test]
fn a_name_another_source_has_is_refused_before_anything_is_registered_or_fetched() {
    let one = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let other = repository(&[("skills/review/SKILL.md", &skill("review", "Look"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let named = |source: &TempDir| Request::git(url(source)).named(Some("skills"));
    gather::gather(&workspace, &ledger, &named(&one)).unwrap();

    let refused = gather::gather(&workspace, &ledger, &named(&other)).unwrap_err();
    // Nor may a source gathered before take it.
    gather::gather(&workspace, &ledger, &Request::git(url(&other))).unwrap();
    let renaming = gather::gather(&workspace, &ledger, &named(&other)).unwrap_err();

    for refused in [&refused, &renaming] {
        assert_eq!(refused.code(), "source_name_taken", "{refused}");
        assert_eq!(
            refused.to_string(),
            format!(
                "`skills` is the name of the source at {} already: gather this one under \
                 another `--name`",
                url(&one)
            )
        );
    }
    let names: Vec<(String, String)> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| (source.url.to_string(), source.name.to_string()))
        .collect();
    assert!(
        names.contains(&(url(&one), String::from("skills"))),
        "{names:?}"
    );
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn a_name_no_source_can_take_is_refused_and_nothing_registered() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    for name in ["acme/skills", "workspace", "-skills", "", "..", "load:team"] {
        let refused = gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).named(Some(name)),
        )
        .unwrap_err();

        assert_eq!(refused.code(), "source_name_invalid", "{name}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    assert!(!workspace.cache_dir().exists());
}

#[test]
fn a_cancelled_gather_leaves_its_source_called_what_it_was() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let watch = cancelled_at(|progress| matches!(progress, Progress::Connecting));

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source))
            .named(Some("renamed"))
            .watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert_eq!(ledger.sources().unwrap()[0].name, first.name);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A watch that keeps every step it is told, and where they are kept.
fn recording() -> (Watch, Arc<Mutex<Vec<Progress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });
    (watch, seen)
}

/// A watch that cancels itself once it is told a step `at` matches.
fn cancelled_at(at: impl Fn(Progress) -> bool + Send + Sync + 'static) -> Watch {
    let watch = Watch::new();
    let cancel = watch.clone();
    watch.reporting_to(move |progress| {
        if at(progress) {
            cancel.cancel();
        }
    })
}

/// What a source to gather from serializes to is a contract: its kind as the
/// ledger files it, its URL without any credential, and the branch asked for.
/// A request is built before a URL with a credential in it is refused, so
/// the URL is taken out of what is printed whatever it holds.
#[test]
fn a_source_to_gather_from_serializes_without_the_credentials_its_url_carries() {
    let typed = Request::git("https://me:s3cret@github.com/acme/skills.git").at(Some("main"));

    let serialized = serde_json::to_string(&typed.source).unwrap();

    assert_eq!(
        serialized,
        r#"{"kind":"git","url":"https://github.com/acme/skills.git","reference":"main"}"#
    );
}

/// What a gather's report serializes to is a contract, as a status report's
/// is. What could not be taken, and what is wrong with what was, stay out:
/// a front end says them as its warnings, which in JSON are the envelope's.
#[test]
fn a_gather_report_serializes_what_changed_and_leaves_its_failures_to_the_warnings() {
    let harvested = |name: &str| gather::Harvested {
        name: name.to_owned().into(),
        directory: name.to_owned(),
        summary: Some(format!("About {name}")),
        path: std::path::PathBuf::from(format!(
            "/work/.mindweaver/skills/github.com-acme-skills/{name}"
        )),
    };
    let report = gather::Report {
        revision: Some("4f9c2a7".into()),
        directory: String::from("github.com-acme-skills"),
        added: vec![harvested("deploy")],
        updated: Vec::new(),
        unchanged: vec![harvested("review")],
        failures: vec![gather::Failure::UnusableName {
            path: std::path::PathBuf::from("/work/.mindweaver/cache/x"),
        }],
        warnings: vec![Warning {
            path: std::path::PathBuf::from("/work/SKILL.md"),
            issue: ValidationIssue::MetadataNotAMap,
        }],
        name: SourceName::parse("acme-skills").unwrap(),
        removed: vec![harvested("ship")],
    };

    let serialized = serde_json::to_string(&report).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"revision":"4f9c2a7","directory":"github.com-acme-skills","#,
            r#""added":[{"name":"deploy","directory":"deploy","summary":"About deploy","#,
            r#""path":"/work/.mindweaver/skills/github.com-acme-skills/deploy"}],"#,
            r#""updated":[],"#,
            r#""unchanged":[{"name":"review","directory":"review","summary":"About review","#,
            r#""path":"/work/.mindweaver/skills/github.com-acme-skills/review"}],"#,
            r#""name":"acme-skills","#,
            r#""removed":[{"name":"ship","directory":"ship","summary":"About ship","#,
            r#""path":"/work/.mindweaver/skills/github.com-acme-skills/ship"}]}"#,
        )
    );
}

// ---------------------------------------------------------------------------
// A dry run
// ---------------------------------------------------------------------------

/// The commit `source` is at.
fn head(source: &TempDir) -> String {
    gix::open(source.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string()
}

#[test]
fn a_dry_run_lists_what_a_gather_would_take_and_writes_nothing() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (
            ".claude/skills/review/SKILL.md",
            &skill("review", "Read a change"),
        ),
        (
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in forms"),
        ),
        ("skills/broken/SKILL.md", "---\nname: [unclosed\n---\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let preview = gather::preview(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let found: Vec<(&str, &str, &str)> = preview
        .found
        .iter()
        .map(|it| {
            (
                it.name.as_str(),
                it.directory.as_str(),
                it.upstream_path.as_str(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("review", "review", ".claude/skills/review"),
            ("deploy", "deploy", "skills/deploy"),
            ("pdf--forms", "pdf--forms", "skills/pdf--forms"),
        ]
    );
    assert_eq!(preview.revision.as_deref(), Some(head(&source).as_str()));
    assert!(!preview.warnings.is_empty());
    assert!(preview
        .warnings
        .iter()
        .all(|warning| warning.path == std::path::Path::new("skills/pdf--forms/SKILL.md")));
    assert_eq!(preview.failures.len(), 1, "{:?}", preview.failures);
    // Where it is in the repository, as the clone it was read in is gone.
    assert_eq!(
        preview.failures[0].path(),
        std::path::Path::new("skills/broken/SKILL.md")
    );
    // Nothing registered, shelved, recorded or logged, and no clone kept.
    assert_eq!(ledger.sources().unwrap(), []);
    assert!(ledger.gathered().unwrap().is_empty());
    assert!(ledger.history(10).unwrap().is_empty());
    assert!(!workspace.gathered_dir(Kind::Skill).exists());
    assert!(!workspace.cache_dir().exists(), "the clone is taken away");
}

#[test]
fn a_dry_run_of_a_source_gathered_before_leaves_its_clone_its_shelf_and_its_rows() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    let first = gather::gather(&workspace, &ledger, &request).unwrap();
    let before = (ledger.sources().unwrap(), ledger.gathered().unwrap());

    write_files(
        source.path(),
        &[
            (
                "skills/deploy/SKILL.md",
                &skill("deploy", "Ship it, carefully"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Read a change")),
        ],
    );
    commit(source.path());
    let preview = gather::preview(&workspace, &ledger, &request).unwrap();

    assert_eq!(preview.revision.as_deref(), Some(head(&source).as_str()));
    assert_eq!(preview.found.len(), 2, "{preview:?}");
    assert_eq!(
        (ledger.sources().unwrap(), ledger.gathered().unwrap()),
        before
    );
    assert_eq!(ledger.history(10).unwrap().len(), 1, "the first gather");
    let clones: Vec<String> = fs::read_dir(workspace.cache_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(clones, std::slice::from_ref(&first.directory));
    let cloned = workspace.cache_dir().join(&first.directory);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    for kept in [cloned.join("skills"), shelf] {
        assert_eq!(
            fs::read_to_string(kept.join("deploy/SKILL.md")).unwrap(),
            skill("deploy", "Ship it"),
            "{}",
            kept.display()
        );
        assert!(!kept.join("review").exists());
    }
}

#[test]
fn a_dry_run_in_a_workspace_nobody_has_made_makes_no_folder_of_it() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let home = TempDir::new().unwrap();
    let workspace = WeaverWorkspace::unmade(home.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();

    let preview = gather::preview(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(preview.found.len(), 1);
    assert_eq!(
        fs::read_dir(home.path()).unwrap().count(),
        0,
        "{}",
        workspace.weaver_dir().display()
    );
}

#[test]
fn a_dry_run_is_refused_for_what_a_gather_is_refused_for_before_it_fetches() {
    let one = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let other = repository(&[("skills/review/SKILL.md", &skill("review", "Look"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&one)).named(Some("team")),
    )
    .unwrap();

    let taken = gather::preview(
        &workspace,
        &ledger,
        &Request::git(url(&other)).named(Some("team")),
    )
    .unwrap_err();
    let pinned = gather::preview(
        &workspace,
        &ledger,
        &Request::git(url(&other)).pinned(Some("4f9c2a7")),
    )
    .unwrap_err();

    assert_eq!(taken.code(), "source_name_taken");
    assert_eq!(pinned.code(), "gather_pin_invalid");
    assert_eq!(ledger.sources().unwrap().len(), 1);
}

#[test]
fn a_dry_run_cancelled_before_it_starts_fetches_nothing() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch = Watch::new();
    watch.cancel();

    let error = gather::preview(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert!(!workspace.cache_dir().exists());
}

#[test]
fn a_dry_run_serializes_what_it_found_and_leaves_its_failures_to_the_warnings() {
    let preview = gather::Preview {
        revision: Some(mindweaver_core::value::Revision::from("4f9c2a7")),
        found: vec![gather::Previewed {
            name: mindweaver_core::value::ArtifactName::from("deploy"),
            directory: String::from("deploy"),
            upstream_path: String::from("skills/team/deploy"),
            summary: Some(String::from("Ship it")),
        }],
        failures: vec![gather::Failure::UnusableName {
            path: std::path::PathBuf::from("/work/.mindweaver/cache/x"),
        }],
        warnings: vec![Warning {
            path: std::path::PathBuf::from("skills/team/deploy/SKILL.md"),
            issue: ValidationIssue::MetadataNotAMap,
        }],
    };

    let serialized = serde_json::to_string(&preview).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"revision":"4f9c2a7","found":[{"name":"deploy","directory":"deploy","#,
            r#""upstream_path":"skills/team/deploy","summary":"Ship it"}]}"#,
        )
    );
}
