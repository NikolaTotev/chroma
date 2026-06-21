//! The capture error type.

use std::fmt;

/// Result alias for capture operations.
pub type Result<T> = core::result::Result<T, CaptureError>;

/// Why a capture operation failed.
///
/// A small hand-rolled enum to keep the contract crate dependency-free
/// (see `DECISIONS.md`). Backends map their platform errors into these variants
/// so consumers handle a stable set regardless of X11/Wayland.
#[derive(Debug)]
pub enum CaptureError {
    /// The requested backend or target is not available on this host (e.g.
    /// Wayland session, or a window that closed). Consumers should report it
    /// clearly and fall back where possible (spec CAP-07).
    Unavailable(String),
    /// Required screen/input permission was denied (spec §3.5).
    PermissionDenied(String),
    /// The capture device produced no frame within the expected interval.
    Timeout,
    /// A backend-specific failure with a human-readable description.
    Backend(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Unavailable(what) => write!(f, "capture unavailable: {what}"),
            CaptureError::PermissionDenied(what) => write!(f, "permission denied: {what}"),
            CaptureError::Timeout => write!(f, "timed out waiting for a frame"),
            CaptureError::Backend(msg) => write!(f, "capture backend error: {msg}"),
        }
    }
}

impl std::error::Error for CaptureError {}
