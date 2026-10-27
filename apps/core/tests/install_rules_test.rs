//! Installing rules: offered by the route they are filed under, copied into
//! a project a `mind.toml` marks, and never over what somebody wrote.

mod common;

use std::fs;
use std::path::PathBuf;

use mindflayer_core::install::{
    self, InstallError, Installed, Removed, Standing, WORKSPACE_ORIGIN,
};
use mindflayer_core::ledger::{InstallRecord, Target};
use mindflayer_core::{hash, AgentHomes, FlayerWorkspace, Kind, Ledger, MindProject};
use tempfile::TempDir;

/// A rule about every file.
const NO_FORCE_PUSH: &str = "Never force-push a branch somebody else may have pulled.\n";
/// Another, with front matter of its own.
const TONE: &str = "---\ndescription: How a review reads\n---\n\nSay what a change does before what is wrong with it.\n";

/// The own rule of `workspace` at `route`, holding `source`.
fn own_rule(workspace: &FlayerWorkspace, route: &str, source: &str) -> PathBuf {
    let file = workspace
        .own()
        .directory_for(Kind::Rule)
        .unwrap()
        .join(format!("{route}.md"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, source).unwrap();
    file
}

/// A workspace linking a project a `mind.toml` marks, which keeps its rules
/// in `rules`.
fn older() -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("app")).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, project, ledger)
}

fn copy_of(project: &MindProject, route: &str) -> PathBuf {
    let mut path = project.directory_for(Kind::Rule).unwrap();
    path.extend(route.split('/'));
    path.with_extension("md")
}

fn try_put(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Installed, InstallError> {
    let candidate = install::offered(workspace, ledger, project, Kind::Rule, route, None)?;
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .map(|done| done.installed)
}

fn put(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Installed {
    try_put(workspace, ledger, project, route).unwrap()
}

fn take(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Removed, InstallError> {
    install::uninstall(
        workspace,
        ledger,
        project,
        Kind::Rule,
        route,
        &AgentHomes::default(),
    )
}

#[test]
fn a_project_a_mind_toml_marks_gets_a_copy_of_the_rule_filed_by_its_route() {
    let (_dir, workspace, project, ledger) = older();
    let file = own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");

    let added = put(&workspace, &ledger, &project, "git/no-force-push");

    assert_eq!(
        added,
        Installed::Added {
            name: String::from("git/no-force-push"),
            paths: vec![copy.clone()],
            unhashed: None,
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), NO_FORCE_PUSH);
    assert!(!project.root().join("AGENTS.md").exists());
    let held = hash::bytes(NO_FORCE_PUSH.as_bytes());
    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap(),
        [InstallRecord {
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            source_id: None,
            origin: Some(String::from(WORKSPACE_ORIGIN)),
            content_hash: Some(held.clone()),
            targets: vec![Target {
                agent: None,
                path: String::from("rules/git/no-force-push.md"),
                content_hash: Some(held),
            }],
        }]
    );
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Unchanged { .. }
    ));
    fs::write(&file, "Never force-push to main.\n").unwrap();
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Updated { .. }
    ));
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        "Never force-push to main.\n"
    );

    let removed = take(&workspace, &ledger, &project, "git/no-force-push").unwrap();

    assert!(
        matches!(&removed, Removed::Removed { paths, still_excluded: None, .. } if *paths == [copy.clone()]),
        "{removed:?}"
    );
    assert!(!copy.exists());
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

#[test]
fn a_file_at_a_rules_place_nothing_recorded_is_somebodys_in_a_project_a_mind_toml_marks() {
    let (_dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::write(&copy, "Our own rule.\n").unwrap();

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Rule).unwrap();
    assert_eq!(surveyed.len(), 1);
    assert_eq!(surveyed[0].standing, Standing::Foreign);

    assert_eq!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Foreign {
            name: String::from("git/no-force-push"),
            path: copy.clone(),
        }
    );
    assert_eq!(
        take(&workspace, &ledger, &project, "git/no-force-push").unwrap(),
        Removed::Foreign {
            name: String::from("git/no-force-push"),
            path: copy.clone(),
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), "Our own rule.\n");
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

#[test]
fn a_rule_from_a_loaded_workspace_records_where_it_came_from() {
    let (dir, mut workspace, project, ledger) = older();
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", TONE);
    workspace.load(&team).unwrap();

    put(&workspace, &ledger, &project, "review/tone");

    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap()[0]
            .origin
            .as_deref(),
        Some("load:team")
    );
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        TONE
    );
}

#[test]
fn two_places_offering_one_route_are_told_apart_with_from() {
    let (dir, mut workspace, project, ledger) = older();
    own_rule(&workspace, "review/tone", TONE);
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", "Be kind.\n");
    workspace.load(&team).unwrap();

    let twice = try_put(&workspace, &ledger, &project, "review/tone").unwrap_err();

    assert!(
        matches!(&twice, InstallError::OfferedTwice { origins, .. }
            if *origins == [String::from(WORKSPACE_ORIGIN), String::from("load:team")]),
        "{twice:?}"
    );
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "review/tone",
        Some("load:team"),
    )
    .unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        "Be kind.\n"
    );
}

#[cfg(unix)]
#[test]
fn a_rule_linked_out_of_the_workspace_that_offers_it_is_not_installed() {
    let (_dir, workspace, project, ledger) = older();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "token=hunter2\n").unwrap();
    let file = own_rule(&workspace, "leak", "placeholder\n");
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&secret, &file).unwrap();

    let refused = try_put(&workspace, &ledger, &project, "leak").unwrap_err();

    assert!(
        matches!(refused, InstallError::Outside { .. }),
        "{refused:?}"
    );
    assert!(!copy_of(&project, "leak").exists());
}

#[test]
fn a_route_that_is_no_name_is_neither_installed_nor_removed() {
    let (_dir, workspace, project, ledger) = older();

    let refused = take(&workspace, &ledger, &project, "../escape").unwrap_err();

    assert!(
        matches!(refused, InstallError::UnsafeRoute { .. }),
        "{refused:?}"
    );
    assert!(install::is_installable_name(
        Kind::Rule,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(
        Kind::Skill,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(Kind::Rule, "Git Notes"));
}
