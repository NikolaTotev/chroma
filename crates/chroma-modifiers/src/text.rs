//! Text overlay modifier (spec EDT-04).

use crate::easing::{progress, trapezoid};
use chroma_core_api::{CompositePass, EvalContext, Modifier, ModifierKind, Rect, TimeRange};

/// Paints styled text over the scene for its time range, fading in and out.
pub struct TextModifier {
    pub range: TimeRange,
    pub content: String,
    /// Placement and extent in normalized canvas coordinates.
    pub rect: Rect,
    /// Linear-RGBA text color; its alpha is modulated by the fade envelope.
    pub rgba: [f32; 4],
}

impl Modifier for TextModifier {
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
        let env = trapezoid(progress(self.range, ctx.time), 0.15);
        let mut color = self.rgba;
        color[3] *= env;
        pass.draw_text(&self.content, self.rect, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::fakes::RecordingPass;
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
    fn paints_only_within_range() {
        let m = TextModifier {
            range: TimeRange::new(TimeStamp::from_nanos(0), TimeStamp::from_nanos(100)),
            content: "hi".to_owned(),
            rect: Rect::new(0.1, 0.1, 0.5, 0.2),
            rgba: [1.0, 1.0, 1.0, 1.0],
        };
        let mut pass = RecordingPass::default();
        m.paint(&ctx(50), &mut pass);
        m.paint(&ctx(150), &mut pass);
        assert_eq!(pass.texts.len(), 1);
        assert_eq!(pass.texts[0].0, "hi");
    }
}
