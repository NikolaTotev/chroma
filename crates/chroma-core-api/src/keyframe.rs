//! Keyframe animation tracks (spec EDT-06).
//!
//! A [`Track`] is the project model's primitive for animating a scalar
//! parameter over time: an ordered list of [`Keyframe`]s the render core
//! samples at frame time. It is plain serializable data — the GUI edits tracks,
//! `chroma-modifiers` samples them. Interpolation between two keys is controlled
//! by the *earlier* key's [`Easing`].

use crate::time::TimeStamp;
use serde::{Deserialize, Serialize};

/// How a value interpolates from one keyframe toward the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Easing {
    /// Step: hold this key's value until the next key (no interpolation).
    Hold,
    /// Constant-rate linear interpolation.
    Linear,
    /// Eased `smoothstep` interpolation (slow in, slow out) — the natural
    /// default for camera moves.
    Smooth,
}

/// One control point on a [`Track`]: a value at an instant, plus how to leave it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    /// When this key's value applies, on the project timebase.
    pub time: TimeStamp,
    /// The scalar value at [`time`](Self::time).
    pub value: f32,
    /// Interpolation from this key toward the next.
    pub easing: Easing,
}

impl Keyframe {
    /// A keyframe with [`Easing::Smooth`].
    pub fn smooth(time: TimeStamp, value: f32) -> Self {
        Keyframe {
            time,
            value,
            easing: Easing::Smooth,
        }
    }
}

/// An ordered set of [`Keyframe`]s sampled to animate a scalar parameter.
///
/// Keys are expected in ascending time order. Sampling clamps to the endpoints
/// (the value is held flat before the first key and after the last), so a track
/// is always defined over all time once it has at least one key.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Track {
    /// Control points in ascending time order.
    pub keys: Vec<Keyframe>,
}

impl Track {
    /// A track from explicit keys (assumed time-ascending).
    pub fn new(keys: Vec<Keyframe>) -> Self {
        Track { keys }
    }

    /// A flat track holding `value` for all time — the non-animated case.
    pub fn constant(value: f32) -> Self {
        Track {
            keys: vec![Keyframe {
                time: TimeStamp::ZERO,
                value,
                easing: Easing::Hold,
            }],
        }
    }

    /// Whether the track has no keys (samples to `None`).
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The animated value at `t`, or `None` if the track has no keys.
    ///
    /// Before the first key and after the last, the value is held flat. Between
    /// two keys it interpolates by the earlier key's [`Easing`].
    pub fn sample(&self, t: TimeStamp) -> Option<f32> {
        let first = self.keys.first()?;
        if t <= first.time {
            return Some(first.value);
        }
        let last = self.keys.last()?;
        if t >= last.time {
            return Some(last.value);
        }
        // Find the segment [a, b) containing t. Linear scan: tracks are short.
        let i = self
            .keys
            .windows(2)
            .position(|w| t >= w[0].time && t < w[1].time)
            .unwrap_or(self.keys.len() - 2);
        let a = self.keys[i];
        let b = self.keys[i + 1];
        let span = b.time.as_nanos().saturating_sub(a.time.as_nanos());
        if span == 0 {
            return Some(b.value);
        }
        let local = (t.as_nanos() - a.time.as_nanos()) as f32 / span as f32;
        let eased = match a.easing {
            Easing::Hold => return Some(a.value),
            Easing::Linear => local,
            Easing::Smooth => local * local * (3.0 - 2.0 * local),
        };
        Some(a.value + (b.value - a.value) * eased)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(n: u64) -> TimeStamp {
        TimeStamp::from_nanos(n)
    }

    #[test]
    fn empty_track_samples_none() {
        assert_eq!(Track::default().sample(ts(10)), None);
    }

    #[test]
    fn constant_holds_everywhere() {
        let t = Track::constant(0.7);
        assert_eq!(t.sample(ts(0)), Some(0.7));
        assert_eq!(t.sample(ts(1_000_000)), Some(0.7));
    }

    #[test]
    fn clamps_outside_the_range() {
        let t = Track::new(vec![
            Keyframe::smooth(ts(100), 1.0),
            Keyframe::smooth(ts(300), 3.0),
        ]);
        assert_eq!(t.sample(ts(0)), Some(1.0), "before first holds");
        assert_eq!(t.sample(ts(500)), Some(3.0), "after last holds");
    }

    #[test]
    fn linear_interpolates_midpoint() {
        let t = Track::new(vec![
            Keyframe {
                time: ts(0),
                value: 0.0,
                easing: Easing::Linear,
            },
            Keyframe {
                time: ts(100),
                value: 10.0,
                easing: Easing::Linear,
            },
        ]);
        assert!((t.sample(ts(50)).unwrap() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn smooth_is_eased_at_midpoint_and_symmetric() {
        let t = Track::new(vec![
            Keyframe::smooth(ts(0), 0.0),
            Keyframe::smooth(ts(100), 1.0),
        ]);
        // smoothstep(0.5) == 0.5, but slopes are flat at the ends.
        assert!((t.sample(ts(50)).unwrap() - 0.5).abs() < 1e-5);
        assert!(t.sample(ts(10)).unwrap() < 0.1, "eased in");
        assert!(t.sample(ts(90)).unwrap() > 0.9, "eased out");
    }

    #[test]
    fn hold_steps_at_the_next_key() {
        let t = Track::new(vec![
            Keyframe {
                time: ts(0),
                value: 1.0,
                easing: Easing::Hold,
            },
            Keyframe {
                time: ts(100),
                value: 2.0,
                easing: Easing::Linear,
            },
        ]);
        assert_eq!(t.sample(ts(99)), Some(1.0), "held until the next key");
        assert_eq!(t.sample(ts(100)), Some(2.0));
    }
}
