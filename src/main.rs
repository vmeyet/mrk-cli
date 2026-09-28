use std::io::IsTerminal;
use std::process::ExitCode;

use mrk::cli::UsageError;

const USAGE_EXIT: u8 = 2;

fn main() -> ExitCode {
    match mrk::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", mrk::terminal::error_report(&error, std::io::stderr().is_terminal()));
            if error.is::<UsageError>() { ExitCode::from(USAGE_EXIT) } else { ExitCode::FAILURE }
        }
    }
}
