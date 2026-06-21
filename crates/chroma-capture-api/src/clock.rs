//! The shared capture timebase.

use chroma_core_api::TimeStamp;

/// The single monotonic clock that stamps both video frames and input events.
///
/// A capture session creates one `Clock` and shares it between its
/// [`ScreenCapturer`](crate::ScreenCapturer) and
/// [`EventSource`](crate::EventSource) so the two streams are on one timebase
/// (spec CAP-05, `ORCHESTRATION.md` "One clock"). Implementations wrap a
/// monotonic OS source (e.g. `CLOCK_MONOTONIC`); the value is nanoseconds since
/// an arbitrary, fixed session origin — never wall-clock.
pub trait Clock {
    /// The current instant on this clock.
    fn now(&self) -> TimeStamp;
}
