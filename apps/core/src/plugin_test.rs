//! Unit tests for `plugin.rs`, kept beside it rather than inside it.

use std::fs;

use tempfile::TempDir;

use super::*;

/// An Agent Plugins `$schema`, on a host of no format's: what the format's
/// own names is matched within the URL ([`AGENT_PLUGINS_SCHEMA`]).
const SCHEMA: &str = "https://example.org/agent-plugins/plugin.schema.json";

/// A folder holding `files`, each written with its parents.
fn folder(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

/// The manifest of the plugin at `dir`'s root, looked for as a catalog in
/// `dialect` has it looked for.
fn read_in(dir: &TempDir, dialect: Dialect) -> Result<Option<Manifest>, Unreadable> {
    let bound = fs::canonicalize(dir.path()).unwrap();
    Manifest::read(dir.path(), &bound, &Format::read_for(dialect))
}

#[test]
fn a_codex_manifest_says_its_skills_folder_its_licence_and_its_servers_and_its_apps_are_passed_over(
) {
    let dir = folder(&[
        (
            ".codex-plugin/plugin.json",
            r#"{"name": "linear", "version": "1.0.0", "license": "MIT", "skills": "./skills/",
            "mcpServers": "./.mcp.json", "apps": "./.app.json", "interface": {"displayName": "Linear"}}"#,
        ),
        (".mcp.json", "{}"),
    ]);

    let manifest = read_in(&dir, Dialect::Codex).unwrap().unwrap();

    assert_eq!(
        manifest,
        Manifest {
            format: Format::Codex,
            skills: vec![String::from("./skills/")],
            license: Some(String::from("MIT")),
            declares: vec![Holding::McpServers],
        }
    );
}

#[test]
fn a_claude_code_manifest_may_name_several_folders_of_skills() {
    let dir = folder(&[
        (
            ".claude-plugin/plugin.json",
            r#"{"name": "ops", "skills": ["./more", "./extra/"], "hooks": "./hooks/hooks.json", "agents": null}"#,
        ),
        ("hooks/hooks.json", "{}"),
    ]);

    let manifest = read_in(&dir, Dialect::Claude).unwrap().unwrap();

    assert_eq!(manifest.format, Format::Claude);
    assert_eq!(manifest.skills, ["./more", "./extra/"]);
    assert_eq!(manifest.license, None);
    assert_eq!(manifest.declares, [Holding::Hooks]);
}

#[test]
fn a_manifest_declares_what_it_names_by_a_path_only_while_something_is_there() {
    let dir = folder(&[(
        ".codex-plugin/plugin.json",
        r#"{"hooks": "./hooks.json", "agents": "..", "mcpServers": {"x": {"command": "x"}}}"#,
    )]);

    let manifest = read_in(&dir, Dialect::Codex).unwrap().unwrap();

    // Its servers inline are declared; a hooks file that is not there, and
    // a path out of the plugin's repository, which is there, declare
    // nothing.
    assert_eq!(manifest.declares, [Holding::McpServers]);
}

#[test]
fn a_manifest_declares_what_it_names_by_a_list_of_paths_only_while_one_of_them_is_there() {
    let dir = folder(&[
        (
            ".claude-plugin/plugin.json",
            r#"{"agents": ["./agents/missing.md", ".."], "hooks": ["./hooks/missing.json", "./hooks/hooks.json"], "mcpServers": []}"#,
        ),
        ("hooks/hooks.json", "{}"),
    ]);

    let manifest = read_in(&dir, Dialect::Claude).unwrap().unwrap();

    // The hooks one of its list keeps; no agent file it lists is there, nor
    // inside the plugin, and an empty list names nothing.
    assert_eq!(manifest.declares, [Holding::Hooks]);
}

#[test]
fn a_plugin_is_read_by_the_manifest_of_the_agent_its_catalog_is_for_first() {
    let dir = folder(&[
        (".codex-plugin/plugin.json", r#"{"license": "MIT"}"#),
        (".claude-plugin/plugin.json", r#"{"license": "Apache-2.0"}"#),
        (
            "plugin.json",
            &format!(r#"{{"$schema": "{SCHEMA}", "license": "BSD-3-Clause"}}"#),
        ),
    ]);
    let format = |dialect| read_in(&dir, dialect).unwrap().unwrap().format;

    assert_eq!(format(Dialect::Codex), Format::Codex);
    assert_eq!(format(Dialect::Claude), Format::Claude);
    assert_eq!(
        Format::read_for(Dialect::Codex),
        [Format::Codex, Format::AgentPlugins, Format::Claude]
    );
    assert_eq!(
        Format::read_for(Dialect::Claude),
        [Format::Claude, Format::AgentPlugins, Format::Codex]
    );
}

#[test]
fn a_root_plugin_json_is_an_agent_plugins_manifest_only_when_its_schema_says_so() {
    for schema in [
        SCHEMA,
        "https://example.org/AgentPlugins/v1.json",
        "https://example.org/schemas/agent_plugin.json",
    ] {
        let dir = folder(&[(
            "plugin.json",
            &format!(
                r#"{{"$schema": "{schema}", "name": "kit", "skills": ["./deep"], "license": "MIT"}}"#
            ),
        )]);

        let manifest = read_in(&dir, Dialect::Codex).unwrap().unwrap();

        assert_eq!(manifest.format, Format::AgentPlugins, "{schema}");
        // Its skills are its `skills/<child>` alone, whatever it says.
        assert!(manifest.skills.is_empty());
        assert_eq!(manifest.license.as_deref(), Some("MIT"));
        assert!(is_package(
            dir.path(),
            &fs::canonicalize(dir.path()).unwrap()
        ));
    }
    // Anybody's `plugin.json`, read or not, is passed over, and so is one
    // that cannot be read: the name is not the format's alone.
    for text in [
        r#"{"name": "kit"}"#,
        r#"{"$schema": "https://json.schemastore.org/package.json"}"#,
        r#"{"$schema": 7}"#,
        "not json",
        "[]",
    ] {
        let dir = folder(&[("plugin.json", text)]);

        assert_eq!(read_in(&dir, Dialect::Codex), Ok(None), "{text}");
        assert!(!is_package(
            dir.path(),
            &fs::canonicalize(dir.path()).unwrap()
        ));
    }
}

#[test]
fn a_plugin_with_no_manifest_has_none() {
    let dir = folder(&[("skills/x/SKILL.md", "")]);

    assert_eq!(read_in(&dir, Dialect::Claude), Ok(None));
    assert_eq!(read_in(&dir, Dialect::Codex), Ok(None));
}

#[test]
fn an_agents_manifest_that_cannot_be_read_is_said_by_where_it_is_and_why() {
    for (text, reason) in [
        ("not json", "it is not JSON"),
        ("[]", "it is not a JSON object"),
        (r#"{"skills": 7}"#, "its `skills` are not paths"),
        (
            r#"{"skills": ["./a", 7]}"#,
            "its `skills` are not all paths",
        ),
        ("{\"name\": \"x\" // a comment\n}", "it is not JSON"),
    ] {
        let dir = folder(&[(".codex-plugin/plugin.json", text)]);

        let unreadable = read_in(&dir, Dialect::Codex).unwrap_err();

        assert_eq!(
            unreadable.path,
            dir.path().join(".codex-plugin/plugin.json")
        );
        assert!(
            unreadable.reason.starts_with(reason),
            "{text}: {}",
            unreadable.reason
        );
    }
}

#[test]
fn a_manifest_larger_than_the_cap_is_not_read() {
    let dir = folder(&[]);
    fs::create_dir_all(dir.path().join(".claude-plugin")).unwrap();
    let file = fs::File::create(dir.path().join(".claude-plugin/plugin.json")).unwrap();
    file.set_len(MANIFEST_CAP + 1).unwrap();

    let unreadable = read_in(&dir, Dialect::Claude).unwrap_err();

    assert_eq!(unreadable.reason, "it is larger than 1024 KiB");
}

#[cfg(unix)]
#[test]
fn a_manifest_that_links_out_of_the_repository_is_never_read() {
    let outside = folder(&[(
        "elsewhere.json",
        &format!(r#"{{"$schema": "{SCHEMA}", "license": "MIT"}}"#),
    )]);
    let dir = folder(&[]);
    fs::create_dir_all(dir.path().join(".codex-plugin")).unwrap();
    for at in ["plugin.json", ".codex-plugin/plugin.json"] {
        std::os::unix::fs::symlink(outside.path().join("elsewhere.json"), dir.path().join(at))
            .unwrap();
    }

    let unreadable = read_in(&dir, Dialect::Codex).unwrap_err();

    assert!(
        unreadable.reason.starts_with("it links to "),
        "{}",
        unreadable.reason
    );
    // A root `plugin.json` that leads out is no package's manifest.
    fs::remove_file(dir.path().join(".codex-plugin/plugin.json")).unwrap();
    assert_eq!(read_in(&dir, Dialect::Codex), Ok(None));
}
