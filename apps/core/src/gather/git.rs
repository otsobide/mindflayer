//! Cloning a git repository into the workspace cache.
//!
//! `gix` rather than shelling out to `git`: nothing here depends on a `git`
//! being installed or on which one is first on the PATH, and core keeps doing
//! its own I/O rather than supervising a process. A clone never prompts for a
//! credential; [`super::credentials`] says why.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use gix::progress::{self as gix_progress, tree};
use thiserror::Error;

use super::credentials::{self, Shortfall, Shortfalls};
use super::{Progress, Watch};

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
            url: url.to_owned(),
            detail: error.to_string(),
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
                detail: error.to_string(),
            })?;
    }

    // Nothing here can be interrupted from outside yet, so the flag is a
    // constant. It is what gix asks for, and a shared one is what a `Ctrl-C`
    // handler would set later.
    let interrupt = AtomicBool::new(false);
    let (mut checkout, _) = counted(watch, |progress| {
        prepare
            .fetch_then_checkout(progress, &interrupt)
            .map_err(|error| fetch_failed(url, shortfalls.last(), error.to_string()))
    })?;
    // Said before the checkout starts rather than once its count is first
    // read, so the step is named even when it is over before that.
    watch.report(Progress::CheckingOut {
        done: 0,
        total: None,
    });
    let (repository, _) = counted(watch, |progress| {
        checkout
            .main_worktree(progress, &interrupt)
            .map_err(|error| GitError::Checkout {
                url: url.to_owned(),
                detail: error.to_string(),
            })
    })?;

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

/// What a failed fetch is reported as: the credential it lacked, when that is
/// what stopped it, and otherwise what `gix` said.
fn fetch_failed(url: &str, shortfall: Option<Shortfall>, detail: String) -> GitError {
    match shortfall {
        Some(Shortfall::Missing) => GitError::Credentials { url: redacted(url) },
        // Whether a refusal over ssh is a missing key is the callback's to
        // say, not `url`'s: a rewrite in git config may have sent the fetch
        // over another transport than the one `url` names.
        Some(Shortfall::Refused) | None => GitError::Fetch {
            url: url.to_owned(),
            detail,
        },
    }
}

/// `url` without the user name and password a `scheme://` URL can carry before
/// its host, so an error can name a source without repeating a token somebody
/// typed into it.
///
/// The scp-like form (`git@host:path`) is left alone: what it carries before
/// the `@` can only be a user name. Parsing with `gix::url` and dropping the
/// user was the other way, and is rejected because the parser also rewrites
/// what it keeps (a default port dropped, a scheme lowercased), so the error
/// would name a URL nobody typed.
fn redacted(url: &str) -> String {
    let Some(scheme) = url.find("://") else {
        return url.to_owned();
    };
    let host = scheme + "://".len();
    let rest = &url[host..];
    let authority = rest.find(['/', '?', '#']).map_or(rest, |end| &rest[..end]);
    match authority.rfind('@') {
        Some(at) => format!("{}{}", &url[..host], &rest[at + 1..]),
        None => url.to_owned(),
    }
}

/// Why a repository could not be cloned.
///
/// Every variant carries the failure as text rather than as the `gix` error it
/// came from. Those types are large and would become part of this crate's
/// public API, tying its version to gix's; what a caller does with them is
/// print them.
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
    /// credential it would take. `url` is shown without its userinfo.
    #[error(
        "{url}: needs credentials, and gathering never prompts for them: \
         configure a git credential helper, or use ssh-agent for an ssh URL"
    )]
    Credentials { url: String },
    #[error("{url}: cloned, but its files could not be checked out: {detail}")]
    Checkout { url: String, detail: String },
}

#[cfg(test)]
#[path = "git_test.rs"]
mod tests;
