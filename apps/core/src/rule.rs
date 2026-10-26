//! What a rule declares about itself.
//!
//! Most rules declare nothing: a rule is a markdown file that gives context.
//! Claude Code's rules can say which files they are about, in front matter
//! (`paths:`), so a rule may open with some. Rules were written before any of
//! them was read, though, and markdown is free to open with a `---` horizontal
//! rule, so a fenced block at the top is front matter only when it holds a
//! YAML mapping. Anything else is content, as it always was.
//!
//! The line is YAML's, not the author's intent, so prose can fall on either
//! side of it: a sentence shaped like `Note: never force-push` is a mapping,
//! and read as front matter, while one opening with `**` or a backtick does
//! not parse, and is content with a warning, since it cannot be told apart
//! from front matter written wrong.

use serde_yaml_ng::Value;

use crate::frontmatter;

/// The front matter of a rule, as written: empty when it has none.
///
/// Only the keys Mindflayer reads are held. Every other key belongs to an
/// agent and passes through, not modelled and not rejected, as a skill's do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleManifest {
    /// The files the rule is about, as glob patterns. Unstated, it is about
    /// every file.
    pub paths: Lenient<Vec<String>>,
    /// What the rule is for, when its author said.
    pub description: Lenient<String>,
    /// Why a fenced block at the top was read as content rather than as front
    /// matter, when that is because it does not parse.
    ///
    /// Kept for `validate` to say so. Fences around prose that YAML reads as
    /// a plain string are not noted: a horizontal rule is a thing markdown is
    /// for, and nothing to warn about. Prose YAML cannot parse is noted like
    /// broken front matter, which is what it looks like.
    pub unparsed: Option<String>,
}

impl RuleManifest {
    /// What `source` declares, and the body that follows: everything after
    /// the closing fence when it has front matter, and all of `source` when it
    /// has none.
    ///
    /// Infallible on purpose. A rule's front matter is optional, so whatever
    /// cannot be read as front matter is read as content, and a rule is never
    /// hidden from a listing for how its first lines are written.
    pub fn read(source: &str) -> (Self, &str) {
        // No fence on the first line, or one that never closes: a horizontal
        // rule, and content like the rest.
        let Ok(document) = frontmatter::split(source) else {
            return (Self::default(), source);
        };
        // Parsed as a value rather than straight into a struct, so that what
        // the block *is* decides whether it is front matter before what it
        // says is read, and so that a key which is not a string is one more
        // key passing through rather than a reason to fail.
        match serde_yaml_ng::from_str::<Value>(document.front_matter) {
            Ok(Value::Mapping(mapping)) => {
                let manifest = Self {
                    paths: paths(mapping.get("paths")),
                    description: description(mapping.get("description")),
                    unparsed: None,
                };
                (manifest, document.body)
            }
            // A scalar or a list is prose between two horizontal rules, and
            // nothing at all is two horizontal rules: YAML reads an empty
            // block, or one of only comments, as null, and to markdown those
            // comments are headings. Taking the fences off either would drop
            // lines that read today, and declare nothing in return.
            Ok(_) => (Self::default(), source),
            Err(error) => {
                let manifest = Self {
                    unparsed: Some(error.to_string()),
                    ..Self::default()
                };
                (manifest, source)
            }
        }
    }
}

/// A key read leniently: written in the shape it is meant to have, written in
/// another and so ignored, or not written at all.
///
/// The shape [`Compatibility`](crate::skill::Compatibility) has for a skill,
/// made generic because a rule has two such keys. Refusing the whole front
/// matter over one of them would read the rule's every line as content, fences
/// included; ignoring it says so through `validate` instead, and leaves it in
/// the file for whichever agent wrote it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Lenient<T> {
    /// Not written, or written with nothing in it: no value, or a blank
    /// string or an empty list where one is read.
    #[default]
    Unstated,
    /// Written in the shape it is meant to have.
    Stated(T),
    /// Written as something else, and so ignored.
    Ignored,
}

impl<T> Lenient<T> {
    /// What it says, when it was written in the shape it is meant to have.
    pub fn stated(&self) -> Option<&T> {
        match self {
            Lenient::Stated(value) => Some(value),
            Lenient::Unstated | Lenient::Ignored => None,
        }
    }
}

/// `paths`, as Claude Code writes it: a list of glob patterns, or one pattern
/// on its own.
///
/// One string is one pattern, never split on commas: a brace expansion such as
/// `src/**/*.{ts,tsx}` has one inside it. A list holding anything but strings
/// is ignored whole, because keeping the strings in it would scope the rule to
/// fewer files than its author wrote. A blank pattern matches no file, so it
/// is left out without changing which files the rule is about; and with no
/// pattern left the key says nothing, as `paths:` alone does, rather than
/// scoping the rule to no file at all, which is a rule no agent is ever given.
fn paths(value: Option<&Value>) -> Lenient<Vec<String>> {
    let patterns = match value {
        None | Some(Value::Null) => return Lenient::Unstated,
        Some(Value::String(pattern)) => vec![pattern.clone()],
        Some(Value::Sequence(items)) => {
            let Some(patterns) = items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
            else {
                return Lenient::Ignored;
            };
            patterns
        }
        Some(_) => return Lenient::Ignored,
    };
    let patterns: Vec<String> = patterns
        .into_iter()
        .filter(|pattern| !pattern.trim().is_empty())
        .collect();
    if patterns.is_empty() {
        Lenient::Unstated
    } else {
        Lenient::Stated(patterns)
    }
}

/// `description`, when it says something: a blank one is unstated, so that
/// nothing shows an empty line as what the rule is for.
fn description(value: Option<&Value>) -> Lenient<String> {
    match value {
        None | Some(Value::Null) => Lenient::Unstated,
        Some(Value::String(text)) if text.trim().is_empty() => Lenient::Unstated,
        Some(Value::String(text)) => Lenient::Stated(text.clone()),
        Some(_) => Lenient::Ignored,
    }
}

#[cfg(test)]
#[path = "rule_test.rs"]
mod tests;
