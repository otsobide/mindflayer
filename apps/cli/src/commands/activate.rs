//! The commands about which workspace a shell's commands use: `activate`,
//! which prints the code that chooses one for the shell.
//!
//! The code itself is core's (`activate::script`), as what it sets is what
//! `resolve` is fed from; what is decided here is only which workspace and
//! which shell, from the command line and the environment.

use std::path::Path;

use mindflayer_core::activate::{self, Shell};
use mindflayer_core::resolve;

use super::{typed, workspace_to_read};
use crate::homes::variable;
use crate::{CliError, Failure, Lookup, Outcome};

/// `flayer activate`: the code that activates the workspace `path` names, or
/// the one a command run from `directory` would use, for `shell`, or for the
/// one `$SHELL` names.
///
/// Nothing on stdout unless it succeeds, so that `eval "$(flayer activate
/// …)"` runs nothing when there is nothing to activate, and nothing written
/// either way: the workspace is opened, or resolved as a read is, which
/// never makes the default one.
pub(crate) fn activate_command(
    directory: &Path,
    path: Option<&Path>,
    shell: Option<Shell>,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let workspace = match path {
        Some(path) => resolve::named(&typed(directory, path)?)?,
        None => workspace_to_read(directory, lookup)?,
    };
    let shell = match shell {
        Some(shell) => shell,
        None => guessed(&variable)?,
    };
    let script = activate::script(&workspace, shell).map_err(CliError::from)?;
    Ok(Outcome::plain(script))
}

/// The shell `$SHELL` names, with the environment passed in, so the guess is
/// tested on values.
///
/// `$SHELL` is the login shell rather than the one running, which is the
/// same shell more often than not, and the only one a program can learn
/// without asking the system about its parent; `--shell` is there for the
/// rest. Empty counts as unset, as it does for every variable read here.
fn guessed(variable: &dyn Fn(&str) -> Option<String>) -> Result<Shell, CliError> {
    match variable("SHELL").filter(|program| !program.is_empty()) {
        None => Err(CliError::NoShell),
        Some(program) => Shell::from_program(&program).ok_or(CliError::UnknownShell { program }),
    }
}

#[cfg(test)]
#[path = "activate_test.rs"]
mod tests;
