//! Chroma effects.
//!
//! One struct per effect, each implementing [`chroma_core_api::Modifier`] and
//! holding composed easing helpers — never inheriting (spec §3.2). New effects
//! are added here without touching the render loop (open/closed).
//!
//! [`build`] is the data→behaviour bridge: it turns a serializable
//! [`ModifierSpec`] (from the project model) into a `Box<dyn Modifier>`, so
//! `chroma-project` can (de)serialize effects without depending on this crate
//! (see `DECISIONS.md`).

mod cropzoom;
mod cursor;
mod easing;
mod keyframe_camera;
mod text;

pub use cropzoom::CropZoomModifier;
pub use cursor::{
    ClickRippleModifier, CursorFollowModifier, CursorMarkerModifier, HighlightModifier,
};
pub use keyframe_camera::KeyframeCameraModifier;
pub use text::TextModifier;

use chroma_core_api::{Modifier, ModifierParams, ModifierSpec};

/// Builds the concrete [`Modifier`] described by `spec`.
pub fn build(spec: &ModifierSpec) -> Box<dyn Modifier> {
    match &spec.params {
        ModifierParams::CropZoom { target } => Box::new(CropZoomModifier {
            range: spec.range,
            target: *target,
        }),
        ModifierParams::Text {
            content,
            rect,
            rgba,
        } => Box::new(TextModifier {
            range: spec.range,
            content: content.clone(),
            rect: *rect,
            rgba: *rgba,
        }),
        ModifierParams::CursorFollow { zoom, tightness } => Box::new(CursorFollowModifier {
            range: spec.range,
            zoom: *zoom,
            tightness: *tightness,
        }),
        ModifierParams::Highlight { radius } => Box::new(HighlightModifier {
            range: spec.range,
            radius: *radius,
        }),
        ModifierParams::CursorMarker { size } => Box::new(CursorMarkerModifier {
            range: spec.range,
            size: *size,
        }),
        ModifierParams::ClickRipple { center, max_radius } => Box::new(ClickRippleModifier {
            range: spec.range,
            center: *center,
            max_radius: *max_radius,
        }),
        ModifierParams::KeyframeCamera {
            center_x,
            center_y,
            scale,
            weight,
        } => Box::new(KeyframeCameraModifier {
            range: spec.range,
            center_x: center_x.clone(),
            center_y: center_y.clone(),
            scale: scale.clone(),
            weight: *weight,
        }),
    }
}

/// Builds every modifier in a project's lane list, preserving order.
pub fn build_all(specs: &[ModifierSpec]) -> Vec<Box<dyn Modifier>> {
    specs.iter().map(build).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{ModifierKind, Rect, TimeRange, TimeStamp};

    #[test]
    fn factory_maps_each_param_to_the_right_kind() {
        let range = TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(100));
        let camera = build(&ModifierSpec {
            kind: ModifierKind::Camera,
            range,
            params: ModifierParams::CropZoom {
                target: Rect::new(0.0, 0.0, 0.3, 0.3),
            },
        });
        assert_eq!(camera.kind(), ModifierKind::Camera);

        let overlay = build(&ModifierSpec {
            kind: ModifierKind::Overlay,
            range,
            params: ModifierParams::Text {
                content: "x".to_owned(),
                rect: Rect::FULL,
                rgba: [1.0; 4],
            },
        });
        assert_eq!(overlay.kind(), ModifierKind::Overlay);
        assert_eq!(overlay.time_range(), range);
    }

    #[test]
    fn build_all_preserves_count() {
        let specs = vec![
            ModifierSpec {
                kind: ModifierKind::Camera,
                range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(10)),
                params: ModifierParams::CursorFollow {
                    zoom: 2.0,
                    tightness: 0.5,
                },
            },
            ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(10)),
                params: ModifierParams::Highlight { radius: 0.05 },
            },
        ];
        assert_eq!(build_all(&specs).len(), 2);
    }
}
