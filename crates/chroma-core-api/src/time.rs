//! The project timebase.

use serde::{Deserialize, Serialize};

/// A point in time on Chroma's single monotonic clock, in nanoseconds.
///
/// Capture video frames and input events are stamped on the same `TimeStamp`
/// origin so any event maps to an exact video frame within ±1 frame
/// (spec CAP-05, `ORCHESTRATION.md` "One clock"). It is *not* a wall-clock
/// time and carries no calendar meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimeStamp(pub u64);

impl TimeStamp {
    /// The clock origin (zero nanoseconds).
    pub const ZERO: TimeStamp = TimeStamp(0);

    /// Constructs a timestamp from a nanosecond count.
    pub const fn from_nanos(nanos: u64) -> Self {
        TimeStamp(nanos)
    }

    /// The raw nanosecond count since the clock origin.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// This instant in seconds since the clock origin, as a float for
    /// timeline/UI math. Lossy for very large values; never use it for
    /// frame-exact comparisons.
    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / 1_000_000_000.0
    }
}

/// A half-open span `[start, end)` on the project timebase.
///
/// Every [`crate::Modifier`] occupies one `TimeRange` (spec EDT-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// First instant the range covers (inclusive).
    pub start: TimeStamp,
    /// First instant past the range (exclusive).
    pub end: TimeStamp,
}

impl TimeRange {
    /// Builds a range from two timestamps. `start` should be `<= end`; an
    /// inverted range simply [`contains`](Self::contains) nothing.
    pub const fn new(start: TimeStamp, end: TimeStamp) -> Self {
        TimeRange { start, end }
    }

    /// Whether `t` falls in `[start, end)`.
    pub fn contains(&self, t: TimeStamp) -> bool {
        self.start <= t && t < self.end
    }

    /// Range length in nanoseconds, saturating to zero when inverted.
    pub fn duration_nanos(&self) -> u64 {
        self.end.0.saturating_sub(self.start.0)
    }
}
