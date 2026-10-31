//! The panel's live updates over the CLI's backend (#74): the server's
//! watcher, watching what core names for a real workspace, tells what a
//! terminal or an editor changes there, by the holder it changed, and
//! nothing for what reading the workspace does. No port is bound; every
//! folder is a temporary one, and every wait is bounded.

// The backend and the server are the panel's, which a build without the
// `ui` feature lacks.
#![cfg(feature = "ui")]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use mindflayer_cli::backend::{PanelBackend, Query};
use mindflayer_server::{Backend, Event, Events, Subscription, Watcher, DEBOUNCE};
use serde_json::json;

/// How long a change that is coming takes at most to be told.
const PATIENCE: Duration = Duration::from_secs(10);

/// A watcher over the backend for the workspace at `root`, as the server
/// starts one, and a subscription past what it says first.
async fn watching(root: &Path) -> (Arc<PanelBackend>, Watcher, Events, Subscription) {
    let backend = Arc::new(PanelBackend::launch(root, &Lookup::default()).unwrap());
    let events = Events::new();
    let mut subscription = events.subscribe();
    let named = Arc::clone(&backend);
    let watcher = Watcher::start(move || named.watched_paths(), events.clone());
    assert_eq!(
        next(&mut subscription).await,
        Some(Event::Resync { switches: 0 })
    );
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));
    (backend, watcher, events, subscription)
}

/// The next event, or none when `wait` passes without one.
async fn within(subscription: &mut Subscription, wait: Duration) -> Option<Event> {
    tokio::time::timeout(wait, subscription.next())
        .await
        .ok()
        .flatten()
}

async fn next(subscription: &mut Subscription) -> Option<Event> {
    within(subscription, PATIENCE).await
}

fn changed(scope: serde_json::Value) -> Option<Event> {
    Some(Event::Changed {
        scopes: vec![scope],
    })
}

/// Were a snapshot to change what is watched, each would set off the
/// next for as long as the panel is open: SQLite makes its log beside the
/// ledger to read it, and every file a snapshot reads is opened.
#[tokio::test]
async fn reading_the_workspace_is_told_as_no_change() {
    let dir = workspace_with_two();
    // A ledger there, as an install leaves one, for the reads to open.
    FlayerWorkspace::open(dir.path()).unwrap().ledger().unwrap();
    let (backend, _watcher, _events, mut subscription) = watching(dir.path()).await;

    for _ in 0..3 {
        backend.query(Query::Snapshot {}).unwrap();
    }

    assert_eq!(within(&mut subscription, DEBOUNCE * 5).await, None);
}

/// An editor saving a project's skill.
#[tokio::test]
async fn a_skill_edited_in_a_project_is_told_by_the_projects_entry() {
    let dir = workspace_with_two();
    let (_backend, _watcher, _events, mut subscription) = watching(dir.path()).await;
    let skill = directory_for(&dir.path().join("alpha"), Kind::Skill).join("alpha");

    fs::write(skill.join("SKILL.md"), skill_file("alpha", "Edited")).unwrap();

    assert_eq!(
        next(&mut subscription).await,
        changed(json!({"type": "project", "entry": "alpha"}))
    );
}

/// A terminal linking a project: the workspace's marker changes, and the
/// project is watched from then on, the list asked for again.
#[tokio::test]
async fn a_project_linked_from_a_terminal_is_told_and_then_watched() {
    let dir = workspace_with_two_unlinked();
    let (_backend, _watcher, _events, mut subscription) = watching(dir.path()).await;

    flayer(dir.path(), &["link", "gamma"]).unwrap();
    assert_eq!(
        next(&mut subscription).await,
        changed(json!({"type": "workspace"}))
    );
    let agents = dir.path().join("gamma").join("AGENTS.md");
    fs::write(agents, "# Working here\n").unwrap();

    assert_eq!(
        next(&mut subscription).await,
        changed(json!({"type": "project", "entry": "gamma"}))
    );
}
