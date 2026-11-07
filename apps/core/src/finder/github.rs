//! GitHub's code search, for the `SKILL.md` files a query finds: the
//! documented REST API (`/search/code`), which answers no code search
//! without a token, so it is asked only with one, and the token goes to its
//! origin alone, as a credential ([`Request::credential`]), which the cache
//! keeps nothing of.

use serde::Deserialize;

use super::{below, entries, request as asking, Entry, Listed, Token, PER_FINDER};
use crate::http::{HttpError, Request};
use crate::text::printable;

/// The version of the REST API asked for, so a later default changes
/// nothing read here.
const API_VERSION: &str = "2022-11-28";

/// What a search of `query` asks at `base`: files named `SKILL.md` that
/// match it, the first page.
pub(super) fn request(
    base: &str,
    query: &str,
    token: Option<&Token>,
) -> Result<Request, HttpError> {
    let searched = format!("{query} filename:SKILL.md");
    let per_page = PER_FINDER.to_string();
    let url = below(
        base,
        &["search", "code"],
        &[("q", &searched), ("per_page", &per_page)],
    )?;
    let request =
        asking(&url, "application/vnd.github+json").header("X-GitHub-Api-Version", API_VERSION);
    Ok(match token {
        Some(token) => request.credential("Authorization", format!("Bearer {}", token.0)),
        None => request,
    })
}

#[derive(Deserialize)]
struct Answer {
    items: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Item {
    path: String,
    repository: Repository,
}

#[derive(Deserialize)]
struct Repository {
    html_url: Option<String>,
    full_name: Option<String>,
}

/// Each file found, as the skill whose folder holds it: the repository
/// GitHub shows it at, or its `owner/repo`, and the folder the `SKILL.md`
/// is in, named after that folder, or after the repository at its root. A
/// file by another name, which a search by file name finds in any case, is
/// no skill's. One that does not read is said by its path, since GitHub
/// names a file found by its file's name alone.
pub(super) fn read(answer: serde_json::Value) -> Result<Vec<Entry>, String> {
    let answer: Answer =
        serde_json::from_value(answer).map_err(|error| printable(&error.to_string()))?;
    Ok(entries(answer.items, "path", |item: Item| {
        let (folder, file) = item.path.rsplit_once('/').unwrap_or(("", &item.path));
        if file != "SKILL.md" {
            return Err(format!(
                "it is the file `{}`, not a `SKILL.md`",
                printable(file)
            ));
        }
        let repository = &item.repository;
        let name = match folder.rsplit('/').next().filter(|name| !name.is_empty()) {
            Some(name) => Some(name.to_owned()),
            None => repository
                .full_name
                .as_deref()
                .or(repository.html_url.as_deref())
                .and_then(crate::url::repository),
        };
        Ok(Listed {
            name,
            address: item.repository.html_url.or(item.repository.full_name),
            path: Some(folder.to_owned()),
            ..Listed::default()
        })
    }))
}
