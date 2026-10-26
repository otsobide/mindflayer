//! JSON and JSONC entries, through jsonc-parser's CST.
//!
//! What an edit keeps, the tests here pin:
//!
//! - An insert goes after the last member of its object, after the comment
//!   that shares that member's line, with the file's line ending, and with a
//!   trailing comma when that member has one. On a line of its own, indented
//!   as that member is, when the object is written on several lines; after
//!   it on the same line when the object is written on one.
//! - A replace rewrites the value and nothing before it: the key, and the
//!   comments on the lines above, stay.
//! - A remove takes the member, its comma, and a comment that shares its
//!   line. A comment on a line above is left where it is: nothing says it
//!   was about this member and not about what follows.
//!
//! An insert is spliced into the text, the CST only saying where, rather
//! than made through the CST's own `append`: that lays the object out again
//! first, dropping the blank lines just inside its braces and breaking one
//! written on a single line onto several. A replace and a remove go through
//! the CST, which touches nothing outside the member there.

use std::fmt::Write as _;

use jsonc_parser::cst::{
    CstContainerNode, CstInputValue, CstLeafNode, CstNewlineKind, CstNode, CstObject,
    CstObjectProp, CstRootNode,
};
use jsonc_parser::{CollectOptions, ParseOptions};

use super::{display, number, quote, unique, Edit, EntryError, Format, Value};

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
    if format == Format::Json {
        strict(text)?;
    }
    CstRootNode::parse(text, &options(format)).map_err(malformed)
}

/// Refuse two things RFC 8259 does not allow, which jsonc-parser 0.34 takes
/// whatever its options say: a control character written as itself in a
/// string, and whitespace between tokens other than space, tab, LF and CR,
/// such as a no-break space or U+2028, which it takes as Rust's
/// `is_whitespace` does. `JSON.parse` refuses both, so Claude would refuse
/// the file.
///
/// A scan of the text rather than a walk of the CST: the text has parsed by
/// now, so outside its strings it holds nothing but tokens, every one of
/// them printable ASCII, and whitespace.
fn strict(text: &str) -> Result<(), EntryError> {
    let mut string = false;
    let mut escape = false;
    for (at, character) in text.char_indices() {
        let refused = if string {
            match character {
                _ if escape => {
                    escape = false;
                    None
                }
                '\\' => {
                    escape = true;
                    None
                }
                '"' => {
                    string = false;
                    None
                }
                control if u32::from(control) < 0x20 => Some("in a string, not escaped,"),
                _ => None,
            }
        } else {
            match character {
                '"' => {
                    string = true;
                    None
                }
                ' ' | '\t' | '\n' | '\r' => None,
                token if token.is_ascii_graphic() => None,
                _ => Some("between tokens, where JSON allows only space, tab, LF and CR,"),
            }
        };
        if let Some(refused) = refused {
            let before = &text[..at];
            let line = before.matches('\n').count() + 1;
            let column = before
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                + 1;
            return Err(EntryError::Malformed {
                format: Format::Json,
                detail: format!(
                    "U+{:04X} {refused} on line {line} column {column}",
                    u32::from(character)
                ),
            });
        }
    }
    Ok(())
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

/// `text` with `edit` made to it. `None` is a file not there yet, which
/// becomes an object holding the entry.
///
/// What [`super::edit`] checks first is checked again here, rather than
/// trusted, so a mistake there is an error and not a panic.
pub(super) fn edit(
    format: Format,
    text: Option<&str>,
    key: &[&str],
    edit: &Edit,
) -> Result<String, EntryError> {
    let text = text.unwrap_or("{}\n");
    // The root must outlive every node taken from it: they hold it weakly.
    let root = parse(format, text)?;
    let top = root
        .object_value()
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
            let mut nested = value.clone();
            for segment in key[depth + 1..].iter().rev() {
                nested = Value::Object(vec![((*segment).to_owned(), nested)]);
            }
            return splice(format, &root, text, &object, key[depth], &nested);
        }
        Edit::Replace(value) => entry(&top, key)?.set_value(input(format, value)?),
        Edit::Remove => entry(&top, key)?.remove(),
    }
    Ok(root.to_string())
}

/// `text` with the member `name` holding `value` written after the last
/// member of `object`, a node of `root`, which was parsed from `text`.
///
/// Every byte of `text` stays, in order: what goes in is the member and the
/// comma before it, or, in an object with no members, the member on a line
/// of its own between the braces.
fn splice(
    format: Format,
    root: &CstRootNode,
    text: &str,
    object: &CstObject,
    name: &str,
    value: &Value,
) -> Result<String, EntryError> {
    let node = CstNode::from(object.clone());
    let open = start(&node);
    let close = end(&node) - 1;
    let newline = match root.newline_kind() {
        CstNewlineKind::CarriageReturnLineFeed => "\r\n",
        _ => "\n",
    };
    let opening = indentation(text, open);
    let unit = || {
        root.single_indent_text()
            .filter(|unit| !unit.is_empty())
            .unwrap_or_else(|| "  ".to_owned())
    };

    let mut member = String::new();
    let (comma, at) = match object.properties().last() {
        Some(last) => {
            let last = CstNode::from(last.clone());
            let trailing = last.trailing_comma().map(CstNode::from);
            // A comma for the member that is last now, right after it and
            // before any comment on its line, unless it has one already.
            let comma = trailing.is_none().then(|| end(&last));
            let after = trailing.clone().unwrap_or_else(|| last.clone());
            let at = after
                .trailing_comments_same_line()
                .last()
                .map_or_else(|| end(&after), |comment| end(&comment.into()));

            if node.children().iter().any(CstNode::is_newline) {
                // Indented as the last member is, unless it shares the
                // opening brace's line, and a level deeper than the brace's
                // line for what is inside the member.
                let (indent, unit) = if line_start(text, start(&last)) > open {
                    let indent = indentation(text, start(&last));
                    let unit = indent
                        .strip_prefix(opening)
                        .filter(|unit| !unit.is_empty())
                        .map_or_else(unit, str::to_owned);
                    (indent.to_owned(), unit)
                } else {
                    let unit = unit();
                    (format!("{opening}{unit}"), unit)
                };
                let lines = Lines {
                    newline,
                    unit: &unit,
                    trailing: trailing.is_some(),
                };
                member.push_str(newline);
                member.push_str(&indent);
                write_member(format, name, value, Some(&lines), &indent, &mut member)?;
            } else {
                member.push(' ');
                write_member(format, name, value, None, "", &mut member)?;
            }
            if trailing.is_some() {
                member.push(',');
            }
            (comma, at)
        }
        None => {
            // After the last comment between the braces, if any.
            let at = node
                .children()
                .iter()
                .rfind(|child| child.is_comment())
                .map_or(open + 1, end);
            let unit = unit();
            let indent = format!("{opening}{unit}");
            // No member to follow, so the file's: whether the top object's
            // last member has a trailing comma.
            let trailing = root.value().is_some_and(|top| top.uses_trailing_commas());
            let lines = Lines {
                newline,
                unit: &unit,
                trailing,
            };
            member.push_str(newline);
            member.push_str(&indent);
            write_member(format, name, value, Some(&lines), &indent, &mut member)?;
            if trailing {
                member.push(',');
            }
            if !text[at..close].contains('\n') {
                member.push_str(newline);
                member.push_str(opening);
            }
            (None, at)
        }
    };

    let mut spliced = String::with_capacity(text.len() + member.len() + 1);
    let mut from = 0;
    if let Some(comma) = comma {
        spliced.push_str(&text[..comma]);
        spliced.push(',');
        from = comma;
    }
    spliced.push_str(&text[from..at]);
    spliced.push_str(&member);
    spliced.push_str(&text[at..]);
    Ok(spliced)
}

/// Where `node` starts in the text its CST was parsed from. The CST keeps no
/// positions, but it keeps every byte, so it is the length of everything
/// written before the node.
fn start(node: &CstNode) -> usize {
    let mut offset = 0;
    let mut node = node.clone();
    loop {
        offset += node
            .previous_siblings()
            .map(|sibling| sibling.to_string().len())
            .sum::<usize>();
        match node.parent() {
            Some(parent) => node = parent.into(),
            None => return offset,
        }
    }
}

/// Where `node` ends: the position just past its last byte.
fn end(node: &CstNode) -> usize {
    start(node) + node.to_string().len()
}

/// Where the line holding the byte at `at` starts.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

/// The spaces and tabs the line holding the byte at `at` starts with.
fn indentation(text: &str, at: usize) -> &str {
    let line = &text[line_start(text, at)..];
    let width = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..width]
}

/// How a member spliced in is laid out over several lines: the file's line
/// ending, its indentation for one level, and whether it writes a trailing
/// comma after the last member of an object or element of an array.
struct Lines<'a> {
    newline: &'a str,
    unit: &'a str,
    trailing: bool,
}

/// `"name": value`, with what is inside `value` laid out as `lines` says,
/// below a line indented by `indent`, or on one line when `lines` is `None`.
///
/// Laid out as jsonc-parser lays out what its CST is given, so a member
/// spliced in reads as one it would have written: an object with members on
/// lines of its own, and an array on one line unless it holds an object or
/// an array with something in it.
fn write_member(
    format: Format,
    name: &str,
    value: &Value,
    lines: Option<&Lines>,
    indent: &str,
    text: &mut String,
) -> Result<(), EntryError> {
    quote(name, text);
    text.push_str(": ");
    write_value(format, value, lines, indent, text)
}

fn write_value(
    format: Format,
    value: &Value,
    lines: Option<&Lines>,
    indent: &str,
    text: &mut String,
) -> Result<(), EntryError> {
    match value {
        Value::Null => text.push_str("null"),
        Value::Bool(value) => text.push_str(if *value { "true" } else { "false" }),
        Value::Integer(value) => {
            let _ = write!(text, "{value}");
        }
        // As `input` writes one, for the same reason.
        Value::Float(value) if value.is_finite() => text.push_str(&number(*value)),
        Value::Float(_) => {
            return Err(EntryError::Unrepresentable {
                format,
                what: "a number that is not finite",
            })
        }
        Value::String(value) => quote(value, text),
        Value::Array(values) => {
            let spread = values.iter().any(|value| match value {
                Value::Array(inner) => !inner.is_empty(),
                Value::Object(inner) => !inner.is_empty(),
                _ => false,
            });
            let items = values.iter().map(|value| (None, value));
            let lines = lines.filter(|_| spread);
            write_container(format, ['[', ']'], items, lines, indent, text)?;
        }
        Value::Object(members) => {
            unique(members)?;
            let items = members
                .iter()
                .map(|(name, value)| (Some(name.as_str()), value));
            write_container(format, ['{', '}'], items, lines, indent, text)?;
        }
    }
    Ok(())
}

/// An array's elements or an object's members between `brackets`: each on a
/// line of its own when `lines` says how, and on one line, `[a, b]` or
/// `{ "a": 1 }`, when it is `None`.
fn write_container<'a>(
    format: Format,
    [open, close]: [char; 2],
    items: impl ExactSizeIterator<Item = (Option<&'a str>, &'a Value)>,
    lines: Option<&Lines>,
    indent: &str,
    text: &mut String,
) -> Result<(), EntryError> {
    let count = items.len();
    text.push(open);
    if count == 0 {
        text.push(close);
        return Ok(());
    }
    let inner = lines.map(|lines| format!("{indent}{}", lines.unit));
    let padded = open == '{';
    for (index, (name, value)) in items.enumerate() {
        match (lines, &inner) {
            (Some(lines), Some(inner)) => {
                text.push_str(lines.newline);
                text.push_str(inner);
            }
            _ if index > 0 || padded => text.push(' '),
            _ => {}
        }
        if let Some(name) = name {
            quote(name, text);
            text.push_str(": ");
        }
        write_value(
            format,
            value,
            lines,
            inner.as_deref().unwrap_or(indent),
            text,
        )?;
        if index + 1 < count || lines.is_some_and(|lines| lines.trailing) {
            text.push(',');
        }
    }
    match lines {
        Some(lines) => {
            text.push_str(lines.newline);
            text.push_str(indent);
        }
        None if padded => text.push(' '),
        None => {}
    }
    text.push(close);
    Ok(())
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
