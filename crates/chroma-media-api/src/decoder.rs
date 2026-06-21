//! Decode + seekable frame-access contracts.

use crate::error::Result;
use crate::frame::RgbaFrame;
use chroma_core_api::{Size, TimeStamp};

/// Random-access decode of source media.
///
/// The editor works against on-disk media with seekable decode so long
/// recordings never buffer entirely in RAM (spec §3.1). Implemented by
/// `chroma-media-ffmpeg`.
pub trait Decoder {
    /// Opens `path` for decoding, returning the source frame size.
    fn open(&mut self, path: &str) -> Result<Size>;

    /// Decodes the frame nearest `t` (the §3.4 "source sample" step) as RGBA8.
    fn frame_at(&mut self, t: TimeStamp) -> Result<RgbaFrame>;
}

/// A forward stream of decoded frames.
///
/// A thinner contract than [`Decoder`] for consumers that only walk frames in
/// order (e.g. a synthetic test-pattern source, or sequential transcode). The
/// render core's golden-frame harness uses a fake `FrameSource` so it needs no
/// real decoder (spec §4.5).
pub trait FrameSource {
    /// Returns the next frame in order, or `Ok(None)` at end of stream.
    fn next_frame(&mut self) -> Result<Option<RgbaFrame>>;
}
