//! Unit tests for `agent.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn each_agents_default_folder_is_its_own_in_the_users_home() {
    let user = Path::new("/home/user");

    assert_eq!(
        AgentHomes::defaults_under(user),
        AgentHomes {
            claude_config_dir: Some(user.join(".claude")),
            codex_home: Some(user.join(".codex")),
            opencode_config_dir: Some(user.join(".config").join("opencode")),
            agents_dir: Some(user.join(".agents")),
        }
    );
}

#[test]
fn an_agent_is_named_by_its_binary_and_shown_the_same_way() {
    let names: Vec<&str> = Agent::ALL.iter().map(|agent| agent.name()).collect();
    assert_eq!(names, ["claude", "codex", "opencode"]);
    for agent in Agent::ALL {
        assert_eq!(agent.name().parse::<Agent>(), Ok(agent));
        assert_eq!(agent.to_string(), agent.name());
    }
}

#[test]
fn a_word_that_is_not_an_agent_is_refused_with_every_name_there_is() {
    let error = "cursor".parse::<Agent>().unwrap_err();

    assert_eq!(error, UnknownAgent("cursor".to_owned()));
    assert_eq!(
        error.to_string(),
        "unknown agent `cursor`; expected claude, codex or opencode"
    );
}

#[test]
fn an_agent_is_taken_by_its_exact_name_only() {
    // One spelling per agent, so a settings file and a command line never
    // hold two that mean the same.
    for word in ["Claude", "OpenCode", "open-code", " codex", ""] {
        assert_eq!(word.parse::<Agent>(), Err(UnknownAgent(word.to_owned())));
    }
}

#[test]
fn every_agent_is_listed_once_in_report_order() {
    // What sorts agents sorts them by the derived order, so `ALL` must be in
    // it too, or two reports would list them two ways.
    assert!(Agent::ALL.windows(2).all(|pair| pair[0] < pair[1]));
}
