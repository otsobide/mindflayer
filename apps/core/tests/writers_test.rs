//! Writers at the same time: the CLI, the TUI and the server can each be
//! writing to one workspace, and none may lose what another wrote.
//!
//! Threads stand in for processes. The lock is taken on a file opened afresh
//! by each writer, and the system keeps apart two of those in one process as
//! it keeps apart two processes, on every platform this runs on.

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use common::{repository, skill, url, workspace};
use mindflayer_core::gather::{self, Request};
use mindflayer_core::install::{self, Installed, Removed, Standing};
use mindflayer_core::ledger::{Action, Outcome, SourceKind};
use mindflayer_core::lifecycle::{self, Record};
use mindflayer_core::lock::{HOLDER_FILE, WAIT};
use mindflayer_core::migrate;
use mindflayer_core::reapply::{self, Change};
use mindflayer_core::AgentHomes;
use mindflayer_core::{
    Agent, Directories, FlayerWorkspace, Kind, Ledger, LockError, MindProject, Reference, Share,
    WorkspaceError, LEDGER_FILE,
};
use tempfile::TempDir;

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
fn a_writer_kept_waiting_gives_up_as_busy_once_its_own_wait_is_over() {
    let (dir, workspace) = workspace();
    let project = projects(dir.path(), "alpha", 1).remove(0);
    let _held = workspace.lock().unwrap();
    let mut linking = workspace.clone();

    // The whole of `WAIT`, for the one thing no shorter wait shows: that a
    // writer's own wait, not only one a test picks, comes to an end.
    let started = Instant::now();
    let error = linking.link(&project).unwrap_err();
    let waited = started.elapsed();

    assert!(waited >= WAIT, "gave up before the wait was over");
    assert!(waited < 2 * WAIT, "went on waiting past it");
    assert!(
        matches!(
            &error,
            WorkspaceError::Lock(LockError::Busy { pid, waited, .. })
                if *pid == Some(std::process::id()) && *waited == WAIT
        ),
        "{error:?}"
    );
    let workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    assert!(!workspace.is_linked(project.root()), "linked all the same");
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
    drop(workspace.ledger().unwrap());
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
fn a_ledger_a_newer_build_wrote_is_refused_before_its_journal_is_changed() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        let newer = rusqlite::Connection::open(&path).unwrap();
        newer
            .query_row("PRAGMA journal_mode = DELETE", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        newer.pragma_update(None, "user_version", 99).unwrap();
    }

    let error = Ledger::open(&path).unwrap_err();

    assert!(
        matches!(
            error,
            mindflayer_core::LedgerError::Version { found: 99, .. }
        ),
        "{error:?}"
    );
    // The mode is kept in the file's header: switching it is a write.
    assert_eq!(journal_mode(&path), "delete");
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

/// Takes write permission away from a folder, and gives it back when
/// dropped, so that a failed assertion does not leave a temporary folder
/// that cannot be cleaned up.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    fn new(folder: &std::path::Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o555)).unwrap();
        ReadOnly(folder.to_path_buf())
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

// Unix only: a folder's write permission is what is taken away, and
// Windows keeps none on a folder.
#[cfg(unix)]
#[test]
fn a_ledger_keeping_a_log_is_read_where_its_folder_cannot_be_written() {
    let dir = TempDir::new().unwrap();
    // Named with what SQLite would read as part of a URI's syntax.
    let folder = dir.path().join("odd #1, 50%?").join(".mindflayer");
    let path = folder.join(LEDGER_FILE);
    {
        let ledger = Ledger::open(&path).unwrap();
        ledger
            .log(Action::Gather, Some("https://h/a"), Outcome::Ok, None)
            .unwrap();
    }
    assert_eq!(journal_mode(&path), "wal");
    // As a sandboxed agent sees a workspace it may read and not write. SQLite
    // took the log's files away when the last connection closed, and cannot
    // make them again here.
    let _read_only = ReadOnly::new(&folder);

    let ledger = Ledger::open_existing(&path).unwrap().unwrap();

    assert_eq!(ledger.history(10).unwrap().len(), 1);
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
    let folder = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("SKILL.md"), skill("deploy", "Ship it")).unwrap();
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
        install::install(
            &installing,
            &ledger,
            &into,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    });

    let copy = project.directory_for(Kind::Skill).unwrap().join("deploy");
    assert!(copy.join("SKILL.md").is_file());
}

#[test]
fn an_install_looks_again_at_the_copy_once_it_holds_the_workspace() {
    let (_dir, mut workspace, project) = offering_deploy();
    workspace.link(&project).unwrap();
    let (ledger, _) = workspace.ledger_to_write().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    // Surveyed while the copy was Mindflayer's, as the install screen
    // surveys when it is built.
    let stale =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    assert_eq!(stale.standing, Standing::Current);
    // Then edited in another terminal, which makes it an edited copy.
    let target = lifecycle::find(&project, &Reference::parse("deploy")).unwrap();
    let edit = lifecycle::start_edit(&target);
    let file = edit.file().to_path_buf();
    let mut text = fs::read_to_string(&file).unwrap();
    text.push_str("Mine.\n");
    fs::write(&file, text).unwrap();
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);
    assert_eq!(edited.copy.unwrap(), Record::Kept);

    let installed = install::install(
        &workspace,
        &ledger,
        &project,
        &stale,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(
        matches!(installed, Installed::Edited { .. }),
        "{installed:?}"
    );
    assert!(fs::read_to_string(&file).unwrap().ends_with("Mine.\n"));
}

#[test]
fn an_uninstall_waits_for_the_writer_before_it() {
    let (_dir, workspace, project) = offering_deploy();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    let (uninstalling, from) = (workspace.clone(), project.clone());

    waits_for_the_writer_before_it(&workspace, move || {
        let removed = install::uninstall(
            &uninstalling,
            &ledger,
            &from,
            Kind::Skill,
            "deploy",
            &AgentHomes::default(),
        )
        .unwrap();
        assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
    });

    assert!(!project
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy")
        .exists());
}

#[test]
fn a_change_to_a_projects_agents_waits_for_the_writer_before_it() {
    let (dir, mut workspace, _) = offering_deploy();
    let root = dir.path().join("beta");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init_settings(
        &root,
        &[Agent::Claude],
        Share::Shared,
        &Directories::default(),
    )
    .unwrap();
    workspace.link(&project).unwrap();
    let (changing, of) = (workspace.clone(), project.clone());

    // Held by another writer, an install say, which would otherwise put a
    // copy where the settings no longer send one.
    waits_for_the_writer_before_it(&workspace, move || {
        reapply::apply(
            Some(&changing),
            &of,
            &Change::AddAgents(vec![Agent::Codex]),
            &AgentHomes::default(),
        )
        .unwrap();
    });

    let changed = MindProject::open(project.root()).unwrap();
    assert_eq!(
        changed.settings().unwrap().agents,
        [Agent::Claude, Agent::Codex]
    );
}

#[test]
fn a_gather_waits_for_the_writer_before_it() {
    let (_dir, workspace) = workspace();
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let request = Request::git(url(&source));
    let gathering = workspace.clone();

    waits_for_the_writer_before_it(&workspace, move || {
        let ledger = Ledger::in_memory().unwrap();
        let report = gather::gather(&gathering, &ledger, &request).unwrap();
        assert_eq!(report.added.len(), 1);
    });
}

/// A workspace keeping its own beside its projects, as one made before the
/// library does, with a skill in its folder.
fn kept_beside() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init_with(dir.path(), &common::beside()).unwrap();
    mindflayer_core::create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    (dir, workspace)
}

#[test]
fn a_migration_waits_for_the_writer_before_it() {
    let (dir, workspace) = kept_beside();
    let mut migrating = workspace.clone();

    waits_for_the_writer_before_it(&workspace, move || {
        let migrated = migrate::library(&mut migrating).unwrap();
        assert_eq!(migrated.plan.moves.len(), 1);
    });

    assert!(dir
        .path()
        .join(".mindflayer/library/skills/deploy/SKILL.md")
        .is_file());
}

#[test]
fn a_migration_plans_again_once_it_holds_the_workspace() {
    let (dir, workspace) = kept_beside();
    let mut migrating = workspace.clone();
    let held = workspace.lock().unwrap();
    let writer = thread::spawn(move || migrate::library(&mut migrating).unwrap());
    thread::sleep(Duration::from_millis(100));
    // Added while the migration waits, after any plan made before it.
    mindflayer_core::create(&workspace.own(), Kind::Skill, "review", "Look").unwrap();
    drop(held);

    let migrated = writer.join().unwrap();

    assert_eq!(migrated.plan.moves.len(), 2);
    let library = dir.path().join(".mindflayer/library/skills");
    assert!(library.join("deploy/SKILL.md").is_file());
    assert!(library.join("review/SKILL.md").is_file());
}

#[test]
fn opening_the_ledger_does_not_wait_for_the_workspace() {
    let (_dir, workspace) = workspace();
    let _held = workspace.lock().unwrap();

    // Made here, as it is migrated here when an older build left it: the
    // migration's own transaction keeps two of those apart, and a lock taken
    // as well would have waited all of it and failed.
    let started = Instant::now();
    drop(workspace.ledger_to_write().unwrap());

    assert!(workspace.ledger_path().is_file());
    assert!(started.elapsed() < WAIT, "waited for the workspace");
}

/// A source an older build stored with a token in its URL, and the clone it
/// made keeping the token in its config. Returns that config.
fn tainted(workspace: &FlayerWorkspace) -> PathBuf {
    let ledger = workspace.ledger().unwrap();
    let source = ledger
        .source_for(
            SourceKind::Git,
            "https://example.com/acme/skills",
            None,
            "skills",
        )
        .unwrap();
    drop(ledger);
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
    let git = workspace.cache_dir().join(source.directory).join(".git");
    fs::create_dir_all(&git).unwrap();
    let config = git.join("config");
    fs::write(
        &config,
        "[remote \"origin\"]\n\turl = https://me:ghp_t0ken@example.com/acme/skills\n",
    )
    .unwrap();
    config
}

#[test]
fn a_scrub_with_a_clone_to_rewrite_waits_for_the_writer_before_it() {
    let (_dir, workspace) = workspace();
    let config = tainted(&workspace);
    let opening = workspace.clone();

    // The writer before it could be a gather fetching into that clone, or
    // removing the folders the scrub is about to remove.
    waits_for_the_writer_before_it(&workspace, move || {
        let (_, scrub) = opening.ledger_to_write().unwrap();
        assert_eq!(scrub.sources.len(), 1, "{scrub:?}");
    });

    assert!(!fs::read_to_string(&config).unwrap().contains("ghp_t0ken"));
}

#[test]
fn a_change_made_while_the_workspace_is_held_still_sees_to_its_record() {
    let (_dir, mut workspace, project) = offering_deploy();
    workspace.link(&project).unwrap();
    let entry = workspace.entry_for(project.root());
    let entry = entry.to_str().unwrap();
    let (ledger, _) = workspace.ledger_to_write().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    assert_eq!(
        ledger.installations(entry, Kind::Skill).unwrap(),
        ["deploy"]
    );

    // As a gather holds it, clone and all, for as long as the network takes.
    let _held = workspace.lock().unwrap();
    let target = lifecycle::find(&project, &Reference::parse("deploy")).unwrap();
    let edit = lifecycle::start_edit(&target);
    let mut text = fs::read_to_string(edit.file()).unwrap();
    text.push_str("Mine.\n");
    fs::write(edit.file(), text).unwrap();
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    // An edit only reads the record, which it keeps.
    assert!(edited.changed);
    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(
        ledger.installations(entry, Kind::Skill).unwrap(),
        ["deploy"]
    );

    let target = lifecycle::find(&project, &Reference::parse("deploy")).unwrap();
    let removed = lifecycle::remove(Some(&workspace), &project, &target).unwrap();

    // The removal is made whatever the lock says, so a record drop that
    // waited for it and gave up would leave a record of a copy gone, which
    // whatever is written there next would inherit.
    assert_eq!(removed.copy.unwrap(), Record::Dropped);
    assert!(ledger.installations(entry, Kind::Skill).unwrap().is_empty());
}
