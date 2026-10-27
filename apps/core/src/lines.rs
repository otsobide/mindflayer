//! A file's bytes as lines that keep their own endings.
//!
//! For a file somebody else owns that Mindflayer edits only inside a marked
//! block of its own: git's `info/exclude` ([`crate::exclude`]) and a
//! project's `AGENTS.md` ([`crate::agents_md`]). Lines rather than one string
//! to search and splice: what is outside the block is written back exactly as
//! read, CRLF, a missing final newline and bytes that are not UTF-8 included,
//! and a line Mindflayer adds takes whichever ending the file already uses.

pub(crate) const LF: &[u8] = b"\n";
pub(crate) const CRLF: &[u8] = b"\r\n";

/// One line of a file, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) text: Vec<u8>,
    /// `\n`, `\r\n`, or nothing for a last line without one.
    pub(crate) end: &'static [u8],
}

impl Line {
    pub(crate) fn new(text: &str, end: &'static [u8]) -> Self {
        Line {
            text: text.as_bytes().to_vec(),
            end,
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.text.trim_ascii().is_empty()
    }
}

/// The file's lines, each with the ending it had.
pub(crate) fn split(bytes: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match rest.iter().position(|&byte| byte == b'\n') {
            Some(at) => {
                let (text, end) = match rest[..at].strip_suffix(b"\r") {
                    Some(text) => (text, CRLF),
                    None => (&rest[..at], LF),
                };
                lines.push(Line {
                    text: text.to_vec(),
                    end,
                });
                rest = &rest[at + 1..];
            }
            None => {
                lines.push(Line {
                    text: rest.to_vec(),
                    end: b"",
                });
                break;
            }
        }
    }
    lines
}

/// The ending a line the file gains is written with: the first one it
/// already has, so a CRLF file stays CRLF, and LF in a file with none.
pub(crate) fn ending(lines: &[Line]) -> &'static [u8] {
    lines
        .iter()
        .map(|line| line.end)
        .find(|end| !end.is_empty())
        .unwrap_or(LF)
}
