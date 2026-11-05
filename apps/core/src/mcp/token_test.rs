use super::*;
use crate::mother::{random, McpMother};

fn placeholders() -> Vec<String> {
    vec![String::from("{token}")]
}

#[test]
fn every_shape_a_token_is_written_in_is_found_by_what_it_starts_with() {
    for _ in 0..100 {
        for (prefix, token) in McpMother::tokens() {
            assert_eq!(found(&token, &[]), [prefix], "{token}");
            // Wherever it stands in a value, after a separator.
            for value in [
                format!("--key={token}"),
                format!("key {token} more"),
                format!("{{nobody}}{token}"),
            ] {
                assert_eq!(found(&value, &placeholders()), [prefix], "{value}");
            }
        }
    }
}

#[test]
fn what_starts_as_a_token_does_and_is_none_is_not_found() {
    for (why, text) in McpMother::not_tokens() {
        assert_eq!(
            found(&text, &placeholders()),
            [] as [&str; 0],
            "{why}: {text}"
        );
    }
}

#[test]
fn a_token_beside_a_placeholder_is_found_and_the_placeholder_joins_nothing() {
    let (prefix, token) = McpMother::token();

    assert_eq!(
        found(&format!("{{token}}{token}"), &placeholders()),
        [prefix]
    );
    // The placeholder ends the word before it: what follows is not read as
    // the rest of a token that started before it.
    let half = format!("ghp_{}", random::drawn_from(random::DIGITS, 15));
    let joined = format!("{half}{{token}}{}", random::drawn_from(random::DIGITS, 20));
    assert_eq!(found(&joined, &placeholders()), [] as [&str; 0], "{joined}");
}

#[test]
fn two_tokens_in_one_text_are_each_found() {
    let tokens = McpMother::tokens();
    let text = tokens
        .iter()
        .map(|(_, token)| token.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let prefixes: Vec<&str> = tokens.iter().map(|(prefix, _)| *prefix).collect();
    assert_eq!(found(&text, &[]), prefixes, "{text}");
}
