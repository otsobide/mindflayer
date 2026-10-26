//! One entry of a config file somebody else owns.
//!
//! An MCP server is installed as one entry in each agent's own config file
//! (D-10): `.mcp.json`, `opencode.json`, `.codex/config.toml`. Those files
//! belong to the team, to the user and to the agents' own CLIs, so Mindflayer
//! may change the one entry it was asked to and nothing else.
//!
//! Nothing here knows which agent reads which file, where in it a server goes
//! or what an entry holds: a caller names those, and the agents' shapes are
//! theirs (#90 to #92).
//!
//! An entry can carry no mark of its own (OpenCode rejects keys it does not
//! know, `codex --strict-config` fails on them, and `.mcp.json` allows no
//! comments), so what Mindflayer keeps of one it wrote is its
//! [fingerprint](Value::fingerprint).

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// What an entry holds, in neither format's terms.
///
/// Its own type rather than `serde_json::Value` or toml_edit's, because one
/// entry is rendered into both formats and neither can stand for the other:
/// JSON has a null and TOML has none, TOML keeps integers and floats apart
/// and JSON does not. A value TOML cannot hold is refused when it is written
/// there, rather than bent into something it was not.
///
/// `==` compares two values as they are written, members in order; whether
/// they mean the same is what their [fingerprints](Value::fingerprint) say.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    /// Members in the order they are written. A list, not a map, so an entry
    /// is written in the order its shape gives (`command` before `args`), and
    /// so a key a JSON file repeats is read as it is rather than one copy
    /// silently winning.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// An object of these members, in this order.
    pub fn object<K: Into<String>>(members: impl IntoIterator<Item = (K, Value)>) -> Self {
        Value::Object(
            members
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// An array of these values.
    pub fn array<V: Into<Value>>(values: impl IntoIterator<Item = V>) -> Self {
        Value::Array(values.into_iter().map(Into::into).collect())
    }

    /// The canonical JSON form of the value: [RFC 8785], the JSON
    /// Canonicalization Scheme.
    ///
    /// Members sorted by key at every depth, no whitespace, strings escaped
    /// only where JSON must be, and numbers written the way ECMAScript writes
    /// a double, so `1.0`, `1` and `1e0` are all `1`. A published scheme rather
    /// than one of ours: a file Claude rewrites goes through JavaScript's
    /// `JSON.stringify`, which keeps no integer apart from a float either, so
    /// anything stricter would call an entry nobody changed edited.
    ///
    /// Two departures, both for what JSON cannot spell. An [`Value::Integer`]
    /// is written exactly, digit for digit, where the scheme would round one
    /// past 2^53 to a double: TOML's integers are 64-bit, and two of them must
    /// not read as one. A float that is not finite, which TOML can hold and
    /// JSON cannot, is written as ECMAScript names it (`NaN`, `Infinity`,
    /// `-Infinity`): no valid JSON, but never the same text as a value that
    /// is.
    ///
    /// [RFC 8785]: https://www.rfc-editor.org/rfc/rfc8785
    pub fn canonical(&self) -> String {
        let mut text = String::new();
        write_canonical(self, &mut text);
        text
    }

    /// What the ledger keeps of an entry Mindflayer wrote: the SHA-256 of its
    /// [canonical form](Value::canonical), as 64 lowercase hex digits.
    ///
    /// The same whatever the file does to the entry's layout: its indentation,
    /// the order of its keys, whether TOML writes it as a table or inline,
    /// which quotes a string is in. Anything that changes a value changes it.
    /// So a file somebody reformatted still holds Mindflayer's entry, and one
    /// somebody edited holds theirs.
    ///
    /// A digest rather than the canonical text itself: every row of the
    /// ledger is then as long as every other, as a copy's
    /// [`crate::hash::tree`] is, however long the entry.
    pub fn fingerprint(&self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in Sha256::digest(self.canonical().as_bytes()) {
            // Writing to a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Integer(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

fn write_canonical(value: &Value, text: &mut String) {
    match value {
        Value::Null => text.push_str("null"),
        Value::Bool(value) => text.push_str(if *value { "true" } else { "false" }),
        Value::Integer(value) => {
            let _ = write!(text, "{value}");
        }
        Value::Float(value) => text.push_str(&number(*value)),
        Value::String(value) => quote(value, text),
        Value::Array(values) => {
            text.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    text.push(',');
                }
                write_canonical(value, text);
            }
            text.push(']');
        }
        Value::Object(members) => {
            // By UTF-16 code units, as the scheme says, which differs from
            // Rust's order of `str`s only where a character past U+FFFF meets
            // one from U+E000 up. Stable, so a key a file repeats keeps the
            // order it was written in.
            let mut sorted: Vec<_> = members.iter().collect();
            sorted.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
            text.push('{');
            for (index, (key, value)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    text.push(',');
                }
                quote(key, text);
                text.push(':');
                write_canonical(value, text);
            }
            text.push('}');
        }
    }
}

/// A string as RFC 8785 writes it: the two characters JSON reserves and the
/// control characters escaped, in their short forms where JSON has one, and
/// every other character as itself.
fn quote(value: &str, text: &mut String) {
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(text, "\\u{:04x}", u32::from(control));
            }
            other => text.push(other),
        }
    }
    text.push('"');
}

/// A double as ECMAScript's `Number.prototype.toString` writes it, which is
/// what RFC 8785 asks for.
///
/// Rust's `{:e}` already gives the fewest significant digits that read back
/// as the same double, which is the digit string ECMAScript starts from, but
/// for one case ([`shortest_digits`]); what is left is where ECMAScript puts
/// the point, and when it turns to an exponent.
pub(crate) fn number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    // Negative zero too, which ECMAScript writes as `0`.
    if value == 0.0 {
        return "0".to_owned();
    }

    let (digits, exponent) = shortest_digits(value.abs());
    // In ECMAScript's terms: k digits, and the value is 0.digits × 10^n.
    let k = digits.len() as i32;
    let n = exponent + 1;

    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (whole, fraction) = digits.split_at(n as usize);
        format!("{whole}.{fraction}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let (first, rest) = digits.split_at(1);
        let sign = if n - 1 < 0 { '-' } else { '+' };
        let power = (n - 1).abs();
        if rest.is_empty() {
            format!("{first}e{sign}{power}")
        } else {
            format!("{first}.{rest}e{sign}{power}")
        }
    };
    if value < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// The fewest significant digits that read back as `value`, a positive,
/// finite double, and the power of ten of the first: `(digits, exponent)`
/// for `d.ddd × 10^exponent`.
///
/// Rust's, but for a tie. When the double lies exactly halfway between two
/// digit strings of that length, both read back as it, and ECMAScript takes
/// the even one where Rust may take the other: 1424953923781206.25 is
/// `1424953923781206.2` to ECMAScript, `…206.3` to Rust. A tie is a double
/// whose exact decimal value ends in a 5 one place past the shortest digits,
/// which the exact value, which Rust also writes, shows.
fn shortest_digits(value: f64) -> (String, i32) {
    let (digits, exponent) = scientific(&format!("{value:e}"));
    // 800 places hold every digit a double has; the most is 767.
    let (exact, exact_exponent) = scientific(&format!("{value:.800e}"));
    let exact = exact.trim_end_matches('0');

    let k = digits.len();
    if exact.len() != k + 1 || !exact.ends_with('5') || exact_exponent != exponent {
        return (digits, exponent);
    }
    let lower = &exact[..k];
    let last = lower.as_bytes()[k - 1] - b'0';
    let even = if last.is_multiple_of(2) {
        lower.to_owned()
    } else if last == 9 {
        // One more would carry into a string ending in 0, which has fewer
        // digits: had it read back, the shortest would have been that one.
        return (digits, exponent);
    } else {
        let mut raised = lower.as_bytes().to_vec();
        raised[k - 1] += 1;
        String::from_utf8(raised).expect("digits are ASCII")
    };
    let (first, rest) = even.split_at(1);
    let reads_back = format!("{first}.{rest}e{exponent}").parse::<f64>() == Ok(value);
    if reads_back {
        (even, exponent)
    } else {
        (digits, exponent)
    }
}

/// The digits and exponent of what `{:e}` wrote: `1.25e3` is `("125", 3)`.
fn scientific(written: &str) -> (String, i32) {
    let (mantissa, exponent) = written
        .split_once('e')
        .expect("`{:e}` always writes an exponent");
    let digits = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent = exponent.parse().expect("`{:e}` writes a whole exponent");
    (digits, exponent)
}

#[cfg(test)]
#[path = "config_entry_test.rs"]
mod tests;
