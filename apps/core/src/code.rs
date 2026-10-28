//! A stable code for everything core can say went wrong: one word a front
//! end acts on, where the message is for whoever reads it.
//!
//! The terminal shows a message and nothing more is asked of it. The panel
//! has to tell a workspace another writer holds from a project that has gone,
//! and the server hands it what went wrong as JSON: matching the English
//! would break the first time a message is reworded, which happens whenever a
//! message is made clearer. So every public error type in core has a
//! `code()`, and `ValidationIssue`, what `validate` finds, has one too.
//!
//! - **Unique across core**, not only within one type: a code reaches the
//!   panel without the type it came from, so `workspace_busy` has to mean the
//!   lock whatever command met it. [`ALL`] lists every code, and a test holds
//!   no two alike.
//! - **A variant that carries another error of core's takes its code** when
//!   that error is what went wrong and the variant adds only where, or what
//!   came of it: an install that met a busy lock says `workspace_busy`, as
//!   the lock does, rather than a code of the install's that the panel would
//!   have to map back. A variant that says something the error it carries
//!   does not, a record dropped while its exclude entry stays, has a code of
//!   its own.
//! - **Named for the condition**, in snake_case, in as few words as tell it
//!   from every other: the variant's own name where that is enough
//!   (`offered_twice`), and the subject before it where the bare name says
//!   nothing alone (`exclude_read_failed`, `adopt_unfit`).
//! - **A contract once given**, as a serialized name is: a code is never
//!   renamed, nor moved to another condition, since a front end acting on
//!   it would stop without a word. A new variant gets a new code; rewording
//!   a message changes none.
//!
//! Each type is given its codes with `codes!`, which writes `code()` as a
//! match with no wildcard, so a new variant does not compile until it has
//! one, and the list of the codes it gives from the same arms, so a code
//! cannot be added without the uniqueness test seeing it. Each arm names one
//! variant, which the macro holds to by what it takes: a wildcard or an
//! or-pattern would give several variants one code, listed once, and the
//! test that no two codes are alike would never see it. Writing the match
//! by hand beside a list kept by hand was the other way, and a list can miss
//! a code its match gives without anything failing.

use std::fmt::Display;

use serde::Serialize;

use crate::activate::{ActivateError, UnknownShell};
use crate::adopt::AdoptError;
use crate::agent::UnknownAgent;
use crate::agents_md::{self, AgentsMdError};
use crate::artifact::{ArtifactError, ValidationIssue};
use crate::catalog::DiscoveryFailure;
use crate::claude_md::{self, ClaudeMdError, Fallback};
use crate::claude_rules::{self, ClaudeRulesError};
use crate::config_entry::{EntryError, FileError};
use crate::create::CreateError;
use crate::everywhere::EverywhereError;
use crate::exclude::{self, ExcludeError};
use crate::frontmatter::FrontMatterError;
use crate::gather::git::GitError;
use crate::gather::{self, GatherError, Leftover, ScrubFailure};
use crate::install::{rules, InstallError, LoadFailure};
use crate::kind::UnknownKind;
use crate::ledger::LedgerError;
use crate::lifecycle::LifecycleError;
use crate::lock::LockError;
use crate::migrate::{MigrateError, ProjectFailure, Refusal};
use crate::reapply::{ReapplyError, ReapplyFailure};
use crate::relink::{Duplicate, RelinkError};
use crate::resolve::ResolveError;
use crate::scan::ScanFailure;
use crate::status::StatusFailure;
use crate::template::TemplateError;
use crate::update::UpdateError;
use crate::workspace::{MisplacedTarget, ProjectWarning, WorkspaceError};

/// Give an error type its `code()` and its `CODES`.
///
/// An enum lists each variant with a code of its own, as a pattern on
/// `Self`, and then, after `carried`, each that takes the code of the error
/// it carries, with the binding that holds it:
///
/// ```text
/// codes!(LedgerError {
///     Self::Open { .. } => "ledger_open_failed",
/// } carried {
///     Self::Lock(error) => error,
/// });
/// ```
///
/// Each arm is `Self::` and one variant's name, with its fields in braces or
/// parentheses when it has any: what an arm may say is spelled out here
/// rather than taken as any pattern, so that `_` or `Self::A | Self::B`,
/// which would give several variants one code and list it once, does not
/// compile.
///
/// A struct, which is one condition, takes one code:
/// `codes!(UnknownKind => "unknown_kind");`. An enum given one that way
/// does not compile either.
macro_rules! codes {
    ($error:ident {
        $(Self::$variant:ident $(($($tuple:tt)*))? $({$($named:tt)*})? => $code:literal,)*
    } $(carried {
        $(Self::$carrier:ident $(($($carrier_tuple:tt)*))? $({$($carrier_named:tt)*})?
            => $carried:ident,)*
    })?) => {
        impl $error {
            /// The word for what went wrong, unique among every code core
            /// gives and never changed once given: what a front end acts
            /// on, where the message may be reworded.
            pub fn code(&self) -> &'static str {
                match self {
                    $(Self::$variant $(($($tuple)*))? $({$($named)*})? => $code,)*
                    $($(
                        Self::$carrier $(($($carrier_tuple)*))? $({$($carrier_named)*})?
                            => $carried.code(),
                    )*)?
                }
            }

            /// Every code this type gives of its own, leaving out those
            /// it takes from an error it carries: what [`crate::code::ALL`]
            /// gathers.
            pub const CODES: &'static [&'static str] = &[$($code),*];
        }
    };
    ($error:ident => $code:literal) => {
        impl $error {
            /// The word for what went wrong, unique among every code core
            /// gives and never changed once given: what a front end acts
            /// on, where the message may be reworded.
            pub fn code(&self) -> &'static str {
                // A pattern only a struct matches, so that an enum given
                // one code for all its variants does not compile.
                let $error { .. } = self;
                $code
            }

            /// The one code this type gives: what [`crate::code::ALL`]
            /// gathers.
            pub const CODES: &'static [&'static str] = &[$code];
        }
    };
}

pub(crate) use codes;

/// Every type core gives codes to, by the module it is defined in, with the
/// codes it gives of its own.
///
/// What the test that no two codes are alike reads, and what a front end
/// that lists every code core can give, the panel's types among them (#70),
/// starts from. A new error type joins it; a test finds one that did not.
pub const ALL: &[(&str, &[&str])] = &[
    ("activate::ActivateError", ActivateError::CODES),
    ("activate::UnknownShell", UnknownShell::CODES),
    ("adopt::AdoptError", AdoptError::CODES),
    ("agent::UnknownAgent", UnknownAgent::CODES),
    ("agents_md::AgentsMdError", AgentsMdError::CODES),
    ("agents_md::Malformed", agents_md::Malformed::CODES),
    ("agents_md::Warning", agents_md::Warning::CODES),
    ("artifact::ArtifactError", ArtifactError::CODES),
    ("artifact::ValidationIssue", ValidationIssue::CODES),
    ("catalog::DiscoveryFailure", DiscoveryFailure::CODES),
    ("claude_md::ClaudeMdError", ClaudeMdError::CODES),
    ("claude_md::Fallback", Fallback::CODES),
    ("claude_md::Warning", claude_md::Warning::CODES),
    ("claude_rules::ClaudeRulesError", ClaudeRulesError::CODES),
    ("claude_rules::Warning", claude_rules::Warning::CODES),
    ("config_entry::EntryError", EntryError::CODES),
    ("config_entry::file::FileError", FileError::CODES),
    ("create::CreateError", CreateError::CODES),
    ("everywhere::EverywhereError", EverywhereError::CODES),
    ("exclude::ExcludeError", ExcludeError::CODES),
    ("exclude::Malformed", exclude::Malformed::CODES),
    ("frontmatter::FrontMatterError", FrontMatterError::CODES),
    ("gather::Failure", gather::Failure::CODES),
    ("gather::GatherError", GatherError::CODES),
    ("gather::Leftover", Leftover::CODES),
    ("gather::ScrubFailure", ScrubFailure::CODES),
    ("gather::git::GitError", GitError::CODES),
    ("install::InstallError", InstallError::CODES),
    ("install::LoadFailure", LoadFailure::CODES),
    ("install::rules::Warning", rules::Warning::CODES),
    ("install::rules::Withheld", rules::Withheld::CODES),
    ("kind::UnknownKind", UnknownKind::CODES),
    ("ledger::LedgerError", LedgerError::CODES),
    ("lifecycle::LifecycleError", LifecycleError::CODES),
    ("lock::LockError", LockError::CODES),
    ("migrate::MigrateError", MigrateError::CODES),
    ("migrate::Refusal", Refusal::CODES),
    ("migrate::project::ProjectFailure", ProjectFailure::CODES),
    ("reapply::ReapplyError", ReapplyError::CODES),
    ("reapply::ReapplyFailure", ReapplyFailure::CODES),
    ("relink::Duplicate", Duplicate::CODES),
    ("relink::RelinkError", RelinkError::CODES),
    ("resolve::ResolveError", ResolveError::CODES),
    ("scan::ScanFailure", ScanFailure::CODES),
    ("status::StatusFailure", StatusFailure::CODES),
    ("template::TemplateError", TemplateError::CODES),
    ("update::UpdateError", UpdateError::CODES),
    ("workspace::MisplacedTarget", MisplacedTarget::CODES),
    ("workspace::ProjectWarning", ProjectWarning::CODES),
    ("workspace::WorkspaceError", WorkspaceError::CODES),
];

/// An error as a serialized type holds one: its code, for a front end to act
/// on, and its message, for whoever reads it.
///
/// Built where the type is serialized rather than kept, since an error holds
/// what JSON has no shape for (an `io::Error`, a parser's own), and only
/// these two are meant to leave core.
#[derive(Debug, Serialize)]
pub(crate) struct Said {
    code: &'static str,
    message: String,
}

impl Said {
    /// `error`, by `code`, the one its `code()` gives.
    pub(crate) fn of(code: &'static str, error: &impl Display) -> Self {
        Said {
            code,
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "code_test.rs"]
mod tests;
