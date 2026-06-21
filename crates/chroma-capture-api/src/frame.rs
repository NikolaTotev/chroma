//! Captured video frame value types.

use chroma_core_api::{Size, TimeStamp};

/// The pixel layout of a captured [`Frame`]'s buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8 bits per channel, byte order B, G, R, A. Common on X11 (XSHM).
    Bgra8,
    /// 8 bits per channel, byte order R, G, B, A.
    Rgba8,
}

impl PixelFormat {
    /// Bytes per pixel for this format.
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
        }
    }
}

/// One captured screen frame, stamped on the session [`Clock`](crate::Clock).
///
/// The buffer is owned CPU-side pixel data here; a zero-copy GPU path
/// (DMA-BUF, spec §3.1) is a backend optimization expressed behind the same
/// trait and is **not** part of this contract's first cut. `stride` accounts
/// for row padding so `width * bytes_per_pixel <= stride`.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Frame dimensions in pixels.
    pub size: Size,
    /// Row stride in bytes (>= `size.width * format.bytes_per_pixel()`).
    pub stride: usize,
    /// Pixel layout of [`data`](Self::data).
    pub format: PixelFormat,
    /// Capture instant on the shared clock.
    pub timestamp: TimeStamp,
    /// Raw pixel bytes, `stride * size.height` long.
    pub data: Vec<u8>,
}
