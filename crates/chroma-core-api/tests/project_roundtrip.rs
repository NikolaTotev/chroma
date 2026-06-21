//! The "empty project round-trips" smoke test (`ORCHESTRATION.md` §10).
//!
//! Proves the project model serializes and deserializes losslessly through
//! serde before any real `chroma-project` (de)serialization logic exists.

use chroma_core_api::{
    Background, GradientStop, ModifierKind, ModifierParams, ModifierSpec, Project, Rect, Size,
    SourceMedia, TimeRange, TimeStamp,
};
use std::path::PathBuf;

fn sample_project() -> Project {
    Project {
        version: 1,
        source: SourceMedia {
            video_path: PathBuf::from("rec/source.mkv"),
            event_log_path: PathBuf::from("rec/events.json"),
            fps: 60,
            size: Size::new(2560, 1440),
        },
        canvas: Size::new(1920, 1080),
        background: Background::Gradient {
            angle_deg: 45.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    rgba: [0.05, 0.05, 0.1, 1.0],
                },
                GradientStop {
                    offset: 1.0,
                    rgba: [0.2, 0.1, 0.4, 1.0],
                },
            ],
        },
        modifiers: vec![
            ModifierSpec {
                kind: ModifierKind::Camera,
                range: TimeRange::new(
                    TimeStamp::from_nanos(0),
                    TimeStamp::from_nanos(2_000_000_000),
                ),
                params: ModifierParams::CursorFollow {
                    zoom: 1.8,
                    tightness: 0.4,
                },
            },
            ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(
                    TimeStamp::from_nanos(500_000_000),
                    TimeStamp::from_nanos(1_500_000_000),
                ),
                params: ModifierParams::Text {
                    content: "Hello, Chroma".to_owned(),
                    rect: Rect::new(0.1, 0.8, 0.5, 0.1),
                    rgba: [1.0, 1.0, 1.0, 1.0],
                },
            },
        ],
    }
}

#[test]
fn empty_project_round_trips() {
    let project = Project {
        version: 1,
        source: SourceMedia {
            video_path: PathBuf::from("empty.mkv"),
            event_log_path: PathBuf::from("empty.json"),
            fps: 30,
            size: Size::new(1280, 720),
        },
        canvas: Size::new(1280, 720),
        background: Background::Solid([0.0, 0.0, 0.0, 1.0]),
        modifiers: Vec::new(),
    };

    let json = serde_json::to_string(&project).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(project, back);
}

#[test]
fn populated_project_round_trips() {
    let project = sample_project();
    let json = serde_json::to_string_pretty(&project).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(project, back);
}
