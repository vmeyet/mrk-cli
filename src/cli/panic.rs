//! The one panic hook: a report quotes whatever panicked, input included, so it is sanitized like any stderr line.
use std::backtrace::{Backtrace, BacktraceStatus};
use std::io::{self, Write};
use std::panic;
use std::thread;

use mrk::{mermaid, sanitize};

/// Replaces the default hook for the whole run; the pager wraps it while it holds the terminal.
pub fn install_hook() {
    panic::set_hook(Box::new(|info| {
        if mermaid::is_catching() {
            return;
        }
        let report = report(thread::current().name(), &info.to_string(), &Backtrace::capture());
        let _ = io::stderr().write_all(report.as_bytes());
    }));
}

/// What the default hook prints, sanitized.
fn report(thread: Option<&str>, panic: &str, backtrace: &Backtrace) -> String {
    let trace = match backtrace.status() {
        BacktraceStatus::Captured => format!("stack backtrace:\n{backtrace}"),
        _ => "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n".to_owned(),
    };
    sanitize::lines(&format!("\nthread '{}' {panic}\n{trace}", thread.unwrap_or("<unnamed>")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: &str = "panicked at src/text.rs:1:1:\nbyte index 3 is not a char boundary of `\u{1b}]52;c;aGk=\u{7}`";

    #[test]
    fn a_report_reads_like_the_default_one_without_the_input_escapes() {
        let report = report(Some("main"), HOSTILE, &Backtrace::disabled());

        assert!(report.starts_with("\nthread 'main' panicked at src/text.rs:1:1:\nbyte index 3"), "{report:?}");
        assert!(report.ends_with("to display a backtrace\n"), "{report:?}");
        assert!(!report.contains(['\u{1b}', '\u{7}']), "{report:?}");
    }
}
