use std::io;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level;

/// The signals that end mrk by default, as a parent program closing it sends them.
const ENDING: [i32; 4] = [SIGHUP, SIGINT, SIGQUIT, SIGTERM];
/// How long the terminal may take to be given back in order, a write in progress ending and the output draining,
/// before it is forced back: a terminal that stopped reading would otherwise hold mrk forever.
const PATIENCE: Duration = Duration::from_secs(1);

/// From now on, an ending signal runs `give_back`, or `force_back` when that takes longer than `PATIENCE`, then ends
/// mrk as that signal would have. The signal thread waits for nothing else the pager does: one that arrives while a new
/// width renders ends mrk at once.
pub fn on_ending(give_back: impl Fn() + Send + Sync + 'static, force_back: impl Fn() + Send + 'static) -> io::Result<()> {
    let mut signals = Signals::new(ENDING)?;
    let give_back = Arc::new(give_back);
    thread::spawn(move || {
        for signal in signals.forever() {
            if !ends_within(PATIENCE, Arc::clone(&give_back)) {
                force_back();
            }
            let _ = low_level::emulate_default_handler(signal);
        }
    });
    Ok(())
}

/// Whether `work`, run on a thread of its own, ends within `patience`; it is left running when it does not.
fn ends_within(patience: Duration, work: Arc<impl Fn() + Send + Sync + 'static>) -> bool {
    let (done, ended) = mpsc::channel();
    thread::spawn(move || {
        work();
        let _ = done.send(());
    });
    ended.recv_timeout(patience).is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn work_that_ends_is_waited_for_and_work_that_hangs_is_given_up() {
        let (_never_sent, never) = mpsc::channel::<()>();
        let never = Mutex::new(never);

        assert!(ends_within(Duration::from_secs(60), Arc::new(|| ())));
        assert!(!ends_within(Duration::from_millis(10), Arc::new(move || drop(never.lock().map(|never| never.recv())))));
    }
}
