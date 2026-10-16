use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use super::DiagramError;
use super::catch::caught;

/// The renderers recurse over the diagram; a stack this deep outlasts any nesting the budget lets through.
const STACK_BYTES: usize = 64 * 1024 * 1024;

/// `render` on a thread of its own, its panics caught, given up at `deadline`.
/// A renderer cannot be stopped, so a thread given up on runs to its end unseen: the result is dropped, and a one-shot
/// `mrk` exits without waiting for it.
pub fn within<T: Send + 'static>(
    deadline: Instant,
    render: impl FnOnce() -> Result<T, DiagramError> + Send + 'static,
) -> Result<T, DiagramError> {
    if Instant::now() >= deadline {
        return Err(DiagramError::TimedOut);
    }
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("mermaid".to_owned())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let _ = sender.send(caught(render).and_then(|rendered| rendered));
        })
        .map_err(|_| DiagramError::Crashed)?;
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(rendered) => rendered,
        Err(RecvTimeoutError::Timeout) => Err(DiagramError::TimedOut),
        Err(RecvTimeoutError::Disconnected) => Err(DiagramError::Crashed),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    fn ample() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn a_render_that_finishes_gives_its_result() {
        assert_eq!(within(ample(), || Ok(7)), Ok(7));
        assert_eq!(within(ample(), || Err::<(), _>(DiagramError::Empty)), Err(DiagramError::Empty));
    }

    #[test]
    fn a_render_past_the_deadline_is_given_up() {
        let (release, blocked) = mpsc::channel::<()>();

        let soon = Instant::now() + Duration::from_millis(10);

        assert_eq!(within(soon, move || blocked.recv().map_err(|_| DiagramError::Empty)), Err(DiagramError::TimedOut));
        drop(release);
    }

    #[test]
    fn a_deadline_already_past_starts_nothing() {
        assert_eq!(within(Instant::now(), || -> Result<(), _> { unreachable!("started") }), Err(DiagramError::TimedOut));
    }

    #[test]
    fn a_panicking_render_is_a_crash() {
        assert_eq!(within(ample(), || -> Result<(), _> { panic!("boom") }), Err(DiagramError::Crashed));
    }
}
