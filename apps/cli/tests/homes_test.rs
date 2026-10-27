//! Where the default workspace and each agent's own folder are: global
//! options on both binaries, which the helpers pin so that no test reaches
//! the real ones.

mod common;
use common::*;

/// Each agent-home option, with what its help must say it falls back to.
const AGENT_HOMES: [(&str, &[&str]); 4] = [
    ("--claude-config-dir", &["$CLAUDE_CONFIG_DIR", "~/.claude"]),
    ("--codex-home", &["$CODEX_HOME", "~/.codex"]),
    (
        "--opencode-config-dir",
        &["$OPENCODE_CONFIG_DIR", "~/.config/opencode"],
    ),
    ("--agents-dir", &["~/.agents"]),
];

fn help(line: &[&str]) -> String {
    let error = match line[0] {
        "flayer" => FlayerCli::try_parse_from(line).unwrap_err(),
        _ => Cli::try_parse_from(line).unwrap_err(),
    };
    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp,
        "{line:?}"
    );
    error.to_string()
}

#[test]
fn both_binaries_document_every_agent_home_and_its_fallbacks() {
    // Every spelling, a nested subcommand included: a global option the
    // help of `flayer gather git` left out would be one nobody finds.
    for line in [
        vec!["mind", "--help"],
        vec!["mind", "flayer", "list", "--help"],
        vec!["flayer", "--help"],
        vec!["flayer", "gather", "git", "--help"],
    ] {
        let text = help(&line);
        for (option, fallbacks) in AGENT_HOMES {
            let described = text
                .lines()
                .skip_while(|row| !row.contains(option))
                .nth(1)
                .unwrap_or_else(|| panic!("{line:?} does not document {option}:\n{text}"));
            for fallback in fallbacks {
                assert!(
                    described.contains(fallback),
                    "{line:?}: {option} does not name {fallback}: {described}"
                );
            }
            assert!(described.contains("empty for none"), "{described}");
        }
    }
}

#[test]
fn an_agent_home_is_taken_wherever_a_global_option_is() {
    let cli =
        FlayerCli::try_parse_from(["flayer", "gather", "list", "--codex-home", "/codex"]).unwrap();
    assert_eq!(cli.agent_homes.codex_home.as_deref(), Some("/codex"));

    let cli = Cli::try_parse_from(["mind", "--agents-dir=", "flayer", "list"]).unwrap();
    assert_eq!(cli.agent_homes.agents_dir.as_deref(), Some(""));
}

#[test]
fn both_binaries_document_the_workspace_option_and_its_fallbacks() {
    for line in [
        vec!["mind", "--help"],
        vec!["mind", "flayer", "list", "--help"],
        vec!["flayer", "--help"],
        vec!["flayer", "gather", "git", "--help"],
    ] {
        let text = help(&line);
        let described: String = text
            .lines()
            .skip_while(|row| !row.contains("--workspace"))
            .take(4)
            .collect();
        assert!(described.contains("-W"), "{line:?}:\n{text}");
        assert!(described.contains("$MINDFLAYER_WORKSPACE"), "{described}");
        assert!(described.contains("empty"), "{described}");
    }
}

#[test]
fn the_helpers_pin_the_default_home_the_workspace_and_every_agent_home_to_none() {
    let dir = TempDir::new().unwrap();
    let empty = Some(String::new());
    let pinned = AgentHomeOptions {
        claude_config_dir: empty.clone(),
        codex_home: empty.clone(),
        opencode_config_dir: empty.clone(),
        agents_dir: empty.clone(),
    };

    let mind = parsed_mind(dir.path(), &["list"]);
    let flayer = parsed_flayer(dir.path(), &["list"]);

    // Given empty, not left out: left out, each would fall through to the
    // environment running the tests and to the developer's own home.
    for (home, workspace, agents) in [
        (&mind.home, &mind.workspace, &mind.agent_homes),
        (&flayer.home, &flayer.workspace, &flayer.agent_homes),
    ] {
        assert_eq!(home, &empty);
        assert_eq!(workspace, &empty);
        assert_eq!(agents, &pinned);
        let lookup = Lookup::given(home.as_deref(), workspace.as_deref(), agents);
        assert_eq!(lookup.home, None);
        assert_eq!(lookup.activated, None);
        assert_eq!(lookup.agents, AgentHomes::default());
        assert_eq!(agent_homes(agents), AgentHomes::default());
    }
}

#[test]
fn a_test_can_still_give_a_home_of_its_own_in_either_spelling() {
    let dir = TempDir::new().unwrap();

    let cli = parsed_flayer(dir.path(), &["list", "--codex-home=/codex", "--home", "/h"]);

    assert_eq!(cli.agent_homes.codex_home.as_deref(), Some("/codex"));
    assert_eq!(cli.home.as_deref(), Some("/h"));
    // The ones it did not give stay pinned.
    assert_eq!(cli.agent_homes.claude_config_dir.as_deref(), Some(""));
}

#[test]
fn a_test_can_still_name_a_workspace_in_any_spelling() {
    let dir = TempDir::new().unwrap();

    for given in [
        vec!["list", "-W", "/w"],
        vec!["list", "-W/w"],
        vec!["list", "--workspace", "/w"],
        vec!["list", "--workspace=/w"],
    ] {
        let cli = parsed_flayer(dir.path(), &given);
        assert_eq!(cli.workspace.as_deref(), Some("/w"), "{given:?}");
    }
}
