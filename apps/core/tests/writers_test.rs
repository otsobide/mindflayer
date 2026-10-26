//! Writers at the same time: the CLI, the TUI and the server can each be
//! writing to one workspace, and none may lose what another wrote.
//!
//! Threads stand in for processes. The lock is taken on a file opened afresh
//! by each writer, and the system keeps apart two of those in one process as
//! it keeps apart two processes, on every platform this runs on.

use std::fs;
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use mindflayer_core::install::{self, Removed};
use mindflayer_core::ledger::{Action, Outcome};
use mindflayer_core::lock::{HOLDER_FILE, WAIT};
use mindflayer_core::{FlayerWorkspace, Kind, Ledger, LockError, MindProject, LEDGER_FILE};
use tempfile::TempDir;

fn workspace() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    (dir, workspace)
}

/// Run `write` while `workspace` is held, let go after a moment, and fail
/// unless `write` finished only after that: it waited for the writer before
/// it rather than writing beside it.
fn waits_for_the_writer_before_it(
    workspace: &FlayerWorkspace,
    write: impl FnOnce() + Send + 'static,
) {
    let held = workspace.lock().unwrap();
    let (sent, done) = mpsc::channel();
    let writer = thread::spawn(move || {
        write();
        sent.send(Instant::now()).unwrap();
    });
    thread::sleep(Duration::from_millis(100));
    let released = Instant::now();
    drop(held);

    let finished = done.recv().unwrap();
    writer.join().unwrap();
    assert!(
        finished >= released,
        "wrote while another writer held the workspace"
    );
}

/// `count` mind projects under `dir`, named `<prefix>-<n>`.
fn projects(dir: &std::path::Path, prefix: &str, count: usize) -> Vec<MindProject> {
    (0..count)
        .map(|n| {
            let root = dir.join(format!("{prefix}-{n}"));
            fs::create_dir(&root).unwrap();
            MindProject::init(&root).unwrap().0
        })
        .collect()
}

#[test]
fn a_writer_kept_waiting_past_the_wait_is_told_the_workspace_is_busy_and_by_whom() {
    let (_dir, workspace) = workspace();
    let held = workspace.lock().unwrap();
    assert!(held.is_held());

    let wait = Duration::from_millis(200);
    let started = Instant::now();
    let error = workspace.lock_waiting(wait).unwrap_err();

    assert!(
        started.elapsed() >= wait,
        "gave up before the wait was over"
    );
    assert!(
        matches!(
            &error,
            LockError::Busy { path, pid, waited }
                if *path == workspace.lock_path()
                    && *pid == Some(std::process::id())
                    && *waited == wait
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "{}: workspace busy: waited 200ms for process {} to finish writing to it; try again once it has",
            workspace.lock_path().display(),
            std::process::id()
        )
    );
}

#[test]
fn a_holder_that_left_no_id_is_called_another_process() {
    let (_dir, workspace) = workspace();
    let _held = workspace.lock().unwrap();
    fs::remove_file(workspace.flayer_dir().join(HOLDER_FILE)).unwrap();

    let error = workspace.lock_waiting(Duration::ZERO).unwrap_err();

    assert!(
        matches!(error, LockError::Busy { pid: None, .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("for another process to finish"),
        "{error}"
    );
}

#[test]
fn dropping_the_lock_releases_it_and_takes_its_id_away() {
    let (_dir, workspace) = workspace();
    let held = workspace.lock().unwrap();
    let holder = workspace.flayer_dir().join(HOLDER_FILE);
    assert_eq!(
        fs::read_to_string(&holder).unwrap(),
        std::process::id().to_string()
    );

    drop(held);

    // Not waited for: free means free at the first try.
    let again = workspace.lock_waiting(Duration::ZERO).unwrap();
    drop(again);
    assert!(!holder.exists(), "names a writer that has let go");
    // The lock file stays: deleting it while another writer waits on it
    // would let a third lock a new file beside it.
    assert!(workspace.lock_path().is_file());
}

#[test]
fn a_writer_kept_waiting_goes_ahead_as_soon_as_the_holder_lets_go() {
    let (_dir, workspace) = workspace();
    let held = workspace.lock().unwrap();

    let (sent, taken) = mpsc::channel();
    let waiting = workspace.clone();
    let writer = thread::spawn(move || {
        let lock = waiting.lock().unwrap();
        sent.send(Instant::now()).unwrap();
        drop(lock);
    });
    thread::sleep(Duration::from_millis(100));
    let released = Instant::now();
    drop(held);

    let at = taken.recv().unwrap();
    writer.join().unwrap();
    assert!(at >= released, "had it while it was still held");
    assert!(at - released < WAIT, "gave up rather than waited");
}

#[test]
fn readers_never_wait_for_a_writer() {
    let (_dir, workspace) = workspace();
    let _held = workspace.lock().unwrap();

    let started = Instant::now();
    let read = FlayerWorkspace::open(workspace.root()).unwrap();
    assert!(read.members().is_empty());
    let ledger = read.ledger_for_reading().unwrap();
    assert!(ledger.gathered().unwrap().is_empty());

    // A read that took the lock would have waited all of it and failed.
    assert!(started.elapsed() < WAIT);
}

/// What `PRAGMA journal_mode` says of the database at `path`.
fn journal_mode(path: &std::path::Path) -> String {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn two_ledger_connections_writing_at_once_both_succeed() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    // Made first, so what races is two writers, not two first opens.
    drop(Ledger::open(&path).unwrap());

    const ENTRIES: usize = 200;
    let start = Arc::new(Barrier::new(2));
    let writers: Vec<_> = (0..2)
        .map(|writer| {
            let (path, start) = (path.clone(), Arc::clone(&start));
            thread::spawn(move || {
                let ledger = Ledger::open(&path).unwrap();
                start.wait();
                for entry in 0..ENTRIES {
                    let target = format!("writer {writer}, entry {entry}");
                    ledger.log(Action::Gather, Some(&target), Outcome::Ok, None)?;
                }
                Ok::<_, mindflayer_core::LedgerError>(())
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap().unwrap();
    }

    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.history(usize::MAX >> 1).unwrap().len(), 2 * ENTRIES);
}

#[test]
fn a_ledger_opened_to_write_keeps_a_write_ahead_log() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);

    drop(Ledger::open(&path).unwrap());

    assert_eq!(journal_mode(&path), "wal");
}

#[test]
fn a_ledger_opened_to_read_keeps_the_journal_it_had() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    drop(Ledger::open(&path).unwrap());
    // A ledger an older Mindflayer made, before it kept a log.
    rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA journal_mode = DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap();

    drop(Ledger::open_existing(&path).unwrap().unwrap());

    assert_eq!(journal_mode(&path), "delete");
}

#[test]
fn two_threads_linking_different_projects_at_once_leave_every_entry_in_the_marker() {
    let (dir, workspace) = workspace();
    // Many each, so that without the lock the two would all but surely read
    // the marker at once at least one time, and one entry would be lost.
    const EACH: usize = 20;
    let start = Arc::new(Barrier::new(2));
    let linkers: Vec<_> = ["one", "two"]
        .into_iter()
        .map(|prefix| {
            let batch = projects(dir.path(), prefix, EACH);
            let root = workspace.root().to_path_buf();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                let mut workspace = FlayerWorkspace::open(&root).unwrap();
                start.wait();
                for project in &batch {
                    workspace.link(project).unwrap();
                }
                batch
            })
        })
        .collect();
    let linked: Vec<MindProject> = linkers
        .into_iter()
        .flat_map(|linker| linker.join().unwrap())
        .collect();

    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    assert_eq!(workspace.config().projects.len(), 2 * EACH);
    for project in &linked {
        assert!(
            workspace.is_linked(project.root()),
            "{} was lost",
            project.root().display()
        );
    }
}

#[test]
fn a_link_waits_for_the_writer_before_it() {
    let (dir, workspace) = workspace();
    let project = projects(dir.path(), "alpha", 1).remove(0);
    let mut linking = workspace.clone();

    waits_for_the_writer_before_it(&workspace, move || {
        linking.link(&project).unwrap();
    });

    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    assert_eq!(workspace.config().projects.len(), 1);
}

/// A workspace keeping a `deploy` skill of its own, and a project beside it
/// to install it into.
fn offering_deploy() -> (TempDir, FlayerWorkspace, MindProject) {
    let (dir, workspace) = workspace();
    let skill = dir.path().join("skills").join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: deploy\ndescription: Ship it\n---\n# deploy\n",
    )
    .unwrap();
    let project = projects(dir.path(), "alpha", 1).remove(0);
    (dir, workspace, project)
}

#[test]
fn an_install_waits_for_the_writer_before_it() {
    let (_dir, workspace, project) = offering_deploy();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    let (installing, into) = (workspace.clone(), project.clone());

    waits_for_the_writer_before_it(&workspace, move || {
        install::install(&installing, &ledger, &into, &candidate).unwrap();
    });

    let copy = project.directory_for(Kind::Skill).join("deploy");
    assert!(copy.join("SKILL.md").is_file());
}

#[test]
fn an_uninstall_waits_for_the_writer_before_it() {
    let (_dir, workspace, project) = offering_deploy();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(&workspace, &ledger, &project, &candidate).unwrap();
    let (uninstalling, from) = (workspace.clone(), project.clone());

    waits_for_the_writer_before_it(&workspace, move || {
        let removed =
            install::uninstall(&uninstalling, &ledger, &from, Kind::Skill, "deploy").unwrap();
        assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
    });

    assert!(!project.directory_for(Kind::Skill).join("deploy").exists());
}
