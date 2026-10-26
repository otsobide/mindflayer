//! What a fetch answers when the server asks who is fetching.
//!
//! `gix`'s own answer runs the credential helpers git config names and, when
//! none of them has a credential, asks a person: the askpass program, then the
//! terminal. Mindflayer keeps the first half and refuses the second. The TUI
//! owns the terminal while a gather runs, so a prompt there is a screen that
//! stops answering, and a server has no terminal at all. A credential nobody
//! has is an error instead, which a person can act on afterwards.
//!
//! Nothing here is public: the callback's types are `gix`'s, and a front end
//! only ever sees the error a missing credential becomes.

// The callback's error is `gix`'s, fixed by the signature it asks for, so
// boxing it as clippy suggests is not ours to do.
#![allow(clippy::result_large_err)]

use std::cell::Cell;
use std::rc::Rc;

use gix::bstr::ByteSlice;
use gix::credentials::helper::{Action, Cascade};
use gix::credentials::protocol;
use gix::prompt;

/// Why a fetch got no credential it could use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shortfall {
    /// The server asked, and nothing that may answer had one it would take:
    /// no helper had one to give, or, over ssh, ssh had no key it accepted.
    Missing,
    /// A helper gave one over http(s), and the server turned it down.
    Refused,
}

/// Where the callback leaves the last [`Shortfall`] it saw, for the clone to
/// read once its fetch has failed.
///
/// Shared rather than read off the fetch's error: that error says the same
/// thing several layers down a chain of `gix` types, and finding it there
/// would tie this crate to the shape of every one of them. An `Rc` rather than
/// an `Arc`, because `gix` asks for credentials on the thread that is cloning.
#[derive(Debug, Clone, Default)]
pub(super) struct Shortfalls(Rc<Cell<Option<Shortfall>>>);

impl Shortfalls {
    pub(super) fn last(&self) -> Option<Shortfall> {
        self.0.get()
    }
}

/// The credentials callback a fetch is given: `helpers`, run with every
/// prompt turned off, noting in `shortfalls` whatever they could not supply.
///
/// `helpers` takes the prompt options it is to run with, because that is where
/// `gix` decides whether to ask a person: [`configured`] is the real cascade,
/// and a test can stand in a cascade whose prompt panics. A config override
/// (`gitoxide.credentials.terminalPrompt=false`) would also stop the prompt,
/// and is rejected: the guarantee would rest on a string `gix` reads among the
/// user's own settings, where no test here can see it, and the credential a
/// front end supplies needs this callback to arrive through anyway.
pub(super) fn never_prompting(
    mut helpers: impl FnMut(Action, prompt::Options) -> protocol::Result,
    shortfalls: Shortfalls,
) -> impl FnMut(Action) -> protocol::Result {
    let mut over_ssh = false;
    move |action| {
        if let Action::Get(context) = &action {
            over_ssh = is_ssh(context);
        }
        // `gix` sends an `Erase` only once the server has refused what the
        // `Get` before it answered.
        let refused = matches!(action, Action::Erase(_));
        let answer = helpers(
            action,
            prompt::Options {
                askpass: None,
                mode: prompt::Mode::Disable,
            },
        );
        if refused {
            // ssh authenticates with keys of its own, and `gix` asks the
            // helpers only for a user name to hand it. A refusal there means
            // ssh had no key the server takes, which is as missing as a
            // credential gets. Over http(s) it is of a credential a helper did
            // give, and `gix` already says so.
            shortfalls.0.set(Some(if over_ssh {
                Shortfall::Missing
            } else {
                Shortfall::Refused
            }));
        } else if let Err(protocol::Error::IdentityMissing { .. }) = answer {
            shortfalls.0.set(Some(Shortfall::Missing));
        }
        answer
    }
}

/// Whether `context` asks about a URL that is fetched by running ssh, the
/// scp-like `host:path` included.
///
/// The `Get`'s URL rather than the one the gather was given, because it is
/// the one `gix` connected to: git config's `url.<base>.insteadOf` rewrites a
/// typed https URL to ssh, or the reverse, before the transport is chosen.
fn is_ssh(context: &protocol::Context) -> bool {
    context
        .url
        .clone()
        .or_else(|| context.to_url())
        .and_then(|url| gix::url::parse(url.as_bstr()).ok())
        .is_some_and(|url| url.scheme == gix::url::Scheme::Ssh)
}

/// The credential helpers git config names, run the way `gix` runs them when
/// it is given no callback.
///
/// The cascade is built for each `Get` from the URL the transport is at, which
/// a redirect may have changed and which `credential.<url>.*` sections match
/// against. It is kept for the `Store` or `Erase` that follows, which tells
/// the same helpers whether their answer was accepted.
pub(super) fn configured(
    repository: gix::Repository,
) -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    let mut current: Option<Cascade> = None;
    move |action, prompt| {
        if let Action::Get(context) = &action {
            let url = context
                .url
                .clone()
                .or_else(|| context.to_url())
                .ok_or(protocol::Error::UrlMissing)?;
            // The prompt options git config asks for come back too, and are
            // dropped: they are exactly what is being refused.
            let (cascade, _, _) = repository
                .config_snapshot()
                .credential_helpers(gix::url::parse(url.as_bstr())?)
                .map_err(|source| protocol::Error::ConfigureCredentialHelpers {
                    source: Box::new(source),
                })?;
            current = Some(cascade);
        }
        match current.as_mut() {
            Some(cascade) => cascade.invoke(action, prompt),
            // A `Store` or `Erase` with no `Get` before it reports on an
            // answer no helper gave.
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "credentials_test.rs"]
mod tests;
