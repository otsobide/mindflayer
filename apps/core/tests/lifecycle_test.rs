//! Finding, renaming and removing what a project already holds.

use std::fs;
use std::path::Path;

use mindflayer_core::lifecycle::{self, Target};
use mindflayer_core::{create, Kind, LifecycleError, MindProject, Reference};
use tempfile::TempDir;

fn project() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    (dir, project)
}

fn write(root: &Path, route: &str, contents: &str) {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn find(project: &MindProject, typed: &str) -> Result<Target, LifecycleError> {
    lifecycle::find(project, &Reference::parse(typed))
}

// ---------------------------------------------------------------------------
// Finding
// ---------------------------------------------------------------------------

#[test]
fn an_artifact_is_found_by_what_it_declares() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let target = find(&project, "deploy").unwrap();

    assert_eq!(target.kind, Kind::Skill);
    assert_eq!(target.name, "deploy");
    assert!(target.artifact.is_some());
    assert_eq!(target.file(), dir.path().join("skills/deploy/SKILL.md"));
}

#[test]
fn a_skill_that_does_not_read_is_still_found_by_its_folder() {
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/broken/SKILL.md",
        "no front matter at all\n",
    );

    let target = find(&project, "broken").unwrap();

    // Left out of every listing, but it is the one that most needs opening.
    assert!(target.artifact.is_none());
    assert_eq!(target.file(), dir.path().join("skills/broken/SKILL.md"));
}

#[test]
fn one_name_for_two_kinds_has_to_be_qualified() {
    let (_dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    create(&project, Kind::Rule, "deploy", "How we ship").unwrap();

    let error = find(&project, "deploy").unwrap_err();

    assert!(matches!(error, LifecycleError::Ambiguous { .. }), "{error}");
    assert!(error.to_string().contains("skill:deploy"), "{error}");
    assert!(error.to_string().contains("rule:deploy"), "{error}");
    assert_eq!(find(&project, "rule:deploy").unwrap().kind, Kind::Rule);
}

#[test]
fn a_name_that_climbs_out_is_never_turned_into_a_place() {
    let outer = TempDir::new().unwrap();
    let root = outer.path().join("project");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    write(
        outer.path(),
        "outside/SKILL.md",
        "---\nname: outside\ndescription: x\n---\n",
    );

    let error = find(&project, "../outside").unwrap_err();

    assert!(matches!(error, LifecycleError::NotFound { .. }), "{error}");
}

#[test]
fn a_file_that_cannot_be_read_is_identified_by_where_it_sits() {
    let (dir, project) = project();
    let root = dir.path();

    assert_eq!(
        lifecycle::identify(&project, &root.join("skills/broken/SKILL.md")),
        Some((Kind::Skill, String::from("broken")))
    );
    assert_eq!(
        lifecycle::identify(&project, &root.join("rules/git/no-force-push.md")),
        Some((Kind::Rule, String::from("git/no-force-push")))
    );
    // A skill's own files are the skill's, not artifacts of their own.
    assert_eq!(
        lifecycle::identify(&project, &root.join("skills/deploy/scripts/run.sh")),
        None
    );
    assert_eq!(lifecycle::identify(&project, &root.join("README.md")), None);
}

// ---------------------------------------------------------------------------
// Removing
// ---------------------------------------------------------------------------

#[test]
fn removing_a_skill_takes_its_whole_folder() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    write(dir.path(), "skills/deploy/scripts/run.sh", "echo ship\n");
    let target = find(&project, "deploy").unwrap();
    assert_eq!(lifecycle::files(&target), 2);

    lifecycle::remove(&project, &target).unwrap();

    assert!(!dir.path().join("skills/deploy").exists());
    assert!(dir.path().join("skills").is_dir());
}

#[test]
fn removing_a_rule_takes_the_folders_it_leaves_empty_and_no_more() {
    let (dir, project) = project();
    create(&project, Kind::Rule, "git/no-force-push", "Never").unwrap();
    create(&project, Kind::Rule, "ci/green", "Keep it green").unwrap();
    create(&project, Kind::Rule, "ci/fast", "Keep it fast").unwrap();

    lifecycle::remove(&project, &find(&project, "git/no-force-push").unwrap()).unwrap();
    lifecycle::remove(&project, &find(&project, "ci/green").unwrap()).unwrap();

    assert!(
        !dir.path().join("rules/git").exists(),
        "left empty, so gone"
    );
    assert!(
        dir.path().join("rules/ci/fast.md").is_file(),
        "still grouping one"
    );
    assert!(
        dir.path().join("rules").is_dir(),
        "the kind's own folder stays"
    );
}

// ---------------------------------------------------------------------------
// Renaming
// ---------------------------------------------------------------------------

const WRITTEN_BY_HAND: &str = "---\n# who owns this: platform\nname: deploy\ndescription: >\n  Ship the service\n  to staging.\nallowed-tools:\n  - Bash\n  - Read\n---\n\n# Deploying\n\nRun the pipeline.\n";

#[test]
fn renaming_a_skill_moves_its_folder_and_rewrites_nothing_but_its_name() {
    let (dir, project) = project();
    write(dir.path(), "skills/deploy/SKILL.md", WRITTEN_BY_HAND);
    write(dir.path(), "skills/deploy/scripts/run.sh", "echo ship\n");

    let renamed = lifecycle::rename(&project, &find(&project, "deploy").unwrap(), "ship").unwrap();

    assert_eq!(renamed.name, "ship");
    assert!(!dir.path().join("skills/deploy").exists());
    assert!(dir.path().join("skills/ship/scripts/run.sh").is_file());
    let manifest = fs::read_to_string(dir.path().join("skills/ship/SKILL.md")).unwrap();
    assert_eq!(
        manifest,
        WRITTEN_BY_HAND.replace("name: deploy\n", "name: ship\n")
    );
    assert_eq!(renamed.artifact.unwrap().validate(), vec![]);
}

#[test]
fn a_name_yaml_would_read_as_something_else_is_quoted() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let renamed = lifecycle::rename(&project, &find(&project, "deploy").unwrap(), "true").unwrap();

    assert_eq!(renamed.name, "true");
    let manifest = fs::read_to_string(dir.path().join("skills/true/SKILL.md")).unwrap();
    assert!(manifest.contains("name: 'true'"), "{manifest}");
}

#[test]
fn renaming_refuses_whatever_would_break_or_clobber_something() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    create(&project, Kind::Skill, "ship", "Already here").unwrap();
    let deploy = find(&project, "deploy").unwrap();

    let refusals = [
        lifecycle::rename(&project, &deploy, "Bad Name").unwrap_err(),
        lifecycle::rename(&project, &deploy, "tools/deploy").unwrap_err(),
        lifecycle::rename(&project, &deploy, "ship").unwrap_err(),
        lifecycle::rename(&project, &deploy, "deploy").unwrap_err(),
    ];

    assert!(matches!(refusals[0], LifecycleError::Name { .. }));
    assert!(matches!(refusals[1], LifecycleError::Nested { .. }));
    assert!(matches!(refusals[2], LifecycleError::Exists { .. }));
    assert!(matches!(refusals[3], LifecycleError::Unchanged { .. }));
    // And every one of them left both skills where they were.
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
    let kept = fs::read_to_string(dir.path().join("skills/ship/SKILL.md")).unwrap();
    assert!(kept.contains("Already here"));
}

#[test]
fn a_skill_whose_name_is_not_one_line_is_left_for_a_person() {
    let (dir, project) = project();
    let written = "---\nname: >-\n  deploy\ndescription: Ship it\n---\n";
    write(dir.path(), "skills/deploy/SKILL.md", written);

    let error =
        lifecycle::rename(&project, &find(&project, "deploy").unwrap(), "ship").unwrap_err();

    assert!(matches!(error, LifecycleError::Rewrite { .. }), "{error}");
    assert!(!dir.path().join("skills/ship").exists(), "nothing moved");
    assert_eq!(
        fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap(),
        written
    );
}

#[test]
fn a_skill_that_does_not_read_is_edited_before_it_is_renamed() {
    let (dir, project) = project();
    write(dir.path(), "skills/broken/SKILL.md", "no front matter\n");

    let error =
        lifecycle::rename(&project, &find(&project, "broken").unwrap(), "fixed").unwrap_err();

    assert!(
        matches!(error, LifecycleError::Unreadable { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("mind edit"), "{error}");
}

#[test]
fn renaming_puts_a_folder_back_in_line_with_what_it_declares() {
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/deploy/SKILL.md",
        "---\nname: deployment\ndescription: Ship it\n---\n",
    );
    let target = find(&project, "deploy").unwrap();
    assert_eq!(
        target.name, "deployment",
        "found by folder, named by itself"
    );

    let renamed = lifecycle::rename(&project, &target, "deployment").unwrap();

    assert!(dir.path().join("skills/deployment/SKILL.md").is_file());
    assert_eq!(renamed.artifact.unwrap().validate(), vec![]);
}

#[test]
fn renaming_a_rule_refiles_it_by_its_new_route() {
    let (dir, project) = project();
    create(
        &project,
        Kind::Rule,
        "git/no-force-push",
        "Never force-push",
    )
    .unwrap();

    let renamed = lifecycle::rename(
        &project,
        &find(&project, "git/no-force-push").unwrap(),
        "vcs/no-force-push",
    )
    .unwrap();

    assert_eq!(renamed.name, "vcs/no-force-push");
    assert!(
        !dir.path().join("rules/git").exists(),
        "left empty, so gone"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("rules/vcs/no-force-push.md")).unwrap(),
        "# Never force-push\n"
    );
}
