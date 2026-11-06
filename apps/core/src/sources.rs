//! A gathered source's own changes, each a source named as `gather list`
//! shows it and `--from` takes it (#100): renaming one. Listing them is the
//! ledger's ([`LedgerStore::source_listing`](crate::port::LedgerStore::source_listing)).
//!
//! A source is found by the name it is called, word for word, among those
//! the ledger keeps, never by its URL or its folder: the name is what a
//! listing shows, and one word a person types for one source (D-13).

use thiserror::Error;

use crate::code::codes;
use crate::ledger::{Ledger, LedgerError, Source};
use crate::lock::LockError;
use crate::value::{Origin, SourceName};
use crate::workspace::WeaverWorkspace;

/// The source called `name`, as the ledger records it.
///
/// Compared with each recorded name as it is, rather than held to
/// [`SourceName::parse`] first: a name an older build recorded is the
/// source's still, and a word no source is called is no source either way.
pub fn named(ledger: &Ledger, name: &str) -> Result<Source, SourceError> {
    ledger
        .sources()?
        .into_iter()
        .find(|source| source.name.as_str() == name)
        .ok_or_else(|| SourceError::NotFound {
            name: name.to_owned(),
        })
}

/// What [`rename`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// What it was called.
    pub was: SourceName,
    /// The source, called what it is called now.
    pub source: Source,
}

impl Renamed {
    /// Whether it was called that already, and nothing changed.
    pub fn unchanged(&self) -> bool {
        self.was == self.source.name
    }
}

/// Call the source `from` names `to` from now on, held to the rules a name
/// is held to when `gather git --name` gives one ([`SourceName::parse`]),
/// and refused with the same codes: what `gather list` shows of what it
/// supplied, and what `--from` takes for it, change with it.
///
/// The origin every install from it recorded stays its URL, as a rename by
/// `--name` leaves it, so what was installed is still found where it came
/// from. Called what it is called already, it changes nothing. Held under
/// the workspace's lock, from finding the source to renaming it, so two
/// renames to one name cannot both find it free.
pub fn rename(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    from: &str,
    to: &str,
) -> Result<Renamed, SourceError> {
    let new = SourceName::parse(to).ok_or_else(|| SourceError::NameInvalid {
        name: to.to_owned(),
    })?;
    let _lock = workspace.lock()?;
    let source = named(ledger, from)?;
    let was = source.name.clone();
    if new == was {
        return Ok(Renamed { was, source });
    }
    if let Some(holder) = ledger.source_named(&new)? {
        return Err(SourceError::NameTaken {
            name: new,
            origin: holder.origin(),
        });
    }
    ledger.rename_source(source.id, &new)?;
    Ok(Renamed {
        was,
        source: Source {
            name: new,
            ..source
        },
    })
}

/// Why a change to a source could not be made, with nothing changed.
#[derive(Debug, Error)]
pub enum SourceError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// No source is called that.
    #[error("no source is called `{name}`: `mindweaver gather sources` lists them by name")]
    NotFound { name: String },
    /// What no source can be called ([`SourceName::parse`]): the condition
    /// `gather git --name` meets, said for a rename.
    #[error(
        "`{name}` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    )]
    NameInvalid { name: String },
    /// Another source is called that already, at `origin`: the condition
    /// `gather git --name` meets, said for a rename.
    #[error("`{name}` is the name of the source at {origin} already: rename this one to another")]
    NameTaken { name: SourceName, origin: Origin },
}

codes!(SourceError {
    Self::NotFound { .. } => "source_not_found",
} carried {
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
} echoed {
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
});

#[cfg(test)]
#[path = "sources_test.rs"]
mod tests;
