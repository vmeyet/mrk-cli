mod cli;
mod config;
mod update;
mod version;

use std::io::IsTerminal;
use std::process::ExitCode;

use cli::UsageError;
use cli::signals::Interrupted;

const USAGE_EXIT: u8 = 2;

fn main() -> ExitCode {
    cli::panic::install_hook();
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(Interrupted(signal)) = error.downcast_ref::<Interrupted>() {
                return ExitCode::from(u8::try_from(128 + signal).unwrap_or(u8::MAX));
            }
            eprint!("{}", mrk::terminal::error_report(&error, std::io::stderr().is_terminal()));
            if error.is::<UsageError>() { ExitCode::from(USAGE_EXIT) } else { ExitCode::FAILURE }
        }
    }
}
