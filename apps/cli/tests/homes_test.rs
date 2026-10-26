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
