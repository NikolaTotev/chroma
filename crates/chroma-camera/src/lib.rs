//! Chroma camera smoothing.
//!
//! Implements the critically-damped spring that turns the render core's raw
//! per-frame camera target into smooth motion (spec CAM-02). [`SpringSmoother`]
//! is a [`CameraSmoother`]: the render loop blends camera modifiers into one
//! target per frame, and this crate damps that target's path so cursor-follow
//! glides instead of snapping.
//!
//! It is the only stateful step in the otherwise pure render pipeline; keep one
//! smoother per render pass. There is no inheritance — the smoother *holds*
//! three independent [`Spring`]s (composition, spec §3.2).

mod spring;

pub use spring::Spring;

use chroma_core_api::{CameraSmoother, CameraState, EvalContext, Point};
use std::f32::consts::PI;

/// Tunable spring response. Damping is fixed critical (no overshoot); only the
/// stiffness is exposed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringParams {
    /// Natural frequency in Hz. Higher snaps to the target faster; lower is
    /// smoother and laggier. A few Hz reads as a deliberate, smooth camera.
    pub frequency_hz: f32,
}

impl Default for SpringParams {
    fn default() -> Self {
        SpringParams { frequency_hz: 3.0 }
    }
}

/// A critically-damped spring smoother for the virtual camera.
///
/// Smooths the camera center (x, y) and zoom independently with a shared
/// frequency. Stateful across frames: it derives its time step from
/// [`EvalContext::time`] (no wall clock, so smoothing stays deterministic), and
/// snaps to the first frame's target so the clip opens already framed.
pub struct SpringSmoother {
    params: SpringParams,
    state: Option<State>,
    last_ns: u64,
}

#[derive(Clone, Copy)]
struct State {
    cx: Spring,
    cy: Spring,
    scale: Spring,
}

impl SpringSmoother {
    /// A smoother with the given response.
    pub fn new(params: SpringParams) -> Self {
        SpringSmoother {
            params,
            state: None,
            last_ns: 0,
        }
    }

    /// Clears the smoothing state so the next frame snaps to its target.
    ///
    /// Call this after a timeline seek: the spring must not glide across a jump
    /// in time, or the camera would smear between unrelated instants.
    pub fn reset(&mut self) {
        self.state = None;
    }
}

impl Default for SpringSmoother {
    fn default() -> Self {
        SpringSmoother::new(SpringParams::default())
    }
}

impl CameraSmoother for SpringSmoother {
    fn smooth(&mut self, target: CameraState, ctx: &EvalContext) -> CameraState {
        let now = ctx.time.as_nanos();
        let omega = 2.0 * PI * self.params.frequency_hz.max(0.0);
        match &mut self.state {
            None => {
                self.state = Some(State {
                    cx: Spring::at(target.center.x),
                    cy: Spring::at(target.center.y),
                    scale: Spring::at(target.scale),
                });
                self.last_ns = now;
                target
            }
            Some(state) => {
                // saturating_sub makes a backward step a zero-dt no-op; an
                // editor seek should call `reset()` rather than relying on this.
                let dt = now.saturating_sub(self.last_ns) as f32 / 1e9;
                state.cx.step(target.center.x, omega, dt);
                state.cy.step(target.center.y, omega, dt);
                state.scale.step(target.scale, omega, dt);
                self.last_ns = now;
                CameraState {
                    center: Point::new(state.cx.pos, state.cy.pos),
                    scale: state.scale.pos,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{CameraState, Point, Size, TimeStamp};

    fn ctx(t_ns: u64) -> EvalContext {
        EvalContext {
            time: TimeStamp::from_nanos(t_ns),
            cursor: None,
            canvas: Size::new(100, 100),
            source: Size::new(100, 100),
        }
    }

    fn target(cx: f32, scale: f32) -> CameraState {
        CameraState {
            center: Point::new(cx, 0.5),
            scale,
        }
    }

    #[test]
    fn first_frame_snaps_to_target() {
        let mut s = SpringSmoother::default();
        let out = s.smooth(target(0.2, 2.0), &ctx(0));
        assert_eq!(out, target(0.2, 2.0));
    }

    #[test]
    fn lags_then_catches_up() {
        let mut s = SpringSmoother::new(SpringParams { frequency_hz: 3.0 });
        s.smooth(target(0.5, 1.0), &ctx(0)); // snap to start
                                             // Jump the target; the smoothed camera should trail it immediately…
        let frame_ns = 1_000_000_000 / 60;
        let early = s.smooth(target(0.9, 1.0), &ctx(frame_ns));
        assert!(
            early.center.x < 0.9,
            "camera lags the jump, got {}",
            early.center.x
        );
        assert!(
            early.center.x > 0.5,
            "but moves toward it, got {}",
            early.center.x
        );
        // …and converge after enough frames.
        let mut last = early;
        for i in 2..200 {
            last = s.smooth(target(0.9, 1.0), &ctx(i * frame_ns));
        }
        assert!(
            (last.center.x - 0.9).abs() < 1e-2,
            "converged, got {}",
            last.center.x
        );
    }

    #[test]
    fn reset_resnaps() {
        let mut s = SpringSmoother::default();
        s.smooth(target(0.5, 1.0), &ctx(0));
        s.reset();
        let out = s.smooth(target(0.1, 3.0), &ctx(1_000_000));
        assert_eq!(out, target(0.1, 3.0), "after reset the next frame snaps");
    }

    #[test]
    fn deterministic_across_runs() {
        let run = || {
            let mut s = SpringSmoother::default();
            let frame_ns = 1_000_000_000 / 30;
            let mut out = CameraState::IDENTITY;
            for i in 0..90 {
                let cx = 0.5 + 0.3 * (i as f32 * 0.2).sin();
                out = s.smooth(target(cx, 1.5), &ctx(i * frame_ns));
            }
            (out.center.x, out.center.y, out.scale)
        };
        assert_eq!(run(), run());
    }
}
