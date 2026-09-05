use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static JOB_DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub fn run_with_deadline<T>(timeout: Duration, operation: impl FnOnce() -> T) -> T {
    JOB_DEADLINE.with(|deadline| {
        let previous = deadline.replace(Some(Instant::now() + timeout));
        let result = operation();
        deadline.set(previous);
        result
    })
}

pub fn remaining() -> Option<Duration> {
    JOB_DEADLINE.with(|deadline| {
        deadline
            .get()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    })
}

pub fn exceeded() -> bool {
    remaining().is_some_and(|remaining| remaining.is_zero())
}
