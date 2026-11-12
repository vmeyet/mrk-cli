#![cfg(all(unix, feature = "cli"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `mrk update` against a fake `git` and a fake `cargo`: nothing reaches the network and nothing builds.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};
use tempfile::TempDir;

const MRK: &str = env!("CARGO_BIN_EXE_mrk");
const PATIENCE: Duration = Duration::from_secs(10);
/// A release newer than any mrk, at a full commit id.
const GIT: &str = "#!/bin/sh\nprintf '2222222222222222222222222222222222222222\\trefs/tags/v99.0.0\\n'\n";
/// Marks that it runs, then fails once the test lets it go on.
const CARGO: &str = "#!/bin/sh\ntouch \"$MRK_TEST_MARKS/started\"\nwhile [ ! -e \"$MRK_TEST_MARKS/go\" ]; do sleep 0.05; done\nexit 1\n";

/// A folder holding the fake programs, the marks the fake `cargo` leaves, and the root it would install under.
struct Fakes(TempDir);

impl Fakes {
    fn new() -> Self {
        let fakes = Self(tempfile::tempdir().unwrap());
        fakes.write_program("git", GIT);
        fakes.write_program("cargo", CARGO);
        fakes
    }

    fn write_program(&self, name: &str, script: &str) {
        let path = self.0.path().join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn mark(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    /// `words` then `update`, with the fakes first in `PATH`.
    fn update(&self, words: &[&str]) -> Child {
        let (program, arguments) = words.split_first().unwrap();
        Command::new(program)
            .args(arguments)
            .arg("update")
            .env("PATH", format!("{}:/usr/bin:/bin", self.0.path().display()))
            .env("CARGO_INSTALL_ROOT", self.0.path().join("root"))
            .env("MRK_TEST_MARKS", self.0.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn wait_for_the_build(&self) {
        assert!(appears(&self.mark("started")), "the fake cargo never ran");
    }

    fn end_the_build(&self) {
        fs::write(self.mark("go"), "").unwrap();
    }
}

fn appears(path: &Path) -> bool {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn send(child: &Child, signal: Signal) {
    kill_process(Pid::from_child(child), signal).unwrap();
}

fn stderr_of(child: Child) -> (ExitStatus, String) {
    let output = child.wait_with_output().unwrap();
    (output.status, String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn under_nohup_a_closed_terminal_leaves_the_build_running() {
    let fakes = Fakes::new();
    let update = fakes.update(&["nohup", MRK]);
    fakes.wait_for_the_build();

    send(&update, Signal::HUP);
    fakes.end_the_build();

    let (status, stderr) = stderr_of(update);
    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("cargo install failed"), "{stderr}");
}

#[test]
fn a_signal_ends_mrk_with_its_code_even_when_stdout_is_gone() {
    let fakes = Fakes::new();
    let mut update = fakes.update(&[MRK]);
    fakes.wait_for_the_build();

    drop(update.stdout.take());
    send(&update, Signal::TERM);

    let (status, stderr) = stderr_of(update);
    assert_eq!(status.code(), Some(128 + Signal::TERM.as_raw()), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
