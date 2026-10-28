//! Unit tests for `error.rs`, kept beside it rather than inside it.

use super::*;

/// A code reaches the panel without the type it came from, so one the CLI
/// gives that core gives too, or that two of the CLI's types give, would be
/// two conditions a front end cannot tell apart.
#[test]
fn no_code_the_cli_gives_is_given_twice_here_or_in_core() {
    let mut seen: Vec<(&str, &str)> = mindflayer_core::code::ALL
        .iter()
        .flat_map(|(owner, codes)| codes.iter().map(move |code| (*owner, *code)))
        .collect();
    let mut twice: Vec<String> = Vec::new();
    for (owner, codes) in CODES {
        for code in *codes {
            if let Some((first, _)) = seen.iter().find(|(_, seen)| seen == code) {
                twice.push(format!("`{code}` in {first} and {owner}"));
            }
            seen.push((owner, code));
        }
    }

    assert!(twice.is_empty(), "codes given twice: {twice:?}");
}

#[test]
fn every_code_the_cli_gives_is_snake_case() {
    let unfit: Vec<&str> = CODES
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .filter(|code| {
            !code.split('_').all(|word| {
                !word.is_empty()
                    && word
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        })
        .collect();

    assert!(unfit.is_empty(), "codes not in snake_case: {unfit:?}");
}
