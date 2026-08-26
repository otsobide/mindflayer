use std::process::ExitCode;

use clap::Parser;
use mindflayer_cli::{run, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(outcome) => outcome.report(),
        Err(failure) => failure.report(),
    }
}
