//! FFmpeg-backed decoding: random-access seek and sequential streaming.
//!
//! The editor edits non-destructively against the recorded source video, so it
//! needs to pull the source frame at an arbitrary time (preview scrubbing) and
//! to walk every frame in order (export). Both go through the `ffmpeg` CLI:
//!
//! - [`FfmpegDecoder`] implements [`Decoder`] — seek to a time and decode one
//!   RGBA frame (`ffmpeg -ss … -frames:v 1`).
//! - [`FfmpegFrameReader`] implements [`FrameSource`] — one long-lived ffmpeg
//!   process streaming raw RGBA frames in order, so a whole clip never buffers
//!   in RAM.
//!
//! [`probe_video`] reads size/fps/duration via `ffprobe`.

use chroma_core_api::{Size, TimeStamp};
use chroma_media_api::{Decoder, FrameSource, MediaError, Result, RgbaFrame};
use std::io::Read;
use std::process::{Child, Command, Stdio};

/// Container metadata for a source video.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoInfo {
    /// Frame size in pixels.
    pub size: Size,
    /// Frames per second (from `avg_frame_rate`).
    pub fps: f64,
    /// Total duration in nanoseconds.
    pub duration_ns: u64,
}

/// Probes `path` with `ffprobe` for size, fps, and duration.
pub fn probe_video(path: &str) -> Result<VideoInfo> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1",
            path,
        ])
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                MediaError::Unsupported("ffprobe not found on PATH".to_owned())
            }
            _ => MediaError::Io(e.to_string()),
        })?;
    if !out.status.success() {
        return Err(MediaError::Backend(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 0.0f64;
    let mut duration = 0.0f64;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "width" => width = v.trim().parse().unwrap_or(0),
            "height" => height = v.trim().parse().unwrap_or(0),
            "avg_frame_rate" => fps = parse_rational(v.trim()),
            "duration" => duration = v.trim().parse().unwrap_or(0.0),
            _ => {}
        }
    }
    if width == 0 || height == 0 {
        return Err(MediaError::Backend(format!(
            "ffprobe returned no video stream for {path}"
        )));
    }
    Ok(VideoInfo {
        size: Size::new(width, height),
        fps,
        duration_ns: (duration * 1e9) as u64,
    })
}

/// Parses an ffmpeg rational like `30/1` or `30000/1001` to a float.
fn parse_rational(s: &str) -> f64 {
    match s.split_once('/') {
        Some((n, d)) => {
            let n: f64 = n.parse().unwrap_or(0.0);
            let d: f64 = d.parse().unwrap_or(1.0);
            if d == 0.0 {
                0.0
            } else {
                n / d
            }
        }
        None => s.parse().unwrap_or(0.0),
    }
}

/// A seek-and-decode [`Decoder`] over a single source file.
#[derive(Default)]
pub struct FfmpegDecoder {
    path: Option<String>,
    size: Size,
}

impl FfmpegDecoder {
    /// A decoder not yet bound to a file (call [`Decoder::open`]).
    pub fn new() -> Self {
        FfmpegDecoder::default()
    }
}

impl Decoder for FfmpegDecoder {
    fn open(&mut self, path: &str) -> Result<Size> {
        let info = probe_video(path)?;
        self.path = Some(path.to_owned());
        self.size = info.size;
        Ok(info.size)
    }

    fn frame_at(&mut self, t: TimeStamp) -> Result<RgbaFrame> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| MediaError::InvalidSpec("decoder not opened".to_owned()))?;
        let secs = t.as_secs_f64().max(0.0);
        // `-ss` before `-i` is a fast (keyframe) seek; one frame to stdout.
        let out = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-ss",
                &format!("{secs:.6}"),
                "-i",
                path,
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{}x{}", self.size.width, self.size.height),
                "-",
            ])
            .stderr(Stdio::null())
            .output()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let want = RgbaFrame::expected_len(self.size);
        if out.stdout.len() < want {
            return Err(MediaError::Backend(format!(
                "decoded {} bytes, expected {want} (seek past end?)",
                out.stdout.len()
            )));
        }
        let mut data = out.stdout;
        data.truncate(want);
        Ok(RgbaFrame {
            size: self.size,
            pts: t,
            data,
        })
    }
}

/// A sequential RGBA [`FrameSource`] driven by one streaming ffmpeg process.
pub struct FfmpegFrameReader {
    child: Child,
    size: Size,
    frame_len: usize,
    index: u64,
    fps: f64,
}

impl FfmpegFrameReader {
    /// Opens `path` and starts streaming raw RGBA frames at the file's size/fps.
    pub fn open(path: &str) -> Result<Self> {
        let info = probe_video(path)?;
        let child = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-i",
                path,
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{}x{}", info.size.width, info.size.height),
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    MediaError::Unsupported("ffmpeg not found on PATH".to_owned())
                }
                _ => MediaError::Io(e.to_string()),
            })?;
        Ok(FfmpegFrameReader {
            child,
            size: info.size,
            frame_len: RgbaFrame::expected_len(info.size),
            index: 0,
            fps: if info.fps > 0.0 { info.fps } else { 30.0 },
        })
    }

    /// The source frame size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The source frame rate.
    pub fn fps(&self) -> f64 {
        self.fps
    }
}

impl FrameSource for FfmpegFrameReader {
    fn next_frame(&mut self) -> Result<Option<RgbaFrame>> {
        let Some(stdout) = self.child.stdout.as_mut() else {
            return Ok(None);
        };
        let mut data = vec![0u8; self.frame_len];
        match stdout.read_exact(&mut data) {
            Ok(()) => {
                let pts = TimeStamp::from_nanos((self.index as f64 / self.fps * 1e9) as u64);
                self.index += 1;
                Ok(Some(RgbaFrame {
                    size: self.size,
                    pts,
                    data,
                }))
            }
            // A partial/empty final read is the end of the stream.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(MediaError::Io(e.to_string())),
        }
    }
}

impl Drop for FfmpegFrameReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg_available;
    use std::process::Command;

    /// Makes a short test-pattern MP4 with ffmpeg's `testsrc`. `tag` keeps each
    /// test's file distinct so the parallel tests don't race on one path.
    fn make_test_mp4(tag: &str) -> Option<String> {
        if !ffmpeg_available() {
            return None;
        }
        let path = std::env::temp_dir()
            .join(format!("chroma_dec_{}_{tag}.mp4", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let status = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=2",
                "-pix_fmt",
                "yuv420p",
                &path,
            ])
            .status()
            .ok()?;
        status.success().then_some(path)
    }

    #[test]
    fn probe_reads_dimensions_and_duration() {
        let Some(path) = make_test_mp4("probe") else {
            return;
        };
        let info = probe_video(&path).unwrap();
        assert_eq!(info.size, Size::new(320, 240));
        assert!(
            info.fps > 9.0 && info.fps < 11.0,
            "fps ~10, got {}",
            info.fps
        );
        assert!(
            info.duration_ns > 1_500_000_000,
            "~2s, got {}",
            info.duration_ns
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decoder_seeks_and_returns_one_frame() {
        let Some(path) = make_test_mp4("seek") else {
            return;
        };
        let mut dec = FfmpegDecoder::new();
        let size = dec.open(&path).unwrap();
        assert_eq!(size, Size::new(320, 240));
        let frame = dec.frame_at(TimeStamp::from_nanos(1_000_000_000)).unwrap();
        assert_eq!(frame.data.len(), RgbaFrame::expected_len(size));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reader_streams_every_frame_in_order() {
        let Some(path) = make_test_mp4("stream") else {
            return;
        };
        let mut reader = FfmpegFrameReader::open(&path).unwrap();
        assert_eq!(reader.size(), Size::new(320, 240));
        let mut count = 0;
        while let Some(frame) = reader.next_frame().unwrap() {
            assert_eq!(
                frame.data.len(),
                RgbaFrame::expected_len(Size::new(320, 240))
            );
            count += 1;
        }
        // ~10fps * 2s ≈ 20 frames.
        assert!(
            (18..=22).contains(&count),
            "expected ~20 frames, got {count}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
