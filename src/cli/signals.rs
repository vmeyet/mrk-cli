//! Signals that would end mrk, held off while a child program runs, so mrk outlives the child and cleans up after it.
use std::io;
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use signal_hook::SigId;
use signal_hook::flag;
use signal_hook::low_level;

/// How often `wait_ending` checks whether a held signal arrived.
const POLL: Duration = Duration::from_millis(50);

/// Per signal, the flag of an action that ends mrk as the default disposition would, armed whenever no `Held` holds
/// that signal: signal-hook cannot give the default disposition back once it handled a signal.
static ENDING: Mutex<Vec<(i32, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// The signals held until dropped; from then on they end mrk again.
pub struct Held {
    arrived: Arc<AtomicBool>,
    catchers: Vec<SigId>,
    endings: Vec<Arc<AtomicBool>>,
}

impl Held {
    pub fn hold(signals: &[i32]) -> io::Result<Self> {
        let mut held = Self { arrived: Arc::default(), catchers: Vec::new(), endings: Vec::new() };
        for &signal in signals {
            let ending = ending(signal)?;
            ending.store(false, Ordering::SeqCst);
            held.endings.push(ending);
            held.catchers.push(flag::register(signal, Arc::clone(&held.arrived))?);
        }
        Ok(held)
    }

    pub fn arrived(&self) -> bool {
        self.arrived.load(Ordering::SeqCst)
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

/// Waits for `child`, ending it once a held signal arrives.
pub fn wait_ending(child: &mut Child, held: &Held) -> io::Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if held.arrived() {
            child.kill()?;
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::process::Command;

    use signal_hook::consts::{SIGUSR1, SIGUSR2};

    use super::*;

    #[test]
    fn a_held_signal_is_noted_instead_of_ending_mrk_however_often_it_is_held() {
        for _ in 0..2 {
            let held = Held::hold(&[SIGUSR1]).unwrap();
            assert!(!held.arrived());

            low_level::raise(SIGUSR1).unwrap();

            assert!(held.arrived());
        }
    }

    #[test]
    fn the_child_is_ended_once_a_held_signal_arrives() {
        let held = Held::hold(&[SIGUSR2]).unwrap();
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();

        low_level::raise(SIGUSR2).unwrap();
        let status = wait_ending(&mut child, &held).unwrap();

        assert!(!status.success());
    }
}
