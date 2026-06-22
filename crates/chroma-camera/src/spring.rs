//! A single critically-damped scalar spring.
//!
//! Uses the closed-form solution of the critically-damped harmonic oscillator
//! (damping ratio = 1), which is exact and unconditionally stable for any time
//! step — unlike an explicit Euler integrator, it never overshoots or blows up
//! at large `dt`. This is the math behind the camera's jitter-free follow
//! (spec CAM-02).

/// One scalar critically-damped spring: a position chasing a target with
/// velocity carried across steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Current position.
    pub pos: f32,
    /// Current velocity (units per second).
    pub vel: f32,
}

impl Spring {
    /// A spring at rest at `pos` (zero velocity).
    pub fn at(pos: f32) -> Self {
        Spring { pos, vel: 0.0 }
    }

    /// Advances the spring toward `target` over `dt` seconds at natural angular
    /// frequency `omega` (radians/second).
    ///
    /// `dt <= 0` is a no-op; `omega <= 0` disables smoothing (snaps to target).
    /// For zero initial velocity the motion is monotonic — critical damping
    /// guarantees no overshoot, so the camera never rubber-bands past the
    /// cursor.
    pub fn step(&mut self, target: f32, omega: f32, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        if omega <= 0.0 || !omega.is_finite() {
            self.pos = target;
            self.vel = 0.0;
            return;
        }
        // Closed-form critically-damped step. With offset x = pos - target and
        // c = v + omega*x:  x(t) = (x + c*dt) e^{-omega*dt},
        //                   v(t) = (v - omega*c*dt) e^{-omega*dt}.
        let x = self.pos - target;
        let exp = (-omega * dt).exp();
        let c = self.vel + omega * x;
        self.pos = target + (x + c * dt) * exp;
        self.vel = (self.vel - omega * c * dt) * exp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_to_target() {
        let mut s = Spring::at(0.0);
        let omega = 2.0 * std::f32::consts::PI * 3.0;
        for _ in 0..600 {
            s.step(1.0, omega, 1.0 / 60.0);
        }
        assert!(
            (s.pos - 1.0).abs() < 1e-3,
            "settled at target, got {}",
            s.pos
        );
        assert!(s.vel.abs() < 1e-2, "velocity decayed, got {}", s.vel);
    }

    #[test]
    fn never_overshoots_from_rest() {
        // Critical damping: a spring released from rest approaches monotonically.
        let mut s = Spring::at(0.0);
        let omega = 2.0 * std::f32::consts::PI * 4.0;
        let mut prev = s.pos;
        for _ in 0..300 {
            s.step(1.0, omega, 1.0 / 120.0);
            assert!(s.pos <= 1.0 + 1e-4, "overshot to {}", s.pos);
            assert!(s.pos >= prev - 1e-6, "moved backward to {}", s.pos);
            prev = s.pos;
        }
    }

    #[test]
    fn zero_dt_is_a_noop() {
        let mut s = Spring { pos: 0.2, vel: 5.0 };
        s.step(1.0, 10.0, 0.0);
        assert_eq!(s, Spring { pos: 0.2, vel: 5.0 });
    }

    #[test]
    fn zero_frequency_snaps() {
        let mut s = Spring::at(0.0);
        s.step(0.7, 0.0, 1.0 / 60.0);
        assert_eq!(s.pos, 0.7);
        assert_eq!(s.vel, 0.0);
    }

    #[test]
    fn is_deterministic() {
        let run = || {
            let mut s = Spring::at(0.0);
            for i in 0..100 {
                s.step(if i % 2 == 0 { 1.0 } else { -1.0 }, 12.0, 1.0 / 60.0);
            }
            (s.pos, s.vel)
        };
        assert_eq!(run(), run());
    }
}
