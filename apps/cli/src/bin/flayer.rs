//! `flayer <cmd>`, the shortcut for `mind flayer <cmd>`.
//!
//! A second entry point into the same parser rather than a wrapper that
//! re-executes `mind`: there is one implementation of every workspace command,
//! so the two spellings cannot drift apart or disagree about an exit code.

use std::process::ExitCode;

use clap::Parser;
use mindflayer_cli::{run_flayer_cli, FlayerCli};

fn main() -> ExitCode {
    let cli = FlayerCli::parse();
    match run_flayer_cli(&cli) {
        Ok(outcome) => outcome.report(),
        Err(failure) => failure.report(),
    }
}
