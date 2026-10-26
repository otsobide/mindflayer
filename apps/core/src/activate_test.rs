//! Unit tests for `activate.rs`, kept beside it rather than inside it.
//!
//! The code is asserted whole, for each shell, because it is run as it is
//! printed: a quote out of place is code somebody's shell runs.

use super::*;

/// A root with a space and both quotes in it, named as its directory is.
const ROOT: &str = r#"/home/you/it's a "flayer""#;
const NAME: &str = r#"it's a "flayer""#;

/// What `ROOT` activates in bash and in zsh, which share it.
const BOURNE: &str = r#"if typeset -f flayer-deactivate >/dev/null 2>&1; then
    flayer-deactivate
fi
if [ -n "${MINDFLAYER_WORKSPACE+set}" ]; then
    _FLAYER_OLD_WORKSPACE="$MINDFLAYER_WORKSPACE"
else
    unset _FLAYER_OLD_WORKSPACE
fi
export MINDFLAYER_WORKSPACE='/home/you/it'\''s a "flayer"'
_FLAYER_PROMPT='(flayer:it'\''s a "flayer") '
PS1="$_FLAYER_PROMPT${PS1-}"
if [ -n "${_OLD_VIRTUAL_PS1+set}" ]; then
    _OLD_VIRTUAL_PS1="$_FLAYER_PROMPT$_OLD_VIRTUAL_PS1"
fi
flayer-deactivate() {
    if [ -n "${_FLAYER_OLD_WORKSPACE+set}" ]; then
        export MINDFLAYER_WORKSPACE="$_FLAYER_OLD_WORKSPACE"
        unset _FLAYER_OLD_WORKSPACE
    else
        unset MINDFLAYER_WORKSPACE
    fi
    PS1="${PS1/"$_FLAYER_PROMPT"/}"
    if [ -n "${_OLD_VIRTUAL_PS1+set}" ]; then
        _OLD_VIRTUAL_PS1="${_OLD_VIRTUAL_PS1/"$_FLAYER_PROMPT"/}"
    fi
    unset _FLAYER_PROMPT
    unset -f flayer-deactivate
}
"#;

#[test]
fn bash_and_zsh_are_given_one_code_with_every_quote_escaped() {
    assert_eq!(written(Shell::Bash, ROOT, &prompt(NAME)), BOURNE);
    assert_eq!(written(Shell::Zsh, ROOT, &prompt(NAME)), BOURNE);
}

#[test]
fn fish_is_given_its_own_code_with_its_two_escapes() {
    let root = r"/home/you/it's a \flayer\";
    let name = r#"it's a "flayer""#;

    assert_eq!(
        written(Shell::Fish, root, &prompt(name)),
        r#"if functions -q flayer-deactivate
    flayer-deactivate
end
if set -q MINDFLAYER_WORKSPACE
    set -g _flayer_old_workspace $MINDFLAYER_WORKSPACE
else
    set -e _flayer_old_workspace
end
set -gx MINDFLAYER_WORKSPACE '/home/you/it\'s a \\flayer\\'
set -g _flayer_prompt '(flayer:it\'s a "flayer") '
if not functions -q _flayer_old_fish_prompt
    functions -c fish_prompt _flayer_old_fish_prompt
    function fish_prompt
        set -l last_status $status
        if set -q _flayer_prompt
            printf '%s' $_flayer_prompt
        end
        echo "exit $last_status" | source
        _flayer_old_fish_prompt
    end
end
function flayer-deactivate
    if set -q _flayer_old_workspace
        set -gx MINDFLAYER_WORKSPACE $_flayer_old_workspace
        set -e _flayer_old_workspace
    else
        set -e MINDFLAYER_WORKSPACE
    end
    set -e _flayer_prompt
    functions -e flayer-deactivate
end
"#
    );
}

#[test]
fn powershell_is_given_its_own_code_with_quotes_doubled() {
    assert_eq!(
        written(Shell::Pwsh, ROOT, &prompt(NAME)),
        r#"if (Test-Path -Path Function:flayer-deactivate) {
    flayer-deactivate
}
if (Test-Path -Path Env:MINDFLAYER_WORKSPACE) {
    $global:_FlayerOldWorkspace = $Env:MINDFLAYER_WORKSPACE
} else {
    Remove-Variable -Name _FlayerOldWorkspace -Scope Global -ErrorAction SilentlyContinue
}
$Env:MINDFLAYER_WORKSPACE = '/home/you/it''s a "flayer"'
$global:_FlayerPrompt = '(flayer:it''s a "flayer") '
if (-not (Get-Variable -Name _FlayerOldPrompt -Scope Global -ErrorAction SilentlyContinue)) {
    $global:_FlayerOldPrompt = $Function:prompt
    function global:prompt {
        Write-Host -NoNewline $global:_FlayerPrompt
        & $global:_FlayerOldPrompt
    }
}
function global:flayer-deactivate {
    $old = Get-Variable -Name _FlayerOldWorkspace -Scope Global -ErrorAction SilentlyContinue
    if ($old) {
        $Env:MINDFLAYER_WORKSPACE = $old.Value
        Remove-Variable -Name _FlayerOldWorkspace -Scope Global
    } else {
        Remove-Item -Path Env:MINDFLAYER_WORKSPACE -ErrorAction SilentlyContinue
    }
    $global:_FlayerPrompt = ''
    Remove-Item -Path Function:flayer-deactivate
}
"#
    );
}

#[test]
fn powershell_doubles_the_typographic_single_quotes_it_also_ends_strings_on() {
    assert_eq!(
        pwsh_quoted("a\u{2018}b\u{2019}c\u{201A}d\u{201B}e'f"),
        "'a\u{2018}\u{2018}b\u{2019}\u{2019}c\u{201A}\u{201A}d\u{201B}\u{201B}e''f'"
    );
}

#[test]
fn a_path_full_of_what_a_shell_expands_is_one_word_and_nothing_more() {
    let root = "/tmp/$(touch pwned) `id` ${HOME} \\n !! *";

    assert_eq!(
        single_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\n !! *'"
    );
    assert_eq!(
        fish_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\\\n !! *'"
    );
    assert_eq!(
        pwsh_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\n !! *'"
    );
}

#[test]
fn the_prompt_shows_what_a_prompt_would_act_on_as_an_underscore() {
    assert_eq!(prompt("writing"), "(flayer:writing) ");
    assert_eq!(prompt("café notes"), "(flayer:café notes) ");
    assert_eq!(
        prompt("$(touch pwned) `id` %n !! \\w\nx\u{1b}"),
        "(flayer:_(touch pwned) _id_ _n __ _w_x_) "
    );
}

#[test]
fn every_shell_sets_the_variable_and_defines_flayer_deactivate_and_never_deactivate() {
    for shell in Shell::ALL {
        let code = written(shell, "/w", &prompt("w"));

        assert!(code.contains(WORKSPACE_VARIABLE), "{shell}:\n{code}");
        assert!(code.contains(DEACTIVATE), "{shell}:\n{code}");
        // Every `deactivate` it names is its own: the plain one is an active
        // virtualenv's, and redefining it would take it away.
        for (at, _) in code.match_indices("deactivate") {
            assert!(code[..at].ends_with("flayer-"), "{shell}:\n{code}");
        }
    }
}

#[test]
fn a_shell_is_named_by_its_name_and_guessed_from_its_program() {
    for shell in Shell::ALL {
        assert_eq!(shell.name().parse::<Shell>(), Ok(shell));
        assert_eq!(Shell::from_program(shell.name()), Some(shell));
    }
    assert_eq!(Shell::from_program("/bin/bash"), Some(Shell::Bash));
    assert_eq!(Shell::from_program("/usr/local/bin/zsh"), Some(Shell::Zsh));
    assert_eq!(Shell::from_program("/usr/bin/fish"), Some(Shell::Fish));
    assert_eq!(
        Shell::from_program("/opt/microsoft/pwsh"),
        Some(Shell::Pwsh)
    );
    assert_eq!(Shell::from_program("powershell.exe"), Some(Shell::Pwsh));
    assert_eq!(Shell::from_program("/bin/tcsh"), None);
    assert_eq!(Shell::from_program("/bin/sh"), None);
    assert_eq!(Shell::from_program(""), None);
}

#[test]
fn a_word_that_is_no_shell_says_which_are() {
    assert_eq!(
        "tcsh".parse::<Shell>().unwrap_err().to_string(),
        "unknown shell `tcsh`; expected bash, zsh, fish or pwsh"
    );
}

#[test]
fn a_workspace_not_made_yet_is_not_activated() {
    let home = tempfile::TempDir::new().unwrap();
    let unmade = FlayerWorkspace::unmade(home.path()).unwrap();

    let error = script(&unmade, Shell::Bash).unwrap_err();

    assert!(matches!(error, ActivateError::Unmade { .. }), "{error}");
}

#[test]
fn a_made_workspace_is_activated_by_its_root_and_named_by_its_marker() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("writing");
    let (workspace, _) = FlayerWorkspace::init(&root).unwrap();

    let code = script(&workspace, Shell::Bash).unwrap();

    let root = root.to_str().unwrap();
    assert_eq!(code, written(Shell::Bash, root, "(flayer:writing) "));
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_is_refused_rather_than_mangled() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join(OsStr::from_bytes(b"not-\xffutf8"));
    let (workspace, _) = FlayerWorkspace::init(&root).unwrap();

    let error = script(&workspace, Shell::Bash).unwrap_err();

    assert!(matches!(error, ActivateError::NotUnicode { .. }), "{error}");
}
