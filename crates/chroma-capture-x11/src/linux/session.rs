//! The runtime capture-session factory.

use super::{X11EventSource, X11ScreenCapturer};
use crate::clock::MonotonicClock;
use chroma_capture_api::Result;

/// A wired X11 capture session: a screen capturer, an event source, and the
/// clock they share.
///
/// The app holds one of these for the duration of a recording. The capturer and
/// event source own independent X connections but stamp on the same
/// [`MonotonicClock`] origin, keeping the two streams on one timebase
/// (spec CAP-05).
pub struct X11Session {
    pub capturer: X11ScreenCapturer,
    pub events: X11EventSource,
    pub clock: MonotonicClock,
}

/// Opens a complete X11 capture session.
///
/// This is the runtime factory the app uses to select the X11 backend (spec
/// §4.3). It fails with [`chroma_capture_api::CaptureError::Unavailable`] if no
/// X server is reachable, letting the caller try another backend.
pub fn open_session() -> Result<X11Session> {
    Ok(X11Session {
        capturer: X11ScreenCapturer::new()?,
        events: X11EventSource::new()?,
        clock: MonotonicClock::new(),
    })
}
