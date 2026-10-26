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
