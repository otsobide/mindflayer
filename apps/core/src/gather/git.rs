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
use std::sync::atomic::AtomicBool;

use thiserror::Error;

use super::credentials::{self, Shortfall, Shortfalls};
use crate::url::redacted;
use crate::workspace::{replace_file, WorkspaceError};

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

    // Nothing here can be interrupted from outside yet, so the flag is a
    // constant. It is what gix asks for, and a shared one is what a `Ctrl-C`
    // handler would set later.
    let interrupt = AtomicBool::new(false);
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &interrupt)
        .map_err(|error| fetch_failed(url, shortfalls.last(), &error))?;
    let (repository, _) = checkout
        .main_worktree(gix::progress::Discard, &interrupt)
        .map_err(|error| GitError::Checkout {
            url: redacted(url).into_owned(),
            detail: said(error),
        })?;
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
    /// A file in the clone that names the URL it was made from could not be
    /// read, or rewritten without the credentials in it.
    #[error(transparent)]
    Scrub(#[from] WorkspaceError),
}

#[cfg(test)]
#[path = "git_test.rs"]
mod tests;
