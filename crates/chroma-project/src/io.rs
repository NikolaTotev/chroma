//! Versioned save/load of a [`Project`] as JSON (spec EDT-11).

use crate::error::{ProjectError, Result};
use chroma_core_api::Project;
use std::path::Path;

/// The newest project-format version this build writes and can read.
pub const CURRENT_VERSION: u32 = 1;

/// Serializes `project` to pretty JSON at `path`, stamping the current version.
pub fn save(project: &Project, path: impl AsRef<Path>) -> Result<()> {
    let mut project = project.clone();
    project.version = CURRENT_VERSION;
    let json =
        serde_json::to_string_pretty(&project).map_err(|e| ProjectError::Serde(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| ProjectError::Io(e.to_string()))
}

/// Loads and migrates a project from `path`.
///
/// Rejects files written by a newer build (`version` ahead of
/// [`CURRENT_VERSION`]); older versions are run through [`migrate`].
pub fn load(path: impl AsRef<Path>) -> Result<Project> {
    let bytes = std::fs::read(path).map_err(|e| ProjectError::Io(e.to_string()))?;
    let mut project: Project =
        serde_json::from_slice(&bytes).map_err(|e| ProjectError::Serde(e.to_string()))?;
    migrate(&mut project)?;
    Ok(project)
}

/// Parses a project from an in-memory JSON string (no filesystem), migrating it.
pub fn from_json(json: &str) -> Result<Project> {
    let mut project: Project =
        serde_json::from_str(json).map_err(|e| ProjectError::Serde(e.to_string()))?;
    migrate(&mut project)?;
    Ok(project)
}

/// Brings an older project up to [`CURRENT_VERSION`], in place.
///
/// There is only one format version today, so this just range-checks; the
/// stepwise migration ladder slots in here as the schema evolves.
fn migrate(project: &mut Project) -> Result<()> {
    if project.version > CURRENT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: project.version,
            supported: CURRENT_VERSION,
        });
    }
    // Future: `while project.version < CURRENT_VERSION { step(project); }`.
    project.version = CURRENT_VERSION;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{
        Background, ModifierKind, ModifierParams, ModifierSpec, Project, Rect, SceneStyle, Size,
        SourceMedia, TimeRange, TimeStamp,
    };
    use std::path::PathBuf;

    fn sample_project() -> Project {
        Project {
            version: 0,
            source: SourceMedia {
                video_path: PathBuf::from("rec.mp4"),
                event_log_path: PathBuf::from("rec.events"),
                fps: 60,
                size: Size::new(1920, 1080),
            },
            canvas: Size::new(1280, 720),
            background: Background::Solid([0.1, 0.1, 0.1, 1.0]),
            scene: SceneStyle::default(),
            modifiers: vec![ModifierSpec {
                kind: ModifierKind::Overlay,
                range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(1000)),
                params: ModifierParams::Text {
                    content: "hi".to_owned(),
                    rect: Rect::FULL,
                    rgba: [1.0; 4],
                },
            }],
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let p = sample_project();
        let path = std::env::temp_dir().join("chroma_io_roundtrip.json");
        save(&p, &path).unwrap();
        let loaded = load(&path).unwrap();
        // Version is stamped to current on save; compare the rest by equality
        // after aligning versions.
        let mut expected = p;
        expected.version = CURRENT_VERSION;
        assert_eq!(loaded, expected);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_future_versions() {
        let mut p = sample_project();
        p.version = CURRENT_VERSION + 5;
        let json = serde_json::to_string(&p).unwrap();
        match from_json(&json) {
            Err(ProjectError::UnsupportedVersion { found, supported }) => {
                assert_eq!(found, CURRENT_VERSION + 5);
                assert_eq!(supported, CURRENT_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn migrates_old_version_to_current() {
        let p = from_json(&serde_json::to_string(&sample_project()).unwrap()).unwrap();
        assert_eq!(p.version, CURRENT_VERSION);
    }
}
