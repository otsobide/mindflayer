//! Command line interface for Mindflayer, split the way the model is.
//!
//! `mind <cmd>` acts on the mind project you are standing in. `mind flayer
//! <cmd>` acts on the flayer workspace above it, and the `flayer` binary is
//! the same tree reached directly, so `flayer link x` and `mind flayer link x`
//! are one command with two spellings.
//!
//! The parser, the work and the rendering all live here rather than in the
//! binaries so the tests drive the real command surface instead of shelling
//! out, and so both binaries cannot drift apart.

pub(crate) mod commands;
pub(crate) mod error;
pub mod install;
pub(crate) mod parse;
pub(crate) mod render;
pub mod tui;

pub use commands::read::WORKSPACE;
pub use commands::{default_home, run, run_flayer_cli};
pub use error::CliError;
pub use parse::{Cli, Command, FlayerCli, FlayerCommand, GatherCommand};
pub use render::{Failure, Outcome};
