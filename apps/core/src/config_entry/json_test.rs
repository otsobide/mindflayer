//! Unit tests for `config_entry/json.rs`: what an edit through the CST keeps,
//! byte for byte, in files written the ways people write them.

use crate::config_entry::{edit, edit_document, get, Edit, EntryError, Format, Value};

/// A server entry, the shape of which is nobody's in particular.
fn server(name: &str) -> Value {
    Value::object([
        ("type", Value::from("local")),
        (
            "command",
            Value::array(["npx", "-y", &format!("@modelcontextprotocol/server-{name}")]),
        ),
    ])
}

/// JSONC as a person keeps it: a comment before the object, comments between
/// members and after them, trailing commas, three-space indentation, and a
/// member written on one line.
const JSONC: &str = r#"// Servers the whole team runs.
{
   "$schema": "https://opencode.ai/config.json",
   /* one entry per server */
   "mcp": {
      "filesystem": {
         "type": "local",
         "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"], // pinned
      },
      // the tracker the team hosts
      "tracker": { "type": "remote", "url": "https://tracker.example/mcp" },
   },
   "theme": "dark", // keep
}
"#;

#[test]
fn an_insert_goes_after_the_last_entry_and_keeps_every_other_byte() {
    let edited = edit(
        Format::Jsonc,
        JSONC,
        &["mcp", "github"],
        &Edit::Insert(server("github")),
    )
    .unwrap();

    assert_eq!(
        edited,
        r#"// Servers the whole team runs.
{
   "$schema": "https://opencode.ai/config.json",
   /* one entry per server */
   "mcp": {
      "filesystem": {
         "type": "local",
         "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"], // pinned
      },
      // the tracker the team hosts
      "tracker": { "type": "remote", "url": "https://tracker.example/mcp" },
      "github": {
         "type": "local",
         "command": ["npx", "-y", "@modelcontextprotocol/server-github"],
      },
   },
   "theme": "dark", // keep
}
"#
    );
}

#[test]
fn a_replace_rewrites_the_value_and_keeps_the_key_and_what_is_around_it() {
    let edited = edit(
        Format::Jsonc,
        JSONC,
        &["mcp", "tracker"],
        &Edit::Replace(Value::object([
            ("type", Value::from("remote")),
            ("url", Value::from("https://tracker.example/v2/mcp")),
        ])),
    )
    .unwrap();

    assert_eq!(
        edited,
        r#"// Servers the whole team runs.
{
   "$schema": "https://opencode.ai/config.json",
   /* one entry per server */
   "mcp": {
      "filesystem": {
         "type": "local",
         "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"], // pinned
      },
      // the tracker the team hosts
      "tracker": {
         "type": "remote",
         "url": "https://tracker.example/v2/mcp"
      },
   },
   "theme": "dark", // keep
}
"#
    );
}

#[test]
fn a_remove_takes_the_entry_and_its_comma_and_leaves_the_comments_above_the_next() {
    let edited = edit(Format::Jsonc, JSONC, &["mcp", "filesystem"], &Edit::Remove).unwrap();

    assert_eq!(
        edited,
        r#"// Servers the whole team runs.
{
   "$schema": "https://opencode.ai/config.json",
   /* one entry per server */
   "mcp": {
      // the tracker the team hosts
      "tracker": { "type": "remote", "url": "https://tracker.example/mcp" },
   },
   "theme": "dark", // keep
}
"#
    );
}

#[test]
fn a_comment_on_a_line_above_a_removed_entry_stays_and_one_on_its_line_goes() {
    let edited = edit(Format::Jsonc, JSONC, &["theme"], &Edit::Remove).unwrap();
    assert!(edited.ends_with("\n   },\n}\n"), "{edited}");

    let edited = edit(Format::Jsonc, JSONC, &["mcp", "tracker"], &Edit::Remove).unwrap();
    assert!(
        edited.contains("// pinned\n      },\n      // the tracker the team hosts\n   },"),
        "{edited}"
    );
}

/// `.mcp.json` as Windows tools leave it: strict JSON, CRLF line endings and
/// three-space indentation.
const CRLF: &str = "{\r\n   \"mcpServers\": {\r\n      \"filesystem\": {\r\n         \"command\": \"npx\"\r\n      }\r\n   }\r\n}\r\n";

#[test]
fn crlf_and_the_file_s_own_indentation_are_kept_by_every_edit() {
    let inserted = edit(
        Format::Json,
        CRLF,
        &["mcpServers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(
        inserted,
        "{\r\n   \"mcpServers\": {\r\n      \"filesystem\": {\r\n         \"command\": \"npx\"\r\n      },\r\n      \"github\": {\r\n         \"command\": \"gh\"\r\n      }\r\n   }\r\n}\r\n"
    );

    let replaced = edit(
        Format::Json,
        &inserted,
        &["mcpServers", "filesystem"],
        &Edit::Replace(Value::object([("command", Value::from("uvx"))])),
    )
    .unwrap();
    assert_eq!(
        replaced,
        "{\r\n   \"mcpServers\": {\r\n      \"filesystem\": {\r\n         \"command\": \"uvx\"\r\n      },\r\n      \"github\": {\r\n         \"command\": \"gh\"\r\n      }\r\n   }\r\n}\r\n"
    );

    let removed = edit(
        Format::Json,
        &replaced,
        &["mcpServers", "filesystem"],
        &Edit::Remove,
    )
    .unwrap();
    assert_eq!(
        removed,
        "{\r\n   \"mcpServers\": {\r\n      \"github\": {\r\n         \"command\": \"gh\"\r\n      }\r\n   }\r\n}\r\n"
    );
}

#[test]
fn tabs_are_kept_by_every_edit() {
    let tabs = "{\n\t\"mcpServers\": {\n\t\t\"filesystem\": {\n\t\t\t\"command\": \"npx\"\n\t\t}\n\t}\n}\n";
    let inserted = edit(
        Format::Json,
        tabs,
        &["mcpServers", "github"],
        &Edit::Insert(Value::object([
            ("command", Value::from("gh")),
            ("env", Value::object([("TOKEN", Value::from("${TOKEN}"))])),
        ])),
    )
    .unwrap();
    assert_eq!(
        inserted,
        "{\n\t\"mcpServers\": {\n\t\t\"filesystem\": {\n\t\t\t\"command\": \"npx\"\n\t\t},\n\t\t\"github\": {\n\t\t\t\"command\": \"gh\",\n\t\t\t\"env\": {\n\t\t\t\t\"TOKEN\": \"${TOKEN}\"\n\t\t\t}\n\t\t}\n\t}\n}\n"
    );
}

#[test]
fn removing_what_was_inserted_gives_back_the_file_byte_for_byte() {
    let tabs = "{\n\t\"mcpServers\": {\n\t\t\"filesystem\": {\n\t\t\t\"command\": \"npx\"\n\t\t}\n\t}\n}\n";
    let blank = "{\n  \"mcpServers\": {\n\n    \"a\": 1\n\n  }\n}\n";
    for (format, original) in [
        (Format::Jsonc, JSONC),
        (Format::Json, CRLF),
        (Format::Json, tabs),
        (Format::Json, blank),
    ] {
        let parent = if format == Format::Jsonc {
            "mcp"
        } else {
            "mcpServers"
        };
        let key = [parent, "github"];
        let inserted = edit(format, original, &key, &Edit::Insert(server("github"))).unwrap();
        assert_ne!(inserted, original);
        let removed = edit(format, &inserted, &key, &Edit::Remove).unwrap();
        assert_eq!(removed, original);
    }
}

#[test]
fn strict_json_refuses_what_jsonc_allows() {
    for text in [
        "{\n  // a comment\n  \"mcpServers\": {}\n}\n",
        "{\n  \"mcpServers\": {},\n}\n",
    ] {
        assert!(matches!(
            get(Format::Json, text, &["mcpServers"]),
            Err(EntryError::Malformed {
                format: Format::Json,
                ..
            })
        ));
        assert!(get(Format::Jsonc, text, &["mcpServers"]).unwrap().is_some());
    }
}

#[test]
fn jsonc_refuses_what_only_json5_allows() {
    for text in [
        "{ 'mcp': {} }",
        "{ mcp: {} }",
        "{ \"port\": 0x1F }",
        "{ \"port\": +1 }",
        "{ \"port\": .5 }",
        "{ \"port\": Infinity }",
        "{ \"a\": 1 \"b\": 2 }",
        "{ \"a\": \"\\x41\" }",
    ] {
        assert!(
            matches!(
                get(Format::Jsonc, text, &["a"]),
                Err(EntryError::Malformed { .. })
            ),
            "{text}"
        );
    }
}

#[test]
fn an_insert_into_strict_json_adds_no_trailing_comma() {
    let text = "{\n  \"mcpServers\": {\n    \"filesystem\": {\n      \"command\": \"npx\"\n    }\n  }\n}\n";
    let edited = edit(
        Format::Json,
        text,
        &["mcpServers", "github"],
        &Edit::Insert(server("github")),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"mcpServers\": {\n    \"filesystem\": {\n      \"command\": \"npx\"\n    },\n    \"github\": {\n      \"type\": \"local\",\n      \"command\": [\"npx\", \"-y\", \"@modelcontextprotocol/server-github\"]\n    }\n  }\n}\n"
    );
}

#[test]
fn objects_missing_on_the_way_go_in_with_the_entry() {
    let text = "{\n  \"theme\": \"dark\"\n}\n";
    let edited = edit(
        Format::Json,
        text,
        &["mcpServers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    }\n  }\n}\n"
    );
}

#[test]
fn a_file_not_there_yet_becomes_an_object_holding_the_entry() {
    let written = edit_document(
        Format::Json,
        None,
        &["mcpServers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(
        written,
        "{\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    }\n  }\n}\n"
    );
}

#[test]
fn removing_the_last_entry_leaves_its_object_empty() {
    let text = "{\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    }\n  },\n  \"theme\": \"dark\"\n}\n";
    let edited = edit(Format::Json, text, &["mcpServers", "github"], &Edit::Remove).unwrap();
    assert_eq!(
        edited,
        "{\n  \"mcpServers\": {},\n  \"theme\": \"dark\"\n}\n"
    );
}

#[test]
fn blank_lines_just_inside_the_object_an_entry_goes_into_stay() {
    let text = "{\n  \"mcpServers\": {\n\n    \"a\": 1,\n\n    \"b\": 2\n\n  }\n}\n";
    let edited = edit(
        Format::Json,
        text,
        &["mcpServers", "c"],
        &Edit::Insert(Value::from(3)),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"mcpServers\": {\n\n    \"a\": 1,\n\n    \"b\": 2,\n    \"c\": 3\n\n  }\n}\n"
    );
}

#[test]
fn an_object_written_on_one_line_takes_an_entry_on_that_line() {
    let gh = Value::object([
        ("command", Value::from("gh")),
        ("args", Value::array(["mcp"])),
        ("env", Value::object(Vec::<(String, Value)>::new())),
    ]);
    let cases = [
        (
            "{ \"mcpServers\": { \"a\": 1 } }\n",
            "{ \"mcpServers\": { \"a\": 1, \"github\": { \"command\": \"gh\", \"args\": [\"mcp\"], \"env\": {} } } }\n",
        ),
        // Minified, as a tool that writes JSON without spaces leaves it.
        (
            "{\"mcpServers\":{\"a\":1,\"b\":2}}",
            "{\"mcpServers\":{\"a\":1,\"b\":2, \"github\": { \"command\": \"gh\", \"args\": [\"mcp\"], \"env\": {} }}}",
        ),
        // An object on one line in a file on several: the file's lines stay.
        (
            "{\n  \"mcpServers\": {\"a\": {\"enabled\": true}}\n}\n",
            "{\n  \"mcpServers\": {\"a\": {\"enabled\": true}, \"github\": { \"command\": \"gh\", \"args\": [\"mcp\"], \"env\": {} }}\n}\n",
        ),
    ];
    for (text, expected) in cases {
        let key = ["mcpServers", "github"];
        let edited = edit(Format::Json, text, &key, &Edit::Insert(gh.clone())).unwrap();
        assert_eq!(edited, expected);
        assert_eq!(
            edit(Format::Json, &edited, &key, &Edit::Remove).unwrap(),
            text
        );
    }
}

#[test]
fn an_insert_goes_after_the_comment_on_the_last_entry_s_line_and_its_comma_before() {
    let text = "{\n  \"a\": 1 // the first\n  // above the brace\n}\n";
    let edited = edit(Format::Jsonc, text, &["b"], &Edit::Insert(Value::from(2))).unwrap();
    assert_eq!(
        edited,
        "{\n  \"a\": 1, // the first\n  \"b\": 2\n  // above the brace\n}\n"
    );
}

#[test]
fn an_entry_goes_into_an_empty_object_after_the_comments_in_it() {
    let text = "{\n  \"mcp\": {\n    // none yet\n  },\n  \"theme\": \"dark\",\n}\n";
    let edited = edit(
        Format::Jsonc,
        text,
        &["mcp", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    // With trailing commas, since the file writes them.
    assert_eq!(
        edited,
        "{\n  \"mcp\": {\n    // none yet\n    \"github\": {\n      \"command\": \"gh\",\n    },\n  },\n  \"theme\": \"dark\",\n}\n"
    );

    let edited = edit(
        Format::Json,
        "{\n  \"mcpServers\": {},\n  \"theme\": \"dark\"\n}\n",
        &["mcpServers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    }\n  },\n  \"theme\": \"dark\"\n}\n"
    );
}

#[test]
fn an_array_of_objects_is_laid_out_on_lines_and_one_of_values_on_one() {
    let edited = edit(
        Format::Json,
        "{\n  \"a\": 1\n}\n",
        &["b"],
        &Edit::Insert(Value::object([
            ("flat", Value::array([Value::from(1), Value::from(2)])),
            (
                "nested",
                Value::array([
                    Value::object([("x", Value::from(1))]),
                    Value::array([] as [Value; 0]),
                ]),
            ),
        ])),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"a\": 1,\n  \"b\": {\n    \"flat\": [1, 2],\n    \"nested\": [\n      {\n        \"x\": 1\n      },\n      []\n    ]\n  }\n}\n"
    );
}

#[test]
fn get_reads_an_entry_and_nothing_where_there_is_none() {
    assert_eq!(
        get(Format::Jsonc, JSONC, &["mcp", "tracker"]).unwrap(),
        Some(Value::object([
            ("type", Value::from("remote")),
            ("url", Value::from("https://tracker.example/mcp")),
        ]))
    );
    assert_eq!(get(Format::Jsonc, JSONC, &["mcp", "github"]).unwrap(), None);
    assert_eq!(
        get(Format::Jsonc, JSONC, &["servers", "github"]).unwrap(),
        None
    );
}

#[test]
fn numbers_are_read_as_javascript_reads_them() {
    let text = r#"{ "a": 10, "b": -3, "c": 1.5, "d": 1e3, "e": 18446744073709551616 }"#;
    let read = |key| get(Format::Json, text, &[key]).unwrap().unwrap();
    assert_eq!(read("a"), Value::Integer(10));
    assert_eq!(read("b"), Value::Integer(-3));
    assert_eq!(read("c"), Value::Float(1.5));
    assert_eq!(read("d"), Value::Float(1000.0));
    assert_eq!(read("e"), Value::Float(18446744073709551616.0));
}

#[test]
fn strings_and_keys_are_escaped_where_json_needs_it() {
    let edited = edit(
        Format::Json,
        "{}\n",
        &["say \"hi\""],
        &Edit::Insert(Value::from("line\nbreak \\ tab\t")),
    )
    .unwrap();
    assert_eq!(
        edited,
        "{\n  \"say \\\"hi\\\"\": \"line\\nbreak \\\\ tab\\t\"\n}\n"
    );
    assert_eq!(
        get(Format::Json, &edited, &["say \"hi\""]).unwrap(),
        Some(Value::from("line\nbreak \\ tab\t"))
    );
}

#[test]
fn a_key_written_twice_on_the_way_is_refused() {
    let text = r#"{ "mcpServers": { "a": 1 }, "mcpServers": { "b": 2 } }"#;
    for result in [
        get(Format::Json, text, &["mcpServers", "a"]).map(|_| ()),
        edit(
            Format::Json,
            text,
            &["mcpServers", "c"],
            &Edit::Insert(Value::from(3)),
        )
        .map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(EntryError::Ambiguous { key }) if key == "mcpServers"),
            "{result:?}"
        );
    }
}

#[test]
fn a_file_whose_top_is_not_an_object_holds_no_entries() {
    let error = edit(
        Format::Json,
        "[1, 2]\n",
        &["a"],
        &Edit::Insert(Value::from(1)),
    )
    .unwrap_err();
    assert!(matches!(&error, EntryError::NotAnObject { key } if key.is_empty()));
    assert_eq!(
        error.to_string(),
        "the top level: neither an object nor a table, so it holds no entries"
    );
}

#[test]
fn a_file_with_no_value_is_refused() {
    for text in ["", "  \n", "// only a comment\n"] {
        assert!(
            matches!(
                get(Format::Jsonc, text, &["a"]),
                Err(EntryError::Malformed { .. })
            ),
            "{text:?}"
        );
    }
}

#[test]
fn a_number_json_cannot_write_is_refused() {
    let error = edit(
        Format::Json,
        "{}\n",
        &["a"],
        &Edit::Insert(Value::Float(f64::NAN)),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "JSON has no way to write a number that is not finite"
    );
}

#[test]
fn a_float_is_written_as_the_fingerprint_writes_it() {
    let edited = edit(
        Format::Json,
        "{}\n",
        &["timeout"],
        &Edit::Insert(Value::Float(1e300)),
    )
    .unwrap();
    assert_eq!(edited, "{\n  \"timeout\": 1e+300\n}\n");
}
