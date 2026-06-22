//! Chroma CPU reference compositor.
//!
//! A deterministic, software implementation of [`chroma_core_api::Compositor`]:
//! it paints the background, then the camera-transformed scene inset (padding,
//! corner radius, drop shadow, border), and exposes a [`CompositePass`] so
//! overlay modifiers can paint on top. Output is tightly-packed RGBA8.
//!
//! This is the *reference* compositor: correct and golden-frame testable on any
//! machine with no GPU. A wgpu compositor (spec §3.1, for live-preview
//! performance) is a later swap behind the same trait — no consumer changes
//! (see `DECISIONS.md`).

mod raster;

use chroma_core_api::{
    Background, CameraState, CompositePass, Compositor, Rect, SceneStyle, Size, SourceImage,
};
use raster::{blend_px, fill_background, rounded_rect_sdf};

/// A software compositor accumulating one frame in a linear `RGBA f32` buffer.
#[derive(Default)]
pub struct CpuCompositor {
    canvas: Size,
    /// `width * height * 4`, linear RGBA, straight alpha.
    buf: Vec<f32>,
}

impl CpuCompositor {
    /// Creates an empty compositor; the buffer is sized on the first
    /// [`render_scene`](Compositor::render_scene).
    pub fn new() -> Self {
        CpuCompositor {
            canvas: Size::new(0, 0),
            buf: Vec::new(),
        }
    }

    /// Converts a normalized canvas rect to clamped integer pixel bounds
    /// `(x0, y0, x1, y1)` (half-open).
    fn pixel_bounds(&self, rect: Rect) -> (usize, usize, usize, usize) {
        let w = self.canvas.width as f32;
        let h = self.canvas.height as f32;
        let x0 = (rect.x * w).floor().clamp(0.0, w) as usize;
        let y0 = (rect.y * h).floor().clamp(0.0, h) as usize;
        let x1 = ((rect.x + rect.width) * w).ceil().clamp(0.0, w) as usize;
        let y1 = ((rect.y + rect.height) * h).ceil().clamp(0.0, h) as usize;
        (x0, y0, x1, y1)
    }
}

impl Compositor for CpuCompositor {
    fn render_scene(
        &mut self,
        canvas: Size,
        background: &Background,
        camera: CameraState,
        style: &SceneStyle,
        source: SourceImage<'_>,
    ) {
        let w = canvas.width as usize;
        let h = canvas.height as usize;
        self.canvas = canvas;
        self.buf = vec![0.0; w * h * 4];

        fill_background(&mut self.buf, w, h, background);
        if w == 0 || h == 0 {
            return;
        }

        // Scene inset rectangle in pixels.
        let pad = style.padding.clamp(0.0, 0.49);
        let ix = pad * w as f32;
        let iy = pad * h as f32;
        let iw = (w as f32 - 2.0 * ix).max(1.0);
        let ih = (h as f32 - 2.0 * iy).max(1.0);
        let (cx, cy) = (ix + iw / 2.0, iy + ih / 2.0);
        let (hw, hh) = (iw / 2.0, ih / 2.0);
        let radius = (style.corner_radius.clamp(0.0, 0.5) * iw.min(ih)).min(hw.min(hh));
        let minside = w.min(h) as f32;

        // Drop shadow (feathered rounded rect behind the inset).
        if let Some(shadow) = style.shadow {
            let blur = (shadow.blur * minside).max(1.0);
            let scx = cx + shadow.dx * w as f32;
            let scy = cy + shadow.dy * h as f32;
            let (x0, y0, x1, y1) = bbox(
                scx - hw - blur,
                scy - hh - blur,
                scx + hw + blur,
                scy + hh + blur,
                w,
                h,
            );
            for y in y0..y1 {
                for x in x0..x1 {
                    let d =
                        rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, scx, scy, hw, hh, radius);
                    let cov = (0.5 - d / blur).clamp(0.0, 1.0);
                    blend_px(&mut self.buf, w, x, y, shadow.rgba, cov);
                }
            }
        }

        // Camera-transformed source inset.
        let half = 0.5 / camera.scale.max(1e-3);
        let sw = source.size.width as usize;
        let sh = source.size.height as usize;
        let (x0, y0, x1, y1) = bbox(ix, iy, ix + iw, iy + ih, w, h);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, cx, cy, hw, hh, radius);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov <= 0.0 || sw == 0 || sh == 0 {
                    continue;
                }
                let lu = ((x as f32 + 0.5) - ix) / iw;
                let lv = ((y as f32 + 0.5) - iy) / ih;
                let su = camera.center.x - half + lu * 2.0 * half;
                let sv = camera.center.y - half + lv * 2.0 * half;
                let sx = (su * sw as f32).clamp(0.0, sw as f32 - 1.0) as usize;
                let sy = (sv * sh as f32).clamp(0.0, sh as f32 - 1.0) as usize;
                let si = (sy * sw + sx) * 4;
                if si + 3 >= source.rgba.len() {
                    continue;
                }
                let rgba = [
                    source.rgba[si] as f32 / 255.0,
                    source.rgba[si + 1] as f32 / 255.0,
                    source.rgba[si + 2] as f32 / 255.0,
                    source.rgba[si + 3] as f32 / 255.0,
                ];
                blend_px(&mut self.buf, w, x, y, rgba, cov);
            }
        }

        // Border stroked on the inset edge.
        if let Some(border) = style.border {
            let bw = (border.width * minside).max(1.0);
            for y in y0..y1 {
                for x in x0..x1 {
                    let d =
                        rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, cx, cy, hw, hh, radius)
                            .abs();
                    let cov = (0.5 - (d - bw / 2.0)).clamp(0.0, 1.0);
                    blend_px(&mut self.buf, w, x, y, border.rgba, cov);
                }
            }
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        self.buf
            .iter()
            .map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
            .collect()
    }
}

impl CompositePass for CpuCompositor {
    fn fill_rect(&mut self, rect: Rect, rgba: [f32; 4]) {
        let w = self.canvas.width as usize;
        let (x0, y0, x1, y1) = self.pixel_bounds(rect);
        for y in y0..y1 {
            for x in x0..x1 {
                blend_px(&mut self.buf, w, x, y, rgba, 1.0);
            }
        }
    }

    fn draw_text(&mut self, _text: &str, rect: Rect, rgba: [f32; 4]) {
        // Glyph rasterization arrives with the Text modifier (M4, via a font
        // crate). Until then, mark the text box faintly so overlay placement is
        // visible in preview and tests.
        let mut faint = rgba;
        faint[3] *= 0.25;
        self.fill_rect(rect, faint);
    }
}

/// Clamps a float rectangle to half-open integer pixel bounds within `w * h`.
fn bbox(x0: f32, y0: f32, x1: f32, y1: f32, w: usize, h: usize) -> (usize, usize, usize, usize) {
    (
        x0.floor().clamp(0.0, w as f32) as usize,
        y0.floor().clamp(0.0, h as f32) as usize,
        x1.ceil().clamp(0.0, w as f32) as usize,
        y1.ceil().clamp(0.0, h as f32) as usize,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::Point;

    /// A solid `n x n` RGBA8 source of one color.
    fn solid_source(n: u32, rgba: [u8; 4]) -> (Size, Vec<u8>) {
        let mut data = Vec::with_capacity((n * n * 4) as usize);
        for _ in 0..n * n {
            data.extend_from_slice(&rgba);
        }
        (Size::new(n, n), data)
    }

    fn px(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn no_padding_fills_canvas_with_source() {
        let (ssize, sdata) = solid_source(2, [255, 0, 0, 255]);
        let style = SceneStyle {
            padding: 0.0,
            corner_radius: 0.0,
            shadow: None,
            border: None,
        };
        let mut c = CpuCompositor::new();
        c.render_scene(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 1.0, 1.0]),
            CameraState::IDENTITY,
            &style,
            SourceImage {
                size: ssize,
                rgba: &sdata,
            },
        );
        let out = c.finish();
        // Center pixel is the red source, not the blue background.
        assert_eq!(px(&out, 16, 8, 8), [255, 0, 0, 255]);
    }

    #[test]
    fn padding_shows_background_in_the_margin() {
        let (ssize, sdata) = solid_source(2, [255, 0, 0, 255]);
        let style = SceneStyle {
            padding: 0.25,
            corner_radius: 0.0,
            shadow: None,
            border: None,
        };
        let mut c = CpuCompositor::new();
        c.render_scene(
            Size::new(20, 20),
            &Background::Solid([0.0, 0.0, 1.0, 1.0]),
            CameraState::IDENTITY,
            &style,
            SourceImage {
                size: ssize,
                rgba: &sdata,
            },
        );
        let out = c.finish();
        assert_eq!(px(&out, 20, 0, 0), [0, 0, 255, 255], "corner is background");
        assert_eq!(px(&out, 20, 10, 10), [255, 0, 0, 255], "center is source");
    }

    #[test]
    fn overlay_fill_rect_paints_over_scene() {
        let (ssize, sdata) = solid_source(2, [255, 0, 0, 255]);
        let style = SceneStyle {
            padding: 0.0,
            corner_radius: 0.0,
            shadow: None,
            border: None,
        };
        let mut c = CpuCompositor::new();
        c.render_scene(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 0.0, 1.0]),
            CameraState::IDENTITY,
            &style,
            SourceImage {
                size: ssize,
                rgba: &sdata,
            },
        );
        c.fill_rect(Rect::new(0.0, 0.0, 0.5, 0.5), [0.0, 1.0, 0.0, 1.0]);
        let out = c.finish();
        assert_eq!(
            px(&out, 16, 2, 2),
            [0, 255, 0, 255],
            "top-left overlaid green"
        );
        assert_eq!(px(&out, 16, 12, 12), [255, 0, 0, 255], "rest still source");
    }

    #[test]
    fn camera_zoom_samples_a_sub_region() {
        // 2x2 source: top-left red, others blue. Zoomed in on the top-left.
        let size = Size::new(2, 2);
        let data = vec![
            255, 0, 0, 255, // (0,0) red
            0, 0, 255, 255, // (1,0) blue
            0, 0, 255, 255, // (0,1) blue
            0, 0, 255, 255, // (1,1) blue
        ];
        let style = SceneStyle {
            padding: 0.0,
            corner_radius: 0.0,
            shadow: None,
            border: None,
        };
        let camera = CameraState {
            center: Point::new(0.25, 0.25),
            scale: 4.0,
        };
        let mut c = CpuCompositor::new();
        c.render_scene(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 0.0, 1.0]),
            camera,
            &style,
            SourceImage { size, rgba: &data },
        );
        let out = c.finish();
        // Zoomed onto the red top-left texel: whole canvas should be red.
        assert_eq!(px(&out, 16, 8, 8), [255, 0, 0, 255]);
    }
}
