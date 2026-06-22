//! Crop/Zoom camera modifier (spec EDT-03).

use crate::easing::{lerp, progress, trapezoid};
use chroma_core_api::{CameraTarget, EvalContext, Modifier, ModifierKind, Point, Rect, TimeRange};

/// Animates the virtual camera from the current framing to a target crop
/// rectangle over its time range, holding at the crop, then easing back out.
///
/// `target` is the rectangle (in normalized source coordinates) to zoom into;
/// the camera scale is derived so that rectangle fills the view.
pub struct CropZoomModifier {
    pub range: TimeRange,
    pub target: Rect,
}

impl Modifier for CropZoomModifier {
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
        let env = trapezoid(progress(self.range, ctx.time), 0.2);
        let center = self.target.center();
        // Zoom so the longer side of the target fills the view; clamp to a sane
        // range so a tiny target can't zoom to absurd magnification.
        let crop_scale =
            (1.0 / self.target.width.max(self.target.height).max(1e-3)).clamp(1.0, 20.0);
        Some(CameraTarget {
            center: Point::new(lerp(0.5, center.x, env), lerp(0.5, center.y, env)),
            scale: lerp(1.0, crop_scale, env),
            weight: 1.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{Size, TimeStamp};

    fn ctx(t: u64) -> EvalContext {
        EvalContext {
            time: TimeStamp::from_nanos(t),
            cursor: None,
            canvas: Size::new(100, 100),
            source: Size::new(100, 100),
        }
    }

    #[test]
    fn zooms_toward_target_at_midpoint_and_is_inactive_outside() {
        let m = CropZoomModifier {
            range: TimeRange::new(TimeStamp::from_nanos(0), TimeStamp::from_nanos(100)),
            target: Rect::new(0.0, 0.0, 0.25, 0.25),
        };
        assert!(m.camera_contribution(&ctx(150)).is_none());
        let mid = m.camera_contribution(&ctx(50)).unwrap();
        assert!(
            mid.scale > 1.5,
            "should be zoomed in at midpoint: {}",
            mid.scale
        );
        assert!(
            mid.center.x < 0.5 && mid.center.y < 0.5,
            "panned toward the corner"
        );
    }
}
