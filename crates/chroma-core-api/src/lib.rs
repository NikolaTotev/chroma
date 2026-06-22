//! Chroma core contract.
//!
//! This crate is the root of the contract graph: it holds the shared value
//! types, the [`Modifier`] trait every effect implements, and the
//! [`CompositePass`] drawing sink overlay modifiers paint through. It contains
//! **no logic** — only data and interfaces — so any implementation crate and
//! any consumer can depend on it without coupling (spec §3.2,
//! `ORCHESTRATION.md` §3).
//!
//! # Invariants this contract encodes
//!
//! - **One clock.** [`TimeStamp`] is a single monotonic nanosecond timebase
//!   shared by capture frames and input events (spec CAP-05).
//! - **Composition, not inheritance.** Shared behaviour (easing, smoothing) is
//!   a composed helper a modifier *holds*, never a base class. The trait
//!   default methods here are genuinely-empty no-ops, the only kind allowed
//!   (`ORCHESTRATION.md` §2).
//! - **Data ≠ behaviour.** [`Project`] is a plain serializable value type that
//!   stores [`ModifierSpec`] descriptions; `chroma-modifiers` turns those into
//!   `Box<dyn Modifier>` at load time (see `DECISIONS.md`).

mod camera;
mod compositor;
mod eval;
mod geometry;
mod modifier;
mod project;
mod time;

pub mod fakes;

pub use camera::{CameraState, CameraTarget};
pub use compositor::{Compositor, SourceImage};
pub use eval::EvalContext;
pub use geometry::{Point, Rect, Size};
pub use modifier::{CompositePass, Modifier, ModifierKind};
pub use project::{
    Background, Border, GradientStop, ModifierParams, ModifierSpec, Project, SceneStyle, Shadow,
    SourceMedia,
};
pub use time::{TimeRange, TimeStamp};
