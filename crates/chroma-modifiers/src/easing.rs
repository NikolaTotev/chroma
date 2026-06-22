//! Small composed easing helpers shared by the effects.
//!
//! These are free functions a modifier *calls*, not a base class it extends
//! (composition, not inheritance — `ORCHESTRATION.md` §2).

use chroma_core_api::{TimeRange, TimeStamp};

/// Linear interpolation.
pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Smooth 0→1 ramp (Hermite `smoothstep`).
pub(crate) fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Trapezoidal envelope over `[0, 1]`: eases 0→1 over the first `edge`, holds at
/// 1, then eases 1→0 over the last `edge`. Lets a range-bounded effect animate
/// in and out without snapping at its boundaries.
pub(crate) fn trapezoid(p: f32, edge: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    let e = edge.clamp(1e-3, 0.5);
    if p < e {
        smoothstep(p / e)
    } else if p > 1.0 - e {
        smoothstep((1.0 - p) / e)
    } else {
        1.0
    }
}

/// Position of `t` within `range`, in `[0, 1]` (0 before, 1 after).
pub(crate) fn progress(range: TimeRange, t: TimeStamp) -> f32 {
    let dur = range.duration_nanos();
    if dur == 0 {
        return 0.0;
    }
    let into = t.as_nanos().saturating_sub(range.start.as_nanos());
    (into as f32 / dur as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_is_zero_at_edges_one_in_middle() {
        assert!(trapezoid(0.0, 0.2) < 0.01);
        assert!(trapezoid(1.0, 0.2) < 0.01);
        assert!((trapezoid(0.5, 0.2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn progress_spans_the_range() {
        let r = TimeRange::new(TimeStamp::from_nanos(100), TimeStamp::from_nanos(300));
        assert!((progress(r, TimeStamp::from_nanos(200)) - 0.5).abs() < 1e-6);
        assert_eq!(progress(r, TimeStamp::from_nanos(50)), 0.0);
        assert_eq!(progress(r, TimeStamp::from_nanos(400)), 1.0);
    }
}
