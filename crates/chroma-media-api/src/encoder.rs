//! Encode + mux contracts.

use crate::error::Result;
use crate::frame::RgbaFrame;
use crate::spec::OutputSpec;
use chroma_core_api::TimeStamp;

/// Encodes composited frames to an output file.
///
/// `Mp4Encoder` and `GifEncoder` (in `chroma-media-ffmpeg`) both implement this;
/// the [`OutputSpec`] carries the §3.6 parameters. Usage is strictly
/// `open` → `push_frame`* → `finish`. Export reports progress and is
/// cancellable via [`MediaError::Cancelled`](crate::MediaError::Cancelled)
/// surfaced from `push_frame` (spec EXP-07).
pub trait Encoder {
    /// Prepares the encoder for `spec`. Must be called before `push_frame`.
    fn open(&mut self, spec: &OutputSpec) -> Result<()>;

    /// Encodes one frame at presentation timestamp `pts`. Frames are pushed in
    /// non-decreasing `pts` order.
    fn push_frame(&mut self, frame: &RgbaFrame, pts: TimeStamp) -> Result<()>;

    /// Flushes and finalizes the output file. The encoder is spent afterward.
    fn finish(&mut self) -> Result<()>;
}

/// Muxes already-encoded elementary streams into a container.
///
/// Reserved for the audio/video mux path (spec CAP-09 keeps an audio track for
/// later mux). v1 export goes straight through [`Encoder`]; this contract exists
/// so adding muxed audio is additive, not a breaking change.
pub trait MuxTarget {
    /// Adds an already-encoded packet for stream `stream_index` at `pts`.
    fn write_packet(&mut self, stream_index: u32, pts: TimeStamp, data: &[u8]) -> Result<()>;

    /// Finalizes the container.
    fn finish(&mut self) -> Result<()>;
}
