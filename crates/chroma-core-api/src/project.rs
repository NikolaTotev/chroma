//! The serializable project model.

use crate::geometry::{Rect, Size};
use crate::modifier::ModifierKind;
use crate::time::TimeRange;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The self-describing, versioned project document (spec EDT-11).
///
/// A `Project` is a plain serializable value type: it references the immutable
/// source media and lists the modifiers as data ([`ModifierSpec`]), never as
/// behaviour. `chroma-modifiers` builds `Box<dyn Modifier>` from each spec at
/// load time (see `DECISIONS.md`), keeping (de)serialization free of any
/// modifier implementation dependency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Project-format version, bumped when the schema or §3.4 evaluation order
    /// changes. Migration is the concern of `chroma-project`.
    pub version: u32,
    /// The immutable recorded media this project edits.
    pub source: SourceMedia,
    /// Output canvas size; may differ from the source size (spec BG-05).
    pub canvas: Size,
    /// The background layer the scene is composited over (spec BG-01/02).
    pub background: Background,
    /// The modifier lanes, in lane order (index 0 = bottom lane). Evaluation
    /// order within a stage is by this index (spec §3.4).
    pub modifiers: Vec<ModifierSpec>,
}

/// The immutable inputs produced by a capture session.
///
/// Source media is never mutated after capture; effects are evaluated against
/// it, never baked in (spec §3.2 non-destructive invariant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMedia {
    /// Path to the recorded screen video.
    pub video_path: PathBuf,
    /// Path to the serialized, timestamped input-event log.
    pub event_log_path: PathBuf,
    /// Captured source frame rate (spec CAP-06).
    pub fps: u32,
    /// Captured source resolution in pixels.
    pub size: Size,
}

/// The background layer beneath the scene inset (spec BG-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Background {
    /// A single linear-RGBA color.
    Solid([f32; 4]),
    /// A multi-stop gradient.
    Gradient {
        /// Gradient angle in degrees, clockwise from the +x axis.
        angle_deg: f32,
        /// Ordered color stops (at least two for a meaningful gradient).
        stops: Vec<GradientStop>,
    },
    /// An image or wallpaper loaded from disk.
    Image { path: PathBuf },
}

/// One stop in a [`Background::Gradient`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient in `[0.0, 1.0]`.
    pub offset: f32,
    /// Linear-RGBA color at this stop.
    pub rgba: [f32; 4],
}

/// A serializable description of one modifier on the timeline.
///
/// This is the data half of the data/behaviour split: it names the effect, its
/// time span, and its parameters, but carries no logic. `chroma-modifiers`
/// constructs the corresponding `dyn Modifier` from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifierSpec {
    /// The render stage this modifier acts in, derived from its params but
    /// stored explicitly so consumers can group lanes without matching on
    /// [`params`](Self::params).
    pub kind: ModifierKind,
    /// The active time span (spec EDT-02).
    pub range: TimeRange,
    /// The effect kind and its tunable parameters.
    pub params: ModifierParams,
}

/// The parameters for each built-in effect (spec §2.2, §2.4).
///
/// One variant per modifier struct in `chroma-modifiers`. New effects extend
/// this enum; the render loop is untouched (open/closed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModifierParams {
    /// Animate the visible crop rectangle to a target (spec EDT-03).
    CropZoom {
        /// The crop rectangle to animate toward, in normalized coordinates.
        target: Rect,
    },
    /// Display styled text over the scene (spec EDT-04).
    Text {
        /// The text to display.
        content: String,
        /// Placement and extent in normalized canvas coordinates.
        rect: Rect,
        /// Linear-RGBA text color.
        rgba: [f32; 4],
    },
    /// Drive the camera to follow the recorded cursor (spec EDT-05, CAM-05).
    CursorFollow {
        /// Zoom factor while following.
        zoom: f32,
        /// Follow tightness in `[0.0, 1.0]`: higher tracks the cursor more
        /// aggressively, lower is smoother/looser.
        tightness: f32,
    },
    /// A click ripple / highlight around the cursor (spec CAM-06).
    Highlight {
        /// Highlight radius in normalized canvas units.
        radius: f32,
    },
}
