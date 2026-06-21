//! The shared monotonic capture clock.

use chroma_capture_api::Clock;
use chroma_core_api::TimeStamp;
use std::sync::OnceLock;
use std::time::Instant;

/// The single time origin for the whole process. Both the capturer and the
/// event source read from it, so they share one timebase without sharing a
/// clock instance (spec CAP-05).
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// A monotonic clock measuring nanoseconds since a fixed, process-global origin.
///
/// Implemented on `std::time::Instant` (monotonic, no `libc` needed). Every
/// `MonotonicClock` reads the same [`ORIGIN`], so frames and events stamped by
/// different instances are directly comparable.
#[derive(Debug, Clone, Copy, Default)]
pub struct MonotonicClock;

impl MonotonicClock {
    /// Returns a clock, initializing the process origin on first use.
    pub fn new() -> Self {
        let _ = ORIGIN.get_or_init(Instant::now);
        MonotonicClock
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> TimeStamp {
        let origin = ORIGIN.get_or_init(Instant::now);
        TimeStamp::from_nanos(origin.elapsed().as_nanos() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic() {
        let clock = MonotonicClock::new();
        let a = clock.now();
        let b = clock.now();
        assert!(b >= a, "clock went backwards: {a:?} then {b:?}");
    }
}
