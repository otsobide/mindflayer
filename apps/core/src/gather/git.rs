//! Cloning a git repository into the workspace cache.
//!
//! `gix` rather than shelling out to `git`: nothing here depends on a `git`
//! being installed or on which one is first on the PATH, and core keeps doing
//! its own I/O rather than supervising a process. A clone never prompts for a
//! credential; [`super::credentials`] says why.

use std::borrow::Cow;
use std::fmt::Display;
use std::fs;
use std::io;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use gix::progress::{self as gix_progress, tree};
use thiserror::Error;

use super::credentials::{self, Shortfall, Shortfalls};
use super::{Progress, Watch};
use crate::url::redacted;
use crate::workspace::{replace_file, WorkspaceError};

/// How often `gix`'s counts are read while it fetches: ten times a second,
/// about as often as a person can read a number that changes.
const POLL: Duration = Duration::from_millis(100);

/// A clone that is on disk and ready to be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clone {
    /// The working tree.
    pub root: PathBuf,
    /// The commit that was checked out, when it could be resolved.
    pub revision: Option<String>,
}

/// Clone `url` into `into`, replacing whatever was there, telling `watch`
/// what `gix` counts as it goes.
///
/// A fetch into the existing clone is the obvious optimisation and is
/// deliberately not done: the clone is shallow, so re-cloning costs about what
/// a fetch would, and it is one code path rather than three (clone, fetch,
/// reconcile a checkout that someone may have edited). Gathering is not
/// something anybody runs in a loop.
pub fn clone(
    url: &str,
    reference: Option<&str>,
    into: &Path,
    watch: &Watch,
) -> Result<Clone, GitError> {
    // Before the clone already there is removed, which a cancel should not
    // cost.
    if watch.is_cancelled() {
        return Err(GitError::Cancelled);
    }
    if into.exists() {
        std::fs::remove_dir_all(into).map_err(|source| GitError::Cache {
            path: into.to_path_buf(),
            source,
        })?;
    }
    if let Some(parent) = into.parent() {
        std::fs::create_dir_all(parent).map_err(|source| GitError::Cache {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let shortfalls = Shortfalls::default();
    let mut prepare = gix::prepare_clone(url, into)
        .map_err(|error| GitError::Url {
            url: redacted(url).into_owned(),
            detail: said(error),
        })?
        // Depth 1: the history is not what is being gathered, the files are.
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("1 is not zero"),
        ))
        // Every connection `gix` lets the clone configure, not only the
        // first: a shallow clone connects once to find the branch and again
        // to fetch it, and either one may be asked for a credential. The one
        // it does not is a retry made when the remote has a branch named
        // `HEAD`, which asks the way `gix` does by default, prompt included;
        // docs/architecture.md keeps it among the open questions.
        .configure_connection({
            let shortfalls = shortfalls.clone();
            move |connection| {
                let repository = connection.remote().repo().clone();
                connection.set_credentials(credentials::never_prompting(
                    credentials::configured(repository),
                    shortfalls.clone(),
                ));
                Ok(())
            }
        });

    if let Some(reference) = reference {
        prepare = prepare
            .with_ref_name(Some(reference))
            .map_err(|error| GitError::Reference {
                reference: reference.to_owned(),
                detail: said(error),
            })?;
    }

    // The watch's own flag, so a cancel stops `gix` where it checks, between
    // the objects it receives and the files it writes, rather than once it
    // is done. A clone it stops partway is removed by `gix` itself.
    let interrupt = watch.flag();
    let (mut checkout, _) = counted(watch, |progress| {
        prepare
            .fetch_then_checkout(progress, interrupt)
            .map_err(|error| {
                if interrupt.load(Ordering::Relaxed) {
                    GitError::Cancelled
                } else {
                    fetch_failed(url, shortfalls.last(), &error)
                }
            })
    })?;
    // Said before the checkout starts rather than once its count is first
    // read, so the step is named even when it is over before that.
    watch.report(Progress::CheckingOut {
        done: 0,
        total: None,
    });
    let checked_out = counted(watch, |progress| {
        checkout
            .main_worktree(progress, interrupt)
            .map_err(|error| GitError::Checkout {
                url: redacted(url).into_owned(),
                detail: said(error),
            })
    });
    // A checkout `gix` stops partway still succeeds, with some of the files
    // written, and is then no longer `gix`'s to remove. What will not go is
    // replaced by the next clone, as every clone replaces what is there.
    if interrupt.load(Ordering::Relaxed) {
        drop(checkout);
        let _ = std::fs::remove_dir_all(into);
        return Err(GitError::Cancelled);
    }
    let (repository, _) = checked_out?;
    // Nothing to do for a URL `gather` let through, which carries no
    // credential. Done here anyway, so that a clone keeps none on disk
    // whoever asks for one, rather than because every caller remembered.
    scrub(into)?;

    Ok(Clone {
        root: repository
            .workdir()
            .map_or_else(|| into.to_path_buf(), Path::to_path_buf),
        // A repository with no commits has no HEAD to resolve, and that is not
        // a failure: it is an empty source, which the harvest then reports as
        // having nothing in it.
        revision: repository.head_id().ok().map(|id| id.to_string()),
    })
}

/// Run `work` with a progress tree of its own, telling `watch` what `gix`
/// counts in it until `work` returns.
///
/// The tree is read from a thread of its own, as it is made to be, rather
/// than through a progress type that reports each time it is told something:
/// `gix` counts most of its work on shared atomics, from threads of its own,
/// without telling the progress anything. That thread is joined before this
/// returns, so nothing it reports can arrive after what the gather says next.
fn counted<T>(watch: &Watch, work: impl FnOnce(tree::Item) -> T) -> T {
    let root = tree::Root::new();
    let progress = root.add_child("gather");
    let (finished, done) = mpsc::channel::<()>();
    thread::scope(|scope| {
        let root = &root;
        scope.spawn(move || {
            let mut tasks = Vec::new();
            let mut said = None;
            // Until `finished` goes, which `recv_timeout` sees at once rather
            // than at the end of the wait.
            while let Err(RecvTimeoutError::Timeout) = done.recv_timeout(POLL) {
                root.sorted_snapshot(&mut tasks);
                // Only what changed, so a count that stands still is said
                // once rather than ten times a second.
                if let Some(now) = reading(&tasks).filter(|now| said != Some(*now)) {
                    watch.report(now);
                    said = Some(now);
                }
            }
        });
        let result = work(progress);
        drop(finished);
        result
    })
}

/// The step a snapshot of `gix`'s progress tree shows, in core's words: the
/// latest of receiving the pack, resolving it, and checking out its files
/// that is under way, with its count.
///
/// Picked out by the identifiers `gix` gives those counts rather than by the
/// names it shows, which it is free to reword. What else the tree holds
/// (bytes read, what the server says it is doing) is left out: the object
/// and file counts are the ones with a total, and so the ones that say how
/// far there is to go.
fn reading(
    tasks: &[(gix_progress::prodash::progress::Key, gix_progress::Task)],
) -> Option<Progress> {
    let count = |id: gix_progress::Id| {
        tasks
            .iter()
            .filter(|(_, task)| task.id == id)
            .find_map(|(_, task)| task.progress.as_ref())
            .map(|value| (value.step.load(Ordering::Relaxed), value.done_at))
    };
    let checkout: gix_progress::Id =
        gix::clone::checkout::main_worktree::ProgressId::CheckoutFiles.into();
    let resolve: gix_progress::Id = gix::odb::pack::index::write::ProgressId::ResolveObjects.into();
    let receive: gix_progress::Id = gix::odb::pack::index::write::ProgressId::IndexObjects.into();
    if let Some((done, total)) = count(checkout) {
        Some(Progress::CheckingOut { done, total })
    } else if let Some((done, total)) = count(resolve) {
        Some(Progress::Resolving { done, total })
    } else {
        count(receive).map(|(done, total)| Progress::Receiving { done, total })
    }
}

/// Rewrite what a clone keeps of the URL it was made from without the
/// credentials in it, returning whether there were any.
///
/// `gix` writes that URL as typed into the remote's entry in `.git/config`,
/// and again into every reflog, as `clone: from <url>`; `FETCH_HEAD` would
/// hold it too. Those files are text and the URL is in them whole, so they
/// are rewritten through [`redacted`] rather than through `gix`'s config
/// editor, which would reach the config and not the logs. Deleting the clone
/// was the other way to be rid of them, and is rejected: the clone is kept so
/// a gather can be looked at afterwards, and a scrub has no reason to take
/// that away. What is missing is an absence, not a failure, and a symlink is
/// not followed out of the clone.
pub fn scrub(clone: &Path) -> Result<bool, GitError> {
    let git = clone.join(".git");
    let mut files = vec![git.join("config"), git.join("FETCH_HEAD")];
    files_under(&git.join("logs"), &mut files)?;

    let mut scrubbed = false;
    for file in files {
        if !fs::symlink_metadata(&file).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let bytes = fs::read(&file).map_err(|source| WorkspaceError::Read {
            path: file.clone(),
            source,
        })?;
        // What `gix` writes there is UTF-8, URLs included. A file that is
        // not was written by something else, and is left as it was.
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        if let Cow::Owned(clean) = redacted(&text) {
            replace_file(&file, &clean)?;
            scrubbed = true;
        }
    }
    Ok(scrubbed)
}

/// Every plain file under `directory`, at any depth, without following a
/// symlink.
fn files_under(directory: &Path, found: &mut Vec<PathBuf>) -> Result<(), GitError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(WorkspaceError::Read {
                path: directory.to_path_buf(),
                source,
            }
            .into())
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| WorkspaceError::Read {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files_under(&path, found)?,
            Ok(kind) if kind.is_file() => found.push(path),
            _ => {}
        }
    }
    Ok(())
}

/// What a failed fetch is reported as: the credential it lacked, when that is
/// what stopped it, and otherwise what `gix` said.
fn fetch_failed(url: &str, shortfall: Option<Shortfall>, detail: &dyn Display) -> GitError {
    let url = redacted(url).into_owned();
    match shortfall {
        Some(Shortfall::Missing) => GitError::Credentials { url },
        // Whether a refusal over ssh is a missing key is the callback's to
        // say, not `url`'s: a rewrite in git config may have sent the fetch
        // over another transport than the one `url` names.
        Some(Shortfall::Refused) | None => GitError::Fetch {
            url,
            detail: said(detail),
        },
    }
}

/// What `gix` said, without the credentials of any URL it repeats.
///
/// Redacting the URL that was typed is not enough: `gix` names the URL it
/// connected to, which `url.<base>.insteadOf` in git config may have given a
/// token the typed one never had.
fn said(error: impl Display) -> String {
    redacted(&error.to_string()).into_owned()
}

/// Why a repository could not be cloned.
///
/// Every variant carries the failure as text rather than as the `gix` error it
/// came from. Those types are large and would become part of this crate's
/// public API, tying its version to gix's; what a caller does with them is
/// print them. Every URL in that text, the one typed included, is
/// [`redacted`]: this is what the gather log and the screen are given.
#[derive(Debug, Error)]
pub enum GitError {
    #[error("{url}: not a URL git understands: {detail}")]
    Url { url: String, detail: String },
    #[error("`{reference}` is not a usable branch or tag name: {detail}")]
    Reference { reference: String, detail: String },
    #[error("{path}: the cache directory cannot be prepared: {source}")]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{url}: cannot be fetched: {detail}")]
    Fetch { url: String, detail: String },
    /// The server asked who is fetching, and nothing that may answer had a
    /// credential it would take. `url` is shown without its credentials.
    #[error(
        "{url}: needs credentials, and gathering never prompts for them: \
         configure a git credential helper, or use ssh-agent for an ssh URL"
    )]
    Credentials { url: String },
    #[error("{url}: cloned, but its files could not be checked out: {detail}")]
    Checkout { url: String, detail: String },
    /// The [`Watch`] was cancelled before the clone was finished. What there
    /// was of it is gone.
    #[error("the clone was cancelled before it was finished")]
    Cancelled,
    /// A file in the clone that names the URL it was made from could not be
    /// read, or rewritten without the credentials in it.
    #[error(transparent)]
    Scrub(#[from] WorkspaceError),
}

#[cfg(test)]
#[path = "git_test.rs"]
mod tests;
