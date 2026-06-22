//! A camera move driven by keyframe tracks (spec EDT-06).
//!
//! Where `CursorFollow` derives the camera from live cursor data, this modifier
//! drives the camera from authored [`Track`]s — the GUI's way to lay down an
//! explicit zoom/pan over the timeline. It holds three independent tracks and
//! samples them at frame time; the spring smoother still glides the result.

use chroma_core_api::{CameraTarget, EvalContext, Modifier, ModifierKind, Point, TimeRange, Track};

/// Drives the virtual camera from per-component keyframe tracks.
pub struct KeyframeCameraModifier {
    pub range: TimeRange,
    pub center_x: Track,
    pub center_y: Track,
    pub scale: Track,
    pub weight: f32,
}

impl Modifier for KeyframeCameraModifier {
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
        // Empty tracks fall back to the identity camera value for that axis.
        let cx = self.center_x.sample(ctx.time).unwrap_or(0.5);
        let cy = self.center_y.sample(ctx.time).unwrap_or(0.5);
        let scale = self.scale.sample(ctx.time).unwrap_or(1.0);
        Some(CameraTarget {
            center: Point::new(cx, cy),
            scale: scale.max(1.0),
            weight: self.weight.clamp(0.0, 1.0).max(1e-3),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{Easing, Keyframe, Size, TimeStamp};

    fn ctx(t: u64) -> EvalContext {
        EvalContext {
            time: TimeStamp::from_nanos(t),
            cursor: None,
            canvas: Size::new(100, 100),
            source: Size::new(100, 100),
        }
    }

    #[test]
    fn samples_tracks_over_time() {
        let m = KeyframeCameraModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(200)),
            center_x: Track::new(vec![
                Keyframe {
                    time: TimeStamp::from_nanos(0),
                    value: 0.2,
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: TimeStamp::from_nanos(100),
                    value: 0.8,
                    easing: Easing::Linear,
                },
            ]),
            center_y: Track::constant(0.5),
            scale: Track::constant(2.0),
            weight: 1.0,
        };
        let mid = m.camera_contribution(&ctx(50)).unwrap();
        assert!((mid.center.x - 0.5).abs() < 1e-5, "linear midpoint");
        assert_eq!(mid.center.y, 0.5);
        assert_eq!(mid.scale, 2.0);
    }

    #[test]
    fn empty_tracks_are_identity() {
        let m = KeyframeCameraModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            center_x: Track::default(),
            center_y: Track::default(),
            scale: Track::default(),
            weight: 1.0,
        };
        let c = m.camera_contribution(&ctx(50)).unwrap();
        assert_eq!(c.center, Point::new(0.5, 0.5));
        assert_eq!(c.scale, 1.0);
    }

    #[test]
    fn out_of_range_contributes_nothing() {
        let m = KeyframeCameraModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            center_x: Track::constant(0.3),
            center_y: Track::constant(0.3),
            scale: Track::constant(1.5),
            weight: 1.0,
        };
        assert!(m.camera_contribution(&ctx(150)).is_none());
    }
}
