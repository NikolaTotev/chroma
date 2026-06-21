//! Linux X11 implementation of the capture contracts.

mod capturer;
mod events;
mod session;

pub use capturer::X11ScreenCapturer;
pub use events::X11EventSource;
pub use session::{open_session, X11Session};

use chroma_capture_api::CaptureError;

/// Maps any X11 error into a [`CaptureError::Backend`].
pub(crate) fn be(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Backend(e.to_string())
}
