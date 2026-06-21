//! The media error type.

use std::fmt;

/// Result alias for media operations.
pub type Result<T> = core::result::Result<T, MediaError>;

/// Why a decode/encode/mux operation failed.
///
/// Hand-rolled to keep the contract crate dependency-free (see `DECISIONS.md`).
#[derive(Debug)]
pub enum MediaError {
    /// The requested codec/container/encoder is not supported on this host
    /// (e.g. NVENC absent). Consumers fall back to software where possible
    /// (spec EXP-08).
    Unsupported(String),
    /// The input or output path could not be opened.
    Io(String),
    /// The supplied [`crate::OutputSpec`] is invalid (e.g. zero fps).
    InvalidSpec(String),
    /// The export was cancelled by the caller (spec EXP-07).
    Cancelled,
    /// A backend-specific failure with a human-readable description.
    Backend(String),
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaError::Unsupported(what) => write!(f, "unsupported: {what}"),
            MediaError::Io(msg) => write!(f, "media io error: {msg}"),
            MediaError::InvalidSpec(msg) => write!(f, "invalid output spec: {msg}"),
            MediaError::Cancelled => write!(f, "export cancelled"),
            MediaError::Backend(msg) => write!(f, "media backend error: {msg}"),
        }
    }
}

impl std::error::Error for MediaError {}
