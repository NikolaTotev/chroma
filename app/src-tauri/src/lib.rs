//! Chroma Studio desktop shell.
//!
//! A thin Tauri command layer over [`chroma_studio::Studio`] — the editor's
//! engine. Each command locks the shared studio, performs one operation, and
//! returns a fresh [`StateSnapshot`] the front end renders from. All the real
//! logic (edits, undo, preview render, export) lives in `chroma-studio` and is
//! unit-tested there; this file only marshals it to the webview.

use base64::Engine;
use chroma_core_api::{Background, ModifierSpec, Project, SceneStyle, TimeStamp};
use chroma_studio::Studio;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

/// Shared editor state behind a mutex (commands are serialized).
struct AppState(Mutex<Studio>);

/// The editor snapshot handed to the front end after every command.
#[derive(Serialize)]
struct StateSnapshot {
    project: Project,
    can_undo: bool,
    can_redo: bool,
    duration_ns: u64,
    presets: Vec<String>,
    is_recording: bool,
    record_elapsed_ns: u64,
}

fn snapshot(studio: &Studio) -> StateSnapshot {
    StateSnapshot {
        project: studio.project().clone(),
        can_undo: studio.can_undo(),
        can_redo: studio.can_redo(),
        duration_ns: studio.timeline_duration_ns(),
        presets: studio.preset_names(),
        is_recording: studio.is_recording(),
        record_elapsed_ns: studio.record_elapsed_ns(),
    }
}

#[tauri::command]
fn studio_state(state: State<AppState>) -> StateSnapshot {
    snapshot(&state.0.lock().unwrap())
}

#[tauri::command]
fn new_project(state: State<AppState>) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    *s = Studio::new();
    snapshot(&s)
}

#[tauri::command]
fn apply_preset(state: State<AppState>, name: String) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.apply_preset(&name);
    snapshot(&s)
}

#[tauri::command]
fn set_background(state: State<AppState>, bg: Background) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.set_background(bg);
    snapshot(&s)
}

#[tauri::command]
fn set_scene(state: State<AppState>, scene: SceneStyle) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.set_scene(scene);
    snapshot(&s)
}

#[tauri::command]
fn add_modifier(state: State<AppState>, spec: ModifierSpec) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.add_modifier(spec);
    snapshot(&s)
}

#[tauri::command]
fn remove_modifier(state: State<AppState>, index: usize) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.remove_modifier(index);
    snapshot(&s)
}

#[tauri::command]
fn undo(state: State<AppState>) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.undo();
    snapshot(&s)
}

#[tauri::command]
fn redo(state: State<AppState>) -> StateSnapshot {
    let mut s = state.0.lock().unwrap();
    s.redo();
    snapshot(&s)
}

/// Starts recording the screen at `fps`. Errors if the capture backend is
/// unavailable (needs a native X11/Xorg session).
#[tauri::command]
fn start_record(state: State<AppState>, fps: u32) -> Result<StateSnapshot, String> {
    let mut s = state.0.lock().unwrap();
    s.start_record(fps)?;
    Ok(snapshot(&s))
}

/// Stops recording and loads the take as the editor's source.
#[tauri::command]
fn stop_record(state: State<AppState>) -> Result<StateSnapshot, String> {
    let mut s = state.0.lock().unwrap();
    s.stop_record()?;
    Ok(snapshot(&s))
}

#[tauri::command]
fn save_project(state: State<AppState>, path: String) -> Result<StateSnapshot, String> {
    let s = state.0.lock().unwrap();
    s.save(&path).map_err(|e| e.to_string())?;
    Ok(snapshot(&s))
}

#[tauri::command]
fn open_project(state: State<AppState>, path: String) -> Result<StateSnapshot, String> {
    let loaded = Studio::load(&path).map_err(|e| e.to_string())?;
    let mut s = state.0.lock().unwrap();
    *s = loaded;
    Ok(snapshot(&s))
}

/// Renders the composited frame at `time_ns` as a `data:image/bmp;base64,…` URL.
#[tauri::command]
fn render_preview(state: State<AppState>, time_ns: u64) -> String {
    let s = state.0.lock().unwrap();
    let bmp = s.render_preview_bmp(TimeStamp::from_nanos(time_ns));
    let b64 = base64::engine::general_purpose::STANDARD.encode(bmp);
    format!("data:image/bmp;base64,{b64}")
}

#[tauri::command]
fn export_video(
    state: State<AppState>,
    path: String,
    secs: u32,
    fps: u32,
) -> Result<String, String> {
    let s = state.0.lock().unwrap();
    s.export(&path, Some(secs), Some(fps))?;
    Ok(format!("Exported {path}"))
}

/// Builds and runs the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState(Mutex::new(Studio::new())))
        .invoke_handler(tauri::generate_handler![
            studio_state,
            new_project,
            apply_preset,
            set_background,
            set_scene,
            add_modifier,
            remove_modifier,
            undo,
            redo,
            start_record,
            stop_record,
            save_project,
            open_project,
            render_preview,
            export_video,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Chroma Studio");
}
