//! Cursor-driven modifiers: follow camera (EDT-05) and click highlight (CAM-06).

use crate::easing::{progress, trapezoid};
use chroma_core_api::{
    CameraTarget, CompositePass, EvalContext, Modifier, ModifierKind, Point, Rect, TimeRange,
};

/// Drives the virtual camera to follow the recorded cursor at a fixed zoom.
///
/// M4 returns the raw cursor as the camera target; the critically-damped spring
/// smoothing that removes cursor jitter (spec CAM-02) is `chroma-camera`'s job
/// (M5) and composes over this without changing the modifier. `tightness` is
/// carried as the blend weight so overlapping camera modifiers combine per
/// spec §3.4.
pub struct CursorFollowModifier {
    pub range: TimeRange,
    pub zoom: f32,
    pub tightness: f32,
}

impl Modifier for CursorFollowModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Camera
    }

    fn camera_contribution(&self, ctx: &EvalContext) -> Option<CameraTarget> {
        if !self.range.contains(ctx.time) {
            return None;
        }
        let center = ctx.cursor?;
        Some(CameraTarget {
            center,
            scale: self.zoom.max(1.0),
            weight: self.tightness.clamp(0.0, 1.0).max(1e-3),
        })
    }
}

/// A soft highlight around the cursor (a placeholder for the M5 click ripple).
///
/// `CompositePass` exposes only rectangles and text, so the highlight is a
/// faint square centered on the cursor; the animated radial ripple arrives with
/// the synthetic cursor in M5.
pub struct HighlightModifier {
    pub range: TimeRange,
    /// Half-size of the highlight in normalized canvas units.
    pub radius: f32,
}

impl Modifier for HighlightModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Overlay
    }

    fn paint(&self, ctx: &EvalContext, pass: &mut dyn CompositePass) {
        if !self.range.contains(ctx.time) {
            return;
        }
        let Some(c) = ctx.cursor else {
            return;
        };
        let env = trapezoid(progress(self.range, ctx.time), 0.25);
        let r = self.radius.max(0.0);
        pass.fill_rect(
            Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r),
            [1.0, 1.0, 1.0, 0.3 * env],
        );
    }
}

/// Draws a synthetic cursor marker at the smoothed cursor position (spec
/// CAM-05) — a crisp, resolution-independent arrow that replaces the recorded
/// OS cursor.
///
/// `CompositePass` exposes only rectangles, so the arrow is rasterized as a
/// stack of horizontal slices (a filled right-triangle pointer), drawn twice:
/// a dark outline behind a white fill for contrast over any background.
pub struct CursorMarkerModifier {
    pub range: TimeRange,
    /// Pointer height in normalized canvas units.
    pub size: f32,
}

/// Paints a triangular arrow with its tip at `tip`, extending down-right.
fn paint_arrow(pass: &mut dyn CompositePass, tip: Point, size: f32, rgba: [f32; 4]) {
    const SLICES: u32 = 12;
    let h = size.max(1e-3);
    let w = h * 0.6;
    for i in 0..SLICES {
        let f0 = i as f32 / SLICES as f32;
        let frac = (i + 1) as f32 / SLICES as f32;
        // Overlap slices slightly so no seams show between them.
        let row_h = h / SLICES as f32 * 1.5;
        pass.fill_rect(Rect::new(tip.x, tip.y + f0 * h, w * frac, row_h), rgba);
    }
}

impl Modifier for CursorMarkerModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Overlay
    }

    fn paint(&self, ctx: &EvalContext, pass: &mut dyn CompositePass) {
        if !self.range.contains(ctx.time) {
            return;
        }
        let Some(tip) = ctx.cursor else {
            return;
        };
        let s = self.size.max(0.0);
        // Outline first (offset up-left and larger), then the white fill.
        paint_arrow(
            pass,
            Point::new(tip.x - s * 0.06, tip.y - s * 0.06),
            s * 1.18,
            [0.0, 0.0, 0.0, 0.85],
        );
        paint_arrow(pass, tip, s, [1.0, 1.0, 1.0, 1.0]);
    }
}

/// An expanding ring emitted at a click, animated over its range (spec CAM-06).
///
/// `CompositePass` has no circle primitive, so the ring is four thin edge
/// rectangles forming a hollow square that grows from the click point and fades
/// out as it expands.
pub struct ClickRippleModifier {
    pub range: TimeRange,
    /// Click location in normalized canvas coordinates.
    pub center: Point,
    /// Outer half-size the ring grows to by the end of the range.
    pub max_radius: f32,
}

impl Modifier for ClickRippleModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Overlay
    }

    fn paint(&self, ctx: &EvalContext, pass: &mut dyn CompositePass) {
        if !self.range.contains(ctx.time) {
            return;
        }
        let p = progress(self.range, ctx.time);
        let r = self.max_radius.max(0.0) * p;
        if r <= 0.0 {
            return;
        }
        let alpha = (1.0 - p) * 0.5;
        let th = (r * 0.16).max(0.004);
        let rgba = [1.0, 1.0, 1.0, alpha];
        let (cx, cy) = (self.center.x, self.center.y);
        let side = 2.0 * r;
        // Four edges of a hollow square centered on the click.
        pass.fill_rect(Rect::new(cx - r, cy - r, side, th), rgba); // top
        pass.fill_rect(Rect::new(cx - r, cy + r - th, side, th), rgba); // bottom
        pass.fill_rect(Rect::new(cx - r, cy - r, th, side), rgba); // left
        pass.fill_rect(Rect::new(cx + r - th, cy - r, th, side), rgba); // right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{Point, Size, TimeStamp};

    fn ctx(t: u64, cursor: Option<Point>) -> EvalContext {
        EvalContext {
            time: TimeStamp::from_nanos(t),
            cursor,
            canvas: Size::new(100, 100),
            source: Size::new(100, 100),
        }
    }

    #[test]
    fn cursor_marker_paints_only_with_a_cursor() {
        use chroma_core_api::fakes::RecordingPass;
        let m = CursorMarkerModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            size: 0.05,
        };
        let mut with = RecordingPass::default();
        m.paint(&ctx(50, Some(Point::new(0.4, 0.6))), &mut with);
        assert!(!with.fills.is_empty(), "arrow drawn over a cursor");

        let mut without = RecordingPass::default();
        m.paint(&ctx(50, None), &mut without);
        assert!(without.fills.is_empty(), "no cursor → nothing drawn");
    }

    #[test]
    fn click_ripple_grows_and_fades() {
        use chroma_core_api::fakes::RecordingPass;
        let m = ClickRippleModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            center: Point::new(0.5, 0.5),
            max_radius: 0.2,
        };
        let mut early = RecordingPass::default();
        m.paint(&ctx(25, None), &mut early);
        let mut late = RecordingPass::default();
        m.paint(&ctx(75, None), &mut late);
        // Four edges each frame; the ring is wider and fainter later.
        assert_eq!(early.fills.len(), 4);
        assert_eq!(late.fills.len(), 4);
        let early_w = early.fills[0].0.width;
        let late_w = late.fills[0].0.width;
        assert!(late_w > early_w, "ring expands: {early_w} -> {late_w}");
        assert!(
            late.fills[0].1[3] < early.fills[0].1[3],
            "ring fades as it grows"
        );
        // Out of range: nothing.
        let mut out = RecordingPass::default();
        m.paint(&ctx(150, None), &mut out);
        assert!(out.fills.is_empty());
    }

    #[test]
    fn follow_targets_the_cursor() {
        let m = CursorFollowModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            zoom: 2.0,
            tightness: 0.5,
        };
        let target = m
            .camera_contribution(&ctx(50, Some(Point::new(0.3, 0.7))))
            .unwrap();
        assert_eq!(target.center, Point::new(0.3, 0.7));
        assert_eq!(target.scale, 2.0);
        // No cursor → no contribution.
        assert!(m.camera_contribution(&ctx(50, None)).is_none());
    }
}
