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
    let chosen = match given {
        Some(given) => given.to_owned(),
        None => match std::env::var("MINDFLAYER_HOME") {
            Ok(from_environment) => from_environment,
            Err(_) => return std::env::home_dir(),
        },
    };
    (!chosen.is_empty()).then(|| PathBuf::from(chosen))
}
