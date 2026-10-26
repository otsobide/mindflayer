//! Where things live outside the directory a command works in: the default
//! workspace's home.
//!
//! Resolved here, in the front end, because core never reads the environment.
//! It takes what this module answers as a parameter, which is what lets the
//! tests hand it an empty value and know nothing real is reached.

use std::path::PathBuf;

/// Where the default workspace lives: `--home`, then `$MINDFLAYER_HOME`, then
/// the user's home directory. Empty, from either, means there is none.
///
/// A string rather than a path because empty is a meaningful answer here,
/// and one a path cannot spell.
pub fn default_home(given: Option<&str>) -> Option<PathBuf> {
    resolve(given, variable("MINDFLAYER_HOME"), std::env::home_dir())
}

/// The order every home is decided in: what the command line gave, then
/// what the environment says, then the fallback. Empty, from either of the
/// first two, means none, and the fallback is not consulted.
///
/// Given the environment's answer rather than reading it, so the order is
/// tested on values. Setting variables in a test would change them for every
/// other test running in the same process.
fn resolve(
    given: Option<&str>,
    from_environment: Option<String>,
    fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    let chosen = match (given, from_environment) {
        (Some(given), _) => given.to_owned(),
        (None, Some(from_environment)) => from_environment,
        (None, None) => return fallback,
    };
    (!chosen.is_empty()).then(|| PathBuf::from(chosen))
}

/// A variable's value. One that is not Unicode counts as unset, as it always
/// has for `$MINDFLAYER_HOME`: a path that cannot be spelled is no answer.
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
#[path = "homes_test.rs"]
mod tests;
