//! Command line interface for Mindflayer, split the way the model is.
//!
//! `mind <cmd>` acts on the mind project you are standing in. `mind flayer
//! <cmd>` acts on the flayer workspace above it, and the `flayer` binary is
//! the same tree reached directly, so `flayer link x` and `mind flayer link x`
//! are one command with two spellings.
//!
//! The parser, the work and the rendering all live in this library rather
//! than in the binaries so the tests drive the real command surface instead of
//! shelling out, and so both binaries cannot drift apart.
//!
//! Each has a module of its own (`parse`, `commands` with a module per family
//! of commands, and `render`), so that work on one command does not collide
//! with work on another in one long file. What the binaries and the tests use
//! is public: the two screens, and what is re-exported here.

pub mod admit;
#[cfg(feature = "ui")]
pub mod backend;
pub(crate) mod commands;
pub(crate) mod error;
pub(crate) mod homes;
pub mod install;
pub(crate) mod parse;
pub(crate) mod progress;
pub(crate) mod render;
pub mod task;
pub mod tui;
#[cfg(feature = "ts")]
pub mod typescript;

pub use commands::read::WORKSPACE;
pub use commands::{run, run_flayer_cli, run_flayer_cli_watched, run_watched};
pub use error::{CliError, CODES};
pub use homes::{agent_homes, default_home, Lookup};
#[cfg(feature = "ui")]
pub use parse::UiOptions;
pub use parse::{
    AgentHomeOptions, AgentNames, AgentsChange, Cli, Command, FlayerCli, FlayerCommand,
    GatherCommand, Migrated, ShareMode, TargetsChange, WorkspacesCommand,
};
pub use progress::ProgressLine;
pub use render::{Failure, Outcome, Warning};
