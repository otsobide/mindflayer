//! Unit tests for `config_entry/toml.rs`: what an edit through toml_edit
//! keeps, byte for byte, in files written the ways people write them.

use crate::config_entry::{edit, edit_document, get, Edit, EntryError, Format, Value};

/// A server entry, the shape of which is nobody's in particular.
fn server(name: &str) -> Value {
    Value::object([
        ("command", Value::from("npx")),
        (
            "args",
            Value::array(["-y", &format!("@modelcontextprotocol/server-{name}")]),
        ),
        ("env", Value::object([("TOKEN", Value::from("${TOKEN}"))])),
    ])
}

/// TOML as a person keeps it: an opening comment filed with the first key,
/// comments above tables and after values, blank lines, and a table that is
/// not a server after the servers.
const TOML: &str = r#"# Servers the whole team runs.
model = "o3"

# one table per server
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"] # pinned

# the tracker the team hosts
[mcp_servers.tracker]
url = "https://tracker.example/mcp"

[profiles.fast]
model = "o4-mini"
"#;

#[test]
fn an_insert_goes_after_the_last_table_beside_it_and_keeps_every_other_byte() {
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "github"],
        &Edit::Insert(server("github")),
    )
    .unwrap();

    assert_eq!(
        edited,
        r#"# Servers the whole team runs.
model = "o3"

# one table per server
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"] # pinned

# the tracker the team hosts
[mcp_servers.tracker]
url = "https://tracker.example/mcp"

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.github.env]
TOKEN = "${TOKEN}"

[profiles.fast]
model = "o4-mini"
"#
    );
}

#[test]
fn a_replace_keeps_the_key_the_comments_around_it_and_its_layout() {
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "tracker"],
        &Edit::Replace(Value::object([(
            "url",
            Value::from("https://tracker.example/v2/mcp"),
        )])),
    )
    .unwrap();
    assert_eq!(
        edited,
        TOML.replace(
            "https://tracker.example/mcp",
            "https://tracker.example/v2/mcp"
        )
    );

    // A value keeps the comment after it on its line.
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "filesystem", "args"],
        &Edit::Replace(Value::array(["@modelcontextprotocol/server-filesystem"])),
    )
    .unwrap();
    assert_eq!(
        edited,
        TOML.replace(
            r#"["-y", "@modelcontextprotocol/server-filesystem"] # pinned"#,
            r#"["@modelcontextprotocol/server-filesystem"] # pinned"#
        )
    );
}

#[test]
fn a_remove_takes_the_entry_and_leaves_the_comments_above_it() {
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "filesystem"],
        &Edit::Remove,
    )
    .unwrap();
    assert_eq!(
        edited,
        r#"# Servers the whole team runs.
model = "o3"

# one table per server

# the tracker the team hosts
[mcp_servers.tracker]
url = "https://tracker.example/mcp"

[profiles.fast]
model = "o4-mini"
"#
    );

    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "tracker"],
        &Edit::Remove,
    )
    .unwrap();
    assert!(
        edited.contains("# pinned\n\n# the tracker the team hosts\n\n[profiles.fast]"),
        "{edited}"
    );
}

#[test]
fn the_file_s_opening_comment_stays_when_the_key_it_is_filed_with_goes() {
    let edited = edit(Format::Toml, TOML, &["model"], &Edit::Remove).unwrap();
    assert_eq!(edited, TOML.replacen("model = \"o3\"\n", "", 1));
}

#[test]
fn a_comment_on_a_removed_value_s_line_goes_with_it() {
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "filesystem", "args"],
        &Edit::Remove,
    )
    .unwrap();
    assert_eq!(
        edited,
        TOML.replace(
            "args = [\"-y\", \"@modelcontextprotocol/server-filesystem\"] # pinned\n",
            ""
        )
    );
}

#[test]
fn a_sub_table_of_a_removed_entry_goes_with_it() {
    let text = "[mcp_servers.github]\ncommand = \"gh\"\n\n[mcp_servers.github.env]\nTOKEN = \"x\"\n\n[other]\na = 1\n";
    let edited = edit(
        Format::Toml,
        text,
        &["mcp_servers", "github"],
        &Edit::Remove,
    )
    .unwrap();
    assert_eq!(edited, "\n[other]\na = 1\n");
}

#[test]
fn crlf_is_kept_by_every_edit() {
    let crlf = TOML.replace('\n', "\r\n");
    let key = ["mcp_servers", "github"];

    let inserted = edit(Format::Toml, &crlf, &key, &Edit::Insert(server("github"))).unwrap();
    let expected = edit(Format::Toml, TOML, &key, &Edit::Insert(server("github"))).unwrap();
    assert_eq!(inserted, expected.replace('\n', "\r\n"));

    let replaced = edit(Format::Toml, &inserted, &key, &Edit::Replace(server("gh"))).unwrap();
    assert!(replaced.contains("server-gh\"]\r\n"), "{replaced:?}");
    assert!(!replaced.replace("\r\n", "").contains('\n'), "{replaced:?}");

    let removed = edit(Format::Toml, &replaced, &key, &Edit::Remove).unwrap();
    assert_eq!(removed, crlf);
}

#[test]
fn a_file_whose_line_endings_are_mixed_is_refused() {
    let error = edit(
        Format::Toml,
        "a = 1\r\nb = 2\n",
        &["c"],
        &Edit::Insert(Value::from(3)),
    )
    .unwrap_err();
    assert!(matches!(error, EntryError::MixedLineEndings), "{error}");
}

#[test]
fn a_key_is_indented_as_the_one_before_it() {
    for indent in ["  ", "\t"] {
        let text = format!(
            "[mcp_servers.filesystem]\n{indent}command = \"npx\"\n{indent}args = [\"-y\"]\n\n[other]\na = 1\n"
        );
        let edited = edit(
            Format::Toml,
            &text,
            &["mcp_servers", "filesystem", "cwd"],
            &Edit::Insert(Value::from("/srv")),
        )
        .unwrap();
        assert_eq!(
            edited,
            text.replace("[\"-y\"]\n", &format!("[\"-y\"]\n{indent}cwd = \"/srv\"\n"))
        );
    }
}

#[test]
fn an_object_is_written_the_way_the_objects_beside_it_are() {
    let gh = Value::object([("command", Value::from("gh"))]);
    let cases = [
        // Inline beside inline, indented as the key before it.
        (
            "[mcp_servers]\nfilesystem = { command = \"npx\" }\n  # hosted\n  tracker = { url = \"x\" }\n",
            "[mcp_servers]\nfilesystem = { command = \"npx\" }\n  # hosted\n  tracker = { url = \"x\" }\n  github = { command = \"gh\" }\n",
        ),
        // Inline inside inline, the closing space kept before the brace.
        (
            "mcp_servers = { filesystem = { command = \"npx\" } }\n",
            "mcp_servers = { filesystem = { command = \"npx\" }, github = { command = \"gh\" } }\n",
        ),
        // Dotted among dotted.
        (
            "mcp_servers.filesystem.command = \"npx\"\n",
            "mcp_servers.filesystem.command = \"npx\"\nmcp_servers.github.command = \"gh\"\n",
        ),
        // A table of its own when there is nothing to follow, with the
        // table above it implied by its header rather than written.
        (
            "model = \"o3\"\n",
            "model = \"o3\"\n\n[mcp_servers.github]\ncommand = \"gh\"\n",
        ),
    ];
    for (text, expected) in cases {
        let edited = edit(
            Format::Toml,
            text,
            &["mcp_servers", "github"],
            &Edit::Insert(gh.clone()),
        )
        .unwrap();
        assert_eq!(edited, expected);
    }
}

#[test]
fn an_empty_object_among_dotted_keys_is_written_inline() {
    // Dotted keys with nothing under them write nothing at all.
    let text = "mcp_servers.filesystem.command = \"npx\"\n";
    let edited = edit(
        Format::Toml,
        text,
        &["mcp_servers", "empty"],
        &Edit::Insert(Value::object(Vec::<(String, Value)>::new())),
    )
    .unwrap();
    assert_eq!(edited, format!("{text}mcp_servers.empty = {{}}\n"));
}

#[test]
fn comments_above_dotted_keys_stay_through_a_replace_and_a_remove() {
    let text = "# above\nmcp_servers.filesystem.command = \"npx\" # on its line\nmcp_servers.filesystem.args = [\"-y\"]\n# below\nother = 1\n";
    let key = ["mcp_servers", "filesystem"];

    let replaced = edit(
        Format::Toml,
        text,
        &key,
        &Edit::Replace(Value::object([("command", Value::from("uvx"))])),
    )
    .unwrap();
    assert_eq!(
        replaced,
        "# above\nmcp_servers.filesystem.command = \"uvx\"\n# below\nother = 1\n"
    );

    let removed = edit(Format::Toml, text, &key, &Edit::Remove).unwrap();
    assert_eq!(removed, "# above\n# below\nother = 1\n");
}

#[test]
fn removing_what_was_inserted_gives_back_the_file_byte_for_byte() {
    let crlf = TOML.replace('\n', "\r\n");
    let texts = [
        TOML,
        &crlf,
        "",
        "# only comments, as a template leaves it\n",
        "model = \"o3\"\n",
        "[mcp_servers]\n  filesystem = { command = \"npx\" }\n",
        "mcp_servers = { filesystem = { command = \"npx\" } }\n",
        "mcp_servers.filesystem.command = \"npx\"\n",
        "[mcp_servers.filesystem]\n\tcommand = \"npx\"\n",
    ];
    let key = ["mcp_servers", "github"];
    for text in texts {
        let inserted = edit(Format::Toml, text, &key, &Edit::Insert(server("github"))).unwrap();
        assert_ne!(inserted, text);
        let removed = edit(Format::Toml, &inserted, &key, &Edit::Remove).unwrap();
        assert_eq!(removed, text);
    }
}

#[test]
fn a_file_of_comments_alone_keeps_them_above_what_goes_in() {
    let edited = edit(
        Format::Toml,
        "# Codex, for this project.\n",
        &["mcp_servers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(
        edited,
        "# Codex, for this project.\n[mcp_servers.github]\ncommand = \"gh\"\n"
    );
}

#[test]
fn a_file_not_there_yet_becomes_the_entry() {
    let written = edit_document(
        Format::Toml,
        None,
        &["mcp_servers", "github"],
        &Edit::Insert(Value::object([("command", Value::from("gh"))])),
    )
    .unwrap();
    assert_eq!(written, "[mcp_servers.github]\ncommand = \"gh\"\n");
}

#[test]
fn a_string_with_a_line_break_is_written_on_one_line() {
    let note = Value::from("first\nsecond\r\n\"quoted\" \\ \u{7f}\ttab");
    for text in [TOML.to_owned(), TOML.replace('\n', "\r\n")] {
        let edited = edit(
            Format::Toml,
            &text,
            &["mcp_servers", "tracker", "note"],
            &Edit::Insert(note.clone()),
        )
        .unwrap();
        assert!(
            edited.contains("note = \"first\\nsecond\\r\\n\\\"quoted\\\" \\\\ \\u007F\ttab\""),
            "{edited:?}"
        );
        assert_eq!(
            get(Format::Toml, &edited, &["mcp_servers", "tracker", "note"]).unwrap(),
            Some(note.clone())
        );
    }
}

#[test]
fn a_float_is_written_as_the_fingerprint_writes_it_and_stays_a_float() {
    let edited = edit(
        Format::Toml,
        "",
        &["numbers"],
        &Edit::Insert(Value::array([
            Value::Float(10.0),
            Value::Float(1e300),
            Value::Float(0.000001),
            Value::Float(f64::NAN),
            Value::Float(f64::NEG_INFINITY),
            Value::Integer(i64::MAX),
        ])),
    )
    .unwrap();
    assert_eq!(
        edited,
        "numbers = [10.0, 1e+300, 0.000001, nan, -inf, 9223372036854775807]\n"
    );
    let read = get(Format::Toml, &edited, &["numbers"]).unwrap().unwrap();
    let Value::Array(read) = read else {
        panic!("{read:?}")
    };
    assert_eq!(read[0], Value::Float(10.0));
    assert_eq!(read[5], Value::Integer(i64::MAX));
}

#[test]
fn null_has_no_way_into_toml() {
    let error = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "github"],
        &Edit::Insert(Value::object([("cwd", Value::Null)])),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "TOML has no way to write null");
}

#[test]
fn a_file_that_does_not_parse_is_refused_with_where() {
    for (text, detail) in [
        (
            "model = \"o3\"\nbroken =\n",
            "string values must be quoted, expected literal string on line 2 column 9",
        ),
        (
            "[a]\nb = 1\n[a]\nc = 2\n",
            "duplicate key on line 3 column 2",
        ),
    ] {
        let error = get(Format::Toml, text, &["a"]).unwrap_err();
        assert_eq!(error.to_string(), format!("not valid TOML: {detail}"));
    }
}

#[test]
fn one_entry_reads_as_one_value_however_toml_lays_it_out() {
    let layouts = [
        "[mcp_servers.github]\ncommand = \"npx\"\nargs = [\"-y\", \"@modelcontextprotocol/server-github\"]\n\n[mcp_servers.github.env]\nTOKEN = \"${TOKEN}\"\n",
        "[mcp_servers]\ngithub = { env = { TOKEN = '${TOKEN}' }, command = \"npx\", args = [\"-y\", \"@modelcontextprotocol/server-github\"] }\n",
        "mcp_servers.github.command = \"npx\"\nmcp_servers.github.args = [\n  \"-y\",\n  \"@modelcontextprotocol/server-github\",\n]\nmcp_servers.github.env.TOKEN = \"\"\"${TOKEN}\"\"\"\n",
    ];
    let key = ["mcp_servers", "github"];
    for text in layouts {
        let read = get(Format::Toml, text, &key).unwrap().unwrap();
        assert_eq!(read.fingerprint(), server("github").fingerprint(), "{text}");
    }
    // And the same entry in a JSON file is the same value.
    let json = r#"{"mcp_servers": {"github": {"args": ["-y", "@modelcontextprotocol/server-github"], "command": "npx", "env": {"TOKEN": "${TOKEN}"}}}}"#;
    let read = get(Format::Json, json, &key).unwrap().unwrap();
    assert_eq!(read.fingerprint(), server("github").fingerprint());
}

#[test]
fn a_datetime_is_read_as_the_text_it_is_written_in() {
    assert_eq!(
        get(Format::Toml, "since = 1979-05-27T07:32:00Z\n", &["since"]).unwrap(),
        Some(Value::from("1979-05-27T07:32:00Z"))
    );
}

#[test]
fn an_array_of_tables_holds_no_entries() {
    let error = edit(
        Format::Toml,
        "[[servers]]\nname = \"a\"\n",
        &["servers", "b"],
        &Edit::Insert(Value::from(1)),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "`servers`: neither an object nor a table, so it holds no entries"
    );
}

#[test]
fn a_table_replaced_by_a_value_becomes_a_key_of_the_table_above_it() {
    // A value cannot stand under a header of its own, so it moves up to the
    // table above, which is written out to hold it, and the comment above
    // the header goes with it.
    let edited = edit(
        Format::Toml,
        TOML,
        &["mcp_servers", "tracker"],
        &Edit::Replace(Value::from("off")),
    )
    .unwrap();
    assert_eq!(
        edited,
        r#"# Servers the whole team runs.
model = "o3"

[mcp_servers]

# the tracker the team hosts
tracker = "off"

# one table per server
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"] # pinned

[profiles.fast]
model = "o4-mini"
"#
    );
}
