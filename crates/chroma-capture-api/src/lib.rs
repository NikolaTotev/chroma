//! Chroma capture contract.
//!
//! The only public surface of the capture layer: the [`ScreenCapturer`],
//! [`EventSource`], and [`Clock`] traits, plus the value types they exchange
//! ([`Frame`], [`InputEvent`], [`TimedInputEvent`], [`ScrollDelta`],
//! [`CaptureTarget`]). Backends (`chroma-capture-x11`, `chroma-capture-wayland`)
//! implement these traits; the app composes whichever pair matches the session
//! via a runtime factory, never a class hierarchy (spec §4.3).
//!
//! **One clock.** Frames and events are stamped on the same [`Clock`] timebase
//! ([`chroma_core_api::TimeStamp`]) so any event maps to an exact frame within
//! ±1 frame (spec CAP-05).

mod capturer;
mod clock;
mod error;
mod event;
mod frame;

pub mod fakes;

pub use capturer::{CaptureTarget, ScreenCapturer};
pub use clock::Clock;
pub use error::{CaptureError, Result};
pub use event::{EventSource, InputEvent, MouseButton, ScrollDelta, TimedInputEvent};
pub use frame::{Frame, PixelFormat};
