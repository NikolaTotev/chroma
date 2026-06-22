//! The shared monotonic capture clock (Wayland backend).
//!
//! A process-global monotonic origin, identical in contract to the X11
//! backend's clock: frames decoded from the PipeWire stream and input events
//! from the portal are stamped against the same timebase (spec CAP-05).

use chroma_capture_api::Clock;
use chroma_core_api::TimeStamp;
use std::sync::OnceLock;
use std::time::Instant;

static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// A monotonic clock measuring nanoseconds since a fixed, process-global origin.
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
