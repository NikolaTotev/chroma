//! Test doubles for the media contract.
//!
//! Let the render core and export scheduler be tested with no FFmpeg present
//! (`ORCHESTRATION.md` §10): a frame source that emits a synthetic test
//! pattern, and an encoder that records the frames it was handed.

use crate::decoder::FrameSource;
use crate::encoder::Encoder;
use crate::error::Result;
use crate::frame::RgbaFrame;
use crate::spec::OutputSpec;
use chroma_core_api::{Size, TimeStamp};

/// A frame source that yields `count` solid-gray frames, then `None`.
pub struct FakeFrameSource {
    size: Size,
    remaining: u32,
    frame_interval_nanos: u64,
    next_ts: u64,
}

impl FakeFrameSource {
    /// Creates a source of `count` `size`-sized frames, `frame_interval_nanos`
    /// apart.
    pub fn new(size: Size, count: u32, frame_interval_nanos: u64) -> Self {
        FakeFrameSource {
            size,
            remaining: count,
            frame_interval_nanos,
            next_ts: 0,
        }
    }
}

impl FrameSource for FakeFrameSource {
    fn next_frame(&mut self) -> Result<Option<RgbaFrame>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        let pts = self.next_ts;
        self.next_ts += self.frame_interval_nanos;
        Ok(Some(RgbaFrame {
            size: self.size,
            pts: TimeStamp::from_nanos(pts),
            data: vec![128u8; RgbaFrame::expected_len(self.size)],
        }))
    }
}

/// An encoder that captures the spec it was opened with and every frame pushed,
/// so tests can assert on export output without producing a real file.
#[derive(Default)]
pub struct RecordingEncoder {
    /// The spec passed to [`open`](Encoder::open), if any.
    pub spec: Option<OutputSpec>,
    /// `(pts, frame)` for each pushed frame, in order.
    pub frames: Vec<(TimeStamp, RgbaFrame)>,
    /// Whether [`finish`](Encoder::finish) was called.
    pub finished: bool,
}

impl Encoder for RecordingEncoder {
    fn open(&mut self, spec: &OutputSpec) -> Result<()> {
        self.spec = Some(spec.clone());
        Ok(())
    }

    fn push_frame(&mut self, frame: &RgbaFrame, pts: TimeStamp) -> Result<()> {
        self.frames.push((pts, frame.clone()));
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.finished = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Codec, Container, GifSettings, RateControl};

    fn spec() -> OutputSpec {
        OutputSpec {
            container: Container::Mp4,
            canvas: Size::new(8, 8),
            fps: 30,
            codec: Codec::H264,
            rate_control: RateControl::Crf { crf: 23 },
            hardware: false,
            gif: GifSettings {
                palette_size: 256,
                dithering: true,
                two_pass_palette: true,
                loop_count: 0,
            },
        }
    }

    #[test]
    fn fake_source_drives_recording_encoder() {
        let mut src = FakeFrameSource::new(Size::new(8, 8), 3, 33_000_000);
        let mut enc = RecordingEncoder::default();
        enc.open(&spec()).unwrap();
        while let Some(frame) = src.next_frame().unwrap() {
            let pts = frame.pts;
            enc.push_frame(&frame, pts).unwrap();
        }
        enc.finish().unwrap();

        assert_eq!(enc.frames.len(), 3);
        assert_eq!(enc.frames[0].0, TimeStamp::from_nanos(0));
        assert_eq!(enc.frames[2].0, TimeStamp::from_nanos(66_000_000));
        assert!(enc.finished);
        assert_eq!(enc.frames[0].1.data.len(), 8 * 8 * 4);
    }
}
