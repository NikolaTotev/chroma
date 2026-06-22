//! Chroma's headless editor engine.
//!
//! [`Studio`] is the editor's brain with no UI attached: it owns the
//! [`Project`] and its undo [`History`], turns edits into reversible commands,
//! renders preview frames, applies look presets, and exports video. The Tauri
//! GUI is a thin shell over these methods, and they are unit-tested here without
//! any window — so the app's logic is verifiable on any machine.
//!
//! Preview and export composite over a synthetic source (see [`source`]) until a
//! real video `Decoder` lands; everything else — camera, styling, overlays,
//! keyframes — is the production render path (`chroma-render`).

mod image;
mod source;

use chroma_camera::SpringSmoother;
use chroma_core_api::{
    Background, ModifierKind, ModifierParams, ModifierSpec, Project, SceneStyle, Size, SourceMedia,
    TimeRange, TimeStamp,
};
use chroma_media_api::{Codec, Container, Encoder, GifSettings, OutputSpec, RateControl};
use chroma_media_ffmpeg::{ffmpeg_available, FfmpegEncoder};
use chroma_modifiers::build_all;
use chroma_project::{builtin_presets, EditCommand, History, Preset};
use chroma_render::{render_frame, SourceFrame};

pub use chroma_project::ProjectError;

/// The default preview/export source resolution and canvas.
const DEFAULT_SIZE: Size = Size::new(1280, 720);
/// Fallback timeline length when nothing finite bounds it.
const DEFAULT_DURATION_NS: u64 = 6_000_000_000;

/// The editor engine: a project plus its undo history and render entry points.
pub struct Studio {
    project: Project,
    history: History,
}

impl Default for Studio {
    fn default() -> Self {
        Studio::new()
    }
}

impl Studio {
    /// A new studio with a lively default project (a demo look plus a
    /// cursor-follow camera, cursor marker, and title) so the preview shows
    /// something immediately.
    pub fn new() -> Self {
        Studio {
            project: default_project(),
            history: History::new(),
        }
    }

    /// Wraps an existing project (fresh, empty history).
    pub fn from_project(project: Project) -> Self {
        Studio {
            project,
            history: History::new(),
        }
    }

    /// Loads a project from a JSON file.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, ProjectError> {
        Ok(Studio::from_project(chroma_project::load(path)?))
    }

    /// Saves the current project to a JSON file.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), ProjectError> {
        chroma_project::save(&self.project, path)
    }

    /// The current project (read-only).
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Whether undo / redo would currently do anything.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies a reversible edit command.
    pub fn apply(&mut self, cmd: EditCommand) {
        self.history.apply(&mut self.project, cmd);
    }

    /// Undoes / redoes the most recent command. Returns whether anything moved.
    pub fn undo(&mut self) -> bool {
        self.history.undo(&mut self.project)
    }
    pub fn redo(&mut self) -> bool {
        self.history.redo(&mut self.project)
    }

    // --- Convenience edits (each one undoable) -------------------------------

    pub fn set_background(&mut self, bg: Background) {
        self.apply(EditCommand::SetBackground(bg));
    }
    pub fn set_scene(&mut self, scene: SceneStyle) {
        self.apply(EditCommand::SetScene(scene));
    }
    pub fn add_modifier(&mut self, spec: ModifierSpec) {
        self.apply(EditCommand::AddModifier(spec));
    }
    pub fn remove_modifier(&mut self, index: usize) {
        self.apply(EditCommand::RemoveModifier { index });
    }
    pub fn update_modifier(&mut self, index: usize, spec: ModifierSpec) {
        self.apply(EditCommand::UpdateModifier { index, spec });
    }

    /// The names of the built-in look presets, in display order.
    pub fn preset_names(&self) -> Vec<String> {
        builtin_presets()
            .into_iter()
            .map(|p| p.name.to_owned())
            .collect()
    }

    /// Applies a built-in preset by name (one undoable step per field). Returns
    /// `false` if no preset has that name.
    pub fn apply_preset(&mut self, name: &str) -> bool {
        let Some(preset) = builtin_presets().into_iter().find(|p| p.name == name) else {
            return false;
        };
        let Preset {
            background, scene, ..
        } = preset;
        self.apply(EditCommand::SetBackground(background));
        self.apply(EditCommand::SetScene(scene));
        true
    }

    /// The timeline length in nanoseconds: the longest finite modifier end, or a
    /// sensible default when nothing bounds it.
    pub fn timeline_duration_ns(&self) -> u64 {
        let finite = self
            .project
            .modifiers
            .iter()
            .map(|m| m.range.end.as_nanos())
            .filter(|&end| end < u64::MAX / 2)
            .max();
        finite.unwrap_or(DEFAULT_DURATION_NS).max(2_000_000_000)
    }

    /// Renders the composited frame at `time` as a 24-bit BMP for the webview.
    pub fn render_preview_bmp(&self, time: TimeStamp) -> Vec<u8> {
        let (size, rgba) = self.render_preview_rgba(time);
        image::rgba_to_bmp(size.width, size.height, &rgba)
    }

    /// Renders the composited frame at `time` as `(canvas, RGBA8)`.
    pub fn render_preview_rgba(&self, time: TimeStamp) -> (Size, Vec<u8>) {
        let canvas = self.project.canvas;
        let src_size = self.project.source.size;
        let source = source::demo_screen(src_size);
        let modifiers = build_all(&self.project.modifiers);
        let mut compositor = chroma_compositor::CpuCompositor::new();
        let mut smoother = SpringSmoother::default();
        let frame = render_frame(
            canvas,
            &self.project.background,
            &self.project.scene,
            &SourceFrame {
                size: src_size,
                rgba: &source,
            },
            Some(source::cursor_at(time)),
            &modifiers,
            &mut smoother,
            &mut compositor,
            time,
        );
        (canvas, frame.data)
    }

    /// Exports the project to a video file (`.mp4`/`.gif`) over the synthetic
    /// source. `secs`/`fps` default to the timeline length and 30fps.
    pub fn export(
        &self,
        path: impl AsRef<std::path::Path>,
        secs: Option<u32>,
        fps: Option<u32>,
    ) -> Result<(), String> {
        if !ffmpeg_available() {
            return Err("ffmpeg not found on PATH".to_owned());
        }
        let path = path.as_ref();
        let fps = fps.unwrap_or(30).max(1);
        let secs = secs
            .unwrap_or_else(|| (self.timeline_duration_ns().div_ceil(1_000_000_000) as u32).max(1));
        // x264 yuv420p needs even canvas dimensions.
        let canvas = Size::new(
            self.project.canvas.width & !1,
            self.project.canvas.height & !1,
        );
        let src_size = self.project.source.size;
        let source = source::demo_screen(src_size);
        let modifiers = build_all(&self.project.modifiers);

        let out = path.to_string_lossy();
        let spec = OutputSpec {
            container: if out.ends_with(".gif") {
                Container::Gif
            } else {
                Container::Mp4
            },
            canvas,
            fps,
            codec: Codec::H264,
            rate_control: RateControl::Crf { crf: 20 },
            hardware: false,
            gif: GifSettings {
                palette_size: 256,
                dithering: true,
                two_pass_palette: true,
                loop_count: 0,
            },
        };

        let mut encoder = FfmpegEncoder::new(path);
        encoder.open(&spec).map_err(|e| e.to_string())?;
        let mut compositor = chroma_compositor::CpuCompositor::new();
        let mut smoother = SpringSmoother::default();
        let frame_ns = 1_000_000_000u64 / fps as u64;
        for i in 0..(secs * fps).max(1) {
            let time = TimeStamp::from_nanos(i as u64 * frame_ns);
            let frame = render_frame(
                canvas,
                &self.project.background,
                &self.project.scene,
                &SourceFrame {
                    size: src_size,
                    rgba: &source,
                },
                Some(source::cursor_at(time)),
                &modifiers,
                &mut smoother,
                &mut compositor,
                time,
            );
            encoder
                .push_frame(&frame, frame.pts)
                .map_err(|e| e.to_string())?;
        }
        encoder.finish().map_err(|e| e.to_string())
    }
}

/// The default project shown on first launch.
fn default_project() -> Project {
    let secs = |s: f32| TimeStamp::from_nanos((s * 1e9) as u64);
    Project {
        version: chroma_project::CURRENT_VERSION,
        source: SourceMedia {
            video_path: std::path::PathBuf::new(),
            event_log_path: std::path::PathBuf::new(),
            fps: 30,
            size: DEFAULT_SIZE,
        },
        canvas: DEFAULT_SIZE,
        background: builtin_presets()[1].background.clone(), // Vibrant
        scene: builtin_presets()[1].scene,
        modifiers: vec![
            ModifierSpec {
                kind: ModifierKind::Camera,
                range: TimeRange::new(secs(0.3), secs(5.7)),
                params: ModifierParams::CursorFollow {
                    zoom: 1.5,
                    tightness: 1.0,
                },
            },
            ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(TimeStamp::ZERO, secs(6.0)),
                params: ModifierParams::CursorMarker { size: 0.05 },
            },
            ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(secs(0.5), secs(5.5)),
                params: ModifierParams::Text {
                    content: "Chroma".to_owned(),
                    rect: chroma_core_api::Rect::new(0.30, 0.82, 0.40, 0.10),
                    rgba: [1.0, 1.0, 1.0, 1.0],
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_renders_a_nonempty_frame() {
        let studio = Studio::new();
        let bmp = studio.render_preview_bmp(TimeStamp::from_nanos(1_000_000_000));
        assert!(bmp.starts_with(b"BM"), "valid BMP header");
        let (size, rgba) = studio.render_preview_rgba(TimeStamp::ZERO);
        assert_eq!(rgba.len(), (size.width * size.height * 4) as usize);
    }

    #[test]
    fn edits_go_through_undoable_history() {
        let mut studio = Studio::new();
        let before = studio.project().modifiers.len();
        studio.add_modifier(ModifierSpec {
            kind: ModifierKind::Overlay,
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(10)),
            params: ModifierParams::Highlight { radius: 0.05 },
        });
        assert_eq!(studio.project().modifiers.len(), before + 1);
        assert!(studio.can_undo());
        studio.undo();
        assert_eq!(studio.project().modifiers.len(), before);
        assert!(studio.can_redo());
        studio.redo();
        assert_eq!(studio.project().modifiers.len(), before + 1);
    }

    #[test]
    fn presets_apply_by_name() {
        let mut studio = Studio::new();
        let names = studio.preset_names();
        assert!(names.contains(&"Clean".to_owned()));
        assert!(studio.apply_preset("Clean"));
        assert_eq!(studio.project().background, builtin_presets()[0].background);
        assert!(!studio.apply_preset("Nonexistent"));
    }

    #[test]
    fn timeline_duration_is_bounded() {
        let studio = Studio::new();
        let d = studio.timeline_duration_ns();
        assert!(d >= 2_000_000_000, "at least the floor");
        assert!(d < u64::MAX / 2, "finite despite infinite-range modifiers");
    }

    #[test]
    fn save_load_round_trips() {
        let studio = Studio::new();
        let path = std::env::temp_dir().join("chroma_studio_roundtrip.json");
        studio.save(&path).unwrap();
        let loaded = Studio::load(&path).unwrap();
        assert_eq!(loaded.project().canvas, studio.project().canvas);
        assert_eq!(
            loaded.project().modifiers.len(),
            studio.project().modifiers.len()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_writes_a_file_when_ffmpeg_present() {
        if !ffmpeg_available() {
            return; // skip cleanly where ffmpeg is absent (e.g. Windows CI)
        }
        let studio = Studio::new();
        let path = std::env::temp_dir().join("chroma_studio_export.mp4");
        studio.export(&path, Some(1), Some(15)).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 0, "export produced a non-empty file");
        let _ = std::fs::remove_file(&path);
    }
}
