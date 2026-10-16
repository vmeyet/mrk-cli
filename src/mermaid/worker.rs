use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use super::DiagramError;
use super::catch::caught;

/// The renderers recurse over the diagram; a stack this deep outlasts any nesting the budget lets through.
const STACK_BYTES: usize = 64 * 1024 * 1024;

/// `render` with its panics caught: on this thread without a deadline, else on a thread of its own given up at it,
/// or never started once it has passed.
/// A renderer cannot be stopped, so a thread given up on runs to its end unseen and its result is dropped.
pub fn run<T: Send + 'static>(
    deadline: Option<Instant>,
    render: impl FnOnce() -> Result<T, DiagramError> + Send + 'static,
) -> Result<T, DiagramError> {
    match deadline {
        None => caught(render)?,
        Some(deadline) => within(deadline, render),
    }
}

fn within<T: Send + 'static>(
    deadline: Instant,
    render: impl FnOnce() -> Result<T, DiagramError> + Send + 'static,
) -> Result<T, DiagramError> {
    if Instant::now() >= deadline {
        return Err(DiagramError::OutOfTime);
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
    fn a_render_that_finishes_gives_its_result_with_or_without_a_deadline() {
        for deadline in [None, Some(ample())] {
            assert_eq!(run(deadline, || Ok(7)), Ok(7));
            assert_eq!(run(deadline, || Err::<(), _>(DiagramError::Empty)), Err(DiagramError::Empty));
        }
    }

    #[test]
    fn without_a_deadline_the_render_runs_on_the_calling_thread() {
        let caller = thread::current().id();

        assert_eq!(run(None, move || Ok(thread::current().id() == caller)), Ok(true));
        assert_eq!(run(Some(ample()), move || Ok(thread::current().id() == caller)), Ok(false));
    }

    #[test]
    fn a_render_past_the_deadline_is_given_up() {
        let (release, blocked) = mpsc::channel::<()>();

        let soon = Some(Instant::now() + Duration::from_millis(10));

        assert_eq!(run(soon, move || blocked.recv().map_err(|_| DiagramError::Empty)), Err(DiagramError::TimedOut));
        drop(release);
    }

    #[test]
    fn a_deadline_already_past_starts_nothing() {
        assert_eq!(run(Some(Instant::now()), || -> Result<(), _> { unreachable!("started") }), Err(DiagramError::OutOfTime));
    }

    #[test]
    fn a_panicking_render_is_a_crash_with_or_without_a_deadline() {
        for deadline in [None, Some(ample())] {
            assert_eq!(run(deadline, || -> Result<(), _> { panic!("boom") }), Err(DiagramError::Crashed));
        }
    }
}
