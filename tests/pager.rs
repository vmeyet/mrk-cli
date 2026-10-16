#![cfg(all(unix, feature = "cli"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags};
use rustix::process::{Pid, Signal, ioctl_tiocsctty, kill_process, setsid};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, Winsize, tcgetattr, tcsetwinsize};

/// Names the pty the test binary, run again as `launcher`, takes as its controlling terminal before it runs mrk.
const PTY: &str = "MRK_TEST_PTY";
/// Marks the lines the launcher reports on, among the test runner's.
const REPORT: &str = "launcher: ";
const DOCUMENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/showcase.md");
const FRAME_END: &[u8] = b"\x1b[?2026l";
const ALTERNATE_SCREEN_LEFT: &[u8] = b"\x1b[?1049l";
const PATIENCE: Duration = Duration::from_secs(10);

fn open(path: impl rustix::path::Arg) -> File {
    File::from(rustix::fs::open(path, OFlags::RDWR | OFlags::NOCTTY, Mode::empty()).unwrap())
}

fn is_cooked(tty: &File) -> bool {
    tcgetattr(tty).unwrap().local_modes.contains(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG)
}

/// Not a check: the session leader `Pager::start` runs, which only acts with `MRK_TEST_PTY` set. It stays alive while
/// mrk runs, since a session leader that ends takes the tty from every process, then reports how mrk ended and the tty
/// modes mrk left.
#[test]
fn launcher() {
    let Some(pty) = env::var_os(PTY) else { return };
    setsid().unwrap();
    let tty = open(pty);
    ioctl_tiocsctty(&tty).unwrap();
    let mut mrk = Command::new(env!("CARGO_BIN_EXE_mrk"))
        .args(["-p", DOCUMENT])
        .env_remove(PTY)
        .env_remove("TMUX")
        .env_remove("XTERM_VERSION")
        .env("TERM", "xterm-256color")
        .env("TERM_PROGRAM", "")
        .env("XDG_CONFIG_HOME", env!("CARGO_TARGET_TMPDIR"))
        .env("HOME", env!("CARGO_TARGET_TMPDIR"))
        .stdin(tty.try_clone().unwrap())
        .stdout(tty.try_clone().unwrap())
        .stderr(tty.try_clone().unwrap())
        .spawn()
        .unwrap();
    println!("{REPORT}pid {}", mrk.id());
    let status = mrk.wait().unwrap();
    let ended = status.code().map_or_else(|| format!("signal {}", status.signal().unwrap_or_default()), |code| format!("code {code}"));
    println!("{REPORT}{ended}, cooked {}", is_cooked(&tty));
}

fn position(bytes: &[u8], part: &[u8]) -> Option<usize> {
    (0..=bytes.len().saturating_sub(part.len())).find(|&start| bytes[start..].starts_with(part))
}

/// mrk's pager on a pty of its own, everything it wrote kept.
struct Pager {
    launcher: Child,
    reports: BufReader<ChildStdout>,
    pid: Pid,
    keyboard: File,
    written: Arc<Mutex<Vec<u8>>>,
}

impl Pager {
    fn start() -> Self {
        let pty = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        grantpt(&pty).unwrap();
        unlockpt(&pty).unwrap();
        let path = ptsname(&pty, Vec::new()).unwrap();
        tcsetwinsize(open(path.as_c_str()), Winsize { ws_row: 24, ws_col: 80, ws_xpixel: 800, ws_ypixel: 480 }).unwrap();
        let mut launcher = Command::new(env::current_exe().unwrap())
            .args(["--exact", "launcher", "--nocapture", "--test-threads=1"])
            .env(PTY, path.to_str().unwrap())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reports = BufReader::new(launcher.stdout.take().unwrap());
        let pid = Pid::from_raw(report(&mut reports).strip_prefix("pid ").unwrap().parse().unwrap()).unwrap();
        let mut terminal = File::from(pty);
        let keyboard = terminal.try_clone().unwrap();
        let written = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&written);
        thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(count @ 1..) = terminal.read(&mut buffer) {
                kept.lock().unwrap().extend_from_slice(&buffer[..count]);
            }
        });
        Self { launcher, reports, pid, keyboard, written }
    }

    /// Whether mrk wrote `bytes` after the first `after` it wrote, within `PATIENCE`.
    fn wrote_after(&self, after: &[u8], bytes: &[u8]) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            let written = self.written.lock().unwrap();
            if position(&written, after).is_some_and(|start| position(&written[start..], bytes).is_some()) {
                return true;
            }
            drop(written);
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn wait_for_the_first_frame(&self) {
        assert!(self.wrote_after(b"", FRAME_END), "{:?}", String::from_utf8_lossy(&self.written.lock().unwrap()));
    }

    fn send(&self, signal: Signal) {
        kill_process(self.pid, signal).unwrap();
    }

    fn press(&mut self, keys: &[u8]) {
        self.keyboard.write_all(keys).unwrap();
    }

    /// How mrk ended, and whether the tty was back to reading lines with echo, as a shell needs it.
    fn ending(mut self) -> String {
        let ending = report(&mut self.reports);
        let _ = self.launcher.wait();
        ending
    }
}

/// The next line the launcher reports.
fn report(reports: &mut BufReader<ChildStdout>) -> String {
    let mut line = String::new();
    while reports.read_line(&mut line).unwrap() > 0 {
        if let Some((_, report)) = line.trim_end().split_once(REPORT) {
            return report.to_owned();
        }
        line.clear();
    }
    panic!("the launcher ended without a report");
}

#[test]
fn an_ending_signal_gives_the_terminal_back_then_ends_mrk_as_it_would_have() {
    let pager = Pager::start();
    pager.wait_for_the_first_frame();

    pager.send(Signal::TERM);

    assert!(pager.wrote_after(FRAME_END, ALTERNATE_SCREEN_LEFT));
    assert_eq!(pager.ending(), format!("signal {}, cooked true", Signal::TERM.as_raw()));
}

#[test]
fn a_second_signal_on_the_heels_of_the_first_finds_the_terminal_given_back() {
    let pager = Pager::start();
    pager.wait_for_the_first_frame();

    pager.send(Signal::HUP);
    pager.send(Signal::TERM);

    assert!(pager.wrote_after(FRAME_END, ALTERNATE_SCREEN_LEFT));
    assert_eq!(pager.ending(), format!("signal {}, cooked true", Signal::HUP.as_raw()));
}

#[test]
fn quitting_gives_the_terminal_back() {
    let mut pager = Pager::start();
    pager.wait_for_the_first_frame();

    pager.press(b"q");

    assert!(pager.wrote_after(FRAME_END, ALTERNATE_SCREEN_LEFT));
    assert_eq!(pager.ending(), "code 0, cooked true");
}
