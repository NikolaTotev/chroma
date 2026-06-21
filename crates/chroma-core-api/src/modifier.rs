//! The `Modifier` trait — Chroma's single extension point for effects.

use crate::camera::CameraTarget;
use crate::eval::EvalContext;
use crate::geometry::Rect;
use crate::time::TimeRange;
use serde::{Deserialize, Serialize};

/// Which render stage a modifier participates in (spec §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierKind {
    /// Contributes to the resolved virtual camera (e.g. crop/zoom,
    /// cursor-follow). Evaluated in the *camera solve* stage.
    Camera,
    /// Paints over the composited scene (e.g. text, highlights). Evaluated in
    /// the *overlay* stage, bottom lane first.
    Overlay,
}

/// The drawing sink an overlay modifier paints through.
///
/// Defined here, in the contract crate, so overlay modifiers can paint without
/// depending on `chroma-compositor`; the compositor implements this trait and
/// the render core hands a `&mut dyn CompositePass` to
/// [`Modifier::paint`] (see `DECISIONS.md`). Coordinates are normalized canvas
/// coordinates ([`crate::Point`]); colors are linear RGBA in `[0.0, 1.0]`.
pub trait CompositePass {
    /// Fills `rect` with a solid `rgba` color.
    fn fill_rect(&mut self, rect: Rect, rgba: [f32; 4]);

    /// Draws `text` with its baseline anchored near `rect`'s top-left, scaled
    /// to fit within `rect`. `rgba` is the text color. Font selection and
    /// precise layout are the compositor's concern.
    fn draw_text(&mut self, text: &str, rect: Rect, rgba: [f32; 4]);
}

/// Every visual effect implements this one trait. The render core knows only
/// `Modifier`, so new effects drop in without touching the render loop
/// (open/closed; spec §3.2, §4.3).
///
/// There is **no inheritance**: shared behavior such as easing is a composed
/// helper struct each implementor holds, never a base type. The two evaluation
/// methods default to empty no-ops — the only trait defaults Chroma permits
/// (`ORCHESTRATION.md` §2) — so a camera modifier ignores [`paint`](Self::paint)
/// and an overlay modifier ignores
/// [`camera_contribution`](Self::camera_contribution).
pub trait Modifier {
    /// The half-open time span over which this modifier is active.
    fn time_range(&self) -> TimeRange;

    /// Whether this modifier acts in the camera or overlay stage.
    fn kind(&self) -> ModifierKind;

    /// A camera modifier's weighted target at `ctx.time`, or `None` when it
    /// contributes nothing at that instant. Overlay modifiers leave this as the
    /// default `None`.
    fn camera_contribution(&self, _ctx: &EvalContext) -> Option<CameraTarget> {
        None
    }

    /// An overlay modifier's painting at `ctx.time`. Camera modifiers leave
    /// this as the default no-op.
    fn paint(&self, _ctx: &EvalContext, _pass: &mut dyn CompositePass) {}
}
