//! One entry of a config file on disk: what `ConfigFile` reads, writes,
//! makes and refuses. What an edit keeps of the text is held to the byte by
//! the unit tests beside `config_entry.rs`; these hold the file around it.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use mindflayer_core::config_entry::{
    ConfigFile, Edit, EntryError, FileError, Format, Value, Written,
};
use tempfile::TempDir;

fn github() -> Value {
    Value::object([
        ("command", Value::from("npx")),
        (
            "args",
            Value::array(["-y", "@modelcontextprotocol/server-github"]),
        ),
    ])
}

const MCP: &str =
    "{\n  \"mcpServers\": {\n    \"filesystem\": {\n      \"command\": \"npx\"\n    }\n  }\n}\n";

/// The names in `dir`, sorted: what a write leaves beside the file.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn an_insert_into_a_file_not_there_yet_makes_it_and_the_folders_on_its_route() {
    let root = TempDir::new().unwrap();
    let file = ConfigFile::new(root.path(), ".codex/config.toml", Format::Toml).unwrap();
    assert_eq!(file.get(&["mcp_servers", "github"]).unwrap(), None);

    let written = file
        .edit(&["mcp_servers", "github"], &Edit::Insert(github()))
        .unwrap();
    assert_eq!(written, Written::Created);
    assert_eq!(
        fs::read_to_string(root.path().join(".codex/config.toml")).unwrap(),
        "[mcp_servers.github]\ncommand = \"npx\"\nargs = [\"-y\", \"@modelcontextprotocol/server-github\"]\n"
    );
    assert_eq!(names(&root.path().join(".codex")), ["config.toml"]);
    assert_eq!(
        file.get(&["mcp_servers", "github"]).unwrap(),
        Some(github())
    );
}

#[test]
fn an_edit_replaces_the_file_whole_and_leaves_nothing_beside_it() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join(".mcp.json"), MCP).unwrap();
    let file = ConfigFile::new(root.path(), ".mcp.json", Format::Json).unwrap();
    let key = ["mcpServers", "github"];

    assert_eq!(
        file.edit(&key, &Edit::Insert(github())).unwrap(),
        Written::Changed
    );
    assert_eq!(
        fs::read_to_string(file.path()).unwrap(),
        "{\n  \"mcpServers\": {\n    \"filesystem\": {\n      \"command\": \"npx\"\n    },\n    \"github\": {\n      \"command\": \"npx\",\n      \"args\": [\"-y\", \"@modelcontextprotocol/server-github\"]\n    }\n  }\n}\n"
    );
    assert_eq!(names(root.path()), [".mcp.json"]);

    assert_eq!(file.edit(&key, &Edit::Remove).unwrap(), Written::Changed);
    assert_eq!(fs::read_to_string(file.path()).unwrap(), MCP);
    assert_eq!(names(root.path()), [".mcp.json"]);
}

#[test]
fn a_replace_with_what_the_entry_already_holds_leaves_the_file_alone() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("opencode.json");
    fs::write(
        &path,
        "{\n  // ours\n  \"mcp\": {\"a\": {\"enabled\": true}}\n}\n",
    )
    .unwrap();
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    let file = ConfigFile::new(root.path(), "opencode.json", Format::Jsonc).unwrap();
    let written = file
        .edit(
            &["mcp", "a"],
            &Edit::Replace(Value::object([("enabled", Value::from(true))])),
        )
        .unwrap();
    assert_eq!(written, Written::Unchanged);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), long_ago);
}

#[test]
fn a_file_that_does_not_parse_is_refused_and_left_as_it_is() {
    let root = TempDir::new().unwrap();
    let path = root.path().join(".mcp.json");
    // A comment, which `.mcp.json`'s own reader refuses.
    let text = "{\n  // the team's\n  \"mcpServers\": {}\n}\n";
    fs::write(&path, text).unwrap();

    let file = ConfigFile::new(root.path(), ".mcp.json", Format::Json).unwrap();
    for result in [
        file.get(&["mcpServers", "github"]).map(|_| ()),
        file.edit(&["mcpServers", "github"], &Edit::Insert(github()))
            .map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert!(
            matches!(
                &error,
                FileError::Entry {
                    source: EntryError::Malformed {
                        format: Format::Json,
                        ..
                    },
                    ..
                }
            ),
            "{error}"
        );
        let message = error.to_string();
        assert!(
            message.starts_with(&format!("{}: not valid JSON: ", path.display())),
            "{message}"
        );
        assert!(message.ends_with(", so it was not changed"), "{message}");
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert_eq!(names(root.path()), [".mcp.json"]);
}

#[test]
fn a_replace_or_remove_in_a_file_not_there_makes_nothing() {
    let root = TempDir::new().unwrap();
    let file = ConfigFile::new(root.path(), ".claude/settings.json", Format::Json).unwrap();
    for edit in [Edit::Replace(github()), Edit::Remove] {
        let error = file.edit(&["mcpServers", "github"], &edit).unwrap_err();
        assert!(
            matches!(
                error,
                FileError::Entry {
                    source: EntryError::Missing { .. },
                    ..
                }
            ),
            "{error}"
        );
    }
    assert!(names(root.path()).is_empty());
}

#[test]
fn a_route_that_could_lead_out_of_its_folder_is_refused() {
    let root = TempDir::new().unwrap();
    for route in [
        "",
        "../.mcp.json",
        "a/../../b.json",
        "./.mcp.json",
        "/etc/config.toml",
    ] {
        let error = ConfigFile::new(root.path(), route, Format::Json).unwrap_err();
        assert!(matches!(error, FileError::Route { .. }), "{route}: {error}");
    }
}

#[test]
fn something_other_than_a_file_or_a_folder_on_the_route_is_refused() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join(".mcp.json")).unwrap();
    fs::write(root.path().join(".codex"), "").unwrap();

    let error = ConfigFile::new(root.path(), ".mcp.json", Format::Json)
        .unwrap()
        .get(&["mcpServers"])
        .unwrap_err();
    assert!(matches!(error, FileError::NotAFile { .. }), "{error}");

    let error = ConfigFile::new(root.path(), ".codex/config.toml", Format::Toml)
        .unwrap()
        .edit(&["mcp_servers", "a"], &Edit::Insert(github()))
        .unwrap_err();
    assert!(matches!(error, FileError::NotAFolder { .. }), "{error}");
    assert_eq!(
        error.to_string(),
        format!("{}: not a folder", root.path().join(".codex").display())
    );
}

#[test]
fn a_file_that_is_not_utf8_is_refused() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("config.toml"), b"a = \"\xff\"\n").unwrap();
    let error = ConfigFile::new(root.path(), "config.toml", Format::Toml)
        .unwrap()
        .get(&["a"])
        .unwrap_err();
    assert!(matches!(error, FileError::NotText { .. }), "{error}");
}

#[cfg(unix)]
#[test]
fn the_file_s_permissions_are_kept() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new().unwrap();
    let path = root.path().join(".mcp.json");
    fs::write(&path, MCP).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    ConfigFile::new(root.path(), ".mcp.json", Format::Json)
        .unwrap()
        .edit(&["mcpServers", "github"], &Edit::Insert(github()))
        .unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_is_refused_and_what_it_points_at_left_alone() {
    let root = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let shared = elsewhere.path().join("mcp.json");
    fs::write(&shared, MCP).unwrap();
    std::os::unix::fs::symlink(&shared, root.path().join(".mcp.json")).unwrap();

    let file = ConfigFile::new(root.path(), ".mcp.json", Format::Json).unwrap();
    let error = file.get(&["mcpServers", "filesystem"]).unwrap_err();
    assert!(matches!(error, FileError::Symlink { .. }), "{error}");
    let error = file
        .edit(&["mcpServers", "github"], &Edit::Insert(github()))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "{}: a symbolic link, which is not followed: what it points at is not this folder's to change",
            root.path().join(".mcp.json").display()
        )
    );
    assert_eq!(fs::read_to_string(&shared).unwrap(), MCP);
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_on_the_route_is_refused_even_for_a_file_not_there() {
    // `.claude` pointing at a folder of the user's own, as a gathered
    // repository could have it point at `~/.claude`.
    let root = TempDir::new().unwrap();
    let users = TempDir::new().unwrap();
    std::os::unix::fs::symlink(users.path(), root.path().join(".claude")).unwrap();

    let file = ConfigFile::new(root.path(), ".claude/settings.json", Format::Json).unwrap();
    for result in [
        file.get(&["mcpServers"]).map(|_| ()),
        file.edit(&["mcpServers", "github"], &Edit::Insert(github()))
            .map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert!(
            matches!(&error, FileError::Symlink { path } if path == &root.path().join(".claude")),
            "{error}"
        );
    }
    assert!(names(users.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn a_root_reached_through_a_link_is_the_caller_s_and_is_followed() {
    let real = TempDir::new().unwrap();
    let links = TempDir::new().unwrap();
    let root = links.path().join("project");
    std::os::unix::fs::symlink(real.path(), &root).unwrap();

    let written = ConfigFile::new(&root, ".mcp.json", Format::Json)
        .unwrap()
        .edit(&["mcpServers", "github"], &Edit::Insert(github()))
        .unwrap();
    assert_eq!(written, Written::Created);
    assert!(real.path().join(".mcp.json").is_file());
}
