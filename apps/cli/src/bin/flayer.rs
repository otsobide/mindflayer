//! `flayer <cmd>`, the shortcut for `mind flayer <cmd>`.
//!
//! A second entry point into the same parser rather than a wrapper that
//! re-executes `mind`: there is one implementation of every workspace command,
//! so the two spellings cannot drift apart or disagree about an exit code.

use std::process::ExitCode;

use clap::Parser;
use mindweaver_cli::{run_weaver_cli_watched, ProgressLine, WeaverCli};

fn main() -> ExitCode {
    let cli = WeaverCli::parse();
    let progress = ProgressLine::on_stderr();
    let result = run_weaver_cli_watched(&cli, &progress.watch());
    progress.clear();
    match result {
        Ok(outcome) => outcome.report(),
        Err(failure) => failure.report(),
    }
}
