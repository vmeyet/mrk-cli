//! Signals that would end mrk, held off while a child program runs, so mrk outlives the child and cleans up after it.
use std::fmt;
use std::io;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
use signal_hook::SigId;
use signal_hook::flag;
use signal_hook::low_level;

/// How often a wait checks whether a held signal arrived or the child is gone.
const POLL: Duration = Duration::from_millis(50);
/// How long the child's process group has to end after `SIGTERM` before it gets `SIGKILL`.
const GRACE: Duration = Duration::from_secs(5);
/// No signal arrived.
const NONE: usize = 0;

/// Per signal, the flag of an action that ends mrk as the default disposition would, armed whenever no `Held` holds
/// that signal: signal-hook cannot give the default disposition back once it handled a signal.
static ENDING: Mutex<Vec<(i32, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// mrk stopped what it ran because a signal asked it to end: it exits as that signal would, `128 + signal`, with no
/// report of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interrupted(pub i32);

impl fmt::Display for Interrupted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "stopped by signal {}", self.0)
    }
}

impl std::error::Error for Interrupted {}

/// The signals held until dropped; from then on they end mrk again.
pub struct Held {
    arrived: Arc<AtomicUsize>,
    catchers: Vec<SigId>,
    endings: Vec<Arc<AtomicBool>>,
}

impl Held {
    pub fn hold(signals: &[i32]) -> io::Result<Self> {
        let mut held = Self { arrived: Arc::new(AtomicUsize::new(NONE)), catchers: Vec::new(), endings: Vec::new() };
        for &signal in signals {
            let ending = ending(signal)?;
            ending.store(false, Ordering::SeqCst);
            held.endings.push(ending);
            let number = usize::try_from(signal).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            held.catchers.push(flag::register_usize(signal, Arc::clone(&held.arrived), number)?);
        }
        Ok(held)
    }

    /// The last held signal that arrived.
    pub fn arrived(&self) -> Option<Interrupted> {
        match self.arrived.load(Ordering::SeqCst) {
            NONE => None,
            signal => i32::try_from(signal).ok().map(Interrupted),
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.catchers.iter().for_each(|catcher| {
            low_level::unregister(*catcher);
        });
        self.endings.iter().for_each(|ending| ending.store(true, Ordering::SeqCst));
    }
}

fn ending(signal: i32) -> io::Result<Arc<AtomicBool>> {
    let mut endings = ENDING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, ending)) = endings.iter().find(|(known, _)| *known == signal) {
        return Ok(Arc::clone(ending));
    }
    let ending = Arc::new(AtomicBool::new(true));
    flag::register_conditional_default(signal, Arc::clone(&ending))?;
    endings.push((signal, Arc::clone(&ending)));
    Ok(ending)
}

/// `command` in a process group of its own, so the terminal's Ctrl-C reaches mrk alone and mrk ends the whole build.
pub fn spawn_alone(command: &mut Command) -> io::Result<Child> {
    use std::os::unix::process::CommandExt;
    command.process_group(0).spawn()
}

/// Waits for a child `spawn_alone` started; once a held signal arrives, ends its whole process group, `SIGTERM` then
/// `SIGKILL` after `GRACE`, and waits until no process of it is left, so nothing it started still writes afterwards.
pub fn wait_ending(child: &mut Child, held: &Held) -> io::Result<Result<ExitStatus, Interrupted>> {
    let group = Pid::from_child(child);
    loop {
        if let Some(interrupted) = held.arrived() {
            end_group(child, group)?;
            return Ok(Err(interrupted));
        }
        if let Some(status) = child.try_wait()? {
            return Ok(Ok(status));
        }
        thread::sleep(POLL);
    }
}

fn end_group(child: &mut Child, group: Pid) -> io::Result<()> {
    let _ = kill_process_group(group, Signal::TERM);
    let given_up = Instant::now() + GRACE;
    while is_alive(group) && Instant::now() < given_up {
        let _ = child.try_wait()?;
        thread::sleep(POLL);
    }
    let _ = kill_process_group(group, Signal::KILL);
    child.wait()?;
    while is_alive(group) {
        thread::sleep(POLL);
    }
    Ok(())
}

fn is_alive(group: Pid) -> bool {
    test_kill_process_group(group).is_ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use signal_hook::consts::{SIGUSR1, SIGUSR2};

    use super::*;

    #[test]
    fn a_held_signal_is_noted_instead_of_ending_mrk_however_often_it_is_held() {
        for _ in 0..2 {
            let held = Held::hold(&[SIGUSR1]).unwrap();
            assert_eq!(held.arrived(), None);

            low_level::raise(SIGUSR1).unwrap();

            assert_eq!(held.arrived(), Some(Interrupted(SIGUSR1)));
        }
    }

    #[test]
    fn the_whole_process_group_is_ended_once_a_held_signal_arrives() {
        let held = Held::hold(&[SIGUSR2]).unwrap();
        let mut shell = spawn_alone(Command::new("sh").args(["-c", "sleep 30 & sleep 30"])).unwrap();
        let group = Pid::from_child(&shell);

        low_level::raise(SIGUSR2).unwrap();

        assert_eq!(wait_ending(&mut shell, &held).unwrap(), Err(Interrupted(SIGUSR2)));
        assert!(!is_alive(group));
    }
}
