//! `mindweaver gather search`: the skill directories a workspace enables
//! asked what they find for a query (#108), each through core's HTTP
//! client, over the one reqwest transport a command builds and hands core,
//! as `gather mcp` asks the MCP Registry.
//!
//! Which finders are asked, what each answers and what of it is a result
//! are core's ([`finder`]): this reads the token a person names, renders
//! what was found, and says what was not. The token is read from the
//! variable `--token-env` names, here, since core never reads the
//! environment, and handed to core for one search: it is never printed,
//! logged or kept, and goes to GitHub's origin alone, as a credential,
//! which the HTTP cache takes nothing of.

use std::path::Path;

use mindweaver_core::adapter::reqwest::Reqwest;
use mindweaver_core::finder::{self, Enabled, Finder, Found, Search, Token};
use mindweaver_core::http::Client;
use mindweaver_core::WeaverWorkspace;

use super::reading_workspace;
use super::registry::{client, http_warned};
use crate::homes::variable;
use crate::render::{json_or_text, plural, printable, table};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// What `gather search` was asked: the query, the finders named, the
/// variable holding a token, and where a finder is asked when not at its
/// own address.
pub(crate) struct Asked<'a> {
    pub query: &'a str,
    pub finders: &'a [Finder],
    pub token_env: Option<&'a str>,
    pub at: &'a [(Finder, String)],
}

/// Run `gather search`.
pub(crate) fn run_search(
    asked: &Asked<'_>,
    json: bool,
    directory: &Path,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let http = Reqwest::new().map_err(CliError::from)?;
    let (token, unset) = token(asked.token_env);
    let mut found = None;
    let text = reading_workspace(directory, lookup, |workspace| {
        let enabled = finder::enabled(workspace);
        let searched = search(&http, workspace, &enabled, asked, token)?;
        let outcome = listed(&searched, asked.query, &enabled, unset.as_ref());
        found = Some(searched);
        Ok(outcome)
    })?;
    Ok(json_or_text(found.filter(|_| json), text))
}

/// The token in the variable `token_env` names, if one is named; and, when
/// that variable is not set or empty, why there is none, to say.
///
/// Read here and handed to core as it is: core never reads the
/// environment, and a token typed on a command line would be in the shell's
/// history, and in `ps`, for anybody to read.
fn token(token_env: Option<&str>) -> (Option<Token>, Option<CliError>) {
    let Some(name) = token_env else {
        return (None, None);
    };
    match variable(name).and_then(Token::new) {
        Some(token) => (Some(token), None),
        None => (
            None,
            Some(CliError::TokenUnset {
                variable: name.to_owned(),
            }),
        ),
    }
}

/// What the finders `asked` names, of those `enabled`, find, asked through
/// a client keeping what they answer in `workspace`'s cache, where it is
/// made: none asked, refused, when one named is not enabled, or none is.
fn search(
    http: &Reqwest,
    workspace: &WeaverWorkspace,
    enabled: &Enabled,
    asked: &Asked<'_>,
    token: Option<Token>,
) -> Result<Found, Failure> {
    let finders = enabled.asked(asked.finders).map_err(CliError::from)?;
    let client: Client<'_> = client(http, workspace);
    let search = asked
        .at
        .iter()
        .fold(Search::new(&client), |search, (finder, base)| {
            search.at(*finder, base.clone())
        })
        .with_token(token);
    search
        .run(asked.query, &finders)
        .map_err(|error| Failure::from(CliError::from(error)))
}

/// Each result found, a row each, numbered in the order core lists them:
/// its name, its repository, with the branch after a `#` where a finder
/// names one, its folder in it, `.` for the root, its installs or stars,
/// and the finders that found it; or that none found anything.
///
/// A finder skipped for want of its token is a hint, which fails nothing;
/// one named a token that is not there, a name the marker lists that is no
/// finder, a finder that failed, a result dropped and the cache that
/// failed are each a warning with its code, which fails the search as a
/// failure collected does.
fn listed(found: &Found, query: &str, enabled: &Enabled, unset: Option<&CliError>) -> Outcome {
    let mut outcome = if found.hits.is_empty() {
        Outcome::plain(format!(
            "no finder found a skill for `{}`\n",
            printable(query)
        ))
    } else {
        let rows: Vec<Vec<String>> = found
            .hits
            .iter()
            .enumerate()
            .map(|(at, hit)| {
                let repository = match &hit.reference {
                    Some(reference) => format!("{}#{reference}", hit.repository),
                    None => hit.repository.to_string(),
                };
                let counted = match (hit.installs, hit.stars) {
                    (Some(installs), _) => plural(installs as usize, "install"),
                    (None, Some(stars)) => plural(stars as usize, "star"),
                    (None, None) => String::new(),
                };
                let finders: Vec<&str> = hit.finders.iter().map(|finder| finder.name()).collect();
                vec![
                    (at + 1).to_string(),
                    printable(&hit.name),
                    printable(&repository),
                    printable(hit.path.as_deref().unwrap_or(".")),
                    counted,
                    finders.join(", "),
                ]
            })
            .collect();
        Outcome::plain(table(&rows))
    };
    if unset.is_none() {
        outcome.stderr.extend(found.skipped.iter().map(|finder| {
            Warning::from(format!(
                "hint: {finder} was not asked, since its search needs a token: --token-env \
                 names the variable that holds one"
            ))
        }));
    }
    let said = unset
        .iter()
        .map(|error| Warning::coded(error.code(), format!("warning: {error}")))
        .chain(
            enabled
                .unknown
                .iter()
                .map(|unknown| Warning::coded(unknown.code(), format!("warning: {unknown}"))),
        )
        .chain(
            found
                .failures
                .iter()
                .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}"))),
        )
        .chain(found.warnings.iter().map(http_warned));
    outcome.stderr.extend(said);
    outcome.ok = outcome.stderr.iter().all(|line| line.code.is_none());
    outcome
}
