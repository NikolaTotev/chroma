//! The screen-capture contract.

use crate::error::Result;
use crate::frame::Frame;

/// What region of the desktop to capture (spec CAP-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    /// An entire output/monitor by index.
    FullScreen { monitor: u32 },
    /// A single window by its platform window id.
    Window { id: u64 },
    /// A user-drawn region in desktop pixel coordinates.
    Region {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

/// Acquires screen frames from the host.
///
/// Implemented by `chroma-capture-x11` (XSHM/XComposite) and
/// `chroma-capture-wayland` (PipeWire portal). Every [`Frame`] returned by
/// [`next_frame`](Self::next_frame) is stamped on the same
/// [`Clock`](crate::Clock) as the paired [`EventSource`](crate::EventSource).
pub trait ScreenCapturer {
    /// Begins capturing `target` at `fps` frames per second. Must be called
    /// before [`next_frame`](Self::next_frame).
    fn start(&mut self, target: CaptureTarget, fps: u32) -> Result<()>;

    /// Blocks until the next frame is available and returns it. Dropped frames
    /// are logged by the backend (spec CAP-06) but do not error here.
    fn next_frame(&mut self) -> Result<Frame>;

    /// Stops capturing and releases host resources. Idempotent.
    fn stop(&mut self) -> Result<()>;
}
