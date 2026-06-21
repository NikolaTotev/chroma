//! The per-frame evaluation context handed to every modifier.

use crate::geometry::{Point, Size};
use crate::time::TimeStamp;

/// Everything a [`crate::Modifier`] is allowed to see when evaluated for one
/// output frame.
///
/// The render core builds one `EvalContext` per frame from a pure function of
/// `(Project, t)` — it carries no wall-clock and no RNG, so evaluation is
/// deterministic (spec §3.4, EXP-06). Modifiers read from it but never mutate
/// it; the source media and event log it derives from are immutable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalContext {
    /// The exact instant being rendered, on the project timebase.
    pub time: TimeStamp,
    /// The smoothed cursor position in normalized canvas coordinates at
    /// [`time`](Self::time), or `None` if the cursor is hidden / off-canvas.
    pub cursor: Option<Point>,
    /// The output canvas size in pixels.
    pub canvas: Size,
    /// The decoded source frame size in pixels (may differ from `canvas`).
    pub source: Size,
}
