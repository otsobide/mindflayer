use std::process::ExitCode;

use clap::Parser;
use mindflayer_cli::{run_watched, Cli, ProgressLine};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let progress = ProgressLine::on_stderr();
    let result = run_watched(&cli, &progress.watch());
    progress.clear();
    match result {
        Ok(outcome) => outcome.report(),
        Err(failure) => failure.report(),
    }
}
