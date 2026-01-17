mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run() -> Result<(), ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Plan(_) | Commands::Apply(_) | Commands::Serve(_) => {
            eprintln!("atomc: command handling not yet implemented");
            Err(ExitCode::from(2))
        }
    }
}
