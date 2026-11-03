//! The ports: what core needs of the world beyond the files it reads and
//! writes, each as a trait core's rules are written against, with the
//! adapter a build runs with in [`crate::adapter`].
//!
//! The architecture is hexagonal. Core's rules (what a skill is, what an
//! install may overwrite, which copy is edited) sit in the middle and depend
//! on nothing outside them; whatever they need from outside is a port here,
//! a trait in core's own words; and an adapter on the edge implements it with
//! a library (`gix` for a [`Remote`], the system clock for a [`Clock`]). The
//! front ends, the CLI, the TUI and the panel's backend, are the other side's
//! adapters: they drive core through its functions and the command lines
//! every one of them runs (`commands::run_line`), and add nothing core has to
//! know. So a new store, transport or clock is an adapter written against a
//! port, and a new front end is one more driver of the same commands: neither
//! changes a rule.
//!
//! This is the seam D-14 chose for HTTP, kept for every port: an injectable
//! trait, a default adapter built in, and a fake in a test. A port speaks only
//! core's types. What an adapter's library says is turned into those at the
//! edge (an error into text, a commit into a [`Revision`]), rather than
//! carried through, which would tie core's public API to the library's
//! version and let a rule depend on how one library happens to work.
//!
//! The files themselves are not behind a port. What core promises about them
//! is how they are written (`create_new`, a temporary renamed into place, a
//! link never followed out of its holder), and those promises are made of the
//! operating system's own calls; an adapter that only seemed to keep them is
//! the failure a port here would invite. The ledger's store is not either,
//! yet: it is one concrete type, [`crate::ledger::Ledger`], whose public
//! methods are already the port it would become (docs/architecture.md,
//! "Ports and adapters").

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::gather::Watch;
use crate::value::Revision;
use crate::workspace::WorkspaceError;

/// Where a source's artifacts are fetched from: a repository a gather clones
/// into the workspace's cache, and later scrubs of anything a credential left
/// in it.
///
/// One method a step of a gather, rather than one that gathers: what a gather
/// does with what was fetched (reading it, holding it to the rules, placing
/// it on the shelf) is core's, and the same whichever remote fetched it.
pub trait Remote: Debug + Send + Sync {
    /// Fetch `url`, at `reference` when one is given, into `into`, replacing
    /// whatever was there, telling `watch` how it goes and stopping when it
    /// is cancelled ([`GitError::Cancelled`]).
    ///
    /// What is left at `into` holds no credential once this returns, whoever
    /// asked: a caller that forgot to refuse one is not trusted to.
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &Path,
        watch: &Watch,
    ) -> Result<Cloned, GitError>;

    /// Take out of the clone at `clone` every credential an older build left
    /// in what it wrote there, returning whether there were any.
    fn scrub(&self, clone: &Path) -> Result<bool, GitError>;
}

/// The time, for what the ledger stamps an action or a gather with.
///
/// A port rather than a call to the system clock where the time is read, so a
/// fixture can pin it and a test can tell one moment from the next without
/// waiting for a second to pass.
pub trait Clock: Debug + Send + Sync {
    /// Seconds since the Unix epoch.
    fn now(&self) -> i64;
}

/// A clone that is on disk and ready to be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloned {
    /// The working tree.
    pub root: PathBuf,
    /// The commit that was checked out, when it could be resolved.
    pub revision: Option<Revision>,
}

/// Why a repository could not be cloned.
///
/// Every variant carries the failure as text rather than as the error of the
/// library an adapter fetched with. Those types are large and would become
/// part of this crate's public API, tying its version to the library's; what a
/// caller does with them is print them. Every URL in that text, the one typed
/// included, is [`redacted`](crate::url::redacted): this is what the gather
/// log and the screen are given.
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

codes!(GitError {
    Self::Url { .. } => "git_url_invalid",
    Self::Reference { .. } => "git_reference_invalid",
    Self::Cache { .. } => "git_cache_failed",
    Self::Fetch { .. } => "git_fetch_failed",
    Self::Credentials { .. } => "git_credentials_needed",
    Self::Checkout { .. } => "git_checkout_failed",
    Self::Cancelled => "git_clone_cancelled",
} carried {
    Self::Scrub(error) => error,
});
