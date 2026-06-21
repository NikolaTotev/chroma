//! Input-event value types and the event-source contract.

use chroma_core_api::TimeStamp;
use serde::{Deserialize, Serialize};

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    /// Any other button, by platform button code.
    Other(u16),
}

/// A scroll-wheel delta, vertical and horizontal (spec CAP-02).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollDelta {
    /// Vertical scroll amount (positive = up/away).
    pub dy: f32,
    /// Horizontal scroll amount (positive = right).
    pub dx: f32,
}

/// A single input event, without its timestamp.
///
/// Pointer positions are in source pixel coordinates as reported by the host;
/// the editor normalizes them against the source size. Keystrokes carry the
/// raw platform keycode and modifier bitflags for the on-screen overlay
/// (spec CAP-03); they are sensitive and never transmitted (spec §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    /// The hardware cursor moved to `(x, y)` in source pixels (spec CAP-04).
    PointerMove { x: f32, y: f32 },
    /// A pointer button was pressed.
    ButtonDown { button: MouseButton, x: f32, y: f32 },
    /// A pointer button was released.
    ButtonUp { button: MouseButton, x: f32, y: f32 },
    /// The scroll wheel moved.
    Scroll { delta: ScrollDelta },
    /// A key was pressed.
    KeyDown { keycode: u32, modifiers: u32 },
    /// A key was released.
    KeyUp { keycode: u32, modifiers: u32 },
}

/// An [`InputEvent`] stamped on the shared capture clock.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimedInputEvent {
    /// When the event occurred, on the same clock as captured frames.
    pub timestamp: TimeStamp,
    /// What happened.
    pub event: InputEvent,
}

/// A source of timestamped input events synchronized to the screen capturer.
///
/// Backends poll the host (XInput2 on X11; libei/evdev on Wayland) and stamp
/// every event on the shared [`Clock`](crate::Clock). [`poll`](Self::poll)
/// drains all events seen since the previous call.
pub trait EventSource {
    /// Drains and returns all events captured since the last poll, in
    /// chronological order. Returns an empty vector when nothing happened.
    fn poll(&mut self) -> Vec<TimedInputEvent>;
}
