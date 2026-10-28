//! The shell code `flayer activate` prints: what activates a workspace in one
//! shell, as a Python virtualenv's `activate` script does, and the
//! `flayer-deactivate` that undoes it.
//!
//! Printed for the shell to run rather than written into a file. Activation
//! lasts as long as the shell, as a virtualenv's does, and leaves nothing in
//! a directory: D-08 rejected an activation file because it is hidden state
//! in repositories meant to stay minimal.
//!
//! Core's rather than the CLI's because what it sets is what [`resolve`]
//! is fed from, and how a path and a name are made safe to hand a shell is a
//! safety rule: any front end that offers to activate a workspace writes this
//! code, and none can write it differently.
//!
//! What it changes, and so what deactivating restores, is two things: the
//! variable, and a prefix on the prompt. `flayer-deactivate` takes out only
//! what activating put in, wherever it is by then, so that it gets along with
//! an active virtualenv in either order. It is never called `deactivate`,
//! which is the virtualenv's.
//!
//! [`resolve`]: crate::resolve::resolve

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;

use crate::code::codes;
use crate::workspace::FlayerWorkspace;

/// The variable that activates a workspace for every command a shell runs,
/// which the front end reads and hands to [`resolve`](crate::resolve::resolve)
/// as the activated workspace.
pub const WORKSPACE_VARIABLE: &str = "MINDFLAYER_WORKSPACE";

/// What undoes an activation. Not `deactivate`, which an active virtualenv
/// has already defined, and which defining again would take from it.
pub const DEACTIVATE: &str = "flayer-deactivate";

/// A shell activation can be written for.
///
/// A closed enum, as [`Kind`](crate::Kind) is: each one is code of its own,
/// so a new shell is a new arm in every `match` rather than a name that
/// quietly falls through to another shell's code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Pwsh,
}

impl Shell {
    /// Every shell, in the order `--help` lists them.
    pub const ALL: [Shell; 4] = [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Pwsh];

    /// How it is named on a command line.
    pub const fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::Pwsh => "pwsh",
        }
    }

    /// The shell a program is, by its file name: what `$SHELL` says, once
    /// the front end has read it.
    ///
    /// By the name alone, without running anything: a guess that has to be
    /// cheap and cannot be wrong in a way that runs code, and `--shell` is
    /// there when it is wrong.
    pub fn from_program(program: &str) -> Option<Shell> {
        let name = Path::new(program).file_name()?.to_str()?;
        match name.strip_suffix(".exe").unwrap_or(name) {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            "pwsh" | "powershell" => Some(Shell::Pwsh),
            _ => None,
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Shell {
    type Err = UnknownShell;

    fn from_str(word: &str) -> Result<Self, Self::Err> {
        Shell::ALL
            .into_iter()
            .find(|shell| shell.name() == word)
            .ok_or_else(|| UnknownShell(word.to_owned()))
    }
}

/// A word that names no shell activation is written for.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct UnknownShell(pub String);

codes!(UnknownShell => "unknown_shell");

impl fmt::Display for UnknownShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown shell `{}`; expected {}", self.0, expected())
    }
}

/// "bash, zsh, fish or pwsh".
pub fn expected() -> String {
    let names: Vec<&str> = Shell::ALL.iter().map(|shell| shell.name()).collect();
    let (last, rest) = names.split_last().expect("there is a shell");
    format!("{} or {last}", rest.join(", "))
}

/// Why a workspace cannot be activated.
#[derive(Debug, Error)]
pub enum ActivateError {
    /// The default workspace before anything has made it: there is no
    /// directory to point the variable at that a command would accept.
    #[error("{path}: the default workspace is not made yet, so there is nothing to activate; `flayer init` makes one")]
    Unmade { path: PathBuf },
    /// A path a shell cannot be handed as text, which is what the code is.
    #[error("{path}: cannot be activated, since the path is not UTF-8")]
    NotUnicode { path: PathBuf },
}

codes!(ActivateError {
    Self::Unmade { .. } => "default_workspace_unmade",
    Self::NotUnicode { .. } => "activate_path_not_utf8",
});

/// The code that activates `workspace` in `shell`, for the shell to run:
/// `eval "$(flayer activate)"` in bash and zsh.
///
/// Refused rather than written for a workspace that is not on disk: the
/// variable would name a directory every command then refuses.
pub fn script(workspace: &FlayerWorkspace, shell: Shell) -> Result<String, ActivateError> {
    if !workspace.is_made() {
        return Err(ActivateError::Unmade {
            path: workspace.root().to_path_buf(),
        });
    }
    let root = workspace
        .root()
        .to_str()
        .ok_or_else(|| ActivateError::NotUnicode {
            path: workspace.root().to_path_buf(),
        })?;
    Ok(written(shell, root, &prompt(workspace.name())))
}

/// What the prompt starts with while a workspace is active:
/// `(flayer:<name>) `.
///
/// The name is the marker's, which comes with whatever directory holds it, a
/// cloned one included, and a prompt is expanded every time it is drawn:
/// bash, and zsh with `PROMPT_SUBST`, would run a `$(…)` or a backquote in
/// it, `\` and `%` are bash's and zsh's escapes, and `!` is a history number
/// in either. Each of those, and every control character, is shown as `_`.
/// Escaping them instead would have to be done once per shell and per
/// option (`promptvars`, `PROMPT_SUBST`, `PROMPT_BANG`), none of which the
/// code can see when it is written.
pub fn prompt(name: &str) -> String {
    let label: String = name
        .chars()
        .map(|c| {
            if c.is_control() || PROMPT_ACTIVE.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("(flayer:{label}) ")
}

/// The characters some shell's prompt acts on, as [`prompt`] says.
const PROMPT_ACTIVE: [char; 5] = ['\\', '$', '`', '%', '!'];

/// The code for one shell, given the root and the prompt prefix as text.
fn written(shell: Shell, root: &str, prompt: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => bourne(&single_quoted(root), &single_quoted(prompt)),
        Shell::Fish => fish(&fish_quoted(root), &fish_quoted(prompt)),
        Shell::Pwsh => pwsh(&pwsh_quoted(root), &pwsh_quoted(prompt)),
    }
}

/// bash and zsh, which take one code: everything it uses means the same in
/// both, and two copies would be two to keep in step.
///
/// The prompt is a variable, so the prefix is put in front of it and taken
/// out again by its text, wherever it is by then. Restoring a copy saved on
/// the way in, as a virtualenv does, would also take out the virtualenv's
/// prefix when it was activated since. A virtualenv's own copy,
/// `_OLD_VIRTUAL_PS1`, gets the prefix and loses it the same way, so its
/// `deactivate` neither takes the prefix away while a workspace is active
/// nor brings it back once none is.
fn bourne(root: &str, prompt: &str) -> String {
    format!(
        r#"if typeset -f flayer-deactivate >/dev/null 2>&1; then
    flayer-deactivate
fi
if [ -n "${{MINDFLAYER_WORKSPACE+set}}" ]; then
    _FLAYER_OLD_WORKSPACE="$MINDFLAYER_WORKSPACE"
else
    unset _FLAYER_OLD_WORKSPACE
fi
export MINDFLAYER_WORKSPACE={root}
_FLAYER_PROMPT={prompt}
PS1="$_FLAYER_PROMPT${{PS1-}}"
if [ -n "${{_OLD_VIRTUAL_PS1+set}}" ]; then
    _OLD_VIRTUAL_PS1="$_FLAYER_PROMPT$_OLD_VIRTUAL_PS1"
fi
flayer-deactivate() {{
    if [ -n "${{_FLAYER_OLD_WORKSPACE+set}}" ]; then
        export MINDFLAYER_WORKSPACE="$_FLAYER_OLD_WORKSPACE"
        unset _FLAYER_OLD_WORKSPACE
    else
        unset MINDFLAYER_WORKSPACE
    fi
    PS1="${{PS1/"$_FLAYER_PROMPT"/}}"
    if [ -n "${{_OLD_VIRTUAL_PS1+set}}" ]; then
        _OLD_VIRTUAL_PS1="${{_OLD_VIRTUAL_PS1/"$_FLAYER_PROMPT"/}}"
    fi
    unset _FLAYER_PROMPT
    unset -f flayer-deactivate
}}
"#
    )
}

/// fish, whose prompt is a function rather than a variable.
///
/// The function is wrapped once per shell, and the wrapper prints the prefix
/// only while a workspace is active, so deactivating unsets variables and
/// leaves the wrapper where it is. Unwrapping, as a virtualenv does, would
/// break the prompt of whatever wrapped it since: a virtualenv activated
/// afterwards keeps a copy of the wrapper, which calls the function
/// unwrapping would take away.
fn fish(root: &str, prompt: &str) -> String {
    format!(
        r#"if functions -q flayer-deactivate
    flayer-deactivate
end
if set -q MINDFLAYER_WORKSPACE
    set -g _flayer_old_workspace $MINDFLAYER_WORKSPACE
else
    set -e _flayer_old_workspace
end
set -gx MINDFLAYER_WORKSPACE {root}
set -g _flayer_prompt {prompt}
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
    )
}

/// PowerShell, whose prompt is a function too, wrapped once per shell for
/// the reason fish's is. The prefix is emptied rather than removed, so the
/// wrapper never reads a variable that is not there, which strict mode
/// would make an error on every prompt.
fn pwsh(root: &str, prompt: &str) -> String {
    format!(
        r#"if (Test-Path -Path Function:flayer-deactivate) {{
    flayer-deactivate
}}
if (Test-Path -Path Env:MINDFLAYER_WORKSPACE) {{
    $global:_FlayerOldWorkspace = $Env:MINDFLAYER_WORKSPACE
}} else {{
    Remove-Variable -Name _FlayerOldWorkspace -Scope Global -ErrorAction SilentlyContinue
}}
$Env:MINDFLAYER_WORKSPACE = {root}
$global:_FlayerPrompt = {prompt}
if (-not (Get-Variable -Name _FlayerOldPrompt -Scope Global -ErrorAction SilentlyContinue)) {{
    $global:_FlayerOldPrompt = $Function:prompt
    function global:prompt {{
        Write-Host -NoNewline $global:_FlayerPrompt
        & $global:_FlayerOldPrompt
    }}
}}
function global:flayer-deactivate {{
    $old = Get-Variable -Name _FlayerOldWorkspace -Scope Global -ErrorAction SilentlyContinue
    if ($old) {{
        $Env:MINDFLAYER_WORKSPACE = $old.Value
        Remove-Variable -Name _FlayerOldWorkspace -Scope Global
    }} else {{
        Remove-Item -Path Env:MINDFLAYER_WORKSPACE -ErrorAction SilentlyContinue
    }}
    $global:_FlayerPrompt = ''
    Remove-Item -Path Function:flayer-deactivate
}}
"#
    )
}

/// Text as one word in bash or zsh: inside single quotes nothing is special
/// but the closing quote, so each `'` closes the quotes, is escaped, and
/// opens them again.
fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Text as one word in fish, whose single quotes take two escapes: `\'` and
/// `\\`.
fn fish_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', r"\\").replace('\'', r"\'"))
}

/// Text as one word in PowerShell, whose single quotes are escaped by
/// doubling. PowerShell takes the typographic single quotes for quotes as
/// well, so those are doubled too: left alone, one would end the string.
fn pwsh_quoted(text: &str) -> String {
    let mut quoted = String::from("'");
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            quoted.push(c);
        }
        quoted.push(c);
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
#[path = "activate_test.rs"]
mod tests;
