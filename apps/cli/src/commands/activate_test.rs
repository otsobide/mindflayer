//! Unit tests for `activate.rs`: guessing the shell from what `$SHELL` says,
//! on values, since setting the variable would set it for every test in the
//! process.

use super::*;

fn shell_is(value: Option<&str>) -> impl Fn(&str) -> Option<String> + '_ {
    move |name| {
        assert_eq!(name, "SHELL");
        value.map(str::to_owned)
    }
}

#[test]
fn the_shell_is_the_one_shell_names() {
    assert_eq!(guessed(&shell_is(Some("/bin/bash"))).unwrap(), Shell::Bash);
    assert_eq!(
        guessed(&shell_is(Some("/usr/bin/zsh"))).unwrap(),
        Shell::Zsh
    );
    assert_eq!(
        guessed(&shell_is(Some("/usr/bin/fish"))).unwrap(),
        Shell::Fish
    );
    assert_eq!(
        guessed(&shell_is(Some("/usr/bin/pwsh"))).unwrap(),
        Shell::Pwsh
    );
}

#[test]
fn with_shell_unset_or_empty_there_is_nothing_to_guess_from() {
    for value in [None, Some("")] {
        assert!(matches!(guessed(&shell_is(value)), Err(CliError::NoShell)));
    }
}

#[test]
fn a_shell_no_code_is_written_for_is_named_in_the_refusal() {
    let error = guessed(&shell_is(Some("/bin/tcsh"))).unwrap_err();

    assert_eq!(
        error.to_string(),
        "cannot tell which shell `/bin/tcsh` is; say which to write for with --shell (bash, zsh, fish or pwsh)"
    );
}
