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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_document_at_its_fences() {
        let doc = split("---\nname: a\n---\nbody\n").unwrap();
        assert_eq!(doc.front_matter, "name: a\n");
        assert_eq!(doc.body, "body\n");
    }

    #[test]
    fn keeps_inner_dashes_that_are_not_fences() {
        let doc = split("---\nname: a\n---\nchapter\n---\nnext\n").unwrap();
        assert_eq!(doc.front_matter, "name: a\n");
        assert_eq!(doc.body, "chapter\n---\nnext\n");
    }

    #[test]
    fn rejects_a_document_without_an_opening_fence() {
        assert_eq!(split("name: a\n"), Err(FrontMatterError::Missing));
        assert_eq!(split(""), Err(FrontMatterError::Missing));
    }

    #[test]
    fn rejects_front_matter_that_is_never_closed() {
        assert_eq!(split("---\nname: a\n"), Err(FrontMatterError::Unterminated));
    }
}
