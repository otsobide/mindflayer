//! An MCP server rendered into OpenCode's project config, and taken out
//! again: what the file holds afterwards, to the byte, for each transport,
//! beside the settings, servers, comments and trailing commas somebody else
//! keeps there, which of the four files it goes into, and what is refused.
//! What an entry holds is pinned beside `opencode_mcp.rs`; these hold the
//! file around it.

use std::fs;
use std::path::Path;

use mindweaver_core::config_entry::{ConfigFile, Format, Written};
use mindweaver_core::mother::McpMother;
use mindweaver_core::opencode_mcp::{self, OpenCodeMcpError, Shape, FILE, FILES, MAX_BYTES};
use mindweaver_core::{AgentHomes, McpDefinition};
use tempfile::TempDir;

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn render(root: &Path, text: &str) -> opencode_mcp::Rendered {
    opencode_mcp::render(root, &parsed(text), &AgentHomes::default())
        .unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn read(root: &Path, route: &str) -> String {
    fs::read_to_string(root.join(route)).unwrap()
}

fn write(root: &Path, route: &str, text: &str) {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn codes(rendered: &opencode_mcp::Rendered) -> Vec<&'static str> {
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

/// A team's own config, as OpenCode reads it, JSONC: comments above and
/// beside what it holds, trailing commas, a setting beside the servers and
/// a server of theirs in V2's shape.
const TEAMS: &str = r#"// The team's OpenCode settings.
{
  "$schema": "https://opencode.ai/config.json",
  "theme": "dark", // kept
  "mcp": {
    /* servers everyone runs */
    "servers": {
      "docs": {
        "type": "local",
        "command": ["docs-mcp", "--port", "0"],
      },
    },
  },
}
"#;

#[test]
fn a_program_is_written_into_an_opencode_json_made_for_it() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::stdio(&name));

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(rendered.file, root.path().join("opencode.json"));
    assert_eq!(
        read(root.path(), FILE),
        format!(
            r#"{{
  "mcp": {{
    "servers": {{
      "{name}": {{
        "type": "local",
        "command": ["npx", "-y", "@example/server", "--workspace", "{{env:EXAMPLE_WORKSPACE}}"],
        "environment": {{
          "API_KEY": "{{env:EXAMPLE_API_KEY}}",
          "LOG_LEVEL": "info"
        }},
        "cwd": "."
      }}
    }}
  }}
}}
"#
        )
    );
    assert_eq!(codes(&rendered), [] as [&str; 0]);
    assert_eq!(names(root.path()), ["opencode.json"]);
}

#[test]
fn a_secret_argument_is_written_as_a_reference_in_the_command() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    render(root.path(), &McpMother::secret_argument(&name));

    assert_eq!(
        read(root.path(), FILE),
        format!(
            r#"{{
  "mcp": {{
    "servers": {{
      "{name}": {{
        "type": "local",
        "command": ["npx", "-y", "@example/server", "--workspace", "{{env:EXAMPLE_WORKSPACE}}"],
        "environment": {{
          "EXAMPLE_API_KEY": "{{env:EXAMPLE_API_KEY}}",
          "LOG_LEVEL": "info"
        }},
        "cwd": "."
      }}
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
        read(root.path(), FILE),
        format!(
            r#"{{
  "mcp": {{
    "servers": {{
      "{name}": {{
        "type": "remote",
        "url": "https://{{env:EXAMPLE_TENANT}}.example.com/mcp",
        "headers": {{
          "Authorization": "Bearer {{env:EXAMPLE_TOKEN}}"
        }}
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
        read(root.path(), FILE),
        format!(
            r#"{{
  "mcp": {{
    "servers": {{
      "{name}": {{
        "type": "remote",
        "url": "https://{{env:EXAMPLE_TENANT}}.example.com/mcp",
        "headers": {{
          "X-Api-Key": "{{env:EXAMPLE_TOKEN}}"
        }}
      }}
    }}
  }}
}}
"#
        )
    );
}

#[test]
fn the_team_s_comments_trailing_commas_and_servers_come_back_byte_for_byte() {
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS);
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::http(&name));

    assert_eq!(rendered.written, Written::Changed);
    // Its trailing commas, since the file writes them.
    assert_eq!(
        read(root.path(), FILE),
        format!(
            r#"// The team's OpenCode settings.
{{
  "$schema": "https://opencode.ai/config.json",
  "theme": "dark", // kept
  "mcp": {{
    /* servers everyone runs */
    "servers": {{
      "docs": {{
        "type": "local",
        "command": ["docs-mcp", "--port", "0"],
      }},
      "{name}": {{
        "type": "remote",
        "url": "https://{{env:EXAMPLE_TENANT}}.example.com/mcp",
        "headers": {{
          "Authorization": "Bearer {{env:EXAMPLE_TOKEN}}",
        }},
      }},
    }},
  }},
}}
"#
        )
    );

    assert_eq!(
        opencode_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path(), FILE), TEAMS);

    // Nor does a file Mindweaver made go while it holds the team's.
    render(root.path(), &McpMother::http(&name));
    assert_eq!(
        opencode_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path(), FILE), TEAMS);
    assert_eq!(names(root.path()), ["opencode.json"]);
}

#[test]
fn the_first_of_the_four_files_there_is_is_the_one_written_into() {
    let name = McpMother::name();
    for (first, route) in FILES.iter().enumerate() {
        let root = TempDir::new().unwrap();
        for later in &FILES[first..] {
            write(root.path(), later, TEAMS);
        }

        let rendered = render(root.path(), &McpMother::http(&name));

        assert_eq!(rendered.written, Written::Changed, "{route}");
        assert_eq!(rendered.file, root.path().join(route));
        assert!(read(root.path(), route).contains(&name), "{route}");
        for later in &FILES[first + 1..] {
            assert_eq!(read(root.path(), later), TEAMS, "{route} before {later}");
        }
        assert!(!root.path().join(FILE).exists() || first == 0, "{route}");
    }

    // An `.opencode` holding none of them is passed over, and kept as it is.
    let root = TempDir::new().unwrap();
    write(root.path(), ".opencode/skills/README.md", "theirs\n");
    let rendered = render(root.path(), &McpMother::http(&name));
    assert_eq!(rendered.written, Written::Created);
    assert_eq!(rendered.file, root.path().join("opencode.json"));
    assert_eq!(names(&root.path().join(".opencode")), ["skills"]);

    // Nor is one that is no folder.
    let root = TempDir::new().unwrap();
    write(root.path(), ".opencode", "a file\n");
    let rendered = render(root.path(), &McpMother::http(&name));
    assert_eq!(rendered.file, root.path().join("opencode.json"));
    assert_eq!(read(root.path(), ".opencode"), "a file\n");
}

#[test]
fn a_server_replaced_where_it_stands_leaves_the_rest_as_it_was() {
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS);
    let name = McpMother::name();
    render(root.path(), &McpMother::http(&name));
    let before = read(root.path(), FILE);

    let rendered = render(root.path(), &McpMother::whole_secret_header(&name));

    assert_eq!(rendered.written, Written::Changed);
    let after = read(root.path(), FILE);
    // Everything before the entry, and from its closing brace on.
    let around = |text: &str| {
        let start = text.find(&format!("\"{name}\": ")).unwrap();
        let end = start + text[start..].find("\n      },").unwrap();
        (text[..start].to_owned(), text[end..].to_owned())
    };
    assert_eq!(around(&after), around(&before));
    assert!(after.contains("\"X-Api-Key\""), "{after}");

    // Again, and nothing is written.
    let rendered = render(root.path(), &McpMother::whole_secret_header(&name));
    assert_eq!(rendered.written, Written::Unchanged);
    assert_eq!(read(root.path(), FILE), after);
}

#[test]
fn an_entry_laid_out_again_is_still_the_one_mindweaver_wrote() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    let rendered = render(root.path(), &McpMother::stdio(&name));
    let file = ConfigFile::new(root.path(), FILE, Format::Jsonc).unwrap();
    let key = ["mcp", "servers", name.as_str()];
    assert_eq!(
        file.get(&key).unwrap().unwrap().fingerprint(),
        rendered.entry.value.fingerprint()
    );

    // As a person might write it again: on one line, its keys in another
    // order, a comment inside it.
    let again = format!(
        "{{\"mcp\": {{\"servers\": {{\"{name}\": {{\"cwd\": \".\", /* ours */ \"type\": \
         \"local\", \"environment\": {{\"LOG_LEVEL\": \"info\", \"API_KEY\": \
         \"{{env:EXAMPLE_API_KEY}}\"}}, \"command\": [\"npx\", \"-y\", \"@example/server\", \
         \"--workspace\", \"{{env:EXAMPLE_WORKSPACE}}\"]}}}}}}}}\n"
    );
    write(root.path(), FILE, &again);
    let held = file.get(&key).unwrap().unwrap();
    assert_eq!(held.canonical(), rendered.entry.canonical());

    // So rendering it again leaves their layout as it is.
    let rendered = render(root.path(), &McpMother::stdio(&name));
    assert_eq!(rendered.written, Written::Unchanged);
    assert_eq!(read(root.path(), FILE), again);
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
        opencode_mcp::remove(root.path(), &first, true).unwrap(),
        Written::Changed
    );
    assert!(read(root.path(), FILE).contains(&second));

    // The last goes, and the file with it.
    assert_eq!(
        opencode_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Removed
    );
    assert!(names(root.path()).is_empty());

    // Nothing there, and nothing is done.
    assert_eq!(
        opencode_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Unchanged
    );
}

#[test]
fn a_file_mindweaver_did_not_make_stays_when_its_last_server_goes() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    render(root.path(), &McpMother::http(&name));

    assert_eq!(
        opencode_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(
        read(root.path(), FILE),
        "{\n  \"mcp\": {\n    \"servers\": {}\n  }\n}\n"
    );

    // Nor does one Mindweaver made once somebody has written beside it.
    let root = TempDir::new().unwrap();
    render(root.path(), &McpMother::http(&name));
    let theirs = read(root.path(), FILE).replacen("{\n", "{\n  \"theme\": \"dark\",\n", 1);
    write(root.path(), FILE, &theirs);
    assert_eq!(
        opencode_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(
        read(root.path(), FILE),
        "{\n  \"theme\": \"dark\",\n  \"mcp\": {\n    \"servers\": {}\n  }\n}\n"
    );
}

#[test]
fn a_file_mindweaver_made_stays_when_somebody_has_written_a_comment_in_it() {
    let name = McpMother::name();
    for (place, from, to) in [
        ("above", "{\n", "// Our servers: ask #infra first.\n{\n"),
        (
            "below",
            "}\n}\n",
            "}\n}\n// Our servers: ask #infra first.\n",
        ),
        (
            "beside the servers",
            "\"servers\": {\n",
            "// ask #infra first\n    \"servers\": {\n",
        ),
    ] {
        let root = TempDir::new().unwrap();
        assert_eq!(
            render(root.path(), &McpMother::http(&name)).written,
            Written::Created
        );
        write(
            root.path(),
            FILE,
            &read(root.path(), FILE).replacen(from, to, 1),
        );

        assert_eq!(
            opencode_mcp::remove(root.path(), &name, true).unwrap(),
            Written::Changed,
            "{place}"
        );
        assert!(read(root.path(), FILE).contains("#infra"), "{place}");
        assert!(!read(root.path(), FILE).contains(&name), "{place}");
    }

    // One inside the server goes with it, and the file with them.
    let root = TempDir::new().unwrap();
    render(root.path(), &McpMother::http(&name));
    write(
        root.path(),
        FILE,
        &read(root.path(), FILE).replacen(
            "\"type\": \"remote\",",
            "\"type\": \"remote\", // ours",
            1,
        ),
    );
    assert_eq!(
        opencode_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Removed
    );
}

#[test]
fn only_the_opencode_json_mindweaver_makes_is_ever_taken_away() {
    let name = McpMother::name();
    let root = TempDir::new().unwrap();
    render(root.path(), &McpMother::http(&name));
    let ours = read(root.path(), FILE);
    fs::remove_file(root.path().join(FILE)).unwrap();
    // Somebody keeps the same text under another name since.
    write(root.path(), ".opencode/opencode.jsonc", &ours);

    assert_eq!(
        opencode_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(
        read(root.path(), ".opencode/opencode.jsonc"),
        "{\n  \"mcp\": {\n    \"servers\": {}\n  }\n}\n"
    );
}

#[test]
fn a_server_not_there_is_taken_out_of_nothing() {
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS);
    for created in [false, true] {
        assert_eq!(
            opencode_mcp::remove(root.path(), &McpMother::name(), created).unwrap(),
            Written::Unchanged
        );
    }
    assert_eq!(read(root.path(), FILE), TEAMS);

    let empty = TempDir::new().unwrap();
    assert_eq!(
        opencode_mcp::remove(empty.path(), &McpMother::name(), true).unwrap(),
        Written::Unchanged
    );
    assert!(names(empty.path()).is_empty());
}

#[test]
fn a_file_that_is_not_jsonc_is_refused_and_left_as_it_is() {
    let name = McpMother::name();
    for (broken, code) in [
        ("{\n  \"mcp\": \n", "config_malformed"),
        ("{ 'mcp': {} }\n", "config_malformed"),
        ("{ mcp: {} }\n", "config_malformed"),
        ("{\n  \"mcp\": {},\n  \"mcp\": {}\n}\n", "entry_ambiguous"),
        ("{\n  \"mcp\": []\n}\n", "entry_not_an_object"),
    ] {
        let root = TempDir::new().unwrap();
        write(root.path(), FILE, broken);

        for result in [
            opencode_mcp::render(
                root.path(),
                &parsed(&McpMother::http(&name)),
                &AgentHomes::default(),
            )
            .map(|_| ()),
            opencode_mcp::remove(root.path(), "docs", true).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.code(), code, "{broken:?}: {error}");
            assert!(matches!(error, OpenCodeMcpError::File(_)), "{error}");
        }
        assert_eq!(read(root.path(), FILE), broken);
    }
}

#[test]
fn a_file_past_two_mib_is_refused_unread() {
    let root = TempDir::new().unwrap();
    let padding = " ".repeat(MAX_BYTES as usize);
    let large = format!("{{\"mcp\": {{}}{padding}}}\n");
    write(root.path(), FILE, &large);

    let error = opencode_mcp::render(
        root.path(),
        &parsed(&McpMother::http(&McpMother::name())),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert_eq!(MAX_BYTES, 2 * 1024 * 1024);
    assert_eq!(error.code(), "config_too_large", "{error}");
    assert_eq!(read(root.path(), FILE), large);
}

#[test]
fn a_definition_opencode_is_refused_is_written_nowhere() {
    let name = McpMother::name();
    let (_, token) = McpMother::token();
    for (text, code) in [
        (McpMother::sse(&name), "sse_unsupported"),
        (McpMother::unread_input(&name), "opencode_mcp_input_unread"),
        (
            McpMother::with_literal(&name, "headers", &token).0,
            "opencode_mcp_unfit",
        ),
        (
            McpMother::with_opencode_extras(&name, "command = [\"other\"]\n"),
            "extra_repeated",
        ),
    ]
    .into_iter()
    .chain(
        McpMother::substitutions()
            .into_iter()
            .flat_map(|(_, text)| {
                ["args", "headers"].map(|place| {
                    (
                        McpMother::with_text(&name, place, &text).0,
                        "text_substituted",
                    )
                })
            }),
    ) {
        let root = TempDir::new().unwrap();
        let error =
            opencode_mcp::render(root.path(), &parsed(&text), &AgentHomes::default()).unwrap_err();
        assert_eq!(error.code(), code, "{error}");
        assert!(names(root.path()).is_empty(), "{code}");

        write(root.path(), FILE, TEAMS);
        opencode_mcp::render(root.path(), &parsed(&text), &AgentHomes::default()).unwrap_err();
        assert_eq!(read(root.path(), FILE), TEAMS, "{code}");
    }
}

#[test]
fn a_project_whose_config_is_inside_an_agent_s_own_folder_is_refused() {
    // A project at OpenCode's own folder, whose `opencode.json` is the one
    // OpenCode reads for every project: made there, or one already there.
    let home = TempDir::new().unwrap();
    let root = home.path().join(".config").join("opencode");
    fs::create_dir_all(&root).unwrap();
    let homes = AgentHomes {
        opencode_config_dir: Some(root.clone()),
        ..AgentHomes::default()
    };
    let definition = parsed(&McpMother::http(&McpMother::name()));

    for held in [None, Some(TEAMS)] {
        if let Some(text) = held {
            write(&root, FILE, text);
        }
        let error = opencode_mcp::render(&root, &definition, &homes).unwrap_err();

        assert_eq!(error.code(), "opencode_mcp_user_scope");
        assert_eq!(
            error.to_string(),
            format!(
                "{} is inside {}, which OpenCode reads for every project, so no server is \
                 written into it: Mindweaver gives a server to one project at a time",
                root.join(FILE).display(),
                root.display()
            )
        );
        match held {
            None => assert!(names(&root).is_empty()),
            Some(text) => assert_eq!(read(&root, FILE), text),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_on_the_way_is_refused_and_what_it_points_at_left_alone() {
    let name = McpMother::name();
    let elsewhere = TempDir::new().unwrap();
    fs::write(elsewhere.path().join("opencode.json"), TEAMS).unwrap();

    // The file itself a link, to somebody's own.
    let root = TempDir::new().unwrap();
    std::os::unix::fs::symlink(
        elsewhere.path().join("opencode.json"),
        root.path().join(FILE),
    )
    .unwrap();
    let error = opencode_mcp::render(
        root.path(),
        &parsed(&McpMother::http(&name)),
        &AgentHomes::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    let error = opencode_mcp::remove(root.path(), "docs", true).unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");

    // `.opencode` a link to a folder holding one: refused, never passed over
    // for a file of the project's own beside it.
    let root = TempDir::new().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join(".opencode")).unwrap();
    let error = opencode_mcp::render(
        root.path(),
        &parsed(&McpMother::http(&name)),
        &AgentHomes::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    assert!(!root.path().join(FILE).exists());

    assert_eq!(
        fs::read_to_string(elsewhere.path().join("opencode.json")).unwrap(),
        TEAMS
    );
}

/// A team's own config keeping its servers in V1's shape, as OpenCode
/// wrote them before V2: straight under `mcp`, each `enabled`, with a
/// comment above and trailing commas.
const TEAMS_V1: &str = r#"{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    // their own, from before V2
    "docs": {
      "type": "local",
      "command": ["docs-mcp"],
      "enabled": true,
    },
  },
}
"#;

#[test]
fn a_file_keeping_its_servers_in_v1_s_shape_is_given_a_program_in_it_without_its_folder() {
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS_V1);
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::stdio(&name));

    assert_eq!(rendered.written, Written::Changed);
    assert_eq!(rendered.entry.shape, Shape::V1);
    assert_eq!(codes(&rendered), ["opencode_mcp_v1_cwd"]);
    assert_eq!(
        read(root.path(), FILE),
        format!(
            r#"{{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {{
    // their own, from before V2
    "docs": {{
      "type": "local",
      "command": ["docs-mcp"],
      "enabled": true,
    }},
    "{name}": {{
      "type": "local",
      "command": ["npx", "-y", "@example/server", "--workspace", "{{env:EXAMPLE_WORKSPACE}}"],
      "enabled": true,
      "environment": {{
        "API_KEY": "{{env:EXAMPLE_API_KEY}}",
        "LOG_LEVEL": "info",
      }},
    }},
  }},
}}
"#
        )
    );

    assert_eq!(
        opencode_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path(), FILE), TEAMS_V1);
}

#[test]
fn a_file_keeping_its_servers_in_v1_s_shape_is_given_an_address_in_it() {
    let root = TempDir::new().unwrap();
    write(root.path(), FILES[1], TEAMS_V1);
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::http(&name));

    assert_eq!(rendered.entry.shape, Shape::V1);
    assert_eq!(codes(&rendered), [] as [&str; 0]);
    let written = read(root.path(), FILES[1]);
    assert!(
        written.contains(&format!(
            r#"    "{name}": {{
      "type": "remote",
      "url": "https://{{env:EXAMPLE_TENANT}}.example.com/mcp",
      "enabled": true,
      "headers": {{
        "Authorization": "Bearer {{env:EXAMPLE_TOKEN}}",
      }},
    }},
"#
        )),
        "{written}"
    );
    assert!(!written.contains("\"servers\""), "{written}");
}

#[test]
fn a_server_is_replaced_in_the_shape_it_stands_in() {
    let name = McpMother::name();
    let key =
        |shape: Shape| -> Vec<String> { shape.key(&name).into_iter().map(str::to_owned).collect() };
    let holds = |root: &Path, shape: Shape| {
        let key = key(shape);
        let key: Vec<&str> = key.iter().map(String::as_str).collect();
        ConfigFile::new(root, FILE, Format::Jsonc)
            .unwrap()
            .get(&key)
            .unwrap()
    };

    // Ours in V2's shape, in a file somebody has given a server of V1's
    // since, stays V2's.
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS);
    render(root.path(), &McpMother::http(&name));
    let mixed = read(root.path(), FILE).replacen(
        "\"mcp\": {\n",
        "\"mcp\": {\n    \"theirs\": { \"type\": \"remote\", \"url\": \"https://example.com/mcp\" },\n",
        1,
    );
    write(root.path(), FILE, &mixed);
    let rendered = render(root.path(), &McpMother::whole_secret_header(&name));
    assert_eq!(rendered.entry.shape, Shape::V2);
    assert_eq!(rendered.written, Written::Changed);
    assert!(holds(root.path(), Shape::V2).is_some());
    assert!(holds(root.path(), Shape::V1).is_none());

    // And one of the name in V1's shape, in a file of V2's, stays V1's.
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        FILE,
        &TEAMS.replacen(
            "\"mcp\": {\n",
            &format!(
                "\"mcp\": {{\n    \"{name}\": {{ \"type\": \"remote\", \"url\": \"https://example.com/old\", \"enabled\": true }},\n"
            ),
            1,
        ),
    );
    let rendered = render(root.path(), &McpMother::http(&name));
    assert_eq!(rendered.entry.shape, Shape::V1);
    assert_eq!(
        holds(root.path(), Shape::V1).unwrap().canonical(),
        rendered.entry.canonical()
    );
    assert!(holds(root.path(), Shape::V2).is_none());
    assert_eq!(
        opencode_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert!(holds(root.path(), Shape::V1).is_none());
}

#[test]
fn a_server_in_both_shapes_is_refused_and_the_file_left_as_it_is() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    let both = format!(
        "{{\n  \"mcp\": {{\n    \"servers\": {{ \"{name}\": {{ \"type\": \"remote\", \"url\": \"https://a.example\" }} }},\n    \"{name}\": {{ \"type\": \"remote\", \"url\": \"https://b.example\" }}\n  }}\n}}\n"
    );
    write(root.path(), FILE, &both);

    for result in [
        opencode_mcp::render(
            root.path(),
            &parsed(&McpMother::http(&name)),
            &AgentHomes::default(),
        )
        .map(|_| ()),
        opencode_mcp::remove(root.path(), &name, false).map(|_| ()),
        opencode_mcp::remove(root.path(), &name, true).map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.code(), "opencode_mcp_twice");
        assert_eq!(
            error.to_string(),
            format!(
                "{}: `{name}` is a server both under `mcp.servers` and under `mcp`, OpenCode's \
                 V2 and V1 shapes of one, so which of the two is meant cannot be told, and the \
                 file was left as it is",
                root.path().join(FILE).display()
            )
        );
    }
    assert_eq!(read(root.path(), FILE), both);
}

#[test]
fn the_keys_opencode_s_mcp_holds_itself_are_no_servers_of_v1_s() {
    // A timeout beside the servers keeps a new one in V2's shape.
    let root = TempDir::new().unwrap();
    let held = "{\n  \"mcp\": {\n    \"timeout\": 5000\n  }\n}\n";
    write(root.path(), FILE, held);
    let name = McpMother::name();
    let rendered = render(root.path(), &McpMother::http(&name));
    assert_eq!(rendered.entry.shape, Shape::V2);
    assert!(
        read(root.path(), FILE)
            .starts_with("{\n  \"mcp\": {\n    \"timeout\": 5000,\n    \"servers\": {\n"),
        "{}",
        read(root.path(), FILE)
    );

    // Nor is `servers` ever taken out as a server, which would take every
    // one of V2's with it.
    let root = TempDir::new().unwrap();
    write(root.path(), FILE, TEAMS);
    for name in ["servers", "timeout"] {
        assert_eq!(
            opencode_mcp::remove(root.path(), name, false).unwrap(),
            Written::Unchanged,
            "{name}"
        );
    }
    assert_eq!(read(root.path(), FILE), TEAMS);
}
