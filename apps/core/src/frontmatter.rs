//! Splitting a `SKILL.md` into its YAML front matter and its markdown body.
//!
//! Only the split happens here: what the front matter *means* is
//! [`crate::skill::SkillManifest`]'s business, and the body is handed back
//! untouched because it is the prompt an agent reads.

use thiserror::Error;

/// A document cut at its front matter fences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<'a> {
    /// Everything between the fences, without either fence line.
    pub front_matter: &'a str,
    /// Everything after the closing fence.
    pub body: &'a str,
}

/// Why a document could not be split.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrontMatterError {
    #[error("the file does not start with a `---` front matter fence")]
    Missing,
    #[error("the front matter is never closed by a `---` fence")]
    Unterminated,
}

/// Split `source` into its front matter and its body.
///
/// The opening fence has to be the very first line, which is what every agent
/// that reads these files requires: front matter further down is content, not
/// metadata. A byte order mark is tolerated because editors on Windows write
/// one and it is invisible to whoever wrote the file.
pub fn split(source: &str) -> Result<Document<'_>, FrontMatterError> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);

    // split_inclusive keeps the line terminators, so summing the lengths of
    // the lines walked so far is a byte offset into `source` — that is what
    // lets the two halves be borrowed slices rather than copies.
    let mut lines = source.split_inclusive('\n');
    let opening = lines.next().ok_or(FrontMatterError::Missing)?;
    if !is_fence(opening) {
        return Err(FrontMatterError::Missing);
    }

    let start = opening.len();
    let mut offset = start;
    for line in lines {
        if is_fence(line) {
            return Ok(Document {
                front_matter: &source[start..offset],
                body: &source[offset + line.len()..],
            });
        }
        offset += line.len();
    }

    Err(FrontMatterError::Unterminated)
}

/// A fence is a line of exactly `---`, whatever it is terminated by.
fn is_fence(line: &str) -> bool {
    line.trim_end_matches(['\n', '\r']) == "---"
}

// ---------------------------------------------------------------------------
// Editing front matter a line at a time
//
// Front matter is written by people, with comments and a key order that mean
// something to them, so a change to one key is made to that key's line and
// nothing else — never by parsing and re-serializing the whole block. Whoever
// calls these parses the result afterwards, to be sure the edit meant what it
// was meant to.
// ---------------------------------------------------------------------------

/// Whether a front matter line starts the top-level key `key`.
pub(crate) fn is_key(line: &str, key: &str) -> bool {
    line.strip_prefix(key)
        .is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(':'))
}

/// Front matter without the top-level `keys`, or the lines that continue them.
///
/// A blank or indented line belongs to the key above it — that is how a block
/// scalar or a list goes on — so it leaves with that key.
pub(crate) fn without_keys(front_matter: &str, keys: &[&str]) -> String {
    let mut kept = String::with_capacity(front_matter.len());
    let mut dropping = false;
    for line in front_matter.split_inclusive('\n') {
        let continues = line.starts_with([' ', '\t']) || line.trim().is_empty();
        if !continues {
            dropping = keys.iter().any(|key| is_key(line, key));
        }
        if !dropping {
            kept.push_str(line);
        }
    }
    kept
}

/// Front matter with the value of the top-level `key` replaced, every other
/// byte as it was.
///
/// `None` unless the key is there exactly once with its whole value on its own
/// line: a value that carries on below the key — a block scalar, a list — is
/// more than one line, and replacing the first of them would leave the rest
/// behind.
pub(crate) fn with_key(front_matter: &str, key: &str, value: &str) -> Option<String> {
    let mut out = String::with_capacity(front_matter.len() + value.len());
    let mut found = 0;
    let mut lines = front_matter.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        if !is_key(line, key) {
            out.push_str(line);
            continue;
        }
        found += 1;
        let (_, old) = line.split_once(':')?;
        let old = old.trim();
        let goes_on = old.is_empty()
            || old.starts_with(['|', '>'])
            || lines
                .peek()
                .is_some_and(|next| next.starts_with([' ', '\t']) && !next.trim().is_empty());
        if goes_on {
            return None;
        }
        let ending = &line[line.trim_end_matches(['\n', '\r']).len()..];
        out.push_str(key);
        out.push_str(": ");
        out.push_str(value);
        out.push_str(ending);
    }
    (found == 1).then_some(out)
}

#[cfg(test)]
#[path = "frontmatter_test.rs"]
mod tests;
