#![forbid(unsafe_code)]

mod cli;
mod format;
mod tui;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    match cli::Cli::try_parse() {
        Ok(parsed) => cli::run(parsed),
        Err(error) => {
            let code = if error.use_stderr() { 1 } else { 0 };
            let _ = error.print();
            ExitCode::from(code)
        }
    }
}
