//! Screen recording into a source clip.
//!
//! Recording runs on a background thread so the editor stays responsive: it
//! drives the platform [`ScreenCapturer`], encodes the raw (unstyled) frames to
//! a temporary MP4 — the immutable *source* the editor edits against — and
//! collects the timestamped input events alongside. On stop it finalizes the
//! file and returns a [`RecordedClip`].
//!
//! The capture backend is `chroma-capture-x11`, which only works on a native
//! X11 / Xorg session (it reports `Unavailable` under rootless Xwayland, on
//! Wayland, and off Linux). Recording is therefore the one part of the studio
//! that needs the right desktop session; everything downstream (decode, edit,
//! export) is platform-independent.

use chroma_capture_api::{
    CaptureTarget, EventSource, Frame, PixelFormat, ScreenCapturer, TimedInputEvent,
};
use chroma_core_api::Size;
use chroma_media_api::{
    Codec, Container, Encoder, GifSettings, OutputSpec, RateControl, RgbaFrame,
};
use chroma_media_ffmpeg::FfmpegEncoder;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// A finished recording: the source video on disk plus its event log.
pub struct RecordedClip {
    /// Path to the recorded (unstyled) source MP4.
    pub video_path: PathBuf,
    /// Encoded frame size (even dimensions for H.264).
    pub size: Size,
    /// Capture frame rate.
    pub fps: u32,
    /// Total duration in nanoseconds.
    pub duration_ns: u64,
    /// Timestamped input events captured during the take.
    pub events: Vec<TimedInputEvent>,
}

/// A recording in progress on its own thread.
pub struct Recorder {
    stop: Arc<AtomicBool>,
    join: JoinHandle<Result<RecordedClip, String>>,
    started: Instant,
}

impl Recorder {
    /// Starts capturing the full screen at `fps`.
    ///
    /// Returns an error immediately (before spawning a long-lived capture) if
    /// the capture backend is unavailable — e.g. on Wayland/rootless Xwayland or
    /// off Linux.
    pub fn start(fps: u32) -> Result<Recorder, String> {
        let fps = fps.clamp(1, 240);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        // The thread reports whether capture init succeeded so `start` can fail
        // synchronously; after that it runs until `stop` is set.
        let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();
        let join = std::thread::spawn(move || run(fps, stop_thread, init_tx));
        match init_rx.recv() {
            Ok(Ok(())) => Ok(Recorder {
                stop,
                join,
                started: Instant::now(),
            }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => Err("recorder thread exited before initializing".to_owned()),
        }
    }

    /// Nanoseconds elapsed since the recording started.
    pub fn elapsed_ns(&self) -> u64 {
        self.started.elapsed().as_nanos() as u64
    }

    /// Signals stop, waits for the encoder to finalize, and returns the clip.
    pub fn stop(self) -> Result<RecordedClip, String> {
        self.stop.store(true, Ordering::Release);
        self.join
            .join()
            .map_err(|_| "recorder thread panicked".to_owned())?
    }
}

/// The capture loop body, run on the recorder thread.
fn run(
    fps: u32,
    stop: Arc<AtomicBool>,
    init_tx: mpsc::Sender<Result<(), String>>,
) -> Result<RecordedClip, String> {
    // Helper to report an init failure exactly once, then bail.
    macro_rules! init_fail {
        ($e:expr) => {{
            let _ = init_tx.send(Err($e));
            return Err("capture init failed".to_owned());
        }};
    }

    let mut session = match chroma_capture_x11::open_session() {
        Ok(s) => s,
        Err(e) => init_fail!(format!("cannot open screen capture: {e}")),
    };
    if let Err(e) = session
        .capturer
        .start(CaptureTarget::FullScreen { monitor: 0 }, fps)
    {
        init_fail!(format!("capture start failed: {e}"));
    }

    // The first frame fixes the (even) encode size.
    let first = match session.capturer.next_frame() {
        Ok(f) => f,
        Err(e) => init_fail!(format!(
            "no frames (a native X11/Xorg session is required): {e}"
        )),
    };
    let size = Size::new(first.size.width & !1, first.size.height & !1);
    let video_path = std::env::temp_dir().join(format!(
        "chroma-recording-{}-{}.mp4",
        std::process::id(),
        first.timestamp.as_nanos()
    ));

    let mut encoder = FfmpegEncoder::new(&video_path);
    if let Err(e) = encoder.open(&source_spec(size, fps)) {
        init_fail!(format!("encoder start failed: {e}"));
    }

    // Init OK — from here, failures are reported through `stop()`/join.
    let _ = init_tx.send(Ok(()));

    let mut events: Vec<TimedInputEvent> = Vec::new();
    let mut count: u64 = 0;
    let push = |encoder: &mut FfmpegEncoder, frame: &Frame| -> Result<(), String> {
        let rgba = to_rgba_even(frame, size);
        let out = RgbaFrame {
            size,
            pts: frame.timestamp,
            data: rgba,
        };
        encoder.push_frame(&out, out.pts).map_err(|e| e.to_string())
    };

    push(&mut encoder, &first)?;
    count += 1;
    events.extend(session.events.poll());

    while !stop.load(Ordering::Acquire) {
        match session.capturer.next_frame() {
            Ok(frame) => {
                events.extend(session.events.poll());
                push(&mut encoder, &frame)?;
                count += 1;
            }
            Err(_) => break, // capture ended; finalize what we have
        }
    }

    let _ = session.capturer.stop();
    encoder.finish().map_err(|e| e.to_string())?;

    let duration_ns = count * (1_000_000_000 / fps as u64);
    Ok(RecordedClip {
        video_path,
        size,
        fps,
        duration_ns,
        events,
    })
}

/// The encode settings for the raw source clip: H.264, near-lossless CRF.
fn source_spec(size: Size, fps: u32) -> OutputSpec {
    OutputSpec {
        container: Container::Mp4,
        canvas: size,
        fps,
        codec: Codec::H264,
        rate_control: RateControl::Crf { crf: 16 },
        hardware: false,
        gif: GifSettings {
            palette_size: 256,
            dithering: true,
            two_pass_palette: true,
            loop_count: 0,
        },
    }
}

/// Converts a captured frame (with row stride, BGRA or RGBA) to tightly-packed
/// RGBA8 cropped to even `size`, forcing opaque alpha (X frames carry none).
fn to_rgba_even(frame: &Frame, size: Size) -> Vec<u8> {
    let w = size.width as usize;
    let h = size.height as usize;
    let mut out = vec![0u8; w * h * 4];
    let bgr = matches!(frame.format, PixelFormat::Bgra8);
    for y in 0..h {
        let row = y * frame.stride;
        for x in 0..w {
            let s = row + x * 4;
            let d = (y * w + x) * 4;
            if s + 3 < frame.data.len() {
                if bgr {
                    out[d] = frame.data[s + 2];
                    out[d + 1] = frame.data[s + 1];
                    out[d + 2] = frame.data[s];
                } else {
                    out[d] = frame.data[s];
                    out[d + 1] = frame.data[s + 1];
                    out[d + 2] = frame.data[s + 2];
                }
                out[d + 3] = 255;
            }
        }
    }
    out
}
