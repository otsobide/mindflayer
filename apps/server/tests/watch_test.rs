//! Watching what the backend names, against real folders: a change there is
//! told on the event stream within the debounce, by the scope the backend
//! gave it, and what the backend names anew is watched once it is asked
//! again. Every folder is a temporary one, and every wait is bounded.

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::*;
use mindflayer_server::DEBOUNCE;
use tempfile::TempDir;

fn watched(path: &Path, scope: &str) -> Watched {
    Watched {
        path: path.to_path_buf(),
        scope: json!(scope),
    }
}

fn changed(scope: &str) -> Value {
    json!({"type": "changed", "scopes": [scope]})
}

/// Long enough for a change to have been told, were it going to be: a few
/// debounces, for the debouncer's tick and a machine under load.
fn quiet() -> Duration {
    DEBOUNCE * 5
}

/// The acceptance's own: a file written into a watched folder is told as
/// a change of that folder's scope, within the debounce and a margin.
#[tokio::test]
async fn a_file_written_into_a_watched_folder_is_told_as_a_change_of_its_scope() {
    let dir = TempDir::new().unwrap();
    let (server, _) = launch_watching(vec![watched(dir.path(), "temp")]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;

    fs::write(dir.path().join("new.md"), "# New\n").unwrap();

    assert_eq!(stream.next().await, changed("temp"));
}

/// Watched on its own, so a change further down is nobody's: told, it
/// would be a snapshot read for every file a skill's scripts write.
#[tokio::test]
async fn a_change_below_what_a_watched_folder_holds_directly_is_not_told() {
    let dir = TempDir::new().unwrap();
    let deep = dir.path().join("deploy").join("scripts");
    fs::create_dir_all(&deep).unwrap();
    let (server, _) = launch_watching(vec![watched(dir.path(), "temp")]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;

    fs::write(deep.join("run.sh"), "true\n").unwrap();

    assert_eq!(stream.within(quiet()).await, None);
}

/// A file named on its own is watched through its folder, and nothing
/// else in that folder is told: a project's `README.md` is no change.
#[tokio::test]
async fn a_file_named_is_told_and_nothing_beside_it() {
    let dir = TempDir::new().unwrap();
    let agents = dir.path().join("AGENTS.md");
    let (server, _) = launch_watching(vec![watched(&agents, "project")]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;

    fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
    assert_eq!(stream.within(quiet()).await, None);
    fs::write(&agents, "# Working here\n").unwrap();

    assert_eq!(stream.next().await, changed("project"));
}

/// A project's `.claude/skills` before its first install: its being made
/// is told, and once made it is watched itself.
#[tokio::test]
async fn a_folder_named_before_it_is_there_is_watched_once_made() {
    let dir = TempDir::new().unwrap();
    let skills = dir.path().join(".claude").join("skills");
    let (server, _) = launch_watching(vec![watched(&skills, "project")]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;

    fs::create_dir_all(&skills).unwrap();
    assert_eq!(stream.next().await, changed("project"));
    fs::create_dir(skills.join("deploy")).unwrap();

    assert_eq!(stream.next().await, changed("project"));
}

/// The list is asked for again after a change, so a path the backend names
/// since, a project linked, is watched from then on.
#[tokio::test]
async fn what_the_backend_names_since_a_change_is_watched() {
    let one = TempDir::new().unwrap();
    let two = TempDir::new().unwrap();
    let (server, list) = launch_watching(vec![watched(one.path(), "one")]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;
    list.lock().unwrap().push(watched(two.path(), "two"));

    fs::write(one.path().join("flayer.toml"), "linked = true\n").unwrap();
    assert_eq!(stream.next().await, changed("one"));
    fs::write(two.path().join("SKILL.md"), "# Two\n").unwrap();

    assert_eq!(stream.next().await, changed("two"));
}

/// A file named and the folder it is in, both of one scope: a change to it
/// is told with that scope once.
#[tokio::test]
async fn a_change_is_told_with_each_scope_once() {
    let dir = TempDir::new().unwrap();
    let agents = dir.path().join("AGENTS.md");
    let (server, _) = launch_watching(vec![
        watched(dir.path(), "project"),
        watched(&agents, "project"),
    ]);
    let mut stream = opened(&server, Auth::Bearer(server.token())).await;

    fs::write(&agents, "# Working here\n").unwrap();

    assert_eq!(stream.next().await, changed("project"));
}
