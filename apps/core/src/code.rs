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
//! cannot be added without the uniqueness test seeing it. Writing the match
//! by hand beside a list kept by hand was the other way, and a list can miss
//! a code its match gives without anything failing.

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
/// A struct, which is one condition, takes one code:
/// `codes!(UnknownKind => "unknown_kind");`.
macro_rules! codes {
    ($error:ident {
        $($variant:pat => $code:literal,)*
    } $(carried {
        $($carrier:pat => $carried:ident,)*
    })?) => {
        impl $error {
            /// The word for what went wrong, unique among every code core
            /// gives and never changed once given: what a front end acts
            /// on, where the message may be reworded.
            pub fn code(&self) -> &'static str {
                match self {
                    $($variant => $code,)*
                    $($($carrier => $carried.code(),)*)?
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
                $code
            }

            /// The one code this type gives: what [`crate::code::ALL`]
            /// gathers.
            pub const CODES: &'static [&'static str] = &[$code];
        }
    };
}

pub(crate) use codes;

use crate::activate::{ActivateError, UnknownShell};
use crate::agent::UnknownAgent;
use crate::agents_md::{self, AgentsMdError};
use crate::artifact::{ArtifactError, ValidationIssue};
use crate::catalog::DiscoveryFailure;
use crate::claude_md::{self, ClaudeMdError, Fallback};
use crate::claude_rules::{self, ClaudeRulesError};
use crate::config_entry::{EntryError, FileError};
use crate::exclude::{self, ExcludeError};
use crate::frontmatter::FrontMatterError;
use crate::gather::git::GitError;
use crate::gather::{self, GatherError, Leftover, ScrubFailure};
use crate::install::{rules, InstallError, LoadFailure};
use crate::kind::UnknownKind;
use crate::ledger::LedgerError;
use crate::lock::LockError;
use crate::resolve::ResolveError;
use crate::template::TemplateError;
use crate::workspace::{MisplacedTarget, ProjectWarning, WorkspaceError};

/// Every type core gives codes to, by its path in the crate, with the codes
/// it gives of its own.
///
/// What the test that no two codes are alike reads, and what a front end
/// that lists every code core can give, the panel's types among them (#70),
/// starts from. A new error type joins it; a test finds one that did not.
pub const ALL: &[(&str, &[&str])] = &[
    ("activate::ActivateError", ActivateError::CODES),
    ("activate::UnknownShell", UnknownShell::CODES),
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
    ("lock::LockError", LockError::CODES),
    ("resolve::ResolveError", ResolveError::CODES),
    ("template::TemplateError", TemplateError::CODES),
    ("workspace::MisplacedTarget", MisplacedTarget::CODES),
    ("workspace::ProjectWarning", ProjectWarning::CODES),
    ("workspace::WorkspaceError", WorkspaceError::CODES),
];

#[cfg(test)]
#[path = "code_test.rs"]
mod tests;
