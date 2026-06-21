//! Output/export parameter types (spec §3.6).

use chroma_core_api::Size;
use serde::{Deserialize, Serialize};

/// The output container/format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Container {
    /// MP4 (spec EXP-01).
    Mp4,
    /// Animated GIF (spec EXP-02).
    Gif,
}

/// Video codec for MP4 export (spec EXP-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    /// H.264 — the universal default.
    H264,
    /// H.265 — smaller, less compatible.
    H265,
    /// VP9 — web-oriented.
    Vp9,
}

/// Rate-control strategy for MP4 export (spec EXP-04).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RateControl {
    /// Quality-targeted, variable size. `crf` in the x264 sane range ~18–28;
    /// lower is higher quality. The default for demos.
    Crf { crf: u8 },
    /// Size-targeted; `bitrate_kbps` is the ceiling for size-sensitive delivery.
    Bitrate { bitrate_kbps: u32 },
}

/// GIF-specific encode settings (spec EXP-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GifSettings {
    /// Palette size, `<= 256`. Higher = better gradients, larger file.
    pub palette_size: u16,
    /// Dithering on reduces banding on colorful backgrounds; off is cleaner on
    /// flat areas and smaller.
    pub dithering: bool,
    /// Build a per-clip optimized palette before encoding — a large quality win
    /// for gradient backgrounds. Strongly recommended.
    pub two_pass_palette: bool,
    /// Loop count; `0` means loop forever.
    pub loop_count: u16,
}

/// The complete description of one export job's output (spec §3.6, EXP-03).
///
/// One `OutputSpec` drives an [`Encoder`](crate::Encoder). The render core is
/// deterministic, so the same `(Project, OutputSpec)` yields the same framing
/// decisions regardless of the preview machine's performance (spec EXP-06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSpec {
    /// Output container, which selects the encoder family.
    pub container: Container,
    /// Final composited canvas size, decoupled from source capture size.
    pub canvas: Size,
    /// Output frame rate. MP4 typically 30/60; GIF usually 12–20.
    pub fps: u32,
    /// Video codec. Ignored for [`Container::Gif`].
    pub codec: Codec,
    /// Rate control. Ignored for [`Container::Gif`].
    pub rate_control: RateControl,
    /// Use a hardware encoder (VAAPI/NVENC) when available, else software
    /// (spec EXP-08).
    pub hardware: bool,
    /// GIF settings, used only for [`Container::Gif`].
    pub gif: GifSettings,
}
