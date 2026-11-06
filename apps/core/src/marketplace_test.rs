//! Unit tests for `marketplace.rs`, kept beside it rather than inside it.

use std::fs;

use tempfile::TempDir;

use super::*;

/// A catalog as anthropics/skills writes one: two plugins in the
/// marketplace's own folder, each naming the skills it is made of.
const ANTHROPIC: &str = r#"{
  "name": "anthropic-agent-skills",
  "owner": {"name": "Keith Lazuka", "email": "klazuka@anthropic.com"},
  "metadata": {"description": "Anthropic example skills", "version": "1.0.0"},
  "plugins": [
    {
      "name": "document-skills",
      "description": "Collection of document processing suite",
      "source": "./",
      "strict": false,
      "skills": ["./skills/xlsx", "./skills/docx"]
    },
    {
      "name": "example-skills",
      "description": "Collection of example skills",
      "source": "./",
      "strict": false,
      "skills": ["./skills/algorithmic-art"]
    }
  ]
}"#;

#[test]
fn a_catalog_like_anthropics_lists_each_plugin_in_its_own_folder_with_the_skills_it_names() {
    let catalog = Catalog::parse(ANTHROPIC, Dialect::Claude).unwrap();

    assert_eq!(catalog.name.as_deref(), Some("anthropic-agent-skills"));
    assert_eq!(catalog.listed, 2);
    assert!(catalog.failures.is_empty(), "{:?}", catalog.failures);
    assert_eq!(
        catalog.entries,
        [
            Entry {
                name: String::from("document-skills"),
                description: Some(String::from("Collection of document processing suite")),
                plugin: Plugin::Relative {
                    route: String::new()
                },
                skills: Some(vec![
                    String::from("./skills/xlsx"),
                    String::from("./skills/docx")
                ]),
                license: None,
                declares: Vec::new(),
            },
            Entry {
                name: String::from("example-skills"),
                description: Some(String::from("Collection of example skills")),
                plugin: Plugin::Relative {
                    route: String::new()
                },
                skills: Some(vec![String::from("./skills/algorithmic-art")]),
                license: None,
                declares: Vec::new(),
            },
        ]
    );
}

/// A catalog as openai/plugins writes one (#104): each plugin a folder of
/// the marketplace's own repository, named by a `local` source, beside the
/// `policy` and the `category` Codex requires.
const OPENAI: &str = r#"{
  "name": "openai-curated",
  "interface": {"displayName": "OpenAI Curated"},
  "plugins": [
    {
      "name": "linear",
      "source": {"source": "local", "path": "./plugins/linear"},
      "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
      "category": "Productivity"
    },
    {
      "name": "figma",
      "source": {"source": "local", "path": "./plugins/figma/"},
      "policy": {"installation": "AVAILABLE", "authentication": "ON_USE"},
      "category": "Design"
    }
  ]
}"#;

/// What a Codex catalog's entry in `route` lists, called `name`.
fn local(name: &str, route: &str) -> Entry {
    Entry {
        name: name.to_owned(),
        description: None,
        plugin: Plugin::Local {
            route: route.to_owned(),
        },
        skills: None,
        license: None,
        declares: Vec::new(),
    }
}

#[test]
fn a_catalog_like_openais_lists_each_plugin_in_a_folder_of_its_repository_reading_past_its_policy()
{
    let catalog = Catalog::parse(OPENAI, Dialect::Codex).unwrap();

    assert_eq!(catalog.name.as_deref(), Some("openai-curated"));
    assert_eq!(catalog.dialect, Dialect::Codex);
    assert_eq!(catalog.listed, 2);
    assert!(catalog.failures.is_empty(), "{:?}", catalog.failures);
    assert_eq!(
        catalog.entries,
        [
            local("linear", "plugins/linear"),
            local("figma", "plugins/figma")
        ]
    );
}

#[test]
fn an_entry_codex_would_not_resolve_is_skipped_with_why_and_the_rest_listed() {
    let catalog = Catalog::parse(
        r#"{"name": "c", "plugins": [
            {"name": "fine", "source": {"source": "local", "path": "./plugins/fine"}, "policy": {}, "category": "x"},
            {"name": "no-dot", "source": {"source": "local", "path": "plugins/no-dot"}},
            {"name": "root", "source": {"source": "local", "path": "./"}},
            {"name": "climbs", "source": {"source": "local", "path": "./../elsewhere"}},
            {"name": "absolute", "source": {"source": "local", "path": ".//etc"}},
            {"name": "remote", "source": {"source": "git", "url": "https://example.com/x.git"}},
            {"name": "no-path", "source": {"source": "local"}},
            {"name": "no-kind", "source": {"path": "./x"}},
            {"name": "as-claude", "source": "./plugins/fine"},
            {"name": "no-source"},
            {"name": "keys", "source": {"source": "local", "path": "./plugins/keys"}, "skills": ["./x"], "license": "MIT", "hooks": {}},
            {"description": "no name"}
        ]}"#,
        Dialect::Codex,
    )
    .unwrap();

    // What only Claude Code's format gives an entry is read past.
    assert_eq!(
        catalog.entries,
        [local("fine", "plugins/fine"), local("keys", "plugins/keys")]
    );
    let failed: Vec<(Option<&str>, &str)> = catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(
        failed,
        [
            (Some("no-dot"), "marketplace_entry_unresolved"),
            (Some("root"), "marketplace_entry_unresolved"),
            (Some("climbs"), "marketplace_entry_outside"),
            (Some("absolute"), "marketplace_entry_outside"),
            (Some("remote"), "marketplace_entry_unresolved"),
            (Some("no-path"), "marketplace_entry_unresolved"),
            (Some("no-kind"), "marketplace_entry_unresolved"),
            (Some("as-claude"), "marketplace_entry_unresolved"),
            (Some("no-source"), "marketplace_entry_unresolved"),
            (None, "marketplace_entry_unreadable"),
        ]
    );
    let said: Vec<String> = catalog.failures[..2]
        .iter()
        .chain(&catalog.failures[4..5])
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        said,
        [
            "`no-dot`: its path `plugins/no-dot` does not start with `./`, so Codex does not \
             resolve it, and it is skipped",
            "`root`: its path `./` names the marketplace's own folder, and no folder below it, so \
             Codex does not resolve it, and it is skipped",
            "`remote`: its `source` is a `git` one, not a `local` one, so Codex does not resolve \
             it, and it is skipped",
        ]
    );
}

#[test]
fn a_catalog_written_wholly_in_the_other_dialects_shape_is_read_as_neither() {
    // Codex's shape at Claude Code's place, and Claude Code's at Codex's.
    assert_eq!(
        Catalog::parse(OPENAI, Dialect::Claude),
        Err(Unparsed::Shaped(Dialect::Codex))
    );
    assert_eq!(
        Catalog::parse(ANTHROPIC, Dialect::Codex),
        Err(Unparsed::Shaped(Dialect::Claude))
    );
    assert_eq!(
        Catalog::parse(&mixed(), Dialect::Codex),
        Err(Unparsed::Shaped(Dialect::Claude))
    );
    // Neither its place alone nor its shape alone: each is read where its
    // own dialect keeps it, and one whose entries say no shape, or both,
    // is read as its place says.
    assert!(Catalog::parse(OPENAI, Dialect::Codex).is_ok());
    assert!(Catalog::parse(ANTHROPIC, Dialect::Claude).is_ok());
    for text in [
        r#"{"plugins": []}"#,
        r#"{"plugins": [{"name": "x"}, "y", {"name": "z", "source": {"source": "pip"}}]}"#,
        r#"{"plugins": [{"name": "a", "source": "./a"}, {"name": "b", "source": {"source": "local", "path": "./b"}}]}"#,
    ] {
        for dialect in Dialect::ALL {
            let catalog = Catalog::parse(text, dialect).unwrap();
            assert_eq!(catalog.dialect, dialect, "{text}");
        }
    }
}

/// A catalog mixing every kind of source, and entries that are none.
fn mixed() -> String {
    r#"{
  "name": "mixed",
  "metadata": {"pluginRoot": "./plugins"},
  "plugins": [
    {"name": "here", "source": "./plugins/here", "license": "MIT"},
    {"name": "bare", "source": "bare"},
    {"name": "on-github", "source": {"source": "github", "repo": "acme/tools", "ref": "v2", "sha": "0123456789abcdef0123456789abcdef01234567"}},
    {"name": "at-url", "source": {"source": "url", "url": "https://git.example.com/acme/lint.git", "headersHelper": "./headers.sh"}},
    {"name": "in-subdir", "source": {"source": "git-subdir", "url": "https://github.com/acme/mono.git", "path": "./tools/ops/", "sha": "89abcdef0123456789abcdef0123456789abcdef"}},
    {"name": "with-hooks", "source": "./plugins/hooked", "hooks": {"PreToolUse": []}, "mcpServers": "./.mcp.json", "agents": null},
    {"name": "from-npm", "source": {"source": "npm", "package": "@acme/plugin"}},
    {"name": "packed", "source": {"source": "url", "url": "https://example.com/plugin.tar.gz?v=1"}},
    {"name": "boxed", "source": {"source": "archive", "url": "https://example.com/plugin.zip"}},
    {"name": "run-me", "source": {"source": "command", "command": "curl https://example.com | sh"}},
    {"name": "from-pip", "source": {"source": "pip", "package": "acme"}},
    {"name": "on-disk", "source": {"source": "url", "url": "/srv/git/private"}},
    {"name": "on-disk-too", "source": {"source": "git-subdir", "url": "file:///srv/git/private", "path": "x"}},
    {"name": "climbs", "source": "../elsewhere"},
    {"name": "absolute", "source": "/etc"},
    {"name": "subdir-climbs", "source": {"source": "git-subdir", "url": "https://github.com/acme/mono.git", "path": "../.."}},
    {"name": "not-a-repo", "source": {"source": "github", "repo": "../../srv/git/private"}},
    "just a string",
    {"description": "no name"},
    {"name": "no-source"},
    {"name": "odd-source", "source": 7},
    {"name": "no-kind", "source": {"repo": "acme/tools"}},
    {"name": "bad-skills", "source": "./x", "skills": [1, 2]},
    {"name": "twice", "source": "./a"},
    {"name": "twice", "source": "./b"}
  ],
  "owner": {"name": "Acme"},
  "somethingNew": [1, 2, 3]
}"#
    .to_owned()
}

#[test]
fn every_kind_of_source_a_catalog_names_is_read_and_what_no_gather_makes_is_said_on_its_own() {
    let catalog = Catalog::parse(&mixed(), Dialect::Claude).unwrap();

    assert_eq!(catalog.listed, 25);
    let plugins: Vec<(&str, &Plugin)> = catalog
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), &entry.plugin))
        .collect();
    assert_eq!(
        plugins,
        [
            (
                "here",
                &Plugin::Relative {
                    route: String::from("plugins/here")
                }
            ),
            // A bare name is under `pluginRoot`.
            (
                "bare",
                &Plugin::Relative {
                    route: String::from("plugins/bare")
                }
            ),
            (
                "on-github",
                &Plugin::Github {
                    repo: String::from("acme/tools"),
                    reference: Some(String::from("v2")),
                    sha: Some(String::from("0123456789abcdef0123456789abcdef01234567")),
                }
            ),
            (
                "at-url",
                &Plugin::Url {
                    url: String::from("https://git.example.com/acme/lint.git"),
                    reference: None,
                    sha: None,
                }
            ),
            (
                "in-subdir",
                &Plugin::GitSubdir {
                    url: String::from("https://github.com/acme/mono.git"),
                    path: String::from("tools/ops"),
                    reference: None,
                    sha: Some(String::from("89abcdef0123456789abcdef0123456789abcdef")),
                }
            ),
            (
                "with-hooks",
                &Plugin::Relative {
                    route: String::from("plugins/hooked")
                }
            ),
        ]
    );
    assert_eq!(catalog.entries[0].license.as_deref(), Some("MIT"));
    // Declared in the catalog itself, and `null` declares nothing.
    assert_eq!(
        catalog.entries[5].declares,
        [Holding::Hooks, Holding::McpServers]
    );
    let failed: Vec<(Option<&str>, &str)> = catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(
        failed,
        [
            (Some("from-npm"), "marketplace_npm_not_gathered"),
            (Some("packed"), "marketplace_archive_not_gathered"),
            (Some("boxed"), "marketplace_archive_not_gathered"),
            (Some("run-me"), "marketplace_command_refused"),
            (Some("from-pip"), "marketplace_source_unknown"),
            (Some("on-disk"), "marketplace_entry_local"),
            (Some("on-disk-too"), "marketplace_entry_local"),
            (Some("climbs"), "marketplace_entry_outside"),
            (Some("absolute"), "marketplace_entry_outside"),
            (Some("subdir-climbs"), "marketplace_entry_outside"),
            (Some("not-a-repo"), "marketplace_entry_unreadable"),
            (None, "marketplace_entry_unreadable"),
            (None, "marketplace_entry_unreadable"),
            (Some("no-source"), "marketplace_entry_unreadable"),
            (Some("odd-source"), "marketplace_entry_unreadable"),
            (Some("no-kind"), "marketplace_entry_unreadable"),
            (Some("bad-skills"), "marketplace_entry_unreadable"),
            (Some("twice"), "marketplace_entry_duplicate"),
            (Some("twice"), "marketplace_entry_duplicate"),
        ]
    );
}

#[test]
fn a_command_entry_is_refused_with_a_message_saying_it_is_never_run() {
    let catalog = Catalog::parse(&mixed(), Dialect::Claude).unwrap();

    let refused = catalog
        .failures
        .iter()
        .find(|failure| failure.name() == Some("run-me"))
        .unwrap();

    assert_eq!(refused.code(), "marketplace_command_refused");
    assert_eq!(
        refused.to_string(),
        "`run-me`: its plugin is made by running a command, which is never run, so it is refused"
    );
}

#[test]
fn an_entry_that_cannot_be_read_is_said_by_where_it_is_in_the_list() {
    let catalog = Catalog::parse(&mixed(), Dialect::Claude).unwrap();
    let said: Vec<String> = catalog
        .failures
        .iter()
        .filter(|failure| failure.code() == "marketplace_entry_unreadable")
        .map(ToString::to_string)
        .collect();

    assert_eq!(
        said,
        [
            "plugins[16] (`not-a-repo`): its `repo` is not `owner/repo`, so it is not listed",
            "plugins[17]: it is not a JSON object, so it is not listed",
            "plugins[18]: it has no name, so it is not listed",
            "plugins[19] (`no-source`): it has no `source`, so it is not listed",
            "plugins[20] (`odd-source`): its `source` is neither a path nor an object, so it is not listed",
            "plugins[21] (`no-kind`): its `source` names no kind, so it is not listed",
            "plugins[22] (`bad-skills`): its `skills` are not all paths, so it is not listed",
        ]
    );
}

#[test]
fn an_entry_whose_name_no_list_shows_as_it_is_is_unreadable_and_said_by_its_name() {
    let catalog = Catalog::parse(
        r#"{"plugins": [
            {"name": " pdf", "source": "./a"},
            {"name": "pdf\t", "source": "./b"},
            {"name": "a\u001b[2Jb", "source": "./c"},
            {"name": "two words", "source": "./d"}
        ]}"#,
        Dialect::Claude,
    )
    .unwrap();

    let listed: Vec<&str> = catalog
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(listed, ["two words"]);
    let failed: Vec<(Option<&str>, &str)> = catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(
        failed,
        [
            (Some(" pdf"), "marketplace_entry_unreadable"),
            (Some("pdf\t"), "marketplace_entry_unreadable"),
            (Some("a\u{1b}[2Jb"), "marketplace_entry_unreadable"),
        ]
    );
    assert_eq!(
        catalog.failures[0].to_string(),
        "plugins[0] (` pdf`): its name has a control character or a space at either end, which \
         no list shows as it is, so it is not listed"
    );
}

#[test]
fn a_url_a_failure_names_is_said_without_the_credential_it_carries() {
    let catalog = Catalog::parse(
        r#"{"plugins": [
            {"name": "packed", "source": {"source": "url", "url": "https://me:s3cret@example.com/p.zip"}},
            {"name": "local", "source": {"source": "url", "url": "file://me:s3cret@localhost/srv/x"}}
        ]}"#,
        Dialect::Claude,
    )
    .unwrap();

    for failure in &catalog.failures {
        assert!(!failure.to_string().contains("s3cret"), "{failure}");
    }
}

#[test]
fn what_is_no_catalog_is_said_as_such() {
    for (text, reason) in [
        ("not json", "it is not JSON"),
        ("[]", "it is not a JSON object"),
        ("{}", "it has no `plugins` list"),
        (r#"{"plugins": {}}"#, "it has no `plugins` list"),
        // JSON as Claude Code reads it, and nothing JSON5 adds.
        ("{\"plugins\": [] // a comment\n}", "it is not JSON"),
        (r#"{"plugins": [],}"#, "it is not JSON"),
    ] {
        let Err(Unparsed::Unreadable(said)) = Catalog::parse(text, Dialect::Claude) else {
            panic!("{text}: read");
        };
        assert!(said.starts_with(reason), "{text}: {said}");
    }
}

#[test]
fn a_catalog_nested_deeper_than_the_parser_goes_is_no_catalog_rather_than_an_overflow() {
    let deep = format!(
        r#"{{"plugins": [], "x": {}{}}}"#,
        "[".repeat(100_000),
        "]".repeat(100_000)
    );

    assert!(matches!(
        Catalog::parse(&deep, Dialect::Claude),
        Err(Unparsed::Unreadable(said)) if said.starts_with("it is not JSON")
    ));
}

#[test]
fn a_catalog_without_a_name_is_read_with_none() {
    let catalog = Catalog::parse(r#"{"name": 7, "plugins": []}"#, Dialect::Claude).unwrap();

    assert_eq!(catalog.name, None);
    assert_eq!(catalog.listed, 0);
}

#[test]
fn an_entry_and_its_plugin_serialize_each_field_by_its_name_and_a_url_without_its_credential() {
    let entry = Entry {
        name: String::from("lint"),
        description: Some(String::from("Lint it")),
        plugin: Plugin::GitSubdir {
            url: String::from("https://me:s3cret@github.com/acme/mono.git"),
            path: String::from("tools/lint"),
            reference: Some(String::from("v2")),
            sha: None,
        },
        skills: Some(vec![String::from("./skills/lint")]),
        license: Some(String::from("MIT")),
        declares: vec![Holding::Hooks, Holding::Agents, Holding::McpServers],
    };

    assert_eq!(
        serde_json::to_string(&entry).unwrap(),
        concat!(
            r#"{"name":"lint","description":"Lint it","#,
            r#""plugin":{"kind":"git-subdir","url":"https://github.com/acme/mono.git","#,
            r#""path":"tools/lint","reference":"v2","sha":null},"#,
            r#""skills":["./skills/lint"],"license":"MIT","#,
            r#""declares":["hooks","agents","mcpServers"]}"#
        )
    );
    for (plugin, json) in [
        (
            Plugin::Relative {
                route: String::from("plugins/a"),
            },
            r#"{"kind":"relative","url":null,"path":"plugins/a","reference":null,"sha":null}"#,
        ),
        (
            Plugin::Local {
                route: String::from("plugins/linear"),
            },
            r#"{"kind":"local","url":null,"path":"plugins/linear","reference":null,"sha":null}"#,
        ),
        (
            Plugin::Github {
                repo: String::from("acme/tools"),
                reference: None,
                sha: Some(String::from("0123456789abcdef0123456789abcdef01234567")),
            },
            concat!(
                r#"{"kind":"github","url":"https://github.com/acme/tools","path":"","#,
                r#""reference":null,"sha":"0123456789abcdef0123456789abcdef01234567"}"#
            ),
        ),
        (
            Plugin::Url {
                url: String::from("git@host:acme/lint"),
                reference: Some(String::from("main")),
                sha: None,
            },
            r#"{"kind":"url","url":"git@host:acme/lint","path":"","reference":"main","sha":null}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&plugin).unwrap(), json);
    }
}

/// A clone holding `files`, each written with its parents.
fn clone(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_catalog_is_read_from_the_folder_the_marketplace_is_in() {
    let at = clone(&[
        (
            "market/.claude-plugin/marketplace.json",
            r#"{"name": "m", "plugins": [{"name": "a", "source": "./a"}]}"#,
        ),
        ("market/a/skills/x/SKILL.md", "---\nname: x\n---\n"),
    ]);

    let catalog = Catalog::read(at.path(), "market").unwrap();

    assert_eq!(catalog.entries.len(), 1, "{:?}", catalog.failures);
    assert!(Catalog::read(at.path(), "").is_err());
}

#[test]
fn a_codex_catalog_is_read_from_its_own_place_its_folders_held_to_the_repository() {
    let at = clone(&[
        (
            CODEX_CATALOG,
            r#"{"name": "c", "plugins": [
                {"name": "linear", "source": {"source": "local", "path": "./plugins/linear"}},
                {"name": "gone", "source": {"source": "local", "path": "./plugins/gone"}}
            ]}"#,
        ),
        ("plugins/linear/skills/x/SKILL.md", "---\nname: x\n---\n"),
    ]);

    let catalog = Catalog::read(at.path(), "").unwrap();

    assert_eq!(catalog.dialect, Dialect::Codex);
    assert_eq!(catalog.entries, [local("linear", "plugins/linear")]);
    assert_eq!(
        catalog.failures,
        [EntryFailure::Missing {
            name: String::from("gone"),
            path: String::from("plugins/gone"),
        }]
    );
}

#[test]
fn a_folder_holding_both_catalogs_is_read_as_claude_code_reads_it() {
    let at = clone(&[(CATALOG, ANTHROPIC), (CODEX_CATALOG, OPENAI)]);

    let catalog = Catalog::read(at.path(), "").unwrap();

    assert_eq!(catalog.dialect, Dialect::Claude);
    assert_eq!(catalog.name.as_deref(), Some("anthropic-agent-skills"));
}

#[test]
fn a_catalog_at_one_dialects_place_in_the_others_shape_is_refused() {
    let codex_at_claudes = clone(&[(CATALOG, OPENAI), ("plugins/linear/SKILL.md", "")]);
    let claude_at_codexs = clone(&[(CODEX_CATALOG, ANTHROPIC)]);

    let refused = Catalog::read(codex_at_claudes.path(), "").unwrap_err();
    let reverse = Catalog::read(claude_at_codexs.path(), "").unwrap_err();

    assert_eq!(refused.code(), "marketplace_catalog_other_dialect");
    assert_eq!(
        refused.to_string(),
        ".claude-plugin/marketplace.json: lists its plugins as a Codex catalog does, not as a \
         Claude Code one, so it is not read: a Codex catalog is .agents/plugins/marketplace.json"
    );
    assert_eq!(reverse.code(), "marketplace_catalog_other_dialect");
    assert_eq!(
        reverse.to_string(),
        ".agents/plugins/marketplace.json: lists its plugins as a Claude Code catalog does, not \
         as a Codex one, so it is not read: a Claude Code catalog is \
         .claude-plugin/marketplace.json"
    );
}

#[test]
fn a_repository_with_no_catalog_is_no_marketplace() {
    let at = clone(&[("README.md", "hello")]);

    let error = Catalog::read(at.path(), "").unwrap_err();

    assert_eq!(error.code(), "marketplace_catalog_missing");
    assert_eq!(
        error.to_string(),
        ".claude-plugin/marketplace.json: no such file, nor .agents/plugins/marketplace.json, so \
         this is no Claude Code or Codex marketplace"
    );
}

#[test]
fn a_relative_entry_whose_folder_is_not_there_is_said_and_the_rest_listed() {
    let at = clone(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{"plugins": [{"name": "a", "source": "./a"}, {"name": "gone", "source": "./gone"}]}"#,
        ),
        ("a/SKILL.md", "---\nname: a\n---\n"),
    ]);

    let catalog = Catalog::read(at.path(), "").unwrap();

    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(
        catalog.failures,
        [EntryFailure::Missing {
            name: String::from("gone"),
            path: String::from("gone"),
        }]
    );
}

#[cfg(unix)]
#[test]
fn a_relative_entry_leading_out_of_the_repository_through_a_link_is_refused() {
    let outside = clone(&[("skills/secret/SKILL.md", "---\nname: secret\n---\n")]);
    let at = clone(&[(
        ".claude-plugin/marketplace.json",
        r#"{"plugins": [{"name": "out", "source": "./out"}]}"#,
    )]);
    std::os::unix::fs::symlink(outside.path(), at.path().join("out")).unwrap();

    let catalog = Catalog::read(at.path(), "").unwrap();

    assert!(catalog.entries.is_empty());
    assert_eq!(catalog.failures[0].code(), "marketplace_entry_outside");
}

#[cfg(unix)]
#[test]
fn a_catalog_that_is_a_link_out_of_the_repository_is_not_read() {
    let outside = clone(&[("elsewhere.json", r#"{"plugins": []}"#)]);
    let at = clone(&[]);
    fs::create_dir_all(at.path().join(".claude-plugin")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("elsewhere.json"),
        at.path().join(CATALOG),
    )
    .unwrap();

    let error = Catalog::read(at.path(), "").unwrap_err();

    assert_eq!(error.code(), "marketplace_catalog_outside");
}

#[test]
fn a_catalog_larger_than_the_cap_is_not_read() {
    let at = clone(&[]);
    fs::create_dir_all(at.path().join(".claude-plugin")).unwrap();
    let file = fs::File::create(at.path().join(CATALOG)).unwrap();
    file.set_len(CATALOG_CAP + 1).unwrap();

    let error = Catalog::read(at.path(), "").unwrap_err();

    assert_eq!(error.code(), "marketplace_catalog_too_large");
}

#[test]
fn what_a_plugin_holds_beside_its_skills_is_found_by_where_a_plugin_keeps_it() {
    let at = clone(&[
        ("hooks/hooks.json", "{}"),
        (".mcp.json", "{}"),
        ("skills/x/SKILL.md", ""),
    ]);

    assert_eq!(
        Holding::held_at(at.path()),
        [Holding::Hooks, Holding::McpServers]
    );
}

#[test]
fn a_plugin_keeping_its_mcp_servers_in_either_file_holds_them_and_says_which_file() {
    let dotted = clone(&[(".mcp.json", "{}")]);
    let plain = clone(&[("mcp.json", "{}"), ("skills/x/SKILL.md", "")]);
    let both = clone(&[("mcp.json", "{}"), (".mcp.json", "{}")]);
    let neither = clone(&[("skills/x/SKILL.md", "")]);

    for at in [&dotted, &plain, &both] {
        assert_eq!(Holding::held_at(at.path()), [Holding::McpServers]);
    }
    assert!(Holding::held_at(neither.path()).is_empty());
    assert_eq!(Holding::mcp_files_at(dotted.path()), [".mcp.json"]);
    assert_eq!(Holding::mcp_files_at(plain.path()), ["mcp.json"]);
    assert_eq!(
        Holding::mcp_files_at(both.path()),
        [".mcp.json", "mcp.json"]
    );
    assert!(Holding::mcp_files_at(neither.path()).is_empty());
    assert!(Holding::kept_in().any(|kept| kept == "mcp.json"));
}

#[test]
fn a_listed_catalog_serializes_each_field_by_its_name_and_leaves_its_failures_to_the_warnings() {
    let listed = Listed {
        name: SourceName::parse("anthropic-agent-skills").unwrap(),
        url: Redacted::new("https://me:s3cret@github.com/anthropics/skills"),
        reference: Some(String::from("main")),
        revision: Some(Revision::from("4f9c2a7")),
        listed: 2,
        entries: vec![Entry {
            name: String::from("pdf"),
            description: None,
            plugin: Plugin::Relative {
                route: String::new(),
            },
            skills: None,
            license: None,
            declares: Vec::new(),
        }],
        failures: vec![EntryFailure::Command {
            name: String::from("run-me"),
        }],
        dialect: Dialect::Claude,
    };

    assert_eq!(
        serde_json::to_string(&listed).unwrap(),
        concat!(
            r#"{"name":"anthropic-agent-skills","url":"https://github.com/anthropics/skills","#,
            r#""reference":"main","revision":"4f9c2a7","listed":2,"#,
            r#""entries":[{"name":"pdf","description":null,"#,
            r#""plugin":{"kind":"relative","url":null,"path":"","reference":null,"sha":null},"#,
            r#""skills":null,"license":null,"declares":[]}],"dialect":"claude"}"#
        )
    );
}

#[test]
fn gathering_every_plugin_waits_for_a_yes_only_beyond_twenty_five() {
    let listed = |count: usize| Listed {
        name: SourceName::parse("m").unwrap(),
        url: Redacted::new("https://github.com/acme/m"),
        reference: None,
        revision: None,
        listed: count,
        entries: Vec::new(),
        failures: Vec::new(),
        dialect: Dialect::Codex,
    };

    assert_eq!(UNCONFIRMED, 25);
    assert!(!listed(25).needs_confirming());
    assert!(listed(26).needs_confirming());
}

#[test]
fn a_dialect_serializes_as_its_word() {
    assert_eq!(
        serde_json::to_string(&Dialect::ALL).unwrap(),
        r#"["claude","codex"]"#
    );
}
