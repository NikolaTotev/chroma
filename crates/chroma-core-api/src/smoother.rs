//! The camera-smoothing contract.
//!
//! The render core blends every active camera modifier into one instantaneous
//! *target* [`CameraState`] per frame (the §3.4 camera solve). A
//! [`CameraSmoother`] maps that raw target to the camera actually applied,
//! letting a stateful implementation damp cursor jitter (spec CAM-02) without
//! the render loop knowing how. The smoother is the single place per-frame
//! camera state lives, so the pipeline stays a pure function of
//! `(inputs, smoother state)`.

use crate::camera::CameraState;
use crate::eval::EvalContext;

/// Maps the solved instantaneous camera target to the camera used this frame.
///
/// Implementations may be stateful (a spring tracks position and velocity
/// across frames); they derive any time step from [`EvalContext::time`] rather
/// than reading a wall clock, keeping evaluation deterministic (spec EXP-06).
pub trait CameraSmoother {
    /// Returns the smoothed camera for the frame at `ctx.time`, given the raw
    /// blended `target` for that instant.
    fn smooth(&mut self, target: CameraState, ctx: &EvalContext) -> CameraState;
}

/// A no-op smoother: returns the target unchanged.
///
/// This is the stateless default used by golden-frame tests and demos that want
/// the raw solved camera with no temporal smoothing. `chroma-camera` provides
/// the critically-damped spring used for cursor-follow.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughSmoother;

impl CameraSmoother for PassthroughSmoother {
    fn smooth(&mut self, target: CameraState, _ctx: &EvalContext) -> CameraState {
        target
    }
}
