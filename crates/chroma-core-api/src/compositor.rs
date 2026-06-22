//! The compositor contract — the render core's drawing backend.

use crate::camera::CameraState;
use crate::geometry::Size;
use crate::modifier::CompositePass;
use crate::project::{Background, SceneStyle};

/// A decoded source frame the compositor insets over the background.
///
/// Borrowed, tightly-packed RGBA8 (`size.width * size.height * 4` bytes) — the
/// compositor reads it but never owns or mutates the source (non-destructive
/// invariant, spec §3.2).
pub struct SourceImage<'a> {
    /// Source dimensions in pixels.
    pub size: Size,
    /// Tightly-packed RGBA8 pixels.
    pub rgba: &'a [u8],
}

/// Draws one composited frame: background, then the camera-transformed scene
/// inset, then overlay modifiers paint on top via [`CompositePass`].
///
/// This is the §3.4 *scene composite* + *overlay* stages behind one trait, so
/// the render core depends only on this contract — a software compositor and a
/// wgpu compositor are interchangeable with no consumer change (spec §3.2,
/// §4.5). Implementors are also a [`CompositePass`]: after
/// [`render_scene`](Self::render_scene), the render core hands `&mut dyn
/// CompositePass` to each overlay modifier, then calls [`finish`](Self::finish).
pub trait Compositor: CompositePass {
    /// Paints a fresh `canvas`-sized frame: the `background`, then `source`
    /// inset and transformed by `camera`, styled per `style`. Replaces any
    /// previous frame's contents.
    fn render_scene(
        &mut self,
        canvas: Size,
        background: &Background,
        camera: CameraState,
        style: &SceneStyle,
        source: SourceImage<'_>,
    );

    /// Finalizes the current frame and returns it as tightly-packed RGBA8
    /// (`canvas.width * canvas.height * 4` bytes).
    fn finish(&mut self) -> Vec<u8>;
}
