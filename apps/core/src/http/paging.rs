//! A listing read a page at a time, each page JSON, as far as a bound.
//!
//! How a listing pages is a rule, and the same for every front end that
//! reads one, so it is here rather than in a catalog's caller: which header
//! or which parameter says where the next page is, where a listing ends, and
//! how many pages one is let run to. A cursor that never runs out, or a
//! listing that links a page to itself, ends at the bound rather than never.
//! A page says where the next one is in a header (`Link`), by the number in
//! its URL, or by a cursor in its body, each a [`Paging`].

use serde::de::DeserializeOwned;

use super::{fetchable, Client, Failed, Fetched, HttpError, Request};
use crate::value::Redacted;

/// How a listing says where its next page is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Paging {
    /// In a `Link` header naming it `rel="next"` (RFC 8288), as GitHub's API
    /// does: the last page names none.
    NextLink,
    /// By number, in the query parameter `param`: one more each page, from
    /// the first URL's own number, or 1 when it names none. A page with
    /// nothing in it is past the last, and not kept.
    Number { param: String },
    /// By a cursor each page carries in its body ([`Page::cursor`]), asked
    /// for again as the query parameter `param` of the first URL, as the MCP
    /// Registry pages: the last page carries none, or an empty one.
    Cursor { param: String },
}

/// One page of a listing, read as JSON, which says whether it holds
/// anything: what tells a listing paged by number that it has run out.
pub trait Page: DeserializeOwned {
    fn is_empty(&self) -> bool;

    /// Where the listing goes on from, for one paged by a cursor its pages
    /// carry ([`Paging::Cursor`]): none on its last page, and none for a
    /// page of a listing paged otherwise.
    fn cursor(&self) -> Option<&str> {
        None
    }
}

/// A page that is the list itself.
impl<T: DeserializeOwned> Page for Vec<T> {
    fn is_empty(&self) -> bool {
        <[T]>::is_empty(self)
    }
}

/// The pages a listing was read as, in order.
#[derive(Debug)]
pub struct Paged<T> {
    pub pages: Vec<T>,
    /// [`HttpError::TooManyPages`] when the listing went on past the bound:
    /// what is in `pages` is all of it before that. A failure for the caller
    /// to collect beside what was read, rather than a reason to hide it.
    pub cut: Option<HttpError>,
    /// What went wrong with the cache on the way, each page's
    /// ([`Fetched::warnings`]); with a listing that failed, in
    /// [`Failed::warnings`].
    pub warnings: Vec<HttpError>,
}

impl Client<'_> {
    /// Read the listing `first` starts, a page at a time as `paging` says,
    /// and no more than `max_pages` of them.
    ///
    /// Each page is fetched as [`Client::get`] fetches, held to the same
    /// limits, and its next page is asked with `first`'s headers, its
    /// credentials only while the next page is on the origin `first` named,
    /// as a redirect leaves them. A page that cannot be fetched, or read as
    /// `T`, fails the listing: what came before it is a listing cut short at
    /// a place nobody chose. What went wrong with the cache on the way to
    /// it comes back with the failure, every page's before it included.
    pub fn pages<T: Page>(
        &self,
        first: &Request,
        paging: &Paging,
        max_pages: usize,
    ) -> Result<Paged<T>, Failed> {
        let mut warnings = Vec::new();
        match self.paged(first, paging, max_pages, &mut warnings) {
            Ok(paged) => Ok(Paged { warnings, ..paged }),
            Err(error) => Err(Failed { error, warnings }),
        }
    }

    /// [`Client::pages`], what went wrong with the cache put in `warnings`
    /// whatever came of the listing.
    fn paged<T: Page>(
        &self,
        first: &Request,
        paging: &Paging,
        max_pages: usize,
        warnings: &mut Vec<HttpError>,
    ) -> Result<Paged<T>, HttpError> {
        let origin = fetchable(first.url())?.origin();
        let mut pages = Vec::new();
        let mut asked = 0;
        let mut next = Some(first.clone());
        while let Some(request) = next.take() {
            if asked == max_pages {
                return Ok(Paged {
                    pages,
                    cut: Some(HttpError::TooManyPages {
                        url: Redacted::new(first.url()),
                        limit: max_pages,
                    }),
                    warnings: Vec::new(),
                });
            }
            asked += 1;
            let fetched = self.cached(&request, warnings)?;
            let page: T = fetched.json()?;
            let after = match paging {
                Paging::NextLink => next_link(&fetched)?,
                Paging::Number { .. } if page.is_empty() => break,
                Paging::Number { param } => Some(numbered(request.url(), param)?),
                Paging::Cursor { param } => match page.cursor() {
                    Some(cursor) if !cursor.is_empty() => {
                        Some(at_cursor(first.url(), param, cursor)?)
                    }
                    _ => None,
                },
            };
            pages.push(page);
            next = after.map(|url| first.to(&url, url.origin() == origin));
        }
        Ok(Paged {
            pages,
            cut: None,
            warnings: Vec::new(),
        })
    }
}

/// The page `fetched` links to as its next, if any, as a URL a fetch may
/// name: relative to the URL the page came from, as RFC 8288 reads one.
fn next_link(fetched: &Fetched) -> Result<Option<reqwest::Url>, HttpError> {
    let Some(target) = fetched
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("link"))
        .find_map(|(_, value)| next_in(value))
    else {
        return Ok(None);
    };
    let base = fetchable(&fetched.url)?;
    let next = base.join(target).map_err(|error| HttpError::Url {
        url: Redacted::new(target),
        detail: error.to_string(),
    })?;
    fetchable(next.as_str()).map(Some)
}

/// The target of the first link in a `Link` header's value whose `rel`
/// names `next` among its relations.
///
/// Each link is `<target>` and its parameters, up to the next link's `<`;
/// a parameter's value may be quoted, and a `rel` may name several
/// relations, a space apart, which case does not tell apart.
fn next_in(value: &str) -> Option<&str> {
    let mut rest = value;
    while let Some(open) = rest.find('<') {
        let close = open + rest[open..].find('>')?;
        let target = &rest[open + 1..close];
        rest = &rest[close + 1..];
        let parameters = &rest[..rest.find('<').unwrap_or(rest.len())];
        let next = parameters.split(';').any(|parameter| {
            parameter
                .split_once('=')
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("rel"))
                .is_some_and(|(_, relations)| {
                    relations
                        .trim()
                        .trim_end_matches(',')
                        .trim()
                        .trim_matches('"')
                        .split_ascii_whitespace()
                        .any(|relation| relation.eq_ignore_ascii_case("next"))
                })
        });
        if next {
            return Some(target);
        }
    }
    None
}

/// `url` with its page number in `param` one more than it is, 1 when it
/// names none: the next page of a listing paged by number.
fn numbered(url: &str, param: &str) -> Result<reqwest::Url, HttpError> {
    let mut parsed = fetchable(url)?;
    let mut pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    let at = pairs.iter().position(|(name, _)| name == param);
    let current = match at {
        None => 1,
        Some(at) => pairs[at].1.parse::<u64>().map_err(|_| HttpError::Url {
            url: Redacted::new(url),
            detail: format!("`{param}` is not a page number"),
        })?,
    };
    let next = (current + 1).to_string();
    match at {
        Some(at) => pairs[at].1 = next,
        None => pairs.push((param.to_owned(), next)),
    }
    parsed.query_pairs_mut().clear().extend_pairs(pairs);
    Ok(parsed)
}

/// `url` with its query parameter `param` set to `cursor`, in place of any
/// it had: the next page of a listing paged by a cursor.
///
/// From the first page's URL rather than the last page's, so the query a
/// listing was asked with goes with every page of it. The cursor is what a
/// page said, somebody else's text, so it is put in the query as a value
/// is, escaped, and can add no parameter of its own.
fn at_cursor(url: &str, param: &str, cursor: &str) -> Result<reqwest::Url, HttpError> {
    let mut parsed = fetchable(url)?;
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(name, _)| name != param)
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    parsed
        .query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair(param, cursor);
    Ok(parsed)
}

#[cfg(test)]
#[path = "paging_test.rs"]
mod tests;
