//! Watching what a backend names, and telling the bus when it changes.
//!
//! Free of axum, as the bus is, so a desktop shell (D-17) watches with the
//! same code. Which paths to watch is the backend's to say
//! ([`Backend::watched_paths`](crate::Backend::watched_paths)): this knows
//! nothing of a project or a skill, only files, folders and the system's
//! watches on them.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify::{ErrorKind, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventHandler, DebounceEventResult, DebouncedEvent, Debouncer,
    RecommendedCache,
};

use crate::events::{Event, Events};
use crate::Watched;

/// How long a change waits for others before it is told, so a burst, a
/// command writing a dozen files or an editor saving through a temporary
/// file, is one event and one snapshot rather than a dozen.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// Watches what a backend names, until dropped, and tells the bus each time
/// something changes, and how things are watched.
///
/// - **Each folder on its own** ([`RecursiveMode::NonRecursive`]), never a
///   tree: a recursive watch costs one of the system's watches for every
///   folder under it, and wakes for every file a build writes.
/// - **A path that is not there yet** is watched from the nearest folder
///   above it that is, for its being made; once it is, the list is asked
///   for again and it is watched itself.
/// - **Asked for again after every change it tells**, since a change can
///   name new paths, and a folder an event named is watched again: a watch
///   stays with the folder it was set on, so one renamed away, or replaced
///   by a rename over it, as an install puts a copy in place, takes the
///   watch with it.
/// - **What a read does is no change.** A file opened or closed unwritten
///   is passed over, or a snapshot reading a watched file would set off
///   the next snapshot, without end.
/// - **A watch that cannot be set** is said, once, as
///   [`Event::Unwatched`], the rest kept, and tried again with the next
///   change; never a failure of the server. The system's limit on watches
///   is the usual one.
pub struct Watcher {
    /// What stops the thread, which holds everything else.
    stop: Option<mpsc::Sender<Message>>,
}

impl Watcher {
    /// Watch every path `paths` names, asking it again after each change,
    /// and tell `events`.
    ///
    /// The first watches are set before this returns, so a change made as
    /// soon as it has is told. A list with nothing in it costs no watcher
    /// at all, and is said to be watched.
    pub fn start(paths: impl Fn() -> Vec<Watched> + Send + 'static, events: Events) -> Watcher {
        let unwatched = |events: &Events, reason: String| {
            events.publish(Event::Unwatched { reason });
            Watcher { stop: None }
        };
        let watched = paths();
        if watched.is_empty() {
            events.publish(Event::Watching);
            return Watcher { stop: None };
        }
        let (stop, messages) = mpsc::channel();
        let debouncer = match new_debouncer(DEBOUNCE, None, Handler(stop.clone())) {
            Ok(debouncer) => debouncer,
            Err(error) => return unwatched(&events, format!("nothing can be watched: {error}")),
        };
        let mut state = State {
            watches: debouncer,
            paths: Box::new(paths),
            plan: Plan::default(),
            watched: BTreeSet::new(),
            events: events.clone(),
            said: None,
        };
        state.apply(&watched, &[]);
        let spawned = thread::Builder::new()
            .name(String::from("mindflayer-watch"))
            .spawn(move || state.run(&messages));
        match spawned {
            Ok(_) => Watcher { stop: Some(stop) },
            Err(error) => unwatched(&events, format!("nothing can be watched: {error}")),
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            let _ = stop.send(Message::Stop);
        }
    }
}

/// What the watcher's thread is handed: what the debouncer took in, or the
/// word to stop.
enum Message {
    Changes(DebounceEventResult),
    Stop,
}

/// Hands the debouncer's reports to the watcher's thread, which owns the
/// watches and so can change them; the debouncer's own thread cannot.
struct Handler(mpsc::Sender<Message>);

impl DebounceEventHandler for Handler {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.0.send(Message::Changes(event));
    }
}

/// What sets and drops the system's watches, each on one folder alone: the
/// debouncer, or a test's stand-in for it.
trait Watches {
    fn watch(&mut self, folder: &Path) -> notify::Result<()>;
    fn unwatch(&mut self, folder: &Path) -> notify::Result<()>;
}

impl Watches for Debouncer<RecommendedWatcher, RecommendedCache> {
    fn watch(&mut self, folder: &Path) -> notify::Result<()> {
        Debouncer::watch(self, folder, RecursiveMode::NonRecursive)
    }

    fn unwatch(&mut self, folder: &Path) -> notify::Result<()> {
        Debouncer::unwatch(self, folder)
    }
}

/// The watcher's thread: its watches, the plan they were set from, and
/// what it last said of them.
struct State<W> {
    watches: W,
    paths: Box<dyn Fn() -> Vec<Watched> + Send>,
    plan: Plan,
    /// Every folder watched now.
    watched: BTreeSet<PathBuf>,
    events: Events,
    /// The last [`Event::Watching`] or [`Event::Unwatched`] told, so each
    /// is told when it changes and not with every change.
    said: Option<Event>,
}

impl<W: Watches> State<W> {
    fn run(mut self, messages: &mpsc::Receiver<Message>) {
        // The debouncer holds a sender, so the channel stays open as long as
        // this does: only `Stop` ends it.
        while let Ok(Message::Changes(changes)) = messages.recv() {
            match changes {
                Ok(changes) => self.changed(&changes),
                // Its errors say events may have been lost, which only asking
                // for everything makes good.
                Err(_) => self.changed_everywhere(),
            }
        }
    }

    /// Tell what `changes` reached, once the watches are set for the list
    /// as it is after them.
    fn changed(&mut self, changes: &[DebouncedEvent]) {
        if changes.iter().any(|change| change.need_rescan()) {
            return self.changed_everywhere();
        }
        let mut reached = BTreeSet::new();
        let mut named = Vec::new();
        for change in changes.iter().filter(|change| counts(&change.kind)) {
            for path in &change.paths {
                reached.extend(self.plan.reached(path));
                named.push(path.clone());
            }
        }
        if reached.is_empty() {
            return;
        }
        let scopes = reached
            .into_iter()
            .map(|scope| self.plan.scopes[scope].clone())
            .collect();
        self.again(&named);
        self.events.publish(Event::Changed { scopes });
    }

    /// Tell every scope changed: the system dropped events, so anything
    /// may have.
    fn changed_everywhere(&mut self) {
        let mut scopes = self.plan.scopes.clone();
        self.again(&[]);
        for scope in &self.plan.scopes {
            if !scopes.contains(scope) {
                scopes.push(scope.clone());
            }
        }
        self.events.publish(Event::Changed { scopes });
    }

    /// Ask for the list again and watch what it names, before the change
    /// is told, so whatever the panel reads on hearing it, every change
    /// after is watched for.
    fn again(&mut self, named: &[PathBuf]) {
        let watched = (self.paths)();
        self.apply(&watched, named);
    }

    /// Set the watches `watched` asks for, dropping those it no longer
    /// does, and setting again each folder in `named`, which an event
    /// named as made, removed or renamed; then say how things are watched,
    /// if that changed.
    fn apply(&mut self, watched: &[Watched], named: &[PathBuf]) {
        let plan = Plan::of(watched, Path::is_dir);
        let dropped: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|folder| !plan.folders.contains(*folder) || named.contains(*folder))
            .cloned()
            .collect();
        for folder in dropped {
            // One the system dropped itself, with the folder, is no failure.
            let _ = self.watches.unwatch(&folder);
            self.watched.remove(&folder);
        }
        let mut failed = None;
        for folder in &plan.folders {
            if self.watched.contains(folder) {
                continue;
            }
            match self.watches.watch(folder) {
                Ok(()) => {
                    self.watched.insert(folder.clone());
                }
                // Gone between looking and watching: the folder above it is
                // watched, and says so.
                Err(error) if is_gone(&error) => {}
                Err(error) => {
                    let limit = matches!(error.kind, ErrorKind::MaxFilesWatch);
                    failed = Some(unwatched(folder, &error));
                    // Every watch after it would be refused the same way.
                    if limit {
                        break;
                    }
                }
            }
        }
        self.plan = plan;
        let state = failed.unwrap_or(Event::Watching);
        if self.said.as_ref() != Some(&state) {
            self.events.publish(state.clone());
            self.said = Some(state);
        }
    }
}

/// Whether a change of this kind is one: anything but a file opened, read
/// or closed unwritten, which a snapshot does to every file it reads.
fn counts(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(_) | EventKind::Other => false,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => true,
    }
}

fn is_gone(error: &notify::Error) -> bool {
    match &error.kind {
        ErrorKind::PathNotFound => true,
        ErrorKind::Io(error) => error.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

/// What the panel is told when `folder` cannot be watched.
///
/// The folder named once: notify's own words for an error end with the
/// paths it was about, which here is the folder again.
fn unwatched(folder: &Path, error: &notify::Error) -> Event {
    let why = |said: &str| format!("{} cannot be watched: {said}", folder.display());
    let reason = match &error.kind {
        // On Linux the limit has a name somebody can raise.
        ErrorKind::MaxFilesWatch if cfg!(target_os = "linux") => String::from(
            "the system's limit on watched folders is reached (fs.inotify.max_user_watches)",
        ),
        ErrorKind::MaxFilesWatch => {
            String::from("the system's limit on watched folders is reached")
        }
        ErrorKind::Io(error) => why(&error.to_string()),
        ErrorKind::Generic(said) => why(said),
        kind => why(&format!("{kind:?}")),
    };
    Event::Unwatched { reason }
}

/// Which folders to watch for a backend's list, and which change is to
/// which scope.
#[derive(Default)]
struct Plan {
    /// Every scope the list names, each once, in the list's order.
    scopes: Vec<serde_json::Value>,
    /// The paths a change to which counts, each with its scopes: every path
    /// listed, and each folder on the way to one that is not there yet.
    paths: HashMap<PathBuf, BTreeSet<usize>>,
    /// The folders listed, a change to whatever is directly in which
    /// counts, each with its scopes.
    within: HashMap<PathBuf, BTreeSet<usize>>,
    /// Every folder to watch, each on its own: each folder listed that is
    /// there, and the nearest folder above each path listed that is.
    folders: BTreeSet<PathBuf>,
}

impl Plan {
    /// The plan for `watched`, asking `is_folder` what each path is.
    fn of(watched: &[Watched], is_folder: impl Fn(&Path) -> bool) -> Plan {
        let mut plan = Plan::default();
        for each in watched {
            let scope = match plan.scopes.iter().position(|scope| *scope == each.scope) {
                Some(scope) => scope,
                None => {
                    plan.scopes.push(each.scope.clone());
                    plan.scopes.len() - 1
                }
            };
            plan.paths
                .entry(each.path.clone())
                .or_default()
                .insert(scope);
            if is_folder(&each.path) {
                plan.within
                    .entry(each.path.clone())
                    .or_default()
                    .insert(scope);
                plan.folders.insert(each.path.clone());
            }
            // The folder above, which sees it made, removed or renamed; or,
            // where that is not there either, the nearest that is, which
            // sees the first folder on the way made.
            let mut below = each.path.as_path();
            while let Some(above) = below.parent() {
                if is_folder(above) {
                    plan.folders.insert(above.to_path_buf());
                    break;
                }
                plan.paths
                    .entry(above.to_path_buf())
                    .or_default()
                    .insert(scope);
                below = above;
            }
        }
        plan
    }

    /// The scopes a change to `path` reaches: those of the path itself, and
    /// of a folder listed that it is directly in.
    fn reached(&self, path: &Path) -> BTreeSet<usize> {
        let mut scopes = self.paths.get(path).cloned().unwrap_or_default();
        if let Some(within) = path.parent().and_then(|folder| self.within.get(folder)) {
            scopes.extend(within);
        }
        scopes
    }
}

#[cfg(test)]
#[path = "watcher_test.rs"]
mod tests;
