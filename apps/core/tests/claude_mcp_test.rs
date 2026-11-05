//! An MCP server rendered into Claude Code's `.mcp.json`, and taken out
//! again: what the file holds afterwards, to the byte, for each transport,
//! beside the servers somebody else keeps there, and what is refused.
//! What an entry holds is pinned beside `claude_mcp.rs`; these hold the
//! file around it.

use std::fs;
use std::path::Path;

use mindweaver_core::claude_mcp::{self, ClaudeMcpError, FILE, MAX_BYTES};
use mindweaver_core::config_entry::Written;
use mindweaver_core::mother::{random, McpMother};
use mindweaver_core::McpDefinition;
use tempfile::TempDir;

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn render(root: &Path, text: &str) -> claude_mcp::Rendered {
    claude_mcp::render(root, &parsed(text)).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn read(root: &Path) -> String {
    fs::read_to_string(root.join(FILE)).unwrap()
}

fn codes(rendered: &claude_mcp::Rendered) -> Vec<&'static str> {
    rendered
        .entry
        .warnings
        .iter()
        .map(|warning| warning.code())
        .collect()
}

/// The names in `dir`, sorted: what a write leaves beside the file.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A team's own `.mcp.json`: four spaces, `type` last where it is
/// written at all, an http server Claude drops for having none, written on
/// one line, and a key beside the servers.
const TEAMS: &str = r#"{
    "mcpServers": {
        "filesystem": {
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "."],
            "type": "stdio"
        },
        "untyped": {"url": "http://localhost:3000/mcp"}
    },
    "theirs": true
}
"#;

#[test]
fn a_program_is_written_into_a_file_made_for_it_its_folder_left_out() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::stdio(&name));

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(rendered.file, root.path().join(".mcp.json"));
    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
  "mcpServers": {{
    "{name}": {{
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@example/server", "--workspace", "${{EXAMPLE_WORKSPACE}}"],
      "env": {{
        "API_KEY": "${{EXAMPLE_API_KEY}}",
        "LOG_LEVEL": "info"
      }},
      "timeout": 30000
    }}
  }}
}}
"#
        )
    );
    assert_eq!(codes(&rendered), ["cwd_unsupported"]);
    assert_eq!(names(root.path()), [".mcp.json"]);
}

#[test]
fn a_secret_argument_is_written_as_a_reference_in_args() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    let text = McpMother::secret_argument(&name).replacen("cwd = \".\"\n", "", 1);

    let rendered = render(root.path(), &text);

    assert_eq!(codes(&rendered), [] as [&str; 0]);
    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
  "mcpServers": {{
    "{name}": {{
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@example/server", "--workspace", "${{EXAMPLE_WORKSPACE}}"],
      "env": {{
        "API_KEY": "${{EXAMPLE_API_KEY}}",
        "LOG_LEVEL": "info"
      }},
      "timeout": 30000
    }}
  }}
}}
"#
        )
    );
}

#[test]
fn an_address_is_written_with_its_secret_header_and_its_url_as_references() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::http(&name));

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(codes(&rendered), [] as [&str; 0]);
    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
  "mcpServers": {{
    "{name}": {{
      "type": "http",
      "url": "https://${{EXAMPLE_TENANT}}.example.com/mcp",
      "headers": {{
        "Authorization": "Bearer ${{EXAMPLE_TOKEN}}"
      }}
    }}
  }}
}}
"#
        )
    );
}

#[test]
fn a_secret_header_that_is_its_whole_value_is_a_reference_alone() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    render(root.path(), &McpMother::whole_secret_header(&name));

    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
  "mcpServers": {{
    "{name}": {{
      "type": "http",
      "url": "https://${{EXAMPLE_TENANT}}.example.com/mcp",
      "headers": {{
        "X-Api-Key": "${{EXAMPLE_TOKEN}}"
      }}
    }}
  }}
}}
"#
        )
    );
}

#[test]
fn server_sent_events_are_written_and_said_to_be_deprecated() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::sse(&name));

    assert_eq!(codes(&rendered), ["claude_mcp_sse_deprecated"]);
    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
  "mcpServers": {{
    "{name}": {{
      "type": "sse",
      "url": "https://example.com/sse",
      "headers": {{
        "X-Api-Key": "${{EXAMPLE_KEY}}"
      }}
    }}
  }}
}}
"#
        )
    );
}

#[test]
fn the_team_s_servers_layout_and_keys_come_back_byte_for_byte() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join(FILE), TEAMS).unwrap();
    let name = McpMother::name();

    // An http entry with no `type` is somebody's, which Claude drops
    // alone: no reason to refuse the file.
    let rendered = render(root.path(), &McpMother::http(&name));

    assert_eq!(rendered.written, Written::Changed);
    assert_eq!(
        read(root.path()),
        format!(
            r#"{{
    "mcpServers": {{
        "filesystem": {{
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "."],
            "type": "stdio"
        }},
        "untyped": {{"url": "http://localhost:3000/mcp"}},
        "{name}": {{
            "type": "http",
            "url": "https://${{EXAMPLE_TENANT}}.example.com/mcp",
            "headers": {{
                "Authorization": "Bearer ${{EXAMPLE_TOKEN}}"
            }}
        }}
    }},
    "theirs": true
}}
"#
        )
    );

    assert_eq!(
        claude_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), TEAMS);

    // Nor does a file Mindweaver made go while it holds the team's.
    render(root.path(), &McpMother::http(&name));
    assert_eq!(
        claude_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), TEAMS);
    assert_eq!(names(root.path()), [".mcp.json"]);
}

#[test]
fn a_server_replaced_where_it_stands_leaves_the_rest_as_it_was() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join(FILE), TEAMS).unwrap();
    let name = McpMother::name();
    render(root.path(), &McpMother::http(&name));
    let before = read(root.path());

    let rendered = render(root.path(), &McpMother::sse(&name));

    assert_eq!(rendered.written, Written::Changed);
    let after = read(root.path());
    // Everything before the entry, and from its closing brace on.
    let around = |text: &str| {
        let start = text.find(&format!("\"{name}\": ")).unwrap();
        let end = start + text[start..].find("\n        }").unwrap();
        (text[..start].to_owned(), text[end..].to_owned())
    };
    assert_eq!(around(&after), around(&before));
    assert!(after.contains("\"type\": \"sse\""), "{after}");

    // Again, and nothing is written.
    let rendered = render(root.path(), &McpMother::sse(&name));
    assert_eq!(rendered.written, Written::Unchanged);
    assert_eq!(read(root.path()), after);
}

#[test]
fn the_file_mindweaver_made_goes_with_its_last_server_and_only_then() {
    let root = TempDir::new().unwrap();
    let (first, second) = (McpMother::name(), format!("{}x", McpMother::name()));
    assert_eq!(
        render(root.path(), &McpMother::stdio(&first)).written,
        Written::Created
    );
    render(root.path(), &McpMother::http(&second));

    // Another server is left in it.
    assert_eq!(
        claude_mcp::remove(root.path(), &first, true).unwrap(),
        Written::Changed
    );
    assert!(read(root.path()).contains(&second));

    // The last goes, and the file with it.
    assert_eq!(
        claude_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Removed
    );
    assert!(names(root.path()).is_empty());

    // Nothing there, and nothing is done.
    assert_eq!(
        claude_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Unchanged
    );
}

#[test]
fn a_file_mindweaver_did_not_make_stays_when_its_last_server_goes() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    render(root.path(), &McpMother::http(&name));

    assert_eq!(
        claude_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), "{\n  \"mcpServers\": {}\n}\n");

    // Nor does one Mindweaver made once somebody has written beside it.
    let root = TempDir::new().unwrap();
    render(root.path(), &McpMother::http(&name));
    let theirs = read(root.path()).replacen("{\n", "{\n  \"theirs\": true,\n", 1);
    fs::write(root.path().join(FILE), &theirs).unwrap();
    assert_eq!(
        claude_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(
        read(root.path()),
        "{\n  \"theirs\": true,\n  \"mcpServers\": {}\n}\n"
    );
}

#[test]
fn a_server_not_there_is_taken_out_of_nothing() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join(FILE), TEAMS).unwrap();
    for created in [false, true] {
        assert_eq!(
            claude_mcp::remove(root.path(), &McpMother::name(), created).unwrap(),
            Written::Unchanged
        );
    }
    assert_eq!(read(root.path()), TEAMS);

    let empty = TempDir::new().unwrap();
    assert_eq!(
        claude_mcp::remove(empty.path(), &McpMother::name(), true).unwrap(),
        Written::Unchanged
    );
    assert!(names(empty.path()).is_empty());
}

#[test]
fn a_comment_or_a_trailing_comma_is_refused_saying_claude_loads_none_of_the_file() {
    let name = McpMother::name();
    for (loose, detail) in [
        (
            "{\n  // the team's\n  \"mcpServers\": {}\n}\n",
            "Comments are not allowed",
        ),
        (
            "{\n  \"mcpServers\": {\n    \"a\": {\"command\": \"x\"},\n  }\n}\n",
            "Trailing commas are not allowed",
        ),
    ] {
        let root = TempDir::new().unwrap();
        let path = root.path().join(FILE);
        fs::write(&path, loose).unwrap();

        for result in [
            claude_mcp::render(root.path(), &parsed(&McpMother::http(&name))).map(|_| ()),
            claude_mcp::remove(root.path(), "a", true).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.code(), "mcp_json_not_strict", "{error}");
            let message = error.to_string();
            assert!(
                message.starts_with(&format!("{}: not strict JSON ({detail}", path.display())),
                "{message}"
            );
            assert!(
                message.ends_with(
                    "), which is how Claude Code reads it, loading none of the servers of a file that holds a comment or a trailing comma, so it was left as it is"
                ),
                "{message}"
            );
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), loose);
    }
}

#[test]
fn a_file_no_reader_takes_is_refused_as_any_config_file_is() {
    let root = TempDir::new().unwrap();
    let broken = "{\n  \"mcpServers\": \n";
    fs::write(root.path().join(FILE), broken).unwrap();

    let error =
        claude_mcp::render(root.path(), &parsed(&McpMother::http(&McpMother::name()))).unwrap_err();

    assert_eq!(error.code(), "config_malformed", "{error}");
    assert!(matches!(error, ClaudeMcpError::File(_)), "{error}");
    assert_eq!(read(root.path()), broken);
}

#[test]
fn a_file_past_two_mib_is_refused_unread() {
    let root = TempDir::new().unwrap();
    let path = root.path().join(FILE);
    let padding = " ".repeat(MAX_BYTES as usize);
    let large = format!("{{\"mcpServers\": {{}}{padding}}}\n");
    fs::write(&path, &large).unwrap();

    let error =
        claude_mcp::render(root.path(), &parsed(&McpMother::http(&McpMother::name()))).unwrap_err();

    assert_eq!(MAX_BYTES, 2 * 1024 * 1024);
    assert_eq!(error.code(), "config_too_large", "{error}");
    assert_eq!(fs::read(&path).unwrap(), large.as_bytes());
}

#[test]
fn a_definition_claude_is_refused_is_written_nowhere() {
    let name = McpMother::name();
    let (_, blanked) = random::pick(&McpMother::blanked_variables());
    let (_, token) = McpMother::token();
    for (text, code) in [
        (
            McpMother::header_read_from(&name, &blanked),
            "variable_blanked",
        ),
        (McpMother::unread_input(&name), "claude_mcp_input_unread"),
        (
            McpMother::with_literal(&name, "headers", &token).0,
            "claude_mcp_unfit",
        ),
        (
            McpMother::with_claude_extras(&name, "command = \"other\"\n"),
            "extra_repeated",
        ),
    ]
    .into_iter()
    .chain(McpMother::references().into_iter().flat_map(|(_, text)| {
        ["args", "headers"]
            .map(|place| (McpMother::with_text(&name, place, &text).0, "text_expanded"))
    })) {
        let root = TempDir::new().unwrap();
        let error = claude_mcp::render(root.path(), &parsed(&text)).unwrap_err();
        assert_eq!(error.code(), code, "{error}");
        assert!(names(root.path()).is_empty(), "{code}");

        fs::write(root.path().join(FILE), TEAMS).unwrap();
        claude_mcp::render(root.path(), &parsed(&text)).unwrap_err();
        assert_eq!(read(root.path()), TEAMS, "{code}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_is_refused_and_what_it_points_at_left_alone() {
    let root = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let shared = elsewhere.path().join("mcp.json");
    fs::write(&shared, TEAMS).unwrap();
    std::os::unix::fs::symlink(&shared, root.path().join(FILE)).unwrap();
    let name = McpMother::name();

    let error = claude_mcp::render(root.path(), &parsed(&McpMother::http(&name))).unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    let error = claude_mcp::remove(root.path(), "filesystem", true).unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    assert_eq!(fs::read_to_string(&shared).unwrap(), TEAMS);
}
