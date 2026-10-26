//! Cloning a git repository into the workspace cache.
//!
//! `gix` rather than shelling out to `git`: nothing here depends on a `git`
//! being installed or on which one is first on the PATH, and core keeps doing
//! its own I/O rather than supervising a process.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use thiserror::Error;

/// A clone that is on disk and ready to be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clone {
    /// The working tree.
    pub root: PathBuf,
    /// The commit that was checked out, when it could be resolved.
    pub revision: Option<String>,
}

/// Clone `url` into `into`, replacing whatever was there.
///
/// A fetch into the existing clone is the obvious optimisation and is
/// deliberately not done: the clone is shallow, so re-cloning costs about what
/// a fetch would, and it is one code path rather than three (clone, fetch,
/// reconcile a checkout that someone may have edited). Gathering is not
/// something anybody runs in a loop.
pub fn clone(url: &str, reference: Option<&str>, into: &Path) -> Result<Clone, GitError> {
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

    let mut prepare = gix::prepare_clone(url, into)
        .map_err(|error| GitError::Url {
            url: url.to_owned(),
            detail: error.to_string(),
        })?
        // Depth 1: the history is not what is being gathered, the files are.
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("1 is not zero"),
        ));

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
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &interrupt)
        .map_err(|error| GitError::Fetch {
            url: url.to_owned(),
            detail: error.to_string(),
        })?;
    let (repository, _) = checkout
        .main_worktree(gix::progress::Discard, &interrupt)
        .map_err(|error| GitError::Checkout {
            url: url.to_owned(),
            detail: error.to_string(),
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
    #[error("{url}: cloned, but its files could not be checked out: {detail}")]
    Checkout { url: String, detail: String },
}
