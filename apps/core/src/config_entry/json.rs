//! JSON and JSONC entries, through jsonc-parser's CST.
//!
//! What the CST keeps, and what it does not, is jsonc-parser's, and the
//! tests here pin it:
//!
//! - An insert goes after the last member of its object, on a line of its
//!   own, indented as that member is, with the file's line ending, and with
//!   a trailing comma when the file writes them.
//! - A replace rewrites the value and nothing before it: the key, and the
//!   comments on the lines above, stay.
//! - A remove takes the member, its comma, and a comment that shares its
//!   line. A comment on a line above is left where it is: nothing says it
//!   was about this member and not about what follows.
//! - Two things are its own layout, not ours. Blank lines just inside the
//!   braces of the object an entry goes into are dropped, and an object
//!   written on one line is broken onto several when an entry goes into it,
//!   since a member is always written on a line of its own.

use jsonc_parser::cst::{
    CstContainerNode, CstInputValue, CstLeafNode, CstNode, CstObject, CstObjectProp, CstRootNode,
};
use jsonc_parser::{CollectOptions, ParseOptions};

use super::{display, number, unique, Edit, EntryError, Format, Value};

/// What `format` lets a text hold, and nothing more.
///
/// Everything off but what the format allows: jsonc-parser's default takes
/// every extension JSON5 has, which would let a file through that the agent
/// reading it then refuses.
fn options(format: Format) -> ParseOptions {
    let jsonc = format == Format::Jsonc;
    ParseOptions {
        allow_comments: jsonc,
        allow_trailing_commas: jsonc,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}

fn parse(format: Format, text: &str) -> Result<CstRootNode, EntryError> {
    let malformed = |error: jsonc_parser::errors::ParseError| EntryError::Malformed {
        format,
        detail: error.to_string(),
    };
    // Parsed once without the CST first, only to be held to the format. The
    // CST's own parse keeps comments as tokens, and on that path jsonc-parser
    // 0.34 lets a comment through `allow_comments: false`: a strict file with
    // a comment in it would be read, and written back with the comment.
    jsonc_parser::parse_to_ast(text, &CollectOptions::default(), &options(format))
        .map_err(malformed)?;
    CstRootNode::parse(text, &options(format)).map_err(malformed)
}

/// The whole of `text` as a value.
pub(super) fn document(format: Format, text: &str) -> Result<Value, EntryError> {
    let root = parse(format, text)?;
    let top = root.value().ok_or_else(|| EntryError::Malformed {
        format,
        detail: "it holds no value".to_owned(),
    })?;
    value_of(format, &top)
}

/// `text` with `edit` made through the CST. `None` is a file not there yet,
/// which becomes an object holding the entry.
///
/// What [`super::edit`] checks first is checked again here, rather than
/// trusted, so a mistake there is an error and not a panic.
pub(super) fn edit(
    format: Format,
    text: Option<&str>,
    key: &[&str],
    edit: &Edit,
) -> Result<String, EntryError> {
    // The root must outlive every node taken from it: they hold it weakly.
    let root = parse(format, text.unwrap_or_default())?;
    let top = match text {
        Some(_) => root.object_value(),
        None => root.object_value_or_create(),
    }
    .ok_or_else(|| EntryError::NotAnObject { key: String::new() })?;

    match edit {
        Edit::Insert(value) => {
            let mut object = top;
            let mut depth = 0;
            while depth < key.len() {
                let Some(member) = member(&object, key, depth)? else {
                    break;
                };
                if depth == key.len() - 1 {
                    return Err(EntryError::Exists { key: display(key) });
                }
                object = member
                    .object_value()
                    .ok_or_else(|| EntryError::NotAnObject {
                        key: display(&key[..=depth]),
                    })?;
                depth += 1;
            }
            // Whatever is missing on the way goes in with the entry, as one
            // member of the deepest object that is there, laid out once.
            let mut nested = input(format, value)?;
            for segment in key[depth + 1..].iter().rev() {
                nested = CstInputValue::Object(vec![((*segment).to_owned(), nested)]);
            }
            object.append(key[depth], nested);
        }
        Edit::Replace(value) => entry(&top, key)?.set_value(input(format, value)?),
        Edit::Remove => entry(&top, key)?.remove(),
    }
    Ok(root.to_string())
}

/// The member of `object` named `key[depth]`, if there is one.
fn member(
    object: &CstObject,
    key: &[&str],
    depth: usize,
) -> Result<Option<CstObjectProp>, EntryError> {
    let mut named = object
        .properties()
        .into_iter()
        .filter(|member| member.decoded_name().as_deref() == Some(key[depth]));
    match (named.next(), named.next()) {
        (_, Some(_)) => Err(EntryError::Ambiguous {
            key: display(&key[..=depth]),
        }),
        (found, None) => Ok(found),
    }
}

/// The member at `key`, which must be there.
fn entry(top: &CstObject, key: &[&str]) -> Result<CstObjectProp, EntryError> {
    let mut object = top.clone();
    for depth in 0..key.len() {
        let found = member(&object, key, depth)?
            .ok_or_else(|| EntryError::Missing { key: display(key) })?;
        if depth == key.len() - 1 {
            return Ok(found);
        }
        object = found
            .object_value()
            .ok_or_else(|| EntryError::NotAnObject {
                key: display(&key[..=depth]),
            })?;
    }
    Err(EntryError::NoKey)
}

/// What a node of the CST holds.
fn value_of(format: Format, node: &CstNode) -> Result<Value, EntryError> {
    let malformed = || EntryError::Malformed {
        format,
        detail: format!("{node} is not a value it can read"),
    };
    Ok(match node {
        CstNode::Container(CstContainerNode::Object(object)) => {
            let mut members = Vec::new();
            for member in object.properties() {
                let name = member.decoded_name().ok_or_else(malformed)?;
                let value = member.value().ok_or_else(malformed)?;
                members.push((name, value_of(format, &value)?));
            }
            Value::Object(members)
        }
        CstNode::Container(CstContainerNode::Array(array)) => Value::Array(
            array
                .elements()
                .iter()
                .map(|element| value_of(format, element))
                .collect::<Result<_, _>>()?,
        ),
        CstNode::Leaf(CstLeafNode::StringLit(string)) => {
            Value::String(string.decoded_value().map_err(|_| malformed())?)
        }
        CstNode::Leaf(CstLeafNode::NumberLit(literal)) => {
            numeric(&literal.to_string()).ok_or_else(malformed)?
        }
        CstNode::Leaf(CstLeafNode::BooleanLit(boolean)) => Value::Bool(boolean.value()),
        CstNode::Leaf(CstLeafNode::NullKeyword(_)) => Value::Null,
        _ => return Err(malformed()),
    })
}

/// A JSON number literal: an integer when it is written as one and fits,
/// a double otherwise, as JavaScript would read it.
fn numeric(literal: &str) -> Option<Value> {
    if !literal.contains(['.', 'e', 'E']) {
        if let Ok(integer) = literal.parse() {
            return Some(Value::Integer(integer));
        }
    }
    literal.parse().ok().map(Value::Float)
}

/// `value` as the CST takes it.
fn input(format: Format, value: &Value) -> Result<CstInputValue, EntryError> {
    Ok(match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(*value),
        Value::Integer(value) => CstInputValue::Number(value.to_string()),
        // Written as the fingerprint writes it, which is valid JSON for
        // every finite double, rather than as Rust's `Display` does: that
        // never uses an exponent, so 1e300 would be three hundred digits.
        Value::Float(value) if value.is_finite() => CstInputValue::Number(number(*value)),
        Value::Float(_) => {
            return Err(EntryError::Unrepresentable {
                format,
                what: "a number that is not finite",
            })
        }
        Value::String(value) => CstInputValue::String(value.clone()),
        Value::Array(values) => CstInputValue::Array(
            values
                .iter()
                .map(|value| input(format, value))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(members) => {
            unique(members)?;
            CstInputValue::Object(
                members
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), input(format, value)?)))
                    .collect::<Result<_, EntryError>>()?,
            )
        }
    })
}

#[cfg(test)]
#[path = "json_test.rs"]
mod tests;
