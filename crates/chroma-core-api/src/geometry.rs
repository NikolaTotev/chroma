//! Plain geometry value types shared across the render pipeline.

use serde::{Deserialize, Serialize};

/// A 2-D point in normalized canvas coordinates, where `(0.0, 0.0)` is the
/// top-left of the output canvas and `(1.0, 1.0)` is the bottom-right.
///
/// Normalized coordinates keep camera and modifier math independent of the
/// preview vs. export resolution (spec §3.1, EXP-06 determinism).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// An integer pixel size (canvas, source, or frame dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Size { width, height }
    }

    /// Aspect ratio `width / height`, or `0.0` for a zero-height size.
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

/// An axis-aligned rectangle in normalized canvas coordinates (see [`Point`]).
///
/// Used for crop targets, scene insets, and overlay placement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// The full normalized canvas, `(0, 0, 1, 1)`.
    pub const FULL: Rect = Rect::new(0.0, 0.0, 1.0, 1.0);

    /// The rectangle's center point.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}
