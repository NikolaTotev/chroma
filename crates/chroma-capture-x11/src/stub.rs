//! Non-Linux stub.
//!
//! Keeps the public API present so the cross-platform workspace builds on
//! Windows; every operation reports
//! [`CaptureError::Unavailable`](chroma_capture_api::CaptureError::Unavailable).
//! The real backend lives in `src/linux/` (see `DECISIONS.md`).

use chroma_capture_api::{
    CaptureError, CaptureTarget, EventSource, Frame, Result, ScreenCapturer, TimedInputEvent,
};

use crate::clock::MonotonicClock;

fn unavailable() -> CaptureError {
    CaptureError::Unavailable("X11 capture backend is only available on Linux".to_owned())
}

/// Stub screen capturer; see the Linux module for the real implementation.
pub struct X11ScreenCapturer;

impl X11ScreenCapturer {
    /// Always fails off Linux with [`CaptureError::Unavailable`].
    pub fn new() -> Result<Self> {
        Err(unavailable())
    }
}

impl ScreenCapturer for X11ScreenCapturer {
    fn start(&mut self, _target: CaptureTarget, _fps: u32) -> Result<()> {
        Err(unavailable())
    }

    fn next_frame(&mut self) -> Result<Frame> {
        Err(unavailable())
    }

    fn stop(&mut self) -> Result<()> {
        Err(unavailable())
    }
}

/// Stub event source; see the Linux module for the real implementation.
pub struct X11EventSource;

impl X11EventSource {
    /// Always fails off Linux with [`CaptureError::Unavailable`].
    pub fn new() -> Result<Self> {
        Err(unavailable())
    }
}

impl EventSource for X11EventSource {
    fn poll(&mut self) -> Vec<TimedInputEvent> {
        Vec::new()
    }
}

/// Stub session bundle, mirroring the Linux [`crate::X11Session`] shape.
pub struct X11Session {
    pub capturer: X11ScreenCapturer,
    pub events: X11EventSource,
    pub clock: MonotonicClock,
}

/// Always fails off Linux with [`CaptureError::Unavailable`].
pub fn open_session() -> Result<X11Session> {
    Err(unavailable())
}
