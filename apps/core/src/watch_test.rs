//! Unit tests for `watch.rs`, kept beside it rather than inside it.
//!
//! The integration suite holds what a workspace and its projects are
//! watched by; these hold the list's own rules: each path once a scope, in
//! the order it was named, the markers a project may be in, and what is
//! named for a workspace or a project that would not open.

use super::*;

use std::fs;

use crate::mother::random;
use crate::mother::{ProjectMother, WorkspaceMother};
use crate::workspace::{WEAVER_CONFIG, WEAVER_DIR};

fn workspace_scope(watched: &[Watched]) -> Vec<&Path> {
    watched
        .iter()
        .filter(|watched| watched.scope == Scope::Workspace)
        .map(|watched| watched.path.as_path())
        .collect()
}

fn project_scope<'a>(watched: &'a [Watched], entry: &str) -> Vec<&'a Path> {
    watched
        .iter()
        .filter(|watched| watched.scope == Scope::Project(entry.to_owned()))
        .map(|watched| watched.path.as_path())
        .collect()
}

#[test]
fn a_workspace_s_marker_is_in_its_weaver_folder() {
    let root = PathBuf::from(format!("/{}", random::word()));

    assert_eq!(
        root.join(WEAVER_DIR).join(WEAVER_CONFIG),
        root.join(".mindweaver").join("weaver.toml")
    );
}

#[test]
fn a_path_named_twice_for_one_scope_is_listed_once_where_it_was_first_named() {
    let mut list = List::default();
    let scope = Scope::Project(String::from("app"));

    list.add(&scope, PathBuf::from("/a"));
    list.add(&scope, PathBuf::from("/b"));
    list.add(&scope, PathBuf::from("/a"));

    let paths: Vec<&Path> = list
        .0
        .iter()
        .map(|watched| watched.path.as_path())
        .collect();
    assert_eq!(paths, [Path::new("/a"), Path::new("/b")]);
}

/// A change to a path of two scopes is a change to both, so a front end
/// that rebuilds part of what it shows is told of each.
#[test]
fn one_path_of_two_scopes_is_listed_for_each() {
    let mut list = List::default();
    let path = PathBuf::from("/w/AGENTS.md");

    list.add(&Scope::Workspace, path.clone());
    list.add(&Scope::Project(String::from(".")), path.clone());

    assert_eq!(
        list.0,
        [
            Watched {
                path: path.clone(),
                scope: Scope::Workspace,
            },
            Watched {
                path,
                scope: Scope::Project(String::from(".")),
            },
        ]
    );
}

#[test]
fn any_paths_added_are_each_listed_once_a_scope_in_the_order_first_named() {
    for _ in 0..100 {
        let mut list = List::default();
        let mut expected: Vec<Watched> = Vec::new();
        for _ in 0..random::between(0, 30) {
            let scope = random::pick(&[
                Scope::Workspace,
                Scope::Project(String::from("a")),
                Scope::Project(String::from("b")),
            ]);
            let path = PathBuf::from(format!("/{}", random::between(0, 6)));
            let watched = Watched {
                path: path.clone(),
                scope: scope.clone(),
            };
            if !expected.contains(&watched) {
                expected.push(watched);
            }
            list.add(&scope, path);
        }

        assert_eq!(list.0, expected);
    }
}

/// Whichever file a project's marker is in, and the two it is not, which
/// a migration or `mind init` would write.
#[test]
fn a_project_is_watched_by_every_file_its_marker_can_be_in() {
    let mut list = List::default();
    let scope = Scope::Project(String::from("app"));

    list.markers(&scope, Path::new("/p/app"));

    let paths: Vec<&Path> = list
        .0
        .iter()
        .map(|watched| watched.path.as_path())
        .collect();
    assert_eq!(
        paths,
        [
            Path::new("/p/app/.mind/settings.toml"),
            Path::new("/p/app/.mind/settings.json"),
            Path::new("/p/app/.mind/mind.toml"),
        ]
    );
}

#[test]
fn the_workspace_comes_before_every_project_and_projects_go_by_entry() {
    let mut scopes = vec![
        Scope::Project(String::from("b")),
        Scope::Workspace,
        Scope::Project(String::from("a")),
    ];
    scopes.sort();

    assert_eq!(
        scopes,
        [
            Scope::Workspace,
            Scope::Project(String::from("a")),
            Scope::Project(String::from("b")),
        ]
    );
}

#[test]
fn a_workspace_is_watched_first_by_its_marker_its_ledger_and_its_shelf() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let watched = watched(&workspace, None);

    let mut expected = vec![workspace.config_path(), workspace.ledger_path()];
    expected.extend(Kind::ALL.iter().map(|kind| workspace.gathered_dir(*kind)));
    let first: Vec<&Path> = watched
        .iter()
        .take(expected.len())
        .map(|watched| watched.path.as_path())
        .collect();
    assert_eq!(
        first,
        expected.iter().map(PathBuf::as_path).collect::<Vec<_>>()
    );
    assert!(watched
        .iter()
        .all(|watched| watched.scope == Scope::Workspace));
}

/// The ledger by its file and never its folder, since SQLite's files
/// beside it come and go with every read.
#[test]
fn the_ledger_s_folder_is_never_watched() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let watched = watched(&workspace, None);

    let folder = workspace.ledger_path().parent().unwrap().to_path_buf();
    assert!(
        watched.iter().all(|watched| watched.path != folder),
        "{watched:?}"
    );
}

#[test]
fn the_default_workspace_s_marker_is_watched_when_there_is_a_home() {
    let (dir, workspace) = WorkspaceMother::empty();
    let home = dir.path().join("home");

    let without = watched(&workspace, None);
    let with = watched(&workspace, Some(&home));

    // Under both names, either of which would make home a workspace.
    let markers = [
        home.join(WEAVER_DIR).join(WEAVER_CONFIG),
        home.join(".mindflayer").join("flayer.toml"),
    ];
    for marker in &markers {
        assert!(!workspace_scope(&without).contains(&marker.as_path()));
    }
    assert_eq!(
        workspace_scope(&with)[workspace_scope(&with).len() - 2..],
        markers.iter().map(PathBuf::as_path).collect::<Vec<_>>()
    );
    assert_eq!(with.len(), without.len() + 2);
}

/// A workspace that is the default itself names its marker once.
#[test]
fn the_default_workspace_watched_from_home_names_its_marker_once() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let watched = watched(&workspace, Some(workspace.root()));

    let marker = workspace.config_path();
    assert_eq!(
        workspace_scope(&watched)
            .iter()
            .filter(|path| **path == marker)
            .count(),
        1,
        "{watched:?}"
    );
}

/// A project that would not open is watched by what would make it one
/// again, and by nothing it would have held.
#[test]
fn a_project_that_has_gone_is_watched_by_its_markers_alone() {
    let (_dir, mut workspace) = WorkspaceMother::empty();
    let project = ProjectMother::linked(&mut workspace, "app");
    fs::remove_dir_all(project.root()).unwrap();

    let watched = watched(&workspace, None);

    let mind = project.root().join(MIND_DIR);
    assert_eq!(
        project_scope(&watched, "app"),
        [
            mind.join("settings.toml"),
            mind.join("settings.json"),
            mind.join("mind.toml"),
        ]
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>()
    );
}

#[test]
fn a_project_linked_is_watched_under_its_entry_and_never_by_its_root() {
    let (_dir, mut workspace) = WorkspaceMother::empty();
    let project = ProjectMother::linked(&mut workspace, "app");

    let watched = watched(&workspace, None);
    let paths = project_scope(&watched, "app");

    assert!(paths.contains(&project.root().join(MIND_DIR).join("mind.toml").as_path()));
    for agent in Agent::ALL {
        for file in agent.reads_files() {
            assert!(
                paths.contains(&project.root().join(&file).as_path()),
                "{agent:?} reads {file:?}"
            );
        }
    }
    assert!(!paths.contains(&project.root()), "{paths:?}");
    // Beside its own, the workspace's templates, which it is offered too.
    let templates = template::looked_in(Some(&workspace), &project);
    for path in &paths {
        assert!(
            path.starts_with(project.root()) || templates.iter().any(|folder| folder == path),
            "{path:?} is neither the project's nor a template folder"
        );
    }
}

/// The folders named are those discovery reads, each with what it looks
/// in below it, so what is listed is what is watched: a folder an agent
/// reads that no install sends a copy to, OpenCode's own, is neither.
#[test]
fn a_project_marked_by_its_settings_is_watched_in_every_folder_discovery_reads() {
    let (dir, mut workspace) = WorkspaceMother::empty();
    let project = ProjectMother::create(
        &dir.path().join("app"),
        &Agent::ALL,
        crate::settings::Share::Local,
    );
    workspace.link(&project).unwrap();

    let watched = watched(&workspace, None);
    let paths = project_scope(&watched, "app");

    for kind in Kind::ALL {
        for folder in Catalog::folders(&project, kind) {
            for looked in Catalog::looked_in(&folder, kind) {
                assert!(paths.contains(&looked.as_path()), "{looked:?}: {paths:?}");
            }
        }
    }
    let opencode = project.root().join(".opencode").join("skills");
    assert!(!paths.contains(&opencode.as_path()), "{paths:?}");
}

#[test]
fn no_path_is_named_twice_for_one_scope_however_many_projects_are_linked() {
    for _ in 0..5 {
        let (dir, mut workspace) = WorkspaceMother::empty();
        for _ in 0..random::between(1, 4) {
            let project = ProjectMother::random(dir.path());
            if !workspace.is_linked(project.root()) {
                workspace.link(&project).unwrap();
            }
        }

        let watched = watched(&workspace, Some(dir.path()));

        for (at, one) in watched.iter().enumerate() {
            assert!(
                !watched[..at].contains(one),
                "{one:?} named twice in {watched:?}"
            );
        }
    }
}

/// A loaded workspace that would not open is watched by its marker, which
/// is what would bring it back.
#[test]
fn a_loaded_workspace_that_has_gone_is_watched_by_its_marker() {
    let (dir, mut workspace) = WorkspaceMother::empty();
    let loaded = WorkspaceMother::create(&dir.path().join("loaded"));
    workspace.load(&loaded).unwrap();
    let marker = loaded.config_path();
    fs::remove_dir_all(loaded.root()).unwrap();

    let watched = watched(&workspace, None);

    assert!(
        workspace_scope(&watched).contains(&marker.as_path()),
        "{watched:?}"
    );
}
