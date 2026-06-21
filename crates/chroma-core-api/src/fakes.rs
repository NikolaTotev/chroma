//! Test doubles for the core contract.
//!
//! These let downstream crates (the camera solver, the render core) be unit
//! tested in isolation against the [`Modifier`] interface with no real effect
//! implementation present (`ORCHESTRATION.md` §10). They are deliberately
//! trivial — fakes, not stubs of real behaviour.

use crate::camera::CameraTarget;
use crate::eval::EvalContext;
use crate::geometry::Rect;
use crate::modifier::{CompositePass, Modifier, ModifierKind};
use crate::time::TimeRange;

/// A camera modifier that returns a fixed [`CameraTarget`] for its whole range.
pub struct FakeCameraModifier {
    /// The range this fake reports as active.
    pub range: TimeRange,
    /// The target returned from every in-range
    /// [`camera_contribution`](Modifier::camera_contribution).
    pub target: CameraTarget,
}

impl Modifier for FakeCameraModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Camera
    }

    fn camera_contribution(&self, ctx: &EvalContext) -> Option<CameraTarget> {
        self.range.contains(ctx.time).then_some(self.target)
    }
}

/// An overlay modifier that fills a fixed rectangle when active.
pub struct FakeOverlayModifier {
    /// The range this fake reports as active.
    pub range: TimeRange,
    /// The rectangle filled by [`paint`](Modifier::paint).
    pub rect: Rect,
    /// The fill color (linear RGBA).
    pub rgba: [f32; 4],
}

impl Modifier for FakeOverlayModifier {
    fn time_range(&self) -> TimeRange {
        self.range
    }

    fn kind(&self) -> ModifierKind {
        ModifierKind::Overlay
    }

    fn paint(&self, ctx: &EvalContext, pass: &mut dyn CompositePass) {
        if self.range.contains(ctx.time) {
            pass.fill_rect(self.rect, self.rgba);
        }
    }
}

/// A [`CompositePass`] that records every draw call, for asserting on overlay
/// output without a GPU.
#[derive(Default)]
pub struct RecordingPass {
    /// `(rect, rgba)` for each `fill_rect` call, in call order.
    pub fills: Vec<(Rect, [f32; 4])>,
    /// `(text, rect, rgba)` for each `draw_text` call, in call order.
    pub texts: Vec<(String, Rect, [f32; 4])>,
}

impl CompositePass for RecordingPass {
    fn fill_rect(&mut self, rect: Rect, rgba: [f32; 4]) {
        self.fills.push((rect, rgba));
    }

    fn draw_text(&mut self, text: &str, rect: Rect, rgba: [f32; 4]) {
        self.texts.push((text.to_owned(), rect, rgba));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::time::TimeStamp;

    fn ctx_at(nanos: u64) -> EvalContext {
        EvalContext {
            time: TimeStamp::from_nanos(nanos),
            cursor: Some(Point::new(0.5, 0.5)),
            canvas: Size::new(1920, 1080),
            source: Size::new(1920, 1080),
        }
    }

    #[test]
    fn camera_fake_contributes_only_in_range() {
        let m = FakeCameraModifier {
            range: TimeRange::new(TimeStamp::from_nanos(10), TimeStamp::from_nanos(20)),
            target: CameraTarget {
                center: Point::new(0.25, 0.75),
                scale: 2.0,
                weight: 1.0,
            },
        };
        assert!(m.camera_contribution(&ctx_at(5)).is_none());
        assert_eq!(m.camera_contribution(&ctx_at(15)).unwrap().scale, 2.0);
        assert!(
            m.camera_contribution(&ctx_at(20)).is_none(),
            "end exclusive"
        );
    }

    #[test]
    fn overlay_fake_paints_only_in_range() {
        let m = FakeOverlayModifier {
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100)),
            rect: Rect::new(0.1, 0.1, 0.2, 0.2),
            rgba: [1.0, 0.0, 0.0, 1.0],
        };
        let mut pass = RecordingPass::default();
        m.paint(&ctx_at(50), &mut pass);
        m.paint(&ctx_at(150), &mut pass);
        assert_eq!(pass.fills.len(), 1);
        assert_eq!(pass.fills[0].1, [1.0, 0.0, 0.0, 1.0]);
    }
}
