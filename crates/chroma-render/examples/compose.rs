//! `compose` — render one styled frame and write it as a viewable BMP.
//!
//! Generates a synthetic color-bar "screen", composites it over a gradient
//! background with the default scene styling (padding, rounded corners, drop
//! shadow) and a slight camera zoom, then writes `out.bmp` (openable directly on
//! Windows). This is the M2 "see it" artifact — a single composited frame.
//!
//! Usage: `cargo run -p chroma-render --example compose -- [out.bmp] [WxH]`

use chroma_compositor::CpuCompositor;
use chroma_core_api::{
    Background, GradientStop, PassthroughSmoother, Point, SceneStyle, Shadow, Size, TimeStamp,
};
use chroma_render::{render_frame, SourceFrame};
use std::io::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "out.bmp".to_string());
    let (cw, ch) = args
        .next()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((640u32, 360u32));

    // Synthetic "captured screen": vertical color bars.
    let (sw, sh) = (320u32, 180u32);
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
    let mut source = Vec::with_capacity((sw * sh * 4) as usize);
    for _y in 0..sh {
        for x in 0..sw {
            let c = bars[(x as usize * bars.len()) / sw as usize];
            source.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }

    let canvas = Size::new(cw, ch);
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

    let mut compositor = CpuCompositor::new();
    let frame = render_frame(
        canvas,
        &background,
        &scene,
        &SourceFrame {
            size: Size::new(sw, sh),
            rgba: &source,
        },
        Some(Point::new(0.5, 0.5)),
        &[],
        &mut PassthroughSmoother,
        &mut compositor,
        TimeStamp::from_nanos(0),
    );

    match write_bmp(&path, cw, ch, &frame.data) {
        Ok(()) => println!("wrote {path} ({cw}x{ch}) — open it to see the composited frame"),
        Err(e) => eprintln!("failed to write {path}: {e}"),
    }
}

/// Writes a 24-bit BMP from tightly-packed RGBA8 (`w * h * 4`). Bottom-up, BGR,
/// rows padded to 4 bytes — the canonical Windows-openable format, no deps.
fn write_bmp(path: &str, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let row = (w * 3).div_ceil(4) * 4;
    let img_size = row * h;
    let file_size = 54 + img_size;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);

    f.write_all(b"BM")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?; // reserved
    f.write_all(&54u32.to_le_bytes())?; // pixel data offset
    f.write_all(&40u32.to_le_bytes())?; // DIB header size
    f.write_all(&(w as i32).to_le_bytes())?;
    f.write_all(&(h as i32).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // planes
    f.write_all(&24u16.to_le_bytes())?; // bpp
    f.write_all(&0u32.to_le_bytes())?; // compression (none)
    f.write_all(&img_size.to_le_bytes())?;
    f.write_all(&2835i32.to_le_bytes())?; // x ppm (~72 dpi)
    f.write_all(&2835i32.to_le_bytes())?; // y ppm
    f.write_all(&0u32.to_le_bytes())?; // palette colors
    f.write_all(&0u32.to_le_bytes())?; // important colors

    let pad = (row - w * 3) as usize;
    for y in (0..h).rev() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            f.write_all(&[rgba[i + 2], rgba[i + 1], rgba[i]])?; // BGR
        }
        f.write_all(&vec![0u8; pad])?;
    }
    Ok(())
}
