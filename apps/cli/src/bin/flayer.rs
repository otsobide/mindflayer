//! `flayer <cmd>`, the shortcut for `mind flayer <cmd>`.
//!
//! A second entry point into the same parser rather than a wrapper that
//! re-executes `mind`: there is one implementation of every workspace command,
//! so the two spellings cannot drift apart or disagree about an exit code.

use std::process::ExitCode;

use clap::Parser;
use mindflayer_cli::{run_flayer_cli_watched, FlayerCli, ProgressLine};

fn main() -> ExitCode {
    let cli = FlayerCli::parse();
    let progress = ProgressLine::on_stderr();
    let result = run_flayer_cli_watched(&cli, &progress.watch());
    progress.clear();
    match result {
        Ok(outcome) => outcome.report(),
        Err(failure) => failure.report(),
    }
}
