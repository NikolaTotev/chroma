//! `chroma` — the application CLI / composition root.
//!
//! Wires the capture, render, effects, and encode crates into runnable
//! commands:
//!
//! - `chroma render [out] [secs] [fps]` — render a built-in demo project
//!   (styled background + crop-zoom + text) to a video. Works anywhere `ffmpeg`
//!   is installed.
//! - `chroma record [out] [secs] [fps]` — record the X11 desktop, then
//!   composite it over a styled background with a cursor-follow camera and
//!   export. Needs a native X11 session (the desktop is not capturable under
//!   rootless Xwayland / WSLg).
//!
//! This is the only crate that depends on implementation crates — it composes
//! them; everything else depends on `-api` contracts (ORCHESTRATION.md §3).

use chroma_capture_api::{CaptureTarget, Clock, EventSource, Frame, InputEvent, ScreenCapturer};
use chroma_compositor::CpuCompositor;
use chroma_core_api::{
    Background, GradientStop, ModifierKind, ModifierParams, ModifierSpec, Point, Rect, SceneStyle,
    Shadow, Size, TimeRange, TimeStamp,
};
use chroma_media_api::{Codec, Container, Encoder, GifSettings, OutputSpec, RateControl};
use chroma_media_ffmpeg::{ffmpeg_available, FfmpegEncoder};
use chroma_modifiers::build_all;
use chroma_render::{render_frame, SourceFrame};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str);
    let out = args.get(2).cloned();
    let secs: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    let fps: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);

    let code = match cmd {
        Some("render") => cmd_render(&out.unwrap_or_else(|| "out.mp4".into()), secs, fps),
        Some("record") => cmd_record(&out.unwrap_or_else(|| "recording.mp4".into()), secs, fps),
        _ => {
            eprintln!(
                "chroma — screen-demo capture & compositing\n\n\
                 USAGE:\n  \
                 chroma render [out.mp4|out.gif] [secs] [fps]   render the demo project\n  \
                 chroma record [out.mp4]        [secs] [fps]   record the X11 desktop, styled\n"
            );
            2
        }
    };
    std::process::exit(code);
}

/// Shared look: a gradient background and an inset scene with rounded corners
/// and a soft shadow.
fn styled() -> (Background, SceneStyle) {
    let background = Background::Gradient {
        angle_deg: 35.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                rgba: [0.08, 0.10, 0.28, 1.0],
            },
            GradientStop {
                offset: 1.0,
                rgba: [0.50, 0.16, 0.42, 1.0],
            },
        ],
    };
    let scene = SceneStyle {
        padding: 0.06,
        corner_radius: 0.05,
        shadow: Some(Shadow {
            dx: 0.0,
            dy: 0.02,
            blur: 0.04,
            rgba: [0.0, 0.0, 0.0, 0.5],
        }),
        border: None,
    };
    (background, scene)
}

fn output_spec(out: &str, canvas: Size, fps: u32) -> OutputSpec {
    OutputSpec {
        container: if out.ends_with(".gif") {
            Container::Gif
        } else {
            Container::Mp4
        },
        canvas,
        fps,
        codec: Codec::H264,
        rate_control: RateControl::Crf { crf: 20 },
        hardware: false,
        gif: GifSettings {
            palette_size: 256,
            dithering: true,
            two_pass_palette: true,
            loop_count: 0,
        },
    }
}

/// `chroma render` — synthetic demo project → styled video.
fn cmd_render(out: &str, secs: u32, fps: u32) -> i32 {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found on PATH — `sudo apt install ffmpeg`");
        return 1;
    }
    let canvas = Size::new(640, 360);
    let (sw, sh) = (320u32, 180u32);
    let source = demo_screen(sw, sh);
    let (background, scene) = styled();
    let secs_ts = |s: f32| TimeStamp::from_nanos((s * 1e9) as u64);

    let specs = vec![
        ModifierSpec {
            kind: ModifierKind::Camera,
            range: TimeRange::new(secs_ts(1.0), secs_ts(secs as f32 - 1.0)),
            params: ModifierParams::CropZoom {
                target: Rect::new(0.05, 0.05, 0.35, 0.35),
            },
        },
        ModifierSpec {
            kind: ModifierKind::Overlay,
            range: TimeRange::new(secs_ts(0.5), secs_ts(secs as f32 - 0.5)),
            params: ModifierParams::Text {
                content: "Chroma".to_owned(),
                rect: Rect::new(0.30, 0.82, 0.40, 0.10),
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
        },
    ];
    let modifiers = build_all(&specs);

    let mut encoder = FfmpegEncoder::new(out);
    if let Err(e) = encoder.open(&output_spec(out, canvas, fps)) {
        eprintln!("ffmpeg start failed: {e}");
        return 1;
    }
    let mut compositor = CpuCompositor::new();
    let frame_ns = 1_000_000_000u64 / fps as u64;
    for i in 0..(secs * fps).max(1) {
        let frame = render_frame(
            canvas,
            &background,
            &scene,
            &SourceFrame {
                size: Size::new(sw, sh),
                rgba: &source,
            },
            None,
            &modifiers,
            &mut compositor,
            TimeStamp::from_nanos(i as u64 * frame_ns),
        );
        if let Err(e) = encoder.push_frame(&frame, frame.pts) {
            eprintln!("encode failed: {e}");
            return 1;
        }
    }
    match encoder.finish() {
        Ok(()) => {
            println!("wrote {out}");
            0
        }
        Err(e) => {
            eprintln!("ffmpeg failed: {e}");
            1
        }
    }
}

/// `chroma record` — capture the X11 desktop, then composite + export.
fn cmd_record(out: &str, secs: u32, fps: u32) -> i32 {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found on PATH — `sudo apt install ffmpeg`");
        return 1;
    }
    let mut session = match chroma_capture_x11::open_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot open X11 capture: {e}");
            return 1;
        }
    };
    if let Err(e) = session
        .capturer
        .start(CaptureTarget::FullScreen { monitor: 0 }, fps)
    {
        eprintln!("capture start failed: {e}");
        return 1;
    }

    // Phase 1: record frames + input events into memory.
    println!("recording {secs}s of the desktop at {fps}fps …");
    let dur_ns = secs as u64 * 1_000_000_000;
    let start = session.clock.now().as_nanos();
    let mut frames: Vec<Frame> = Vec::new();
    let mut events = Vec::new();
    loop {
        match session.capturer.next_frame() {
            Ok(frame) => {
                let t = frame.timestamp.as_nanos();
                frames.push(frame);
                events.extend(session.events.poll());
                if t.saturating_sub(start) >= dur_ns {
                    break;
                }
            }
            Err(e) => {
                eprintln!("capture failed: {e}");
                eprintln!("(the desktop is not capturable under rootless Xwayland/WSLg — run on a native X11 session)");
                return 1;
            }
        }
    }
    let _ = session.capturer.stop();
    let Some(first) = frames.first() else {
        eprintln!("no frames captured");
        return 1;
    };
    // x264 yuv420p needs even dimensions.
    let canvas = Size::new(first.size.width & !1, first.size.height & !1);
    println!("captured {} frames; rendering …", frames.len());

    // A cursor-follow camera over the whole clip.
    let full = TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(u64::MAX));
    let modifiers = build_all(&[ModifierSpec {
        kind: ModifierKind::Camera,
        range: full,
        params: ModifierParams::CursorFollow {
            zoom: 1.6,
            tightness: 1.0,
        },
    }]);
    let (background, scene) = styled();

    let mut encoder = FfmpegEncoder::new(out);
    if let Err(e) = encoder.open(&output_spec(out, canvas, fps)) {
        eprintln!("ffmpeg start failed: {e}");
        return 1;
    }
    let mut compositor = CpuCompositor::new();
    for frame in &frames {
        let rgba = bgra_to_rgba(frame);
        let cursor = cursor_at(&events, frame.timestamp, frame.size);
        let out_frame = render_frame(
            canvas,
            &background,
            &scene,
            &SourceFrame {
                size: frame.size,
                rgba: &rgba,
            },
            cursor,
            &modifiers,
            &mut compositor,
            frame.timestamp,
        );
        if let Err(e) = encoder.push_frame(&out_frame, out_frame.pts) {
            eprintln!("encode failed: {e}");
            return 1;
        }
    }
    match encoder.finish() {
        Ok(()) => {
            println!("wrote {out}");
            0
        }
        Err(e) => {
            eprintln!("ffmpeg failed: {e}");
            1
        }
    }
}

/// Converts a captured BGRA frame (with row stride) to tightly-packed RGBA,
/// forcing opaque alpha (X server frames carry no usable alpha).
fn bgra_to_rgba(frame: &Frame) -> Vec<u8> {
    let w = frame.size.width as usize;
    let h = frame.size.height as usize;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let row = y * frame.stride;
        for x in 0..w {
            let s = row + x * 4;
            let d = (y * w + x) * 4;
            if s + 3 < frame.data.len() {
                out[d] = frame.data[s + 2]; // R
                out[d + 1] = frame.data[s + 1]; // G
                out[d + 2] = frame.data[s]; // B
                out[d + 3] = 255;
            }
        }
    }
    out
}

/// The most recent pointer position at time `t`, normalized to source size.
fn cursor_at(
    events: &[chroma_capture_api::TimedInputEvent],
    t: TimeStamp,
    src: Size,
) -> Option<Point> {
    let mut last = None;
    for e in events {
        if e.timestamp.as_nanos() > t.as_nanos() {
            break;
        }
        match e.event {
            InputEvent::PointerMove { x, y }
            | InputEvent::ButtonDown { x, y, .. }
            | InputEvent::ButtonUp { x, y, .. } => last = Some((x, y)),
            _ => {}
        }
    }
    last.map(|(x, y)| {
        Point::new(
            (x / src.width.max(1) as f32).clamp(0.0, 1.0),
            (y / src.height.max(1) as f32).clamp(0.0, 1.0),
        )
    })
}

/// A simple demo "screen": a grid so the camera move is obvious.
fn demo_screen(sw: u32, sh: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((sw * sh * 4) as usize);
    for y in 0..sh {
        for x in 0..sw {
            let cell = ((x / 32) + (y / 32)) % 2 == 0;
            let c = if cell { [60, 70, 90] } else { [220, 225, 235] };
            data.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    data
}
