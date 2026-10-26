//! What may be kept of a URL somebody typed.
//!
//! A module of its own rather than a helper inside [`crate::gather::git`],
//! because the question is asked wherever a URL is stored, logged or shown,
//! not only where a clone fails.

/// `url` without the user name and password a `scheme://` URL can carry before
/// its host, so an error can name a source without repeating a token somebody
/// typed into it.
///
/// The scp-like form (`git@host:path`) is left alone: what it carries before
/// the `@` can only be a user name. Parsing with `gix::url` and dropping the
/// user was the other way, and is rejected because the parser also rewrites
/// what it keeps (a default port dropped, a scheme lowercased), so the error
/// would name a URL nobody typed.
pub fn redacted(url: &str) -> String {
    let Some(scheme) = url.find("://") else {
        return url.to_owned();
    };
    let host = scheme + "://".len();
    let rest = &url[host..];
    let authority = rest.find(['/', '?', '#']).map_or(rest, |end| &rest[..end]);
    match authority.rfind('@') {
        Some(at) => format!("{}{}", &url[..host], &rest[at + 1..]),
        None => url.to_owned(),
    }
}

#[cfg(test)]
#[path = "url_test.rs"]
mod tests;
