//! Chroma project persistence and editing.
//!
//! The data layer the editor sits on, built on the plain serializable
//! [`Project`](chroma_core_api::Project) value type:
//!
//! - [`save`] / [`load`] — versioned JSON persistence with a migration hook
//!   (spec EDT-11).
//! - [`History`] / [`EditCommand`] — a lossless undo/redo command stack; every
//!   edit is a reversible command (spec EDT-10).
//! - [`Preset`] / [`builtin_presets`] — one-click background + scene looks
//!   (spec EDT-09).
//!
//! It depends only on the `-api` contract crate, never on render/encode
//! internals — the GUI and CLI compose it with those.

mod error;
mod history;
mod io;
mod preset;

pub use error::{ProjectError, Result};
pub use history::{EditCommand, History};
pub use io::{from_json, load, save, CURRENT_VERSION};
pub use preset::{builtin_presets, Preset};
