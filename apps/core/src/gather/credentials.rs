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

use gix::bstr::ByteSlice;
use gix::credentials::helper::{Action, Cascade};
use gix::credentials::protocol;
use gix::prompt;

/// The credentials callback a fetch is given: `helpers`, run with every
/// prompt turned off.
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
) -> impl FnMut(Action) -> protocol::Result {
    move |action| {
        helpers(
            action,
            prompt::Options {
                askpass: None,
                mode: prompt::Mode::Disable,
            },
        )
    }
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
