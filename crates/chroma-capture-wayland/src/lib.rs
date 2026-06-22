//! Chroma Wayland capture backend (M7).
//!
//! The Wayland counterpart to `chroma-capture-x11`: it implements the same
//! [`chroma_capture_api`] contracts so the app can pick a backend at runtime
//! (spec §4.3), but sources frames from the **XDG ScreenCast portal + PipeWire**
//! and input from the **RemoteDesktop portal / libei** — the only sanctioned
//! way to read the screen under Wayland's security model. See [`portal`] for the
//! full flow.
//!
//! # Status / platform
//!
//! The portal + PipeWire implementation is gated behind the **`portal`** Cargo
//! feature, which needs a live Wayland session and system libraries
//! (`libpipewire-0.3-dev`) that no CI/WSL environment provides. With the feature
//! off (the default) every operation reports
//! [`CaptureError::Unavailable`](chroma_capture_api::CaptureError::Unavailable),
//! exactly like the X11 backend's non-Linux stub, so the cross-platform
//! workspace still builds and tests. The session shape and target→source
//! mapping ([`portal::source_type_for`]) are real and unit-tested now; the live
//! stream wiring lands with the feature (see `DECISIONS.md`).

mod clock;
pub mod portal;

pub use clock::MonotonicClock;

use chroma_capture_api::{
    CaptureError, CaptureTarget, EventSource, Frame, Result, ScreenCapturer, TimedInputEvent,
};

fn unavailable() -> CaptureError {
    CaptureError::Unavailable(
        "Wayland capture needs the PipeWire portal backend — build with \
         `--features portal` on a live Wayland session (not available here)"
            .to_owned(),
    )
}

/// Screen capturer over the ScreenCast portal + PipeWire.
///
/// Without the `portal` feature, construction and every call report
/// [`CaptureError::Unavailable`].
pub struct WaylandScreenCapturer;

impl WaylandScreenCapturer {
    /// Creates a capturer, or fails if the portal backend is unavailable.
    pub fn new() -> Result<Self> {
        Err(unavailable())
    }
}

impl ScreenCapturer for WaylandScreenCapturer {
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

/// Input event source over the RemoteDesktop portal / libei.
pub struct WaylandEventSource;

impl WaylandEventSource {
    /// Creates an event source, or fails if the portal backend is unavailable.
    pub fn new() -> Result<Self> {
        Err(unavailable())
    }
}

impl EventSource for WaylandEventSource {
    fn poll(&mut self) -> Vec<TimedInputEvent> {
        Vec::new()
    }
}

/// The three capture pieces wired to one clock, mirroring `X11Session`.
pub struct WaylandSession {
    pub capturer: WaylandScreenCapturer,
    pub events: WaylandEventSource,
    pub clock: MonotonicClock,
}

/// Opens a Wayland capture session.
///
/// Reports [`CaptureError::Unavailable`] until the `portal` feature is built on
/// a real Wayland desktop; the app falls back to the X11 backend in the
/// meantime (see `chroma-app`).
pub fn open_session() -> Result<WaylandSession> {
    Err(unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_until_portal_feature() {
        assert!(open_session().is_err());
        assert!(WaylandScreenCapturer::new().is_err());
        // The event source still satisfies the trait (polls empty), so the app
        // can hold one without special-casing the backend.
        let mut ev = WaylandEventSource;
        assert!(ev.poll().is_empty());
    }
}
