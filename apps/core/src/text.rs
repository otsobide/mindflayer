//! Text somebody else wrote, made fit to be shown as it is.
//!
//! A name, a description or a line of a catalog comes from a file or an
//! answer somebody else wrote, and what is shown of it is the same rule
//! wherever it is shown: the CLI's rows, and whatever core makes of a
//! catalog's text before a front end is handed it. One function in core
//! rather than one in each front end, which would let a front end and what
//! core hands it disagree about what a name reads as.

/// `text` with every control character made a space, and the ends trimmed.
///
/// A carriage return in a name would let a row overwrite the row above it
/// on a terminal, hiding an entry or fabricating one that looks real, and an
/// escape sequence would change what the terminal shows after it. A space
/// rather than nothing, so two words a line break kept apart stay apart.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
