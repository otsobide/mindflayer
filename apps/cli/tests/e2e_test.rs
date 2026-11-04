//! The CLI end to end: `mind` and `flayer`, the binaries a person runs,
//! driven one use case at a time.
//!
//! Every other suite of the CLI stops short of the process: it parses a line
//! with the real parsers and runs it in the test's own process
//! (`common::mind`, `common::flayer`), its options pinned so it reaches no
//! real home. Here each command is the built binary, started from a folder
//! with an environment of its own, and what a test holds is what a person
//! sees: the exit status, stdout and stderr, word for word, and what the
//! command left on the disk.
//!
//! Each test is one use case, in three parts:
//!
//! - the scenario it needs, built from nothing for that test alone
//!   ([`Scenario`]): folders and git repositories as a person or a git
//!   server makes them, and Mindflayer's own state only through the commands
//!   themselves (`given`), each held to exit 0 with nothing on stderr;
//! - one command, the input of the use case, as it is typed in a shell;
//! - its output, as the reference shows it (`docs/reference.md`): every path
//!   under the scenario reads as `/home`, the home being `/home/you`.
//!
//! No test shares a scenario with another or depends on one having run, so
//! any one of them runs alone (`cargo test -p mindflayer-cli --test e2e_test
//! <name>`). The use cases are grouped by what a person is doing: making
//! projects and workspaces, writing skills and rules, installing them,
//! gathering them from git, keeping copies up to date, reading what is
//! where, and saying which workspace a command is about.
//!
//! Unix alone: the reference's paths are a Unix home's, which a Windows
//! path, its drive and its backslashes, never reads as, so every use case
//! printing a path would fail there for how the path is spelled rather
//! than for what the command did.

#![cfg(unix)]

#[path = "e2e/scenario.rs"]
mod scenario;

#[path = "e2e/projects.rs"]
mod projects;

#[path = "e2e/authoring.rs"]
mod authoring;

#[path = "e2e/installing.rs"]
mod installing;

#[path = "e2e/gathering.rs"]
mod gathering;

#[path = "e2e/updating.rs"]
mod updating;

#[path = "e2e/reading.rs"]
mod reading;

#[path = "e2e/workspaces.rs"]
mod workspaces;

#[path = "e2e/mistakes.rs"]
mod mistakes;

pub use scenario::{Ran, Scenario};

/// A skill's file as a person writes one: front matter with its name and
/// description, then what it says.
pub fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
}
