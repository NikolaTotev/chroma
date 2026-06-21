//! Chroma X11 capture backend (M1).
//!
//! Implements the [`chroma_capture_api`] contracts on X11:
//!
//! - [`X11ScreenCapturer`] grabs frames with the core protocol's `GetImage`
//!   (ZPixmap), from the root window (full screen / region) or a window id.
//! - [`X11EventSource`] receives global pointer, scroll, button, and keystroke
//!   events via XInput2 *raw* events selected on the root window (spec
//!   CAP-02/03/04).
//! - [`MonotonicClock`] stamps both streams from one process-global monotonic
//!   origin, so any event maps to a frame on the same timebase (spec CAP-05,
//!   `ORCHESTRATION.md` "One clock").
//!
//! [`open_session`] is the runtime factory that wires the three together — the
//! app selects a backend this way, never via a class hierarchy (spec §4.3).
//!
//! # Platform
//!
//! The real implementation is Linux-only. On other targets every type still
//! exists but reports [`chroma_capture_api::CaptureError::Unavailable`], so the
//! cross-platform workspace builds on Windows (see `DECISIONS.md`).
//!
//! # Performance note
//!
//! `GetImage` copies each frame over the X socket. For sustained HiDPI capture
//! the spec recommends MIT-SHM (XSHM, zero-copy); that is a planned internal
//! optimization behind this same contract and does not change the public API.

mod clock;

pub use clock::MonotonicClock;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{open_session, X11EventSource, X11ScreenCapturer, X11Session};

#[cfg(not(target_os = "linux"))]
mod stub;
#[cfg(not(target_os = "linux"))]
pub use stub::{open_session, X11EventSource, X11ScreenCapturer, X11Session};
