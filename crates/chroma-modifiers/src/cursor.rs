//! Cursor-driven modifiers: follow camera (EDT-05) and click highlight (CAM-06).

use crate::easing::{progress, trapezoid};
use chroma_core_api::{
    CameraTarget, CompositePass, EvalContext, Modifier, ModifierKind, Rect, TimeRange,
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
