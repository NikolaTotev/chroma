//! Chroma deterministic render core.
//!
//! Wires the fixed §3.4 evaluation pipeline for a single output frame at time
//! `t`, as a pure function of its inputs (no wall-clock, no RNG) so preview and
//! export produce identical framing (spec EXP-06):
//!
//! 1. **Source sample** — the caller supplies the decoded source frame at `t`.
//! 2. **Camera solve** — blend every active camera modifier into one
//!    [`CameraState`].
//! 3. **Scene composite** — background, then the camera-transformed scene inset.
//! 4. **Overlay** — overlay modifiers paint in lane order, bottom lane first.
//! 5. The finished frame is returned as an [`RgbaFrame`].
//!
//! The compositor is injected as a `&mut dyn Compositor`, so the same pipeline
//! drives the software compositor (tests, export) or a wgpu one (preview) with
//! no change here.

use chroma_core_api::{
    Background, CameraSmoother, CameraState, Compositor, EvalContext, Modifier, ModifierKind,
    Point, SceneStyle, Size, SourceImage, TimeStamp,
};
use chroma_media_api::RgbaFrame;

/// The decoded source frame plus its pixel size, sampled at the render time.
pub struct SourceFrame<'a> {
    pub size: Size,
    /// Tightly-packed RGBA8 (`size.width * size.height * 4` bytes).
    pub rgba: &'a [u8],
}

/// Renders one composited output frame at `time`.
///
/// `modifiers` are in lane order (index 0 = bottom lane); camera modifiers drive
/// the virtual camera, overlay modifiers paint after the scene. `cursor` is the
/// smoothed cursor position in normalized canvas coordinates, or `None`.
///
/// `smoother` maps the solved instantaneous camera target to the camera applied
/// this frame. It is stateful (a spring keeps velocity across frames), so the
/// caller threads one smoother through the whole clip; pass a
/// [`chroma_core_api::PassthroughSmoother`] for the raw, unsmoothed camera.
#[allow(clippy::too_many_arguments)]
pub fn render_frame(
    canvas: Size,
    background: &Background,
    scene: &SceneStyle,
    source: &SourceFrame<'_>,
    cursor: Option<Point>,
    modifiers: &[Box<dyn Modifier>],
    smoother: &mut dyn CameraSmoother,
    compositor: &mut dyn Compositor,
    time: TimeStamp,
) -> RgbaFrame {
    let ctx = EvalContext {
        time,
        cursor,
        canvas,
        source: source.size,
    };

    // Stage 2: camera solve — blend modifiers into the instantaneous target,
    // then smooth it across frames (spec CAM-02).
    let target = solve_camera_target(modifiers, &ctx);
    let camera = smoother.smooth(target, &ctx);

    // Stage 3: scene composite.
    compositor.render_scene(
        canvas,
        background,
        camera,
        scene,
        SourceImage {
            size: source.size,
            rgba: source.rgba,
        },
    );

    // Stage 4: overlay pass, bottom lane first.
    for modifier in modifiers {
        if modifier.kind() == ModifierKind::Overlay && modifier.time_range().contains(time) {
            modifier.paint(&ctx, compositor);
        }
    }

    // Stage 5: hand off the finished frame.
    RgbaFrame {
        size: canvas,
        pts: time,
        data: compositor.finish(),
    }
}

/// Blends all active camera modifiers into one instantaneous [`CameraState`]
/// target by weighted average of their contributions, falling back to the
/// identity camera.
///
/// This is the raw per-frame goal; temporal smoothing of that goal across
/// frames is the injected [`CameraSmoother`]'s job (`chroma-camera`, M5).
fn solve_camera_target(modifiers: &[Box<dyn Modifier>], ctx: &EvalContext) -> CameraState {
    let (mut cx, mut cy, mut cs, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for modifier in modifiers {
        if modifier.kind() != ModifierKind::Camera || !modifier.time_range().contains(ctx.time) {
            continue;
        }
        if let Some(t) = modifier.camera_contribution(ctx) {
            let w = t.weight.max(0.0);
            cx += t.center.x * w;
            cy += t.center.y * w;
            cs += t.scale * w;
            wsum += w;
        }
    }
    if wsum > 0.0 {
        CameraState {
            center: Point::new(cx / wsum, cy / wsum),
            scale: cs / wsum,
        }
    } else {
        CameraState::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_compositor::CpuCompositor;
    use chroma_core_api::fakes::{FakeCameraModifier, FakeOverlayModifier};
    use chroma_core_api::{CameraTarget, PassthroughSmoother, Rect, TimeRange};

    fn red_source(n: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..n * n {
            v.extend_from_slice(&[255, 0, 0, 255]);
        }
        v
    }

    fn no_pad_scene() -> SceneStyle {
        SceneStyle {
            padding: 0.0,
            corner_radius: 0.0,
            shadow: None,
            border: None,
        }
    }

    fn px(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn composites_source_over_background() {
        let data = red_source(2);
        let source = SourceFrame {
            size: Size::new(2, 2),
            rgba: &data,
        };
        let mut comp = CpuCompositor::new();
        let frame = render_frame(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 1.0, 1.0]),
            &no_pad_scene(),
            &source,
            None,
            &[],
            &mut PassthroughSmoother,
            &mut comp,
            TimeStamp::from_nanos(0),
        );
        assert_eq!(frame.size, Size::new(16, 16));
        assert_eq!(frame.data.len(), RgbaFrame::expected_len(frame.size));
        assert_eq!(px(&frame.data, 16, 8, 8), [255, 0, 0, 255]);
    }

    #[test]
    fn overlay_modifier_paints_after_scene() {
        let data = red_source(2);
        let source = SourceFrame {
            size: Size::new(2, 2),
            rgba: &data,
        };
        let overlay: Box<dyn Modifier> = Box::new(FakeOverlayModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            rect: Rect::new(0.0, 0.0, 0.5, 0.5),
            rgba: [0.0, 1.0, 0.0, 1.0],
        });
        let mut comp = CpuCompositor::new();
        let frame = render_frame(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 0.0, 1.0]),
            &no_pad_scene(),
            &source,
            None,
            std::slice::from_ref(&overlay),
            &mut PassthroughSmoother,
            &mut comp,
            TimeStamp::from_nanos(10),
        );
        assert_eq!(
            px(&frame.data, 16, 2, 2),
            [0, 255, 0, 255],
            "overlay painted"
        );
        assert_eq!(
            px(&frame.data, 16, 12, 12),
            [255, 0, 0, 255],
            "source elsewhere"
        );
    }

    #[test]
    fn camera_modifier_drives_the_zoom() {
        // 2x2 source, red top-left, blue elsewhere; a camera modifier zooms in.
        let data = vec![
            255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
        ];
        let source = SourceFrame {
            size: Size::new(2, 2),
            rgba: &data,
        };
        let cam: Box<dyn Modifier> = Box::new(FakeCameraModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            target: CameraTarget {
                center: Point::new(0.25, 0.25),
                scale: 4.0,
                weight: 1.0,
            },
        });
        let mut comp = CpuCompositor::new();
        let frame = render_frame(
            Size::new(16, 16),
            &Background::Solid([0.0, 0.0, 0.0, 1.0]),
            &no_pad_scene(),
            &source,
            None,
            std::slice::from_ref(&cam),
            &mut PassthroughSmoother,
            &mut comp,
            TimeStamp::from_nanos(10),
        );
        // Zoomed onto the red texel.
        assert_eq!(px(&frame.data, 16, 8, 8), [255, 0, 0, 255]);
    }

    #[test]
    fn render_is_deterministic() {
        let data = red_source(4);
        let source = SourceFrame {
            size: Size::new(4, 4),
            rgba: &data,
        };
        let scene = SceneStyle::default();
        let render_once = || {
            let mut comp = CpuCompositor::new();
            render_frame(
                Size::new(32, 24),
                &Background::Gradient {
                    angle_deg: 30.0,
                    stops: vec![
                        chroma_core_api::GradientStop {
                            offset: 0.0,
                            rgba: [0.0, 0.0, 0.0, 1.0],
                        },
                        chroma_core_api::GradientStop {
                            offset: 1.0,
                            rgba: [0.2, 0.1, 0.4, 1.0],
                        },
                    ],
                },
                &scene,
                &source,
                None,
                &[],
                &mut PassthroughSmoother,
                &mut comp,
                TimeStamp::from_nanos(500),
            )
            .data
        };
        assert_eq!(render_once(), render_once(), "render must be byte-stable");
    }
}
