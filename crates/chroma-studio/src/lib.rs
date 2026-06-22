//! Chroma's headless editor engine.
//!
//! [`Studio`] is the editor's brain with no UI attached: it records the screen,
//! owns the [`Project`] and its undo [`History`], turns edits into reversible
//! commands, renders preview frames, applies look presets, and exports video.
//! The Tauri GUI is a thin shell over these methods, and they are unit-tested
//! here without any window — so the app's logic is verifiable on any machine.
//!
//! # Source of truth
//!
//! The editor is non-destructive: it edits against an immutable *source*. That
//! source is either
//!
//! - a **recorded clip** — an MP4 captured by [`Recorder`] plus its input-event
//!   log, decoded on demand (preview scrub seeks one frame; export streams the
//!   whole clip), so long recordings never buffer in RAM; or
//! - a **synthetic** test screen, used before anything is recorded so the
//!   preview shows the styling/camera/overlays immediately.

mod image;
mod recorder;
mod source;

pub use recorder::{RecordedClip, Recorder};

use chroma_camera::SpringSmoother;
use chroma_capture_api::{InputEvent, TimedInputEvent};
use chroma_core_api::{
    Background, ModifierKind, ModifierParams, ModifierSpec, Point, Project, SceneStyle, Size,
    SourceMedia, TimeRange, TimeStamp,
};
use chroma_media_api::{
    Codec, Container, Decoder, Encoder, FrameSource, GifSettings, OutputSpec, RateControl,
};
use chroma_media_ffmpeg::{ffmpeg_available, FfmpegDecoder, FfmpegEncoder, FfmpegFrameReader};
use chroma_modifiers::build_all;
use chroma_project::{builtin_presets, EditCommand, History, Preset};
use chroma_render::{render_frame, SourceFrame};
use std::path::PathBuf;

pub use chroma_project::ProjectError;

/// The default preview/export source resolution and canvas.
const DEFAULT_SIZE: Size = Size::new(1280, 720);
/// Fallback timeline length when nothing finite bounds it.
const DEFAULT_DURATION_NS: u64 = 6_000_000_000;
/// Ripple length after a recorded click.
const RIPPLE_NS: u64 = 600_000_000;

/// What the editor composites over.
enum Source {
    /// A synthetic checkerboard screen with a synthetic cursor path.
    Synthetic,
    /// A recorded clip decoded from disk.
    Clip(RecordedClip),
}

/// The editor engine: a project plus its undo history and render entry points.
pub struct Studio {
    project: Project,
    history: History,
    source: Source,
    /// Lazily-opened decoder for the current clip source (preview seeks).
    decoder: Option<FfmpegDecoder>,
    /// The in-progress recording, if any.
    recorder: Option<Recorder>,
}

impl Default for Studio {
    fn default() -> Self {
        Studio::new()
    }
}

impl Studio {
    /// A new studio with a lively default project (a demo look plus a
    /// cursor-follow camera, cursor marker, and title) over the synthetic
    /// source, so the preview shows something before anything is recorded.
    pub fn new() -> Self {
        Studio {
            project: default_project(),
            history: History::new(),
            source: Source::Synthetic,
            decoder: None,
            recorder: None,
        }
    }

    /// Wraps an existing project (fresh history, synthetic source).
    pub fn from_project(project: Project) -> Self {
        Studio {
            project,
            history: History::new(),
            source: Source::Synthetic,
            decoder: None,
            recorder: None,
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

    // --- Recording -----------------------------------------------------------

    /// Whether a recording is currently in progress.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Nanoseconds elapsed in the current recording (0 if not recording).
    pub fn record_elapsed_ns(&self) -> u64 {
        self.recorder.as_ref().map(|r| r.elapsed_ns()).unwrap_or(0)
    }

    /// Begins recording the screen at `fps`.
    ///
    /// Errors immediately if a recording is already running or the capture
    /// backend is unavailable (Wayland/rootless Xwayland, or off Linux — a
    /// native X11/Xorg session is required).
    pub fn start_record(&mut self, fps: u32) -> Result<(), String> {
        if self.recorder.is_some() {
            return Err("already recording".to_owned());
        }
        self.recorder = Some(Recorder::start(fps)?);
        Ok(())
    }

    /// Stops the recording, loads it as the editor's source with a sensible
    /// starter timeline (cursor-follow + marker + a ripple per click), and
    /// returns the clip's duration in nanoseconds.
    pub fn stop_record(&mut self) -> Result<u64, String> {
        let recorder = self.recorder.take().ok_or("not recording")?;
        let clip = recorder.stop()?;
        let duration = clip.duration_ns;
        self.set_clip(clip);
        Ok(duration)
    }

    /// Opens a previously recorded clip (MP4 + optional event log) as the source.
    pub fn load_clip(
        &mut self,
        video_path: PathBuf,
        events: Vec<TimedInputEvent>,
    ) -> Result<(), String> {
        let info = chroma_media_ffmpeg::probe_video(&video_path.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let fps = if info.fps >= 1.0 {
            info.fps.round() as u32
        } else {
            30
        };
        self.set_clip(RecordedClip {
            video_path,
            size: info.size,
            fps,
            duration_ns: info.duration_ns,
            events,
        });
        Ok(())
    }

    /// Makes `clip` the editor's source: sizes the canvas to it, lays down a
    /// starter timeline, and resets the undo history for the new session.
    fn set_clip(&mut self, clip: RecordedClip) {
        self.project.canvas = clip.size;
        self.project.source = SourceMedia {
            video_path: clip.video_path.clone(),
            event_log_path: PathBuf::new(),
            fps: clip.fps,
            size: clip.size,
        };
        self.project.modifiers = starter_timeline(&clip);
        self.source = Source::Clip(clip);
        self.decoder = None;
        self.history = History::new();
    }

    /// The timeline length in nanoseconds: the recorded clip's length, else the
    /// longest finite modifier end, else a default.
    pub fn timeline_duration_ns(&self) -> u64 {
        if let Source::Clip(clip) = &self.source {
            return clip.duration_ns.max(1_000_000_000);
        }
        self.project
            .modifiers
            .iter()
            .map(|m| m.range.end.as_nanos())
            .filter(|&end| end < u64::MAX / 2)
            .max()
            .unwrap_or(DEFAULT_DURATION_NS)
            .max(2_000_000_000)
    }

    // --- Rendering -----------------------------------------------------------

    /// Renders the composited frame at `time` as a 24-bit BMP for the webview.
    pub fn render_preview_bmp(&mut self, time: TimeStamp) -> Vec<u8> {
        let (size, rgba) = self.render_preview_rgba(time);
        image::rgba_to_bmp(size.width, size.height, &rgba)
    }

    /// Renders the composited frame at `time` as `(canvas, RGBA8)`.
    pub fn render_preview_rgba(&mut self, time: TimeStamp) -> (Size, Vec<u8>) {
        let (src_size, src_rgba, cursor) = self.source_frame(time);
        let canvas = self.project.canvas;
        let modifiers = build_all(&self.project.modifiers);
        let mut compositor = chroma_compositor::CpuCompositor::new();
        let mut smoother = SpringSmoother::default();
        let frame = render_frame(
            canvas,
            &self.project.background,
            &self.project.scene,
            &SourceFrame {
                size: src_size,
                rgba: &src_rgba,
            },
            cursor,
            &modifiers,
            &mut smoother,
            &mut compositor,
            time,
        );
        (canvas, frame.data)
    }

    /// The decoded/synthetic source pixels and cursor at `time`.
    fn source_frame(&mut self, time: TimeStamp) -> (Size, Vec<u8>, Option<Point>) {
        // Synthetic source: no borrow of the clip needed.
        if matches!(self.source, Source::Synthetic) {
            let size = self.project.source.size;
            return (
                size,
                source::demo_screen(size),
                Some(source::cursor_at(time)),
            );
        }
        // Clip source: pull what we need, then (re)open the decoder and seek.
        let (size, path, cursor) = match &self.source {
            Source::Clip(clip) => (
                clip.size,
                clip.video_path.to_string_lossy().into_owned(),
                cursor_from_events(&clip.events, time, clip.size),
            ),
            Source::Synthetic => unreachable!(),
        };
        if self.decoder.is_none() {
            let mut dec = FfmpegDecoder::new();
            if dec.open(&path).is_ok() {
                self.decoder = Some(dec);
            }
        }
        let decoded = self
            .decoder
            .as_mut()
            .and_then(|d| d.frame_at(time).ok())
            .map(|f| f.data);
        (
            size,
            decoded.unwrap_or_else(|| solid(size, [20, 22, 28])),
            cursor,
        )
    }

    /// Exports the project to a video file (`.mp4`/`.gif`). `secs`/`fps` default
    /// to the source length and its frame rate.
    pub fn export(
        &self,
        path: impl AsRef<std::path::Path>,
        secs: Option<u32>,
        fps: Option<u32>,
    ) -> Result<(), String> {
        if !ffmpeg_available() {
            return Err("ffmpeg not found on PATH".to_owned());
        }
        match &self.source {
            Source::Clip(clip) => self.export_clip(clip, path.as_ref(), secs, fps),
            Source::Synthetic => self.export_synthetic(path.as_ref(), secs, fps),
        }
    }

    /// Streams the recorded clip through the render pipeline and re-encodes it.
    fn export_clip(
        &self,
        clip: &RecordedClip,
        path: &std::path::Path,
        secs: Option<u32>,
        fps: Option<u32>,
    ) -> Result<(), String> {
        let fps = fps.unwrap_or(clip.fps).max(1);
        let canvas = even(clip.size);
        let modifiers = build_all(&self.project.modifiers);
        let frame_cap = secs.map(|s| (s * fps) as u64);

        let mut reader = FfmpegFrameReader::open(&clip.video_path.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let mut encoder = FfmpegEncoder::new(path);
        encoder
            .open(&output_spec(&path.to_string_lossy(), canvas, fps))
            .map_err(|e| e.to_string())?;
        let mut compositor = chroma_compositor::CpuCompositor::new();
        let mut smoother = SpringSmoother::default();
        let mut i: u64 = 0;
        while let Some(src) = reader.next_frame().map_err(|e| e.to_string())? {
            if frame_cap.is_some_and(|cap| i >= cap) {
                break;
            }
            let cursor = cursor_from_events(&clip.events, src.pts, clip.size);
            let frame = render_frame(
                canvas,
                &self.project.background,
                &self.project.scene,
                &SourceFrame {
                    size: src.size,
                    rgba: &src.data,
                },
                cursor,
                &modifiers,
                &mut smoother,
                &mut compositor,
                src.pts,
            );
            encoder
                .push_frame(&frame, frame.pts)
                .map_err(|e| e.to_string())?;
            i += 1;
        }
        encoder.finish().map_err(|e| e.to_string())
    }

    /// Exports the synthetic demo (no recording yet).
    fn export_synthetic(
        &self,
        path: &std::path::Path,
        secs: Option<u32>,
        fps: Option<u32>,
    ) -> Result<(), String> {
        let fps = fps.unwrap_or(30).max(1);
        let secs = secs
            .unwrap_or_else(|| (self.timeline_duration_ns().div_ceil(1_000_000_000) as u32).max(1));
        let canvas = even(self.project.canvas);
        let src_size = self.project.source.size;
        let source = source::demo_screen(src_size);
        let modifiers = build_all(&self.project.modifiers);

        let mut encoder = FfmpegEncoder::new(path);
        encoder
            .open(&output_spec(&path.to_string_lossy(), canvas, fps))
            .map_err(|e| e.to_string())?;
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

/// Even-dimensions copy of a size (H.264 yuv420p needs even width/height).
fn even(size: Size) -> Size {
    Size::new(size.width & !1, size.height & !1)
}

/// A tightly-packed opaque RGBA fill of `size`.
fn solid(size: Size, rgb: [u8; 3]) -> Vec<u8> {
    let mut v = Vec::with_capacity((size.width * size.height * 4) as usize);
    for _ in 0..size.width * size.height {
        v.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    v
}

/// The output encode settings for an export to `out`.
fn output_spec(out: &str, canvas: Size, fps: u32) -> OutputSpec {
    OutputSpec {
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
    }
}

/// The most recent cursor position at `t`, normalized to the source size.
fn cursor_from_events(events: &[TimedInputEvent], t: TimeStamp, src: Size) -> Option<Point> {
    let mut last = None;
    for e in events {
        if e.timestamp.as_nanos() > t.as_nanos() {
            break;
        }
        match e.event {
            InputEvent::PointerMove { x, y }
            | InputEvent::ButtonDown { x, y, .. }
            | InputEvent::ButtonUp { x, y, .. } => last = Some((x, y)),
            _ => {}
        }
    }
    last.map(|(x, y)| {
        Point::new(
            (x / src.width.max(1) as f32).clamp(0.0, 1.0),
            (y / src.height.max(1) as f32).clamp(0.0, 1.0),
        )
    })
}

/// A starter timeline for a fresh recording: a cursor-follow camera and marker
/// over the whole clip, plus a click ripple at each recorded mouse-down.
fn starter_timeline(clip: &RecordedClip) -> Vec<ModifierSpec> {
    let full = TimeRange::new(
        TimeStamp::ZERO,
        TimeStamp::from_nanos(clip.duration_ns.max(1)),
    );
    let mut specs = vec![
        ModifierSpec {
            kind: ModifierKind::Camera,
            range: full,
            params: ModifierParams::CursorFollow {
                zoom: 1.6,
                tightness: 1.0,
            },
        },
        ModifierSpec {
            kind: ModifierKind::Overlay,
            range: full,
            params: ModifierParams::CursorMarker { size: 0.04 },
        },
    ];
    for e in &clip.events {
        if let InputEvent::ButtonDown { x, y, .. } = e.event {
            let center = Point::new(
                (x / clip.size.width.max(1) as f32).clamp(0.0, 1.0),
                (y / clip.size.height.max(1) as f32).clamp(0.0, 1.0),
            );
            let end = TimeStamp::from_nanos(e.timestamp.as_nanos() + RIPPLE_NS);
            specs.push(ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(e.timestamp, end),
                params: ModifierParams::ClickRipple {
                    center,
                    max_radius: 0.12,
                },
            });
        }
    }
    specs
}

/// The default project shown on first launch (synthetic source).
fn default_project() -> Project {
    let secs = |s: f32| TimeStamp::from_nanos((s * 1e9) as u64);
    Project {
        version: chroma_project::CURRENT_VERSION,
        source: SourceMedia {
            video_path: PathBuf::new(),
            event_log_path: PathBuf::new(),
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
    use std::process::Command;

    #[test]
    fn default_project_renders_a_nonempty_frame() {
        let mut studio = Studio::new();
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
    fn not_recording_initially() {
        let studio = Studio::new();
        assert!(!studio.is_recording());
        assert_eq!(studio.record_elapsed_ns(), 0);
    }

    #[test]
    fn save_load_round_trips() {
        let studio = Studio::new();
        let path = std::env::temp_dir().join("chroma_studio_roundtrip.json");
        studio.save(&path).unwrap();
        let loaded = Studio::load(&path).unwrap();
        assert_eq!(loaded.project().canvas, studio.project().canvas);
        let _ = std::fs::remove_file(&path);
    }

    /// Makes a short test-pattern MP4, or `None` when ffmpeg is absent.
    fn make_test_mp4(name: &str) -> Option<String> {
        if !ffmpeg_available() {
            return None;
        }
        let path = std::env::temp_dir()
            .join(format!("chroma_studio_{name}_{}.mp4", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let ok = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=2",
                "-pix_fmt",
                "yuv420p",
                &path,
            ])
            .status()
            .ok()?
            .success();
        ok.then_some(path)
    }

    #[test]
    fn loads_a_clip_and_renders_real_footage() {
        let Some(path) = make_test_mp4("load") else {
            return;
        };
        let mut studio = Studio::new();
        studio.load_clip(PathBuf::from(&path), vec![]).unwrap();
        // Canvas snapped to the clip size; timeline length ≈ clip duration.
        assert_eq!(studio.project().canvas, Size::new(320, 240));
        assert!(studio.timeline_duration_ns() > 1_500_000_000);
        let (size, rgba) = studio.render_preview_rgba(TimeStamp::from_nanos(1_000_000_000));
        assert_eq!(size, Size::new(320, 240));
        assert_eq!(rgba.len(), (size.width * size.height * 4) as usize);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn exports_a_loaded_clip() {
        let Some(path) = make_test_mp4("export") else {
            return;
        };
        let mut studio = Studio::new();
        studio.load_clip(PathBuf::from(&path), vec![]).unwrap();
        let out = std::env::temp_dir().join("chroma_studio_clip_out.mp4");
        studio.export(&out, None, None).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn export_synthetic_when_ffmpeg_present() {
        if !ffmpeg_available() {
            return;
        }
        let studio = Studio::new();
        let path = std::env::temp_dir().join("chroma_studio_export.mp4");
        studio.export(&path, Some(1), Some(15)).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        let _ = std::fs::remove_file(&path);
    }
}
