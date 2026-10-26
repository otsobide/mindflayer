//! Unit tests for `config_entry.rs`, kept beside it rather than inside it.

use super::*;

/// An MCP server entry as an agent's shape might render it.
fn server() -> Value {
    Value::object([
        ("command", Value::from("npx")),
        (
            "args",
            Value::array(["-y", "@modelcontextprotocol/server-github"]),
        ),
        (
            "env",
            Value::object([("GITHUB_TOKEN", Value::from("${GITHUB_TOKEN}"))]),
        ),
    ])
}

#[test]
fn the_canonical_form_sorts_members_at_every_depth_and_drops_whitespace() {
    assert_eq!(
        server().canonical(),
        r#"{"args":["-y","@modelcontextprotocol/server-github"],"command":"npx","env":{"GITHUB_TOKEN":"${GITHUB_TOKEN}"}}"#
    );
}

#[test]
fn members_are_sorted_by_utf16_code_units_as_rfc_8785_sorts_them() {
    // U+1F600 is the pair D83D DE00 in UTF-16, so it sorts before U+E000,
    // though its code point, and its UTF-8, sort after.
    let value = Value::object([
        ("\u{e000}", Value::from(1)),
        ("\u{1f600}", Value::from(2)),
        ("a", Value::from(3)),
    ]);
    assert_eq!(
        value.canonical(),
        "{\"a\":3,\"\u{1f600}\":2,\"\u{e000}\":1}"
    );
}

#[test]
fn a_key_given_twice_keeps_both_in_the_order_written() {
    let value = Value::object([
        ("b", Value::from(1)),
        ("a", Value::from(2)),
        ("b", Value::from(3)),
    ]);
    assert_eq!(value.canonical(), r#"{"a":2,"b":1,"b":3}"#);
}

#[test]
fn strings_escape_only_what_json_must() {
    let value = Value::from("quote \" backslash \\ \u{8}\u{c}\n\r\t \u{1} \u{1f} \u{7f} é /");
    assert_eq!(
        value.canonical(),
        "\"quote \\\" backslash \\\\ \\b\\f\\n\\r\\t \\u0001 \\u001f \u{7f} é /\""
    );
}

#[test]
fn literals_are_written_as_json_writes_them() {
    let value = Value::array([Value::Null, Value::Bool(true), Value::Bool(false)]);
    assert_eq!(value.canonical(), "[null,true,false]");
}

#[test]
fn floats_are_written_as_ecmascript_writes_a_double() {
    // RFC 8785's own examples (appendix B), by their bits, and a few more
    // where ECMAScript moves the point or turns to an exponent.
    let cases: &[(f64, &str)] = &[
        (f64::from_bits(0x0000000000000000), "0"),
        (f64::from_bits(0x8000000000000000), "0"),
        (f64::from_bits(0x0000000000000001), "5e-324"),
        (f64::from_bits(0x8000000000000001), "-5e-324"),
        (
            f64::from_bits(0x7fefffffffffffff),
            "1.7976931348623157e+308",
        ),
        (
            f64::from_bits(0xffefffffffffffff),
            "-1.7976931348623157e+308",
        ),
        (f64::from_bits(0x4340000000000000), "9007199254740992"),
        (f64::from_bits(0xc340000000000000), "-9007199254740992"),
        (f64::from_bits(0x4430000000000000), "295147905179352830000"),
        (f64::from_bits(0x44b52d02c7e14af5), "9.999999999999997e+22"),
        (f64::from_bits(0x44b52d02c7e14af6), "1e+23"),
        (f64::from_bits(0x44b52d02c7e14af7), "1.0000000000000001e+23"),
        (f64::from_bits(0x444b1ae4d6e2ef4e), "999999999999999700000"),
        (f64::from_bits(0x444b1ae4d6e2ef4f), "999999999999999900000"),
        (f64::from_bits(0x444b1ae4d6e2ef50), "1e+21"),
        (f64::from_bits(0x3eb0c6f7a0b5ed8c), "9.999999999999997e-7"),
        (f64::from_bits(0x3eb0c6f7a0b5ed8d), "0.000001"),
        (f64::from_bits(0x41b3de4355555553), "333333333.3333332"),
        (f64::from_bits(0x41b3de4355555554), "333333333.33333325"),
        (f64::from_bits(0x41b3de4355555555), "333333333.3333333"),
        (f64::from_bits(0x41b3de4355555556), "333333333.3333334"),
        (f64::from_bits(0x41b3de4355555557), "333333333.33333343"),
        (
            f64::from_bits(0xbecbf647612f3696),
            "-0.0000033333333333333333",
        ),
        (f64::from_bits(0x43143ff3c1cb0959), "1424953923781206.2"),
        (1.0, "1"),
        (1.5, "1.5"),
        (-0.25, "-0.25"),
        (100.0, "100"),
        (1e-7, "1e-7"),
        (1.5e-7, "1.5e-7"),
        (123456.789, "123456.789"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ];
    for (value, written) in cases {
        assert_eq!(&Value::Float(*value).canonical(), written, "{value:e}");
    }
}

#[test]
fn a_float_halfway_between_two_shortest_spellings_takes_the_even_one() {
    // 1424953923781206.25 exactly, between …206.2 and …206.3, both of which
    // read back as it. Rust's own shortest digits end in 3; ECMAScript's in 2.
    let tie = f64::from_bits(0x43143ff3c1cb0959);
    assert_eq!(format!("{tie:.2}"), "1424953923781206.25");
    assert_eq!(format!("{tie:e}"), "1.4249539237812063e15");
    assert_eq!(Value::Float(tie).canonical(), "1424953923781206.2");
}

#[test]
fn integers_are_written_exactly_even_past_what_a_double_holds() {
    assert_eq!(
        Value::Integer(9007199254740993).canonical(),
        "9007199254740993"
    );
    assert_eq!(Value::Integer(i64::MIN).canonical(), "-9223372036854775808");
}

#[test]
fn an_integer_and_the_float_of_the_same_number_are_one_value() {
    assert_eq!(
        Value::Integer(10).fingerprint(),
        Value::Float(10.0).fingerprint()
    );
}

#[test]
fn a_fixed_entry_has_the_same_fingerprint_on_every_platform() {
    // Pinned, so the macOS and Windows runs of CI check the recipe against
    // the value Linux gives: the SHA-256 of the canonical text above.
    assert_eq!(
        server().fingerprint(),
        "8018e6426ad14596754b46ee1abfc8f9281866b8648b31f965b853eeb4ff7d19"
    );
}

#[test]
fn the_fingerprint_ignores_the_order_of_keys() {
    let reordered = Value::object([
        (
            "env",
            Value::object([("GITHUB_TOKEN", Value::from("${GITHUB_TOKEN}"))]),
        ),
        ("command", Value::from("npx")),
        (
            "args",
            Value::array(["-y", "@modelcontextprotocol/server-github"]),
        ),
    ]);
    assert_eq!(reordered.fingerprint(), server().fingerprint());
}

#[test]
fn the_fingerprint_changes_when_any_value_does() {
    let changed = [
        Value::object([
            ("command", Value::from("npx")),
            (
                "args",
                Value::array(["-y", "@modelcontextprotocol/server-gitlab"]),
            ),
            (
                "env",
                Value::object([("GITHUB_TOKEN", Value::from("${GITHUB_TOKEN}"))]),
            ),
        ]),
        // The same arguments in another order: an array's order is its value.
        Value::object([
            ("command", Value::from("npx")),
            (
                "args",
                Value::array(["@modelcontextprotocol/server-github", "-y"]),
            ),
            (
                "env",
                Value::object([("GITHUB_TOKEN", Value::from("${GITHUB_TOKEN}"))]),
            ),
        ]),
        // One member fewer.
        Value::object([
            ("command", Value::from("npx")),
            (
                "args",
                Value::array(["-y", "@modelcontextprotocol/server-github"]),
            ),
        ]),
    ];
    for value in changed {
        assert_ne!(value.fingerprint(), server().fingerprint(), "{value:?}");
    }
    // A number and the string of it are not one value.
    assert_ne!(Value::from(1).fingerprint(), Value::from("1").fingerprint());
}
