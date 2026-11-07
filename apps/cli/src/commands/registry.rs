//! `mindweaver gather mcp`: the MCP Registry searched (#107), read through
//! core's HTTP client.
//!
//! The one transport a command sends with is built here, reqwest's
//! ([`Reqwest`]), and handed to core, which reads no environment: no proxy
//! is read either, which core's adapter never does. Where the registry is
//! comes from `--registry`, the official one unless it names another, and
//! is held to the client's rules, https or http on this machine, so a test
//! serves one on 127.0.0.1. What the registry answers is kept in the
//! workspace's HTTP cache only where the workspace is made, since a read
//! never makes one.

use std::path::Path;

use mindweaver_core::adapter::reqwest::Reqwest;
use mindweaver_core::http::{Cache, Client, Failed, HttpError};
use mindweaver_core::registry::{Listing, Registry};
use mindweaver_core::WeaverWorkspace;

use super::reading_workspace;
use crate::render::{json_or_text, printable, table};
use crate::{CliError, Failure, Lookup, McpCommand, Outcome, Warning};

/// Run `gather mcp`.
pub(crate) fn run_mcp(
    command: &McpCommand,
    directory: &Path,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let http = Reqwest::new().map_err(CliError::from)?;
    match command {
        McpCommand::Search {
            query,
            registry,
            json,
        } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                let client = client(&http, workspace);
                let listing = Registry::new(&client)
                    .at(registry.as_str())
                    .search(query)
                    .map_err(failed)?;
                let outcome = searched(&listing, query);
                found = Some(listing);
                Ok(outcome)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
    }
}

/// A client sending through `http`, keeping what it is answered in the
/// cache of `workspace` when it has one: none for one not made yet, which a
/// read never makes.
fn client<'a>(http: &'a Reqwest, workspace: &WeaverWorkspace) -> Client<'a> {
    let client = Client::new(http);
    match Cache::of(workspace) {
        Some(cache) => client.with_cache(cache),
        None => client,
    }
}

/// A fetch that brought nothing back, and what went wrong with the cache on
/// the way, each a warning before it.
fn failed(failed: Failed) -> Failure {
    Failure {
        warnings: failed.warnings.iter().map(http_warned).collect(),
        error: Box::new(CliError::from(failed.error)),
    }
}

/// What went wrong with the cache, or a listing cut at its bound, as a
/// warning with its code.
fn http_warned(error: &HttpError) -> Warning {
    Warning::coded(error.code(), format!("warning: {error}"))
}

/// Each server a search found, a row each, in the order the registry
/// listed them: the name it is added by, its version, what the registry
/// says of it and what it is for; or that it found none. A server that
/// could not be read, a listing cut at its bound and the cache that failed
/// are each a warning, which fails the search as a failure collected does.
fn searched(listing: &Listing, query: &str) -> Outcome {
    let mut outcome = if listing.servers.is_empty() {
        Outcome::plain(match query {
            "" => String::from("the registry lists no server\n"),
            query => format!("the registry lists no server for `{}`\n", printable(query)),
        })
    } else {
        let rows: Vec<Vec<String>> = listing
            .servers
            .iter()
            .map(|server| {
                vec![
                    server.name.clone(),
                    server.version.clone(),
                    server
                        .status
                        .as_ref()
                        .map_or_else(String::new, |status| status.slug().to_owned()),
                    server.description.clone(),
                ]
            })
            .collect();
        Outcome::plain(table(&rows))
    };
    let said = listing
        .failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .chain(listing.cut.iter().map(http_warned))
        .chain(listing.warnings.iter().map(http_warned));
    outcome.stderr.extend(said);
    outcome.ok = outcome.stderr.is_empty();
    outcome
}
