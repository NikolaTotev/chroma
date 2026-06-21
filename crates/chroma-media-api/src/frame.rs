//! The pipeline frame value type.

use chroma_core_api::{Size, TimeStamp};

/// A fully-decoded or fully-composited frame in linear RGBA8, the common
/// currency between decode, render, and encode.
///
/// The compositor produces these; encoders consume them. Golden-frame tests
/// compare `RgbaFrame` buffers *before* encoding to dodge encoder
/// nondeterminism (spec §3.3, `ORCHESTRATION.md` §9). The buffer is tightly
/// packed (`width * 4` bytes per row, no padding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    /// Frame dimensions in pixels.
    pub size: Size,
    /// Presentation timestamp on the project timebase.
    pub pts: TimeStamp,
    /// Tightly-packed RGBA8 pixels, `size.width * size.height * 4` bytes.
    pub data: Vec<u8>,
}

impl RgbaFrame {
    /// Expected byte length of [`data`](Self::data) for [`size`](Self::size).
    pub fn expected_len(size: Size) -> usize {
        size.width as usize * size.height as usize * 4
    }
}
