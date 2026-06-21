//! The virtual camera value types.

use crate::geometry::Point;
use serde::{Deserialize, Serialize};

/// The resolved virtual camera for one output frame.
///
/// All zoom/pan/follow effects drive this single camera (spec CAM-01). The
/// camera solver (`chroma-camera`) blends every camera-contributing modifier
/// into exactly one `CameraState` per frame, which the compositor then applies
/// to the scene inset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    /// Camera center in normalized canvas coordinates (`0.5, 0.5` = centered).
    pub center: Point,
    /// Zoom factor: `1.0` frames the whole scene, `>1.0` zooms in.
    pub scale: f32,
}

impl CameraState {
    /// The neutral camera: centered, no zoom.
    pub const IDENTITY: CameraState = CameraState {
        center: Point::new(0.5, 0.5),
        scale: 1.0,
    };
}

impl Default for CameraState {
    fn default() -> Self {
        CameraState::IDENTITY
    }
}

/// A weighted camera target contributed by a single camera-affecting modifier.
///
/// Returned from [`crate::Modifier::camera_contribution`]. The solver combines
/// overlapping targets by a documented rule (spec §3.4: later-starting modifier
/// on a higher lane wins blending weight, cross-faded over its range). `weight`
/// is the normalized blend weight in `[0.0, 1.0]` at the evaluated instant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraTarget {
    /// Desired camera center in normalized canvas coordinates.
    pub center: Point,
    /// Desired zoom factor (see [`CameraState::scale`]).
    pub scale: f32,
    /// Blend weight in `[0.0, 1.0]` for this contribution at the evaluated time.
    pub weight: f32,
}
