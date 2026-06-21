//! Chroma media contract.
//!
//! The public surface of the media-I/O layer: the [`Decoder`], [`Encoder`],
//! [`FrameSource`], and [`MuxTarget`] traits, plus the [`RgbaFrame`] exchanged
//! through the render pipeline and the [`OutputSpec`] carrying the §3.6 export
//! parameters. `chroma-media-ffmpeg` implements these; swapping encoders
//! (software x264 ↔ VAAPI/NVENC) is a backend choice behind these traits with
//! no consumer change (spec §3.2, EXP-08).

mod decoder;
mod encoder;
mod error;
mod frame;
mod spec;

pub mod fakes;

pub use decoder::{Decoder, FrameSource};
pub use encoder::{Encoder, MuxTarget};
pub use error::{MediaError, Result};
pub use frame::RgbaFrame;
pub use spec::{Codec, Container, GifSettings, OutputSpec, RateControl};
