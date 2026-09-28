use std::process::ExitCode;

use mrk::cli::UsageError;

const USAGE_EXIT: u8 = 2;

fn main() -> ExitCode {
    match mrk::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("✗ {error}");
            for cause in error.chain().skip(1) {
                eprintln!("  \x1b[2m{cause}\x1b[0m");
            }
            if error.is::<UsageError>() { ExitCode::from(USAGE_EXIT) } else { ExitCode::FAILURE }
        }
    }
}
