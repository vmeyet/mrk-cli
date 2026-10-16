mod cli;
mod config;
mod update;

use std::io::IsTerminal;
use std::process::ExitCode;

use cli::UsageError;

const USAGE_EXIT: u8 = 2;

fn main() -> ExitCode {
    cli::panic::install_hook();
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", mrk::terminal::error_report(&error, std::io::stderr().is_terminal()));
            if error.is::<UsageError>() { ExitCode::from(USAGE_EXIT) } else { ExitCode::FAILURE }
        }
    }
}
