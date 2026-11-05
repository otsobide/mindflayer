use super::*;
use crate::mother::{random, ArtifactNameMother, McpMother};

const DRAWS: usize = 200;

#[test]
fn a_name_of_letters_in_either_case_digits_hyphens_and_underscores_is_a_key() {
    for _ in 0..DRAWS {
        let name = McpMother::name();

        assert_eq!(issues(&name), [], "{name}");
        assert!(is_key(&name), "{name}");
    }
    // What every other kind's name is held to is one too.
    for _ in 0..DRAWS {
        let name = ArtifactNameMother::random();
        assert_eq!(issues(&name), [], "{name}");
    }
    for name in ["GitHub", "my_server", "MY-SERVER_2", "_", "-"] {
        assert_eq!(issues(name), [], "{name}");
    }
}

#[test]
fn a_name_holding_what_no_agent_keys_a_server_by_is_no_key() {
    for (why, name) in McpMother::unkeyed() {
        assert_eq!(
            issues(&name),
            [ValidationIssue::McpNameNotAKey { name: name.clone() }],
            "{why}"
        );
        assert!(!is_key(&name), "{why}");
        assert_eq!(issues(&name)[0].code(), "mcp_name_not_a_key");
    }
}

#[test]
fn a_name_is_held_to_be_something_and_to_the_length_every_name_is() {
    assert_eq!(issues(""), [ValidationIssue::NameEmpty]);
    assert!(!is_key(""));

    let long = "a".repeat(MAX_NAME_SEGMENT_LEN + 1);
    assert_eq!(
        issues(&long),
        [ValidationIssue::NameSegmentTooLong {
            segment: long.clone(),
            length: MAX_NAME_SEGMENT_LEN + 1,
        }]
    );
    assert!(!is_key(&long));
    assert!(is_key(&"a".repeat(MAX_NAME_SEGMENT_LEN)));
}

#[test]
fn a_name_an_agent_keeps_for_itself_is_refused_in_any_case_and_spelling() {
    for _ in 0..20 {
        for (name, agent, reserved) in McpMother::reserved() {
            let said = issues(&name);

            assert_eq!(
                said,
                [ValidationIssue::McpNameReserved {
                    name: name.clone(),
                    agent,
                    reserved: reserved.to_owned(),
                }],
                "{name}"
            );
            assert!(!said[0].is_warning(), "{name}");
            assert_eq!(said[0].code(), "mcp_name_reserved");
            assert!(said[0].to_string().contains(&format!("`{reserved}`")));
            // Still a key: a definition can be filed under it, and only
            // `validate` says an agent keeps it.
            assert!(is_key(&name), "{name}");
        }
    }
}

#[test]
fn a_name_near_one_an_agent_keeps_is_no_name_it_keeps() {
    for (why, name) in McpMother::near_reserved() {
        assert_eq!(issues(&name), [], "{why}: {name}");
    }
}

#[test]
fn a_name_an_agent_uses_for_its_own_is_only_worth_a_warning() {
    for (name, agent, claimed) in McpMother::claimed() {
        let said = issues(&name);

        assert_eq!(
            said,
            [ValidationIssue::McpNameClaimed {
                name: name.clone(),
                agent,
                claimed: claimed.to_owned(),
            }],
            "{name}"
        );
        assert!(said[0].is_warning(), "{name}");
        assert_eq!(said[0].code(), "mcp_name_claimed");
    }
}

#[test]
fn a_name_s_twins_are_the_names_beside_it_that_are_it_in_another_case() {
    let name = format!(
        "{}{}",
        random::word(),
        random::drawn_from(random::DIGITS, 2)
    );
    let twin = McpMother::twin(&name);
    let other = McpMother::name();

    assert_ne!(twin, name);
    assert_eq!(
        twins(&name, [name.as_str(), twin.as_str(), other.as_str()]),
        [twin.as_str()]
    );
    assert_eq!(
        twins(&name, [name.to_ascii_uppercase().as_str()]),
        [name.to_ascii_uppercase().as_str()]
    );
    assert!(twins(&name, [name.as_str(), other.as_str()]).is_empty());
}

#[test]
fn each_agent_keeps_the_names_docs_agents_md_lists_and_codex_keeps_none() {
    let listed = |agent: Agent| -> Vec<&'static str> { reserved(agent).to_vec() };

    assert_eq!(
        listed(Agent::Claude),
        [
            "workspace",
            "claude-in-chrome",
            "computer-use",
            "Claude Preview",
            "Claude Browser"
        ]
    );
    assert_eq!(listed(Agent::Codex), [] as [&str; 0]);
    assert_eq!(listed(Agent::OpenCode), ["servers", "timeout"]);
    assert_eq!(claimed(Agent::Claude), ["anthropic-skills"]);
    assert!(claimed(Agent::Codex).is_empty() && claimed(Agent::OpenCode).is_empty());
    // Drawn so that the name is never one of them.
    let name = format!(
        "{}{}",
        random::word(),
        random::drawn_from(random::DIGITS, 1)
    );
    assert_eq!(issues(&name), []);
}
