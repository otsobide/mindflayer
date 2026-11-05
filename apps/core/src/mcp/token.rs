//! What looks like a token written as it is: the literals `validate`
//! refuses a definition for holding in its transport (D-11), told by what
//! each kind of token starts with and what follows it.
//!
//! A heuristic, and meant to be one: no rule tells a credential from text
//! that resembles one, so this looks for the shapes the issuers give their
//! tokens (GitHub's prefixes, the `sk-` of OpenAI's and Anthropic's keys,
//! a `Bearer` value), long enough and at the start of a word, where a
//! false alarm costs a definition somebody can still write with an input,
//! and a miss costs a token committed with the project. A value an input
//! is written into is judged without its placeholder, so `Bearer {token}`
//! holds none; a brace naming no input is text, and judged as text.
//!
//! What is found is said by what it starts with and never as the text,
//! since saying the text would print the secret wherever the message goes:
//! a terminal, a CI log, the panel.

/// One shape a token is written in.
struct Shape {
    /// What it starts with, as `validate` says it.
    prefix: &'static str,
    /// Whether the prefix is matched in any case, as an HTTP scheme is:
    /// `bearer` is `Bearer`, where `GHP_` is no token GitHub gives.
    folded: bool,
    /// Whether whitespace comes between the prefix and the rest, as it
    /// does after a scheme.
    spaced: bool,
    /// What the rest is written in.
    body: fn(char) -> bool,
    /// How long the rest is at least: shorter, and it is a word that
    /// happens to start the same way.
    least: usize,
    /// Whether the rest holds a digit, as a key drawn at random does and a
    /// package named `sk-` and some words does not.
    digit: bool,
}

/// Every shape looked for, in the order tried: the longer prefix first, so
/// `github_pat_` is not taken for something shorter.
const SHAPES: [Shape; 8] = [
    Shape {
        prefix: "github_pat_",
        folded: false,
        spaced: false,
        body: word,
        least: 20,
        digit: false,
    },
    github("ghp_"),
    github("gho_"),
    github("ghu_"),
    github("ghs_"),
    github("ghr_"),
    Shape {
        prefix: "sk-",
        folded: false,
        spaced: false,
        body: key,
        least: 20,
        digit: true,
    },
    Shape {
        prefix: "Bearer",
        folded: true,
        spaced: true,
        body: token68,
        least: 16,
        digit: false,
    },
];

/// One of GitHub's tokens: its prefix, then letters and digits, 36 of them
/// as GitHub writes one, held to 30 so one cut short is still found.
const fn github(prefix: &'static str) -> Shape {
    Shape {
        prefix,
        folded: false,
        spaced: false,
        body: alphanumeric,
        least: 30,
        digit: false,
    }
}

/// What each token `text` holds starts with, in the order written, every
/// placeholder in `placeholders` taken out first, so that a value an input
/// is written into is judged on what it holds besides.
pub(crate) fn found(text: &str, placeholders: &[String]) -> Vec<&'static str> {
    // Each placeholder stands as a character no token holds, so the text on
    // either side of it is never read as one word.
    let mut text = text.to_owned();
    for placeholder in placeholders {
        text = text.replace(placeholder.as_str(), "\u{0}");
    }
    let mut found = Vec::new();
    let mut previous: Option<char> = None;
    let mut rest = text.as_str();
    while let Some(next) = rest.chars().next() {
        let starts_word = previous.is_none_or(|before| !word(before) && before != '-');
        let matched = starts_word
            .then(|| SHAPES.iter().find_map(|shape| matched(shape, rest)))
            .flatten();
        match matched {
            Some((prefix, length)) => {
                found.push(prefix);
                previous = rest[..length].chars().last();
                rest = &rest[length..];
            }
            None => {
                previous = Some(next);
                rest = &rest[next.len_utf8()..];
            }
        }
    }
    found
}

/// Whether `text` starts with a token of `shape`, and how many bytes of it
/// the token is.
fn matched(shape: &Shape, text: &str) -> Option<(&'static str, usize)> {
    let head = text.get(..shape.prefix.len())?;
    let same = if shape.folded {
        head.eq_ignore_ascii_case(shape.prefix)
    } else {
        head == shape.prefix
    };
    if !same {
        return None;
    }
    let mut at = shape.prefix.len();
    if shape.spaced {
        let gap = text[at..].len() - text[at..].trim_start_matches([' ', '\t']).len();
        if gap == 0 {
            return None;
        }
        at += gap;
    }
    let body: String = text[at..]
        .chars()
        .take_while(|c| (shape.body)(*c))
        .collect();
    let long = body.chars().count() >= shape.least;
    let digit = !shape.digit || body.chars().any(|c| c.is_ascii_digit());
    (long && digit).then(|| (shape.prefix, at + body.len()))
}

/// A letter, a digit or an underscore, as a word is written.
fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A letter or a digit.
fn alphanumeric(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// What a key drawn at random is written in: letters, digits, hyphens and
/// underscores.
fn key(c: char) -> bool {
    word(c) || c == '-'
}

/// What a bearer token is written in (RFC 6750's `b64token`), its padding
/// included.
fn token68(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | '+' | '/' | '=')
}

#[cfg(test)]
#[path = "token_test.rs"]
mod tests;
