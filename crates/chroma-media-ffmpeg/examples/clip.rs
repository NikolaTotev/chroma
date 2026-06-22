//! `clip` — render an animated, styled clip and encode it to a playable file.
//!
//! Ties M2 + M3 together end to end: a synthetic color-bar "screen" is
//! composited over a gradient background while a virtual camera zooms and pans,
//! and every frame is streamed to ffmpeg. The result is an actual video you can
//! play.
//!
//! Usage: `cargo run -p chroma-media-ffmpeg --example clip -- [out.mp4|out.gif] [seconds] [fps]`

use chroma_compositor::CpuCompositor;
use chroma_core_api::fakes::FakeCameraModifier;
use chroma_core_api::{
    Background, CameraTarget, GradientStop, Modifier, Point, SceneStyle, Shadow, Size, TimeRange,
    TimeStamp,
};
use chroma_media_api::{Codec, Container, Encoder, GifSettings, OutputSpec, RateControl};
use chroma_media_ffmpeg::{ffmpeg_available, FfmpegEncoder};
use chroma_render::{render_frame, SourceFrame};
use std::f32::consts::PI;

fn main() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found on PATH — install it (e.g. `sudo apt install ffmpeg`)");
        std::process::exit(1);
    }

    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "out.mp4".to_string());
    let seconds: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4);
    let fps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let frames = (seconds * fps).max(1);

    let canvas = Size::new(640, 360);
    let (sw, sh) = (320u32, 180u32);
    let source = color_bars(sw, sh);

    let background = Background::Gradient {
        angle_deg: 35.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                rgba: [0.10, 0.12, 0.30, 1.0],
            },
            GradientStop {
                offset: 1.0,
                rgba: [0.55, 0.18, 0.45, 1.0],
            },
        ],
    };
    let scene = SceneStyle {
        padding: 0.07,
        corner_radius: 0.06,
        shadow: Some(Shadow {
            dx: 0.0,
            dy: 0.02,
            blur: 0.04,
            rgba: [0.0, 0.0, 0.0, 0.5],
        }),
        border: None,
    };

    let container = if out.ends_with(".gif") {
        Container::Gif
    } else {
        Container::Mp4
    };
    let spec = OutputSpec {
        container,
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
    };

    let mut encoder = FfmpegEncoder::new(&out);
    if let Err(e) = encoder.open(&spec) {
        eprintln!("failed to start ffmpeg: {e}");
        std::process::exit(1);
    }

    let mut compositor = CpuCompositor::new();
    let frame_ns = 1_000_000_000u64 / fps as u64;
    for i in 0..frames {
        let t = i as f32 / frames as f32;
        // Ease the camera: zoom in to ~2x and back, with a gentle pan.
        let scale = 1.0 + 0.9 * (t * PI).sin();
        let center = Point::new(
            0.5 + 0.12 * (t * 2.0 * PI).cos(),
            0.5 + 0.08 * (t * PI).sin(),
        );
        let camera: Box<dyn Modifier> = Box::new(FakeCameraModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(u64::MAX)),
            target: CameraTarget {
                center,
                scale,
                weight: 1.0,
            },
        });

        let frame = render_frame(
            canvas,
            &background,
            &scene,
            &SourceFrame {
                size: Size::new(sw, sh),
                rgba: &source,
            },
            Some(center),
            std::slice::from_ref(&camera),
            &mut compositor,
            TimeStamp::from_nanos(i as u64 * frame_ns),
        );
        if let Err(e) = encoder.push_frame(&frame, frame.pts) {
            eprintln!("encode failed at frame {i}: {e}");
            std::process::exit(1);
        }
    }

    match encoder.finish() {
        Ok(()) => println!(
            "wrote {out} ({frames} frames, {seconds}s @ {fps}fps) — play it with any video player"
        ),
        Err(e) => {
            eprintln!("ffmpeg finalize failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Builds an `sw x sh` RGBA8 color-bar test pattern.
fn color_bars(sw: u32, sh: u32) -> Vec<u8> {
    let bars = [
        [230, 230, 230],
        [230, 230, 30],
        [30, 230, 230],
        [30, 230, 30],
        [230, 30, 230],
        [230, 30, 30],
        [30, 30, 230],
        [20, 20, 20],
    ];
    let mut data = Vec::with_capacity((sw * sh * 4) as usize);
    for _y in 0..sh {
        for x in 0..sw {
            let c = bars[(x as usize * bars.len()) / sw as usize];
            data.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    data
}
