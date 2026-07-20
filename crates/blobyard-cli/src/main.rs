//! Blobyard command-line binary entry point.

use blobyard_cli::{Cli, run_cli, write_output};
use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let output = run_cli(Cli::parse()).await;
    let code = write_output(&output, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(code)
}
