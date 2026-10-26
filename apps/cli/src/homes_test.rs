use super::*;

fn path(spelled: &str) -> Option<PathBuf> {
    Some(PathBuf::from(spelled))
}

#[test]
fn the_command_line_wins_over_the_environment_and_the_fallback() {
    let chosen = resolve(Some("/given"), Some("/variable".into()), path("/fallback"));

    assert_eq!(chosen, path("/given"));
}

#[test]
fn the_environment_wins_over_the_fallback() {
    let chosen = resolve(None, Some("/variable".into()), path("/fallback"));

    assert_eq!(chosen, path("/variable"));
}

#[test]
fn the_fallback_answers_only_when_nothing_else_does() {
    assert_eq!(resolve(None, None, path("/fallback")), path("/fallback"));
    assert_eq!(resolve(None, None, None), None);
}

#[test]
fn an_empty_value_on_the_command_line_means_none_whatever_else_says() {
    // What the test helpers pass: an empty option has to switch the lookup
    // off, not fall through to a developer's environment or home.
    let chosen = resolve(Some(""), Some("/variable".into()), path("/fallback"));

    assert_eq!(chosen, None);
}

#[test]
fn an_empty_variable_means_none_rather_than_the_fallback() {
    let chosen = resolve(None, Some(String::new()), path("/fallback"));

    assert_eq!(chosen, None);
}

#[test]
fn default_home_takes_the_command_line_first() {
    // Only the command-line half is asserted here: whatever the environment
    // running the tests says must not change either answer.
    assert_eq!(default_home(Some("/given")), path("/given"));
    assert_eq!(default_home(Some("")), None);
}

// ---------------------------------------------------------------------------
// Agent homes
// ---------------------------------------------------------------------------

/// An environment holding only these variables.
fn environment(set: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        set.iter()
            .find(|(variable, _)| *variable == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

/// Every agent-home option given as `value`.
fn every_option(value: &str) -> AgentHomeOptions {
    AgentHomeOptions {
        claude_config_dir: Some(format!("{value}claude")),
        codex_home: Some(format!("{value}codex")),
        opencode_config_dir: Some(format!("{value}opencode")),
        agents_dir: Some(format!("{value}agents")),
    }
}

/// Every agent-home option given empty, as the test helpers give them.
fn every_option_empty() -> AgentHomeOptions {
    AgentHomeOptions {
        claude_config_dir: Some(String::new()),
        codex_home: Some(String::new()),
        opencode_config_dir: Some(String::new()),
        agents_dir: Some(String::new()),
    }
}

const EVERY_VARIABLE: &[(&str, &str)] = &[
    ("CLAUDE_CONFIG_DIR", "/variable/claude"),
    ("CODEX_HOME", "/variable/codex"),
    ("OPENCODE_CONFIG_DIR", "/variable/opencode"),
];

#[test]
fn with_nothing_given_each_agent_home_is_its_folder_in_the_users_home() {
    let user = PathBuf::from("/home/user");

    let homes = agent_homes_from(&AgentHomeOptions::default(), &environment(&[]), Some(&user));

    assert_eq!(
        homes,
        AgentHomes {
            claude_config_dir: Some(user.join(".claude")),
            codex_home: Some(user.join(".codex")),
            opencode_config_dir: Some(user.join(".config").join("opencode")),
            agents_dir: Some(user.join(".agents")),
        }
    );
}

#[test]
fn each_variable_moves_its_own_agent_home_and_no_other() {
    let user = PathBuf::from("/home/user");

    let homes = agent_homes_from(
        &AgentHomeOptions::default(),
        &environment(EVERY_VARIABLE),
        Some(&user),
    );

    assert_eq!(
        homes,
        AgentHomes {
            claude_config_dir: path("/variable/claude"),
            codex_home: path("/variable/codex"),
            opencode_config_dir: path("/variable/opencode"),
            // `~/.agents` has no variable of its own to follow.
            agents_dir: Some(user.join(".agents")),
        }
    );
}

#[test]
fn each_option_wins_over_its_variable_and_its_default() {
    let homes = agent_homes_from(
        &every_option("/option/"),
        &environment(EVERY_VARIABLE),
        Some(Path::new("/home/user")),
    );

    assert_eq!(
        homes,
        AgentHomes {
            claude_config_dir: path("/option/claude"),
            codex_home: path("/option/codex"),
            opencode_config_dir: path("/option/opencode"),
            agents_dir: path("/option/agents"),
        }
    );
}

#[test]
fn an_empty_option_means_that_agent_has_no_home_whatever_else_says() {
    // What the test helpers pass, and the reason they can: nothing set in
    // the environment or found in a home reaches a command given these.
    let homes = agent_homes_from(
        &every_option_empty(),
        &environment(EVERY_VARIABLE),
        Some(Path::new("/home/user")),
    );

    assert_eq!(homes, AgentHomes::default());
}

#[test]
fn an_empty_variable_means_that_agent_has_no_home_rather_than_its_default() {
    let homes = agent_homes_from(
        &AgentHomeOptions::default(),
        &environment(&[("CODEX_HOME", "")]),
        Some(Path::new("/home/user")),
    );

    assert_eq!(homes.codex_home, None);
    assert_eq!(
        homes.claude_config_dir,
        Some(Path::new("/home/user").join(".claude"))
    );
}

#[test]
fn with_no_users_home_an_agent_left_to_its_default_has_none() {
    let homes = agent_homes_from(
        &AgentHomeOptions {
            codex_home: Some("/option/codex".into()),
            ..AgentHomeOptions::default()
        },
        &environment(&[("CLAUDE_CONFIG_DIR", "/variable/claude")]),
        None,
    );

    assert_eq!(
        homes,
        AgentHomes {
            claude_config_dir: path("/variable/claude"),
            codex_home: path("/option/codex"),
            opencode_config_dir: None,
            agents_dir: None,
        }
    );
}

#[test]
fn agent_homes_takes_every_option_first() {
    // As for `default_home`, only what the environment cannot change.
    assert_eq!(
        agent_homes(&every_option("")),
        agent_homes_from(&every_option(""), &environment(&[]), None)
    );
    assert_eq!(agent_homes(&every_option_empty()), AgentHomes::default());
}
