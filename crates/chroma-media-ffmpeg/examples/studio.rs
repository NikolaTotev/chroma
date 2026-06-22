//! `studio` — render a clip with real effects (crop-zoom + text) and export it.
//!
//! Builds a list of `ModifierSpec`s (as a saved project would hold), turns them
//! into live `Modifier`s via `chroma-modifiers::build_all`, then renders every
//! frame through the §3.4 pipeline and encodes to MP4. This is the M2+M3+M4
//! path end to end: styled background + animated camera + text overlay → video.
//!
//! Usage: `cargo run -p chroma-media-ffmpeg --example studio -- [out.mp4] [seconds] [fps]`

use chroma_compositor::CpuCompositor;
use chroma_core_api::{
    Background, GradientStop, ModifierKind, ModifierParams, ModifierSpec, PassthroughSmoother,
    Rect, SceneStyle, Shadow, Size, TimeRange, TimeStamp,
};
use chroma_media_api::{Codec, Container, Encoder, GifSettings, OutputSpec, RateControl};
use chroma_media_ffmpeg::{ffmpeg_available, FfmpegEncoder};
use chroma_modifiers::build_all;
use chroma_render::{render_frame, SourceFrame};

fn main() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found on PATH — install it (e.g. `sudo apt install ffmpeg`)");
        std::process::exit(1);
    }

    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "out.mp4".to_string());
    let seconds: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let fps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let frames = (seconds * fps).max(1);
    let frame_ns = 1_000_000_000u64 / fps as u64;
    let secs = |s: f32| TimeStamp::from_nanos((s * 1e9) as u64);

    let canvas = Size::new(640, 360);
    let (sw, sh) = (320u32, 180u32);
    let source = grid(sw, sh);

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

    // The "project": effects on lanes, as ModifierSpec data.
    let specs = vec![
        ModifierSpec {
            kind: ModifierKind::Camera,
            range: TimeRange::new(secs(1.0), secs(4.0)),
            params: ModifierParams::CropZoom {
                target: Rect::new(0.05, 0.05, 0.35, 0.35),
            },
        },
        ModifierSpec {
            kind: ModifierKind::Overlay,
            range: TimeRange::new(secs(0.5), secs(4.5)),
            params: ModifierParams::Text {
                content: "Chroma".to_owned(),
                rect: Rect::new(0.30, 0.82, 0.40, 0.10),
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
        },
    ];
    let modifiers = build_all(&specs);

    let spec = OutputSpec {
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
    };

    let mut encoder = FfmpegEncoder::new(&out);
    if let Err(e) = encoder.open(&spec) {
        eprintln!("failed to start ffmpeg: {e}");
        std::process::exit(1);
    }

    let mut compositor = CpuCompositor::new();
    for i in 0..frames {
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
            &mut PassthroughSmoother,
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
            "wrote {out} — crop-zoom + text over a styled background, {seconds}s @ {fps}fps"
        ),
        Err(e) => {
            eprintln!("ffmpeg finalize failed: {e}");
            std::process::exit(1);
        }
    }
}

/// A checkerboard-ish grid "screen" so the camera move is easy to see.
fn grid(sw: u32, sh: u32) -> Vec<u8> {
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
