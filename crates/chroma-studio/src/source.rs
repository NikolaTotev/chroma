//! The synthetic preview source.
//!
//! Chroma's source media is normally the recorded screen video, decoded frame
//! by frame. A real `Decoder` is a later milestone, so the editor engine renders
//! its preview and export over a synthetic "screen" — a checkerboard with a
//! moving synthetic cursor — which is enough to see the camera, styling, and
//! overlays the project defines.

use chroma_core_api::{Point, Size, TimeStamp};

/// Builds an `size`-pixel RGBA8 checkerboard "screen".
pub fn demo_screen(size: Size) -> Vec<u8> {
    let (w, h) = (size.width.max(1), size.height.max(1));
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let cell = ((x / 64) + (y / 64)) % 2 == 0;
            let c = if cell { [60, 70, 90] } else { [220, 225, 235] };
            data.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    data
}

/// A smooth synthetic cursor path in normalized canvas coordinates at `time`,
/// so cursor-follow and the cursor marker have something to track in preview.
pub fn cursor_at(time: TimeStamp) -> Point {
    let t = time.as_secs_f64() as f32;
    Point::new(
        0.5 + 0.28 * (t * 1.3).sin(),
        0.5 + 0.22 * (t * 0.9 + 1.0).sin(),
    )
}
