//! A server version a registry gave, written into a workspace's library
//! (#107): what `mindweaver gather mcp add` changes, `add`'s change once
//! the version is mapped to a definition
//! ([`ServerVersion::import`](crate::registry::ServerVersion::import)).
//!
//! Written as `add` writes a server ([`super::add`]): planned and held to
//! every rule a definition `add` writes is, its name to what sits beside
//! it in the library, made with `create_new`, and a record of an install
//! under its name that outlived its copy dropped. A version nothing maps
//! to a definition is refused rather than written in part, and a name the
//! library holds is refused as `add` refuses it, so what is there is left
//! as it is.

use std::path::PathBuf;

use thiserror::Error;

use super::{disowned, Added};
use crate::code::codes;
use crate::create::{self, scaffold, CreateError, Start};
use crate::gather::Scrub;
use crate::kind::Kind;
use crate::mcp::McpDefinition;
use crate::registry::{Manual, ServerVersion, Warning};
use crate::workspace::{WeaverWorkspace, WorkspaceError};

/// A server version mapped to a definition for a library, and where in it
/// the definition goes: what importing it writes, worked out and checked.
#[derive(Debug, Clone)]
pub struct Importing {
    /// The definition, named and described as it is written.
    pub definition: McpDefinition,
    /// What was left out of it, or is worth knowing of it, as the mapping
    /// said.
    pub warnings: Vec<Warning>,
    /// The file it is written in.
    pub file: PathBuf,
}

/// What importing `server` into the library of `workspace` would write,
/// named `name`, or after what the registry names it when no name is given,
/// without writing it: refused for what writing it would be refused for,
/// as far as can be told without writing, so a dry run says what the write
/// would. A read: nothing is made, the workspace's own folders included.
pub fn importing(
    workspace: &WeaverWorkspace,
    server: &ServerVersion,
    name: Option<&str>,
) -> Result<Importing, ImportError> {
    let imported = server.import(name);
    let Some(definition) = imported.definition else {
        return Err(ImportError::Unmapped {
            server: server.name.clone(),
            version: server.version.clone(),
            manual: imported.manual,
        });
    };
    let planned = create::plan(
        &workspace.own(),
        Kind::Mcp,
        &definition.name,
        &definition.description,
        Start::Definition(&definition),
    )?;
    if let Some(taken) = planned.taken() {
        return Err(taken.into());
    }
    Ok(Importing {
        file: planned.path().to_path_buf(),
        definition,
        warnings: imported.warnings,
    })
}

/// Write `server` into the library of `workspace`, as [`importing`] says
/// it would be, and drop any record of an install under its name, as
/// [`super::add`] does.
///
/// The definition is planned again and written holding the workspace's
/// lock, since the library is the workspace's and its writers take turns:
/// a migration of the library, say, plans what it moves under the lock.
/// The record is dropped once the lock is let go, as `add` drops it, since
/// opening the ledger to write may scrub it, which takes the lock, and the
/// lock is not re-entrant.
pub fn import(
    workspace: &WeaverWorkspace,
    server: &ServerVersion,
    name: Option<&str>,
) -> Result<(Importing, Added), ImportError> {
    let importing = importing(workspace, server, name)?;
    let own = workspace.own();
    let definition = &importing.definition;
    let artifact = {
        let _lock = workspace
            .lock()
            .map_err(|error| CreateError::from(WorkspaceError::from(error)))?;
        scaffold(
            &own,
            Kind::Mcp,
            &definition.name,
            &definition.description,
            Start::Definition(definition),
        )?
    };
    let mut scrub = Scrub::default();
    let stale = disowned(
        Some(workspace),
        &own,
        Kind::Mcp,
        &definition.name,
        artifact.path(),
        &mut scrub,
    );
    Ok((
        importing,
        Added {
            artifact,
            stale,
            scrub,
        },
    ))
}

/// Why a server version was not written into a library.
#[derive(Debug, Error)]
pub enum ImportError {
    /// Nothing it names maps to a definition, so there is nothing to
    /// write: each of `manual` says what to do by hand instead.
    #[error("`{server}` {version}: nothing it names maps to a definition Mindweaver writes, so nothing was written; it is set up by hand instead")]
    Unmapped {
        /// What the registry names it.
        server: String,
        version: String,
        manual: Vec<Manual>,
    },
    /// What `add` refuses of a definition, refused by its checks: a name
    /// the library holds among it, which is left as it is.
    #[error(transparent)]
    Create(#[from] CreateError),
}

codes!(ImportError {
    Self::Unmapped { .. } => "registry_import_unmapped",
} carried {
    Self::Create(error) => error,
});
