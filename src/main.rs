use std::process::ExitCode;

fn main() -> ExitCode {
    match mrk::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("✗ {error}");
            for cause in error.chain().skip(1) {
                eprintln!("  \x1b[2m{cause}\x1b[0m");
            }
            ExitCode::FAILURE
        }
    }
}
