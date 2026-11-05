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
fn show_says_what_a_server_is_for_how_it_is_started_and_what_is_supplied_for_it() {
    let dir = library();
    let file = own_directory_for(dir.path(), Kind::Mcp).join("github.toml");

    let shown = weaver(dir.path(), &["show", "mcp:github"]).unwrap();

    assert_eq!(
        shown.stdout,
        format!(
            "github\n{}\n\n{GITHUB}\n\n\
             transport: stdio\n  \
             command: npx\n  \
             args:\n    \
             - -y\n    \
             - @example/server\n    \
             - --workspace\n    \
             - {{workspace}}\n  \
             env:\n    \
             API_KEY: {{api_key}}\n    \
             LOG_LEVEL: info\n  \
             cwd: .\n\
             \n\
             inputs:\n  \
             api_key: env, secret, required, read from $EXAMPLE_API_KEY\n    \
             The key the server signs in with\n  \
             workspace: arg, read from $EXAMPLE_WORKSPACE\n\
             \n\
             agents:\n  \
             claude: given it without what it has no place for:\n    \
             - the transport's `cwd` is left out, since a program is started there in no folder \
             it is told\n  \
             codex: given it whole\n  \
             opencode: given it whole\n",
            file.display(),
        )
    );
    assert!(shown.ok && shown.stderr.is_empty(), "{:?}", shown.stderr);
}

#[test]
fn show_lays_an_address_out_by_its_url_and_headers_and_never_its_credential() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    let text = McpMother::sse("events").replacen(
        "https://example.com/sse",
        "https://me:hunter2@example.com/sse",
        1,
    );
    fs::write(folder.join("events.toml"), text).unwrap();

    let shown = weaver(dir.path(), &["show", "mcp:events"]).unwrap();
    let json = weaver(dir.path(), &["show", "mcp:events", "--json"]).unwrap();

    assert!(
        shown.stdout.contains(
            "transport: sse\n  \
             url: https://example.com/sse\n  \
             headers:\n    \
             X-Api-Key: {key}\n\
             \n\
             inputs:\n  \
             key: header, secret, read from $EXAMPLE_KEY\n\
             \n\
             agents:\n"
        ),
        "{}",
        shown.stdout
    );
    let (data, warnings): (serde_json::Value, _) = enveloped(&json);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(data[0]["transport"]["url"], "https://example.com/sse");
    // Kept out of the text and the transport alike; the body is the file,
    // which holds what it was written with.
    assert!(!shown.stdout.contains("hunter2"), "{}", shown.stdout);
    assert!(
        !data[0]["transport"].to_string().contains("hunter2"),
        "{data}"
    );
}

#[test]
fn show_says_which_agent_is_given_none_of_a_server_and_why() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(folder.join("events.toml"), McpMother::sse("events")).unwrap();
    fs::write(
        folder.join("workspace-server.toml"),
        McpMother::secret_argument("workspace-server"),
    )
    .unwrap();

    let events = weaver(dir.path(), &["show", "mcp:events"]).unwrap();
    let argued = weaver(dir.path(), &["show", "mcp:workspace-server"]).unwrap();

    let sse = "    - the sse transport, server-sent events, is not one taken there\n";
    assert!(
        events.stdout.ends_with(&format!(
            "agents:\n  claude: given it whole\n  codex: given none of it:\n{sse}  \
             opencode: given none of it:\n{sse}"
        )),
        "{}",
        events.stdout
    );
    assert!(
        argued.stdout.contains(
            "  codex: given none of it:\n    \
             - secret input `workspace` is written in the transport's `args`, which hold no \
             reference to an environment variable there\n  \
             opencode: given it whole\n"
        ),
        "{}",
        argued.stdout
    );
    // Shown, never refused: what an agent cannot be given is not wrong.
    assert!(events.ok && argued.ok);
    assert!(events.stderr.is_empty() && argued.stderr.is_empty());
}

#[test]
fn show_json_says_what_each_agent_cannot_be_given_by_its_code() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(
        folder.join("linear.toml"),
        McpMother::composite_secret_header("linear"),
    )
    .unwrap();

    let shown = weaver(dir.path(), &["show", "mcp:linear", "--json"]).unwrap();

    let (data, warnings): (serde_json::Value, _) = enveloped(&shown);
    assert!(warnings.is_empty(), "{warnings:?}");
    let report = &data[0]["expressibility"];
    assert_eq!(
        report,
        &serde_json::json!([
            {"agent": "claude", "refused": [], "dropped": []},
            {
                "agent": "codex",
                "refused": [{
                    "code": "secret_header_composite",
                    "message": "secret input `token` is written in header `Authorization` beside \
                                other text, and a header is read from an environment variable \
                                there only as its whole value, or as `Authorization: Bearer` and \
                                the token",
                }],
                "dropped": [],
            },
            {"agent": "opencode", "refused": [], "dropped": []},
        ])
    );
    let skill = weaver(dir.path(), &["show", "skill:deploy", "--json"]).unwrap();
    let (data, _): (serde_json::Value, _) = enveloped(&skill);
    assert_eq!(data[0]["expressibility"], serde_json::Value::Null);
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
fn validate_reports_a_reserved_name_and_a_literal_token_and_never_the_token() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(folder.join("Workspace.toml"), McpMother::stdio("Workspace")).unwrap();
    let (prefix, token) = McpMother::token();
    let (leaky, at) = McpMother::with_literal("leaky", "env", &token);
    fs::write(folder.join("leaky.toml"), leaky).unwrap();
    fs::write(
        folder.join("anthropic-skills.toml"),
        McpMother::sse("anthropic-skills"),
    )
    .unwrap();

    let failure = weaver(dir.path(), &["validate", "mcp"]).unwrap();

    assert!(!failure.ok);
    assert_eq!(
        failure.stdout,
        format!(
            "Workspace: 1 problem\n  \
             - `Workspace` is a name Claude Code keeps for a server of its own (`workspace`), \
             so no server of yours can be keyed by it\n\
             anthropic-skills: ok, 1 warning\n  \
             - warning: `anthropic-skills` is the name Claude Code gives the skills Anthropic \
             provides (`anthropic-skills`), so a server of yours by it may be taken for theirs; \
             it is written all the same\n\
             github: ok\n\
             leaky: 1 problem\n  \
             - `{at}` holds what looks like a token, starting `{prefix}`, and a definition holds \
             no secret: declare a secret input read from an environment variable, and write it \
             there between braces\n\
             \n\
             4 MCP servers checked, 2 invalid\n"
        )
    );
    // Said where it is and what it starts with, never what follows.
    let rest = token[prefix.len()..].trim_start();
    let json = weaver(dir.path(), &["validate", "mcp", "--json"]).unwrap();
    assert!(!failure.stdout.contains(rest) && !json.stdout.contains(rest));
    let (data, _): (serde_json::Value, _) = enveloped(&json);
    let codes: Vec<&str> = data["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|checked| checked["issues"].as_array().unwrap())
        .map(|issue| issue["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["mcp_name_reserved", "mcp_name_claimed", "secret_literal"]
    );
    assert_eq!(data["invalid"], 2);
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

#[cfg(unix)]
#[test]
fn edit_says_a_server_clashes_with_one_beside_it_in_another_case_as_validate_does() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    let twin = McpMother::twin("github");
    fs::write(folder.join(format!("{twin}.toml")), McpMother::stdio(&twin)).unwrap();

    // A stand-in editor that changes nothing.
    let edited = weaver(dir.path(), &["edit", "mcp:github", "--editor", "true"]).unwrap();
    let validated = weaver(dir.path(), &["validate", "mcp:github"]).unwrap();

    let problem = format!(
        "1 problem\n  \
         - `github` and `{twin}` differ only by case, which a file system that folds case, as \
         macOS's and Windows' do, holds as one file, so no library holds both everywhere: \
         rename one of them\n"
    );
    assert!(!edited.ok && !validated.ok);
    assert_eq!(
        edited.stdout,
        format!("left mcp github unchanged: {problem}")
    );
    assert!(
        validated.stdout.starts_with(&format!("github: {problem}")),
        "{}",
        validated.stdout
    );
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
fn adding_a_server_told_nothing_of_its_transport_and_installing_one_are_refused_with_what_to_do() {
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
            "an MCP server is added with how an agent starts it, `--command`, or where it \
             reaches it, `--url`, or from a `--template` that says so; or write its definition \
             as <name>.toml in {}",
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

#[test]
fn the_targets_of_servers_are_spoken_of_as_settings_since_none_is_installed() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    let name = dir.path().file_name().unwrap().to_str().unwrap().to_owned();

    let reset = mind(dir.path(), &["targets", "reset", "mcp"]).unwrap();
    let set = mind(dir.path(), &["targets", "set", "mcp", ".x"]).unwrap_err();

    assert!(reset.ok, "{:?}", reset.stderr);
    assert_eq!(
        reset.stdout,
        format!(
            "nothing to change: {name} names no folder for its MCP servers already, and none \
             is installed into a project yet\n"
        )
    );
    assert_eq!(
        set.error.to_string(),
        ".x: cannot be where a project's MCP servers are installed, because MCP servers are \
         not installed into a project as folders yet"
    );
}
