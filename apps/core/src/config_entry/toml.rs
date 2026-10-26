//! TOML entries, through toml_edit.
//!
//! The crate cargo edits `Cargo.toml` with, and the one the markers are
//! edited with: it keeps every byte it is not told to change. What it keeps,
//! and what this module does about what it does not, the tests here pin:
//!
//! - An object is written the way the objects already beside it are: a
//!   `[table]` of its own after the last of theirs, inline when theirs are
//!   inline, dotted keys among dotted keys, and a `[table]` when there are
//!   none. Anything else is a key after the keys beside it, indented as the
//!   one before it.
//! - A string with a line break in it is written on one line, the break
//!   escaped, where toml_edit would open a multi-line string: the break
//!   would then be the file's line ending to the file and part of the value
//!   to the agent reading it. A float is written as the fingerprint writes
//!   it, where toml_edit writes every digit of `1e300`.
//! - A replace keeps the entry's key, its place, the comments above it and
//!   the one after it on its line, and its layout: a table stays a table, an
//!   inline table inline, dotted keys dotted.
//! - A remove takes the entry's lines, its own sub-tables included. The
//!   comments on the lines above it stay, as jsonc-parser leaves them, though
//!   toml_edit files them with the entry: a file's opening comment is filed
//!   with its first table. So an entry with comments above it is first
//!   replaced by a stand-in that keeps them, and the stand-in's own line,
//!   which a mark nothing else in the file holds makes plain, is cut out of
//!   what toml_edit writes.
//! - toml_edit writes every line ending as LF. A file whose lines all end in
//!   CRLF has them put back; one that mixes the two is refused, since which
//!   line had which would be lost. It ends the last line too, which a file
//!   that left it unended gets back unended.

use std::fmt::Write as _;

use toml_edit::{Array, DocumentMut, InlineTable, Item, RawString, Table, Value as TomlValue};

use super::{display, number, unique, Edit, EntryError, Format, Value};

/// The whole of `text` as a value.
pub(super) fn document(text: &str) -> Result<Value, EntryError> {
    Ok(table_value(parse(text)?.as_table()))
}

fn parse(text: &str) -> Result<DocumentMut, EntryError> {
    text.parse::<DocumentMut>()
        .map_err(|error| EntryError::Malformed {
            format: Format::Toml,
            detail: describe(&error, text),
        })
}

/// toml_edit's message on one line, where its `Display` draws the line in
/// question and a caret under it: a warning is one line.
fn describe(error: &toml_edit::TomlError, text: &str) -> String {
    let message = error.message().trim_end();
    let Some(span) = error.span() else {
        return message.to_owned();
    };
    let before = text.get(..span.start).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    format!("{message} on line {line} column {column}")
}

/// `text` with `edit` made through toml_edit. `None` is a file not there yet,
/// which is an empty document.
///
/// What [`super::edit`] checks first is checked again here, rather than
/// trusted, so a mistake there is an error and not a panic.
pub(super) fn edit(text: Option<&str>, key: &[&str], edit: &Edit) -> Result<String, EntryError> {
    if key.is_empty() {
        return Err(EntryError::NoKey);
    }
    let text = text.unwrap_or_default();
    let crlf = crlf(text)?;
    let mut document = parse(text)?;
    // A file of comments alone holds them after everything, where toml_edit
    // writes what is inserted before them; they are put back on top.
    if document.is_empty() && matches!(edit, Edit::Insert(_)) {
        let mut comments = document.trailing().as_str().unwrap_or_default().to_owned();
        // A last comment with no line ending would take in what is written
        // after it; the ending goes back out below.
        if comments.contains('#') && !comments.ends_with('\n') {
            comments.push('\n');
        }
        document.set_trailing("");
        document.decor_mut().set_prefix(comments);
    }

    let mut parent = Parent::Table(document.as_table_mut());
    for depth in 0..key.len() - 1 {
        parent = child(parent, key, depth, matches!(edit, Edit::Insert(_)))?;
    }
    let stand_in = match edit {
        Edit::Insert(value) => insert(parent, key, value).map(|()| None)?,
        Edit::Replace(value) => replace(parent, key, value).map(|()| None)?,
        Edit::Remove => remove(parent, key, text)?,
    };

    let mut written = document.to_string();
    if let Some(mark) = stand_in {
        written =
            cut_line(&written, &mark).ok_or_else(|| EntryError::Unkept { key: display(key) })?;
    }
    if crlf {
        written = to_crlf(&written);
    }
    // toml_edit ends every line it writes, the last one too; a file whose
    // last line had no ending is left without one.
    if !text.is_empty() && !text.ends_with('\n') {
        let ending = if crlf { "\r\n" } else { "\n" };
        if let Some(unended) = written.strip_suffix(ending) {
            written.truncate(unended.len());
        }
    }
    Ok(written)
}

/// What an entry is filed in: a table, written as `[header]` or dotted keys,
/// or an inline table, `{ … }`.
enum Parent<'a> {
    Table(&'a mut Table),
    Inline(&'a mut InlineTable),
}

/// What `parent` holds at `key[depth]`, made when it is missing and `make`
/// says to.
fn child<'a>(
    parent: Parent<'a>,
    key: &[&str],
    depth: usize,
    make: bool,
) -> Result<Parent<'a>, EntryError> {
    let name = key[depth];
    let missing = || EntryError::Missing { key: display(key) };
    let not_a_table = || EntryError::NotAnObject {
        key: display(&key[..=depth]),
    };
    match parent {
        Parent::Table(table) => {
            if !table.contains_key(name) {
                if !make {
                    return Err(missing());
                }
                // Implied by the entry's own header, `[mcp_servers.github]`,
                // rather than written as an empty `[mcp_servers]` above it;
                // among dotted keys, one more dotted key.
                let mut made = Table::new();
                if table.is_dotted() {
                    made.set_dotted(true);
                } else {
                    made.set_implicit(true);
                }
                table.insert(name, Item::Table(made));
            }
            match table.get_mut(name) {
                Some(Item::Table(inner)) => Ok(Parent::Table(inner)),
                Some(Item::Value(TomlValue::InlineTable(inner))) => Ok(Parent::Inline(inner)),
                _ => Err(not_a_table()),
            }
        }
        Parent::Inline(table) => {
            if !table.contains_key(name) {
                if !make {
                    return Err(missing());
                }
                table.insert(name, TomlValue::InlineTable(InlineTable::new()));
            }
            match table.get_mut(name) {
                Some(TomlValue::InlineTable(inner)) => Ok(Parent::Inline(inner)),
                _ => Err(not_a_table()),
            }
        }
    }
}

fn insert(parent: Parent<'_>, key: &[&str], value: &Value) -> Result<(), EntryError> {
    let name = key[key.len() - 1];
    let exists = || EntryError::Exists { key: display(key) };
    match parent {
        Parent::Table(table) => {
            if table.contains_key(name) {
                return Err(exists());
            }
            match value {
                // Empty dotted keys write nothing at all, so an empty object
                // among them is the one that goes inline.
                Value::Object(members)
                    if !inline_beside(table) && !(table.is_dotted() && members.is_empty()) =>
                {
                    let made = table_of(members, table.is_dotted())?;
                    table.insert(name, Item::Table(made));
                }
                _ => {
                    let indent = indentation(table);
                    table.insert(name, Item::Value(toml_value(value)?));
                    if let (Some(indent), Some(mut written)) = (indent, table.key_mut(name)) {
                        written.leaf_decor_mut().set_prefix(indent);
                    }
                }
            }
        }
        Parent::Inline(table) => {
            if table.contains_key(name) {
                return Err(exists());
            }
            let mut written = toml_value(value)?;
            // The space before the closing brace is filed with the last
            // member, so it moves to the one that is last now; left where it
            // was, it would come before the comma: `{ a = 1 } , b = 2 }`.
            if let Some((_, last)) = table.iter_mut().last() {
                if let Some(closing) = last.decor().suffix().cloned() {
                    last.decor_mut().set_suffix("");
                    written.decor_mut().set_suffix(closing);
                }
            }
            table.insert(name, written);
        }
    }
    Ok(())
}

/// Whether the objects already in `table` are written inline, judged by the
/// last of them, which is the one an entry goes after.
fn inline_beside(table: &Table) -> bool {
    let last = table
        .iter()
        .filter(|(_, item)| item.is_table() || item.is_inline_table())
        .last();
    matches!(last, Some((_, item)) if item.is_inline_table())
}

/// The spaces or tabs the last key written in `table` is indented by, for a
/// key written after it.
fn indentation(table: &Table) -> Option<String> {
    let (last, _) = table.iter().filter(|(_, item)| item.is_value()).last()?;
    let above = table.key(last)?.leaf_decor().prefix()?.as_str()?;
    let indent = above.rsplit('\n').next()?;
    let blank = !indent.is_empty() && indent.chars().all(|c| c == ' ' || c == '\t');
    blank.then(|| indent.to_owned())
}

fn replace(parent: Parent<'_>, key: &[&str], value: &Value) -> Result<(), EntryError> {
    let name = key[key.len() - 1];
    let missing = || EntryError::Missing { key: display(key) };
    match parent {
        Parent::Table(table) => {
            let slot = table.get_mut(name).ok_or_else(missing)?;
            // Assigned through the slot, never `insert`ed: toml_edit's
            // `insert` resets the formatting of a key that is already there.
            let mut moved = None;
            let replacement = match (&*slot, value) {
                (Item::Table(old), Value::Object(members))
                    if !(old.is_dotted() && members.is_empty()) =>
                {
                    let mut new = table_of(members, old.is_dotted())?;
                    new.set_implicit(old.is_implicit());
                    new.set_position(old.position());
                    *new.decor_mut() = old.decor().clone();
                    if old.is_dotted() {
                        if let Some(above) = first_line_prefix(old).cloned() {
                            set_first_line_prefix(&mut new, above);
                        }
                    }
                    Item::Table(new)
                }
                (Item::Value(old), _) => {
                    let mut new = toml_value(value)?;
                    *new.decor_mut() = old.decor().clone();
                    Item::Value(new)
                }
                (Item::Table(old), _) => {
                    // A table becomes a key of the table above it, so the
                    // comments above its header go with it, to the key.
                    moved = Some(above(table_key_prefix(old, None)).cloned());
                    Item::Value(toml_value(value)?)
                }
                _ => Item::Value(toml_value(value)?),
            };
            *slot = replacement;
            if let Some(above) = moved {
                if let Some(mut written) = table.key_mut(name) {
                    let decor = written.leaf_decor_mut();
                    decor.clear();
                    if let Some(above) = above {
                        decor.set_prefix(above);
                    }
                }
            }
        }
        Parent::Inline(table) => {
            let slot = table.get_mut(name).ok_or_else(missing)?;
            let mut new = toml_value(value)?;
            *new.decor_mut() = slot.decor().clone();
            *slot = new;
        }
    }
    Ok(())
}

/// Take the entry out, and say what marks the line of the stand-in left in
/// its place, when it had comments above it to keep.
fn remove(parent: Parent<'_>, key: &[&str], text: &str) -> Result<Option<String>, EntryError> {
    let name = key[key.len() - 1];
    let missing = || EntryError::Missing { key: display(key) };
    let table = match parent {
        Parent::Inline(table) => {
            let was_last = table.iter().last().is_some_and(|(last, _)| last == name);
            let gone = table.remove(name).ok_or_else(missing)?;
            // The space before the closing brace goes back to the member
            // that is last again, as an insert took it from it. Only from
            // the last member: what any other holds is the space before its
            // comma, and the space before the brace is still where it was.
            if was_last {
                if let Some(closing) = gone.decor().suffix().cloned() {
                    if let Some((_, last)) = table.iter_mut().last() {
                        last.decor_mut().set_suffix(closing);
                    }
                }
            }
            return Ok(None);
        }
        Parent::Table(table) => table,
    };

    // An implicit table has no line of its own, and an array of tables is
    // several entries, not one: neither has a line above it to keep.
    let leading = match table.get(name).ok_or_else(missing)? {
        Item::Value(_) => table.key(name).and_then(|key| key.leaf_decor().prefix()),
        Item::Table(entry) if entry.is_dotted() => first_line_prefix(entry),
        Item::Table(entry) if !entry.is_implicit() => table_key_prefix(entry, table.key(name)),
        _ => None,
    };
    let Some(leading) = above(leading).cloned() else {
        table.remove(name);
        return Ok(None);
    };

    let mark = mark(text);
    let mut stand_in = TomlValue::from(false);
    stand_in.decor_mut().set_suffix(format!(" #{mark}"));
    match table.get_mut(name) {
        Some(Item::Table(entry)) if !entry.is_dotted() => {
            let mut header = Table::new();
            header.set_position(entry.position());
            *header.decor_mut() = entry.decor().clone();
            header.decor_mut().set_suffix(format!(" #{mark}"));
            *entry = header;
        }
        Some(Item::Value(entry)) => *entry = stand_in,
        Some(slot) => {
            // Dotted keys: one line of them stands in for all of them, with
            // the comments of the first of them above it.
            *slot = Item::Value(stand_in);
            if let Some(mut written) = table.key_mut(name) {
                written.leaf_decor_mut().set_prefix(leading);
            }
        }
        None => return Err(missing()),
    }
    Ok(Some(mark))
}

/// `prefix`, when it holds a comment: what the lines above an entry hold
/// that is worth keeping.
fn above(prefix: Option<&RawString>) -> Option<&RawString> {
    prefix.filter(|prefix| prefix.as_str().is_some_and(|text| text.contains('#')))
}

/// What is written above a `[table]`'s header. toml_edit files it with the
/// table, or with the last key of the header, depending on how it was made,
/// so the one holding a comment is taken.
fn table_key_prefix<'a>(
    table: &'a Table,
    key: Option<&'a toml_edit::Key>,
) -> Option<&'a RawString> {
    let own = table.decor().prefix();
    let keyed = key.and_then(|key| key.leaf_decor().prefix());
    above(own).or(above(keyed)).or(own)
}

/// What is written above the first line of a table of dotted keys, which
/// toml_edit files with the last key of that line.
fn first_line_prefix(table: &Table) -> Option<&RawString> {
    let (name, item) = table.iter().next()?;
    match item {
        Item::Table(inner) if inner.is_dotted() => first_line_prefix(inner),
        Item::Value(_) => table.key(name)?.leaf_decor().prefix(),
        _ => None,
    }
}

/// Put `prefix` above the first line of a table of dotted keys.
fn set_first_line_prefix(table: &mut Table, prefix: RawString) {
    let Some(name) = table.iter().next().map(|(name, _)| name.to_owned()) else {
        return;
    };
    match table.get_mut(&name) {
        Some(Item::Table(inner)) if inner.is_dotted() => set_first_line_prefix(inner, prefix),
        Some(Item::Value(_)) => {
            if let Some(mut key) = table.key_mut(&name) {
                key.leaf_decor_mut().set_prefix(prefix);
            }
        }
        _ => {}
    }
}

/// A comment's text that `text` does not hold, to find a line by.
fn mark(text: &str) -> String {
    loop {
        let mark = format!("mindflayer-removed-{}", crate::copy::unique_name());
        if !text.contains(&mark) {
            return mark;
        }
    }
}

/// `written` without the one line that holds `mark`, its line ending
/// included; `None` unless exactly one line holds it.
fn cut_line(written: &str, mark: &str) -> Option<String> {
    let at = written.find(mark)?;
    if written[at + mark.len()..].contains(mark) {
        return None;
    }
    let start = written[..at].rfind('\n').map_or(0, |index| index + 1);
    let end = written[at..]
        .find('\n')
        .map_or(written.len(), |index| at + index + 1);
    Some(format!("{}{}", &written[..start], &written[end..]))
}

/// Whether the lines of `text` end in CRLF, refusing a text where some do
/// and some do not.
fn crlf(text: &str) -> Result<bool, EntryError> {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    match (crlf, lf) {
        (0, _) => Ok(false),
        (_, 0) => Ok(true),
        _ => Err(EntryError::MixedLineEndings),
    }
}

/// `written` with every LF that toml_edit wrote as a bare LF turned back
/// into CRLF. A CRLF already there, inside a multi-line string toml_edit
/// copied as the file had it, is left as it is.
fn to_crlf(written: &str) -> String {
    let mut crlf = String::with_capacity(written.len() + written.len() / 16);
    let mut previous = None;
    for character in written.chars() {
        if character == '\n' && previous != Some('\r') {
            crlf.push('\r');
        }
        crlf.push(character);
        previous = Some(character);
    }
    crlf
}

/// An object as a table of its own, objects in it as tables under it, and
/// all of them dotted keys when `dotted` says so.
fn table_of(members: &[(String, Value)], dotted: bool) -> Result<Table, EntryError> {
    unique(members)?;
    let mut table = Table::new();
    table.set_dotted(dotted);
    for (name, value) in members {
        let item = match value {
            Value::Object(inner) if !(dotted && inner.is_empty()) => {
                Item::Table(table_of(inner, dotted)?)
            }
            other => Item::Value(toml_value(other)?),
        };
        table.insert(name, item);
    }
    Ok(table)
}

/// `value` as one TOML value, objects inline.
fn toml_value(value: &Value) -> Result<TomlValue, EntryError> {
    Ok(match value {
        Value::Null => {
            return Err(EntryError::Unrepresentable {
                format: Format::Toml,
                what: "null",
            })
        }
        Value::Bool(value) => TomlValue::from(*value),
        Value::Integer(value) => TomlValue::from(*value),
        Value::Float(value) if value.is_finite() => {
            let mut written = number(*value);
            // `1` is an integer to TOML, and the value is a float.
            if !written.contains(['.', 'e']) {
                written.push_str(".0");
            }
            spelled(&written)
        }
        // `nan`, `inf` and `-inf`, which toml_edit spells as TOML does.
        Value::Float(value) => TomlValue::from(*value),
        Value::String(value) if value.contains(['\n', '\r']) => spelled(&escaped(value)),
        Value::String(value) => TomlValue::from(value.as_str()),
        Value::Array(values) => {
            let mut array = Array::new();
            for value in values {
                array.push(toml_value(value)?);
            }
            TomlValue::Array(array)
        }
        Value::Object(members) => {
            unique(members)?;
            let mut table = InlineTable::new();
            for (name, value) in members {
                table.insert(name, toml_value(value)?);
            }
            TomlValue::InlineTable(table)
        }
    })
}

/// A value written as `repr` spells it. toml_edit takes a spelling of our own
/// only by reading it, so it is read; every caller spells valid TOML.
fn spelled(repr: &str) -> TomlValue {
    repr.parse()
        .unwrap_or_else(|error| panic!("{repr} is not a TOML value: {error}"))
}

/// `value` as a basic string on one line: the quote, the backslash and every
/// control character TOML refuses written as itself (all of them but tab)
/// escaped.
fn escaped(value: &str) -> String {
    let mut text = String::with_capacity(value.len() + 2);
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push('\t'),
            control if control.is_control() && u32::from(control) < 0x80 => {
                let _ = write!(text, "\\u{:04X}", u32::from(control));
            }
            other => text.push(other),
        }
    }
    text.push('"');
    text
}

fn table_value(table: &Table) -> Value {
    Value::Object(
        table
            .iter()
            .filter_map(|(name, item)| Some((name.to_owned(), item_value(item)?)))
            .collect(),
    )
}

fn item_value(item: &Item) -> Option<Value> {
    match item {
        Item::None => None,
        Item::Value(value) => Some(value_of(value)),
        Item::Table(table) => Some(table_value(table)),
        Item::ArrayOfTables(tables) => Some(Value::Array(tables.iter().map(table_value).collect())),
    }
}

fn value_of(value: &TomlValue) -> Value {
    match value {
        TomlValue::String(value) => Value::String(value.value().clone()),
        TomlValue::Integer(value) => Value::Integer(*value.value()),
        TomlValue::Float(value) => Value::Float(*value.value()),
        TomlValue::Boolean(value) => Value::Bool(*value.value()),
        // JSON has no datetime, and so neither has the canonical form: read
        // as the text it is written in. No agent's server entry holds one.
        TomlValue::Datetime(value) => Value::String(value.value().to_string()),
        TomlValue::Array(values) => Value::Array(values.iter().map(value_of).collect()),
        TomlValue::InlineTable(table) => Value::Object(
            table
                .iter()
                .map(|(name, value)| (name.to_owned(), value_of(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
#[path = "toml_test.rs"]
mod tests;
