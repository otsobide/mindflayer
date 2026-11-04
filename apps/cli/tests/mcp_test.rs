//! MCP servers at the command line: listed beside skills and rules, shown,
//! validated, renamed and removed as every kind is, and what no release
//! does with one yet refused, saying what to do instead.

mod common;

use common::*;
use mindweaver_core::mother::McpMother;

const GITHUB: &str = "GitHub's issues, pull requests and code";

/// A workspace whose library holds a skill, a rule and an MCP server: the
/// first two added with `add`, the server's definition written into the
/// library, as `add` writes none yet.
fn library() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    weaver(dir.path(), &["add", "skill", "deploy", "Ship the service"]).unwrap();
    weaver(
        dir.path(),
        &["add", "rule", "git/commits", "Commit one step at a time"],
    )
    .unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("github.toml"),
        McpMother::create("github", GITHUB, "stdio"),
    )
    .unwrap();
    dir
}

/// What a repository implementing a server keeps at `mcp/`: its code, and
/// a definition it ships for its users, none of it the project's own.
fn unrelated_mcp_folder(root: &Path) {
    let folder = root.join("mcp");
    fs::create_dir_all(folder.join("src")).unwrap();
    fs::write(folder.join("Cargo.toml"), "[package]\nname = \"server\"\n").unwrap();
    fs::write(folder.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(folder.join("github.toml"), McpMother::stdio("github")).unwrap();
}

#[test]
fn mindweaver_list_shows_mcp_servers_beside_skills_and_rules() {
    let dir = library();

    let listed = weaver(dir.path(), &["list"]).unwrap();
    let narrowed = weaver(dir.path(), &["list", "mcp"]).unwrap();

    assert_eq!(
        listed.stdout,
        format!(
            "skill  deploy       Ship the service\n\
             rule   git/commits  Commit one step at a time\n\
             mcp    github       {GITHUB}\n"
        )
    );
    assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
    assert_eq!(narrowed.stdout, format!("github  {GITHUB}\n"));
}

#[test]
fn list_json_carries_each_server_as_it_carries_every_kind() {
    let dir = library();

    let outcome = weaver(dir.path(), &["list", "--json"]).unwrap();

    let (data, warnings): (serde_json::Value, _) = enveloped(&outcome);
    assert!(warnings.is_empty(), "{warnings:?}");
    let github = data["library"]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["kind"] == "mcp")
        .unwrap_or_else(|| panic!("{data}"));
    assert_eq!(github["name"], "github");
    assert_eq!(github["summary"], GITHUB);
    assert_eq!(
        github["targets"],
        serde_json::json!([".mindweaver/library/mcp/github.toml"])
    );
}

#[test]
fn show_says_what_a_server_is_for_and_prints_its_definition() {
    let dir = library();
    let file = own_directory_for(dir.path(), Kind::Mcp).join("github.toml");

    let shown = weaver(dir.path(), &["show", "mcp:github"]).unwrap();

    assert_eq!(
        shown.stdout,
        format!(
            "github\n{}\n\n{GITHUB}\n\n{}",
            file.display(),
            fs::read_to_string(&file).unwrap()
        )
    );
}

#[test]
fn validate_counts_a_server_in_words_and_says_what_is_wrong_with_one() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(
        folder.join("bare.toml"),
        McpMother::incomplete("bare", "http"),
    )
    .unwrap();

    let failure = weaver(dir.path(), &["validate"]).unwrap();

    assert!(!failure.ok);
    assert_eq!(
        failure.stdout,
        "skill:deploy: ok\n\
         rule:git/commits: ok\n\
         mcp:bare: 1 problem\n  \
         - the http transport has no `url`, which no agent can start or reach the server without\n\
         mcp:github: ok\n\
         \n\
         1 skill, 1 rule and 2 MCP servers checked, 1 invalid\n"
    );
}

#[test]
fn a_server_is_renamed_and_removed_as_every_kind_is() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);

    weaver(dir.path(), &["rename", "mcp:github", "octocat"]).unwrap();

    assert!(!folder.join("github.toml").exists());
    let renamed = fs::read_to_string(folder.join("octocat.toml")).unwrap();
    assert!(renamed.contains("name = \"octocat\"\n"), "{renamed}");
    let listed = weaver(dir.path(), &["list", "mcp"]).unwrap();
    assert_eq!(listed.stdout, format!("octocat  {GITHUB}\n"));

    let dry = weaver(dir.path(), &["remove", "mcp:octocat"]).unwrap_err();
    assert!(folder.join("octocat.toml").is_file(), "{}", dry.error);
    weaver(dir.path(), &["remove", "mcp:octocat", "--yes"]).unwrap();
    assert!(!folder.join("octocat.toml").exists());
}

#[test]
fn contents_given_whole_that_name_a_server_otherwise_are_refused() {
    let dir = library();
    fs::write(dir.path().join("draft.toml"), McpMother::sse("octocat")).unwrap();

    let failure = weaver(
        dir.path(),
        &["edit", "mcp:github", "--contents", "draft.toml"],
    )
    .unwrap_err();

    assert_eq!(failure.error.code(), "edit_mcp_name_changed");
}

#[test]
fn adding_installing_and_gathering_a_server_are_refused_with_what_to_do() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    new_project(&dir.path().join("app"));
    weaver(dir.path(), &["link", "app"]).unwrap();

    let added = weaver(dir.path(), &["add", "mcp", "linear", "Linear's issues"]).unwrap_err();
    let installed = weaver(dir.path(), &["install", "-p", "app", "mcp:github"]).unwrap_err();

    assert_eq!(added.error.code(), "add_mcp_unscaffolded");
    assert_eq!(
        added.error.to_string(),
        format!(
            "an MCP server cannot be added yet, since `add` takes no command or address to \
             start or reach it by; write its definition as <name>.toml in {}",
            folder.display()
        )
    );
    assert_eq!(installed.error.code(), "not_installable");
    assert_eq!(
        installed.error.to_string(),
        "MCP servers cannot be installed into a project yet"
    );
    // Typed bare, the name is asked of the kinds an install writes alone.
    let bare = weaver(dir.path(), &["install", "-p", "app", "github"]).unwrap_err();
    assert_eq!(bare.error.code(), "not_offered");
}

#[test]
fn a_project_an_older_mind_init_made_keeps_no_servers_and_says_how_to_keep_some() {
    let dir = TempDir::new().unwrap();
    let older = dir.path().join("older");
    let json = dir.path().join("json");
    fs::create_dir_all(&older).unwrap();
    fs::create_dir_all(&json).unwrap();
    older_project(&older);
    json_project(&json);

    for (root, marker) in [(&older, "mind.toml"), (&json, "settings.json")] {
        unrelated_mcp_folder(root);

        let listed = mind(root, &["list", "mcp"]).unwrap();
        let added = mind(root, &["add", "mcp", "github", "GitHub"]).unwrap_err();

        assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
        assert!(!listed.stdout.contains("github"), "{}", listed.stdout);
        assert_eq!(added.error.code(), "no_folder");
        assert_eq!(
            added.error.to_string(),
            format!(
                "{}: keeps no MCP servers of its own, since its {marker} names no folder for \
                 them; `mind migrate` moves it to a settings.toml, whose `[directories]` can \
                 name one",
                root.display()
            )
        );
    }
}

#[test]
fn a_project_mind_init_makes_lists_nothing_of_an_unrelated_mcp_folder() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    unrelated_mcp_folder(dir.path());

    let listed = mind(dir.path(), &["list"]).unwrap();
    let added = mind(dir.path(), &["add", "mcp", "github", "GitHub"]).unwrap_err();

    assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
    assert!(!listed.stdout.contains("github"), "{}", listed.stdout);
    assert_eq!(
        added.error.to_string(),
        format!(
            "{}: keeps no MCP servers of its own, since its settings.toml names no folder for \
             them; its `[directories]` can name one",
            dir.path().display()
        )
    );
}
