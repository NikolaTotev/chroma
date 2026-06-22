//! Pure rasterization helpers for the CPU compositor.
//!
//! Everything here works on a flat `RGBA f32` buffer (`width * height * 4`,
//! linear, straight alpha) and is a deterministic pure function of its inputs —
//! the basis for golden-frame tests (spec §3.3).

use chroma_core_api::{Background, GradientStop};

/// Linear "over" blend of `src` (with effective alpha `src[3] * coverage`) onto
/// the pixel at `(x, y)`.
pub(crate) fn blend_px(
    buf: &mut [f32],
    w: usize,
    x: usize,
    y: usize,
    src: [f32; 4],
    coverage: f32,
) {
    let a = (src[3] * coverage).clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let i = (y * w + x) * 4;
    for c in 0..3 {
        buf[i + c] = src[c] * a + buf[i + c] * (1.0 - a);
    }
    buf[i + 3] = a + buf[i + 3] * (1.0 - a);
}

/// Overwrites the pixel at `(x, y)`.
pub(crate) fn set_px(buf: &mut [f32], w: usize, x: usize, y: usize, rgba: [f32; 4]) {
    let i = (y * w + x) * 4;
    buf[i..i + 4].copy_from_slice(&rgba);
}

/// Samples a sorted gradient at position `t`, clamping outside `[first, last]`.
pub(crate) fn gradient_color(stops: &[GradientStop], t: f32) -> [f32; 4] {
    match stops {
        [] => [0.0, 0.0, 0.0, 1.0],
        [only] => only.rgba,
        _ => {
            let t = t.clamp(0.0, 1.0);
            if t <= stops[0].offset {
                return stops[0].rgba;
            }
            let last = stops[stops.len() - 1];
            if t >= last.offset {
                return last.rgba;
            }
            for pair in stops.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if t >= a.offset && t <= b.offset {
                    let span = (b.offset - a.offset).max(1e-6);
                    let f = (t - a.offset) / span;
                    let mut out = [0.0; 4];
                    for (c, o) in out.iter_mut().enumerate() {
                        *o = a.rgba[c] + (b.rgba[c] - a.rgba[c]) * f;
                    }
                    return out;
                }
            }
            last.rgba
        }
    }
}

/// Paints the background across the whole buffer.
pub(crate) fn fill_background(buf: &mut [f32], w: usize, h: usize, bg: &Background) {
    match bg {
        Background::Solid(rgba) => {
            for y in 0..h {
                for x in 0..w {
                    set_px(buf, w, x, y, *rgba);
                }
            }
        }
        Background::Gradient { angle_deg, stops } => {
            let rad = angle_deg.to_radians();
            let (dx, dy) = (rad.cos(), rad.sin());
            for y in 0..h {
                let v = if h > 1 {
                    y as f32 / (h - 1) as f32
                } else {
                    0.0
                };
                for x in 0..w {
                    let u = if w > 1 {
                        x as f32 / (w - 1) as f32
                    } else {
                        0.0
                    };
                    // Project onto the gradient axis, centered so 0deg = L→R.
                    let t = (u - 0.5) * dx + (v - 0.5) * dy + 0.5;
                    set_px(buf, w, x, y, gradient_color(stops, t));
                }
            }
        }
        Background::Image { .. } => {
            // Image/wallpaper backgrounds are not yet rasterized (deferred to a
            // later slice); fill a neutral gray so output stays well-defined.
            for y in 0..h {
                for x in 0..w {
                    set_px(buf, w, x, y, [0.5, 0.5, 0.5, 1.0]);
                }
            }
        }
    }
}

/// Signed distance from `(px, py)` to a rounded rectangle centered at
/// `(cx, cy)` with half-extents `(hw, hh)` and corner radius `r`. Negative
/// inside, positive outside; `|value| < 0.5` is the ~1px antialiased edge.
pub(crate) fn rounded_rect_sdf(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    r: f32,
) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let ax = qx.max(0.0);
    let ay = qy.max(0.0);
    (ax * ax + ay * ay).sqrt() + qx.max(qy).min(0.0) - r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(offset: f32, rgba: [f32; 4]) -> GradientStop {
        GradientStop { offset, rgba }
    }

    #[test]
    fn gradient_clamps_and_interpolates() {
        let stops = [
            stop(0.0, [0.0, 0.0, 0.0, 1.0]),
            stop(1.0, [1.0, 1.0, 1.0, 1.0]),
        ];
        assert_eq!(gradient_color(&stops, -0.5)[0], 0.0);
        assert_eq!(gradient_color(&stops, 1.5)[0], 1.0);
        assert!((gradient_color(&stops, 0.5)[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn sdf_is_negative_inside_and_positive_outside() {
        // 100x100 box centered at (50,50), no rounding.
        assert!(rounded_rect_sdf(50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 0.0) < 0.0);
        assert!(rounded_rect_sdf(120.0, 50.0, 50.0, 50.0, 50.0, 50.0, 0.0) > 0.0);
    }

    #[test]
    fn blend_respects_alpha() {
        let mut buf = vec![0.0f32; 4];
        set_px(&mut buf, 1, 0, 0, [0.0, 0.0, 0.0, 1.0]);
        blend_px(&mut buf, 1, 0, 0, [1.0, 1.0, 1.0, 1.0], 0.5);
        assert!((buf[0] - 0.5).abs() < 1e-6);
    }
}
