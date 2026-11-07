//! `mindweaver gather mcp`: the MCP Registry searched, and a server it lists
//! written into the library (#107), each read through core's HTTP client.
//!
//! The one transport a command sends with is built here, reqwest's
//! ([`Reqwest`]), and handed to core, which reads no environment: no proxy
//! is read either, which core's adapter never does. Where the registry is
//! comes from `--registry`, the official one unless it names another, and
//! is held to the client's rules, https or http on this machine, so a test
//! serves one on 127.0.0.1. What the registry answers is kept in the
//! workspace's HTTP cache only where the workspace is made, since a read
//! never makes one.
//!
//! What a server is written as is core's ([`lifecycle::importing`] and
//! [`lifecycle::import`]), and nothing here starts it: not the server, and
//! not the program that would fetch it, which only an agent runs.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::adapter::reqwest::Reqwest;
use mindweaver_core::http::{Cache, Client, Failed, HttpError};
use mindweaver_core::lifecycle::{self, Added, ImportError, Importing};
use mindweaver_core::reading::{ShownInput, ShownTransport};
use mindweaver_core::registry::{Found, Listing, Registry};
use mindweaver_core::WeaverWorkspace;

use super::artifact::{recorded, stale};
use super::gather::scrubbed;
use super::read::{input_lines, transport_lines};
use super::{reading_workspace, workspace_here};
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
        McpCommand::Add {
            server,
            version,
            name,
            registry,
            yes,
        } => {
            let version = version.as_deref().unwrap_or("latest");
            let name = name.as_deref();
            let fetch = |client: &Client<'_>| {
                Registry::new(client)
                    .at(registry.as_str())
                    .version(server, version)
                    .map_err(failed)
            };
            // Without --yes it writes nothing, so it is resolved as a read
            // and makes no workspace, the default one in home included: a
            // dry run that wrote a marker would not be one.
            if !*yes {
                return reading_workspace(directory, lookup, |workspace| {
                    let found = fetch(&client(&http, workspace))?;
                    let importing = lifecycle::importing(workspace, &found.server, name)
                        .map_err(|error| unimported(error, &found))?;
                    Ok(imported(&found, &importing, None))
                });
            }
            let workspace = workspace_here(directory, lookup)?;
            let found = fetch(&client(&http, &workspace))?;
            let (importing, added) = lifecycle::import(&workspace, &found.server, name)
                .map_err(|error| unimported(error, &found))?;
            Ok(imported(&found, &importing, Some(added)))
        }
    }
}

/// What importing `found` came to, or would: the version, as the registry
/// names it, what the registry says of it and where it was read; what the
/// library calls it and what it is for; how an agent starts it or reaches
/// it and what is supplied for it, as `show` lays a server out, each secret
/// by the variable it is read from; and where it is written, once `added`,
/// or that nothing was.
///
/// What the mapping left out is a warning with its code that fails
/// nothing, since the definition is what was asked for all the same; what
/// went wrong with the cache fails it, as it fails a search.
fn imported(found: &Found, importing: &Importing, added: Option<Added>) -> Outcome {
    let server = &found.server;
    let definition = &importing.definition;
    let mut stdout = format!("{} {}", server.name, server.version);
    if let Some(status) = &server.status {
        let _ = write!(stdout, ", {}", printable(status.slug()));
    }
    let _ = writeln!(stdout, ", from {}", server.registry);
    let _ = writeln!(
        stdout,
        "as the MCP server {}: {}\n",
        definition.name,
        printable(&definition.description)
    );
    stdout.push_str(&transport_lines(&ShownTransport::of(&definition.transport)));
    if !definition.inputs.is_empty() {
        let inputs: Vec<ShownInput> = definition.inputs.iter().map(ShownInput::of).collect();
        stdout.push('\n');
        stdout.push_str(&input_lines(&inputs));
    }
    stdout.push('\n');
    let mut outcome = Outcome::plain(stdout);
    outcome.stderr.extend(
        importing
            .warnings
            .iter()
            .map(|warning| Warning::coded(warning.code(), format!("warning: {warning}"))),
    );
    let cache: Vec<Warning> = found.warnings.iter().map(http_warned).collect();
    outcome.ok = cache.is_empty();
    outcome.stderr.extend(cache);
    let Some(added) = added else {
        let _ = writeln!(
            outcome.stdout,
            "would create mcp {} at {}\nnothing written: add --yes to go ahead",
            definition.name,
            importing.file.display()
        );
        return outcome;
    };
    let _ = writeln!(
        outcome.stdout,
        "created mcp {} at {}",
        definition.name,
        added.artifact.file().display()
    );
    recorded(&mut outcome, added.stale, stale);
    scrubbed(&added.scrub).before(outcome)
}

/// Why `found` was not imported, after what went wrong with the cache on
/// the way and, for a version nothing maps to a definition, each step to
/// take by hand, a warning with its code.
fn unimported(error: ImportError, found: &Found) -> Failure {
    let mut warnings: Vec<Warning> = found.warnings.iter().map(http_warned).collect();
    if let ImportError::Unmapped { manual, .. } = &error {
        warnings.extend(
            manual
                .iter()
                .map(|step| Warning::coded(step.code(), format!("warning: {step}"))),
        );
    }
    Failure {
        error: Box::new(CliError::from(error)),
        warnings,
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
