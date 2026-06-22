//! Built-in look presets (spec EDT-09).
//!
//! A [`Preset`] is a named background + scene-style pairing — a one-click look.
//! Applying one goes through the [`History`] as two `Set*` commands, so it is a
//! single undoable step like any other edit.

use crate::history::{EditCommand, History};
use chroma_core_api::{Background, GradientStop, Project, SceneStyle, Shadow};

/// A named, ready-made look: a background and the scene styling that suits it.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// Display name for the GUI.
    pub name: &'static str,
    /// The background this preset applies.
    pub background: Background,
    /// The scene styling this preset applies.
    pub scene: SceneStyle,
}

impl Preset {
    /// Applies this preset's background and scene to `project` through `history`
    /// (two undoable commands).
    pub fn apply(&self, history: &mut History, project: &mut Project) {
        history.apply(project, EditCommand::SetBackground(self.background.clone()));
        history.apply(project, EditCommand::SetScene(self.scene));
    }
}

fn gradient(angle_deg: f32, a: [f32; 4], b: [f32; 4]) -> Background {
    Background::Gradient {
        angle_deg,
        stops: vec![
            GradientStop {
                offset: 0.0,
                rgba: a,
            },
            GradientStop {
                offset: 1.0,
                rgba: b,
            },
        ],
    }
}

/// The presets shipped with Chroma, in display order.
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        Preset {
            name: "Clean",
            background: Background::Solid([0.96, 0.96, 0.97, 1.0]),
            scene: SceneStyle {
                padding: 0.05,
                corner_radius: 0.03,
                shadow: Some(Shadow {
                    dx: 0.0,
                    dy: 0.012,
                    blur: 0.025,
                    rgba: [0.0, 0.0, 0.0, 0.25],
                }),
                border: None,
            },
        },
        Preset {
            name: "Vibrant",
            background: gradient(35.0, [0.08, 0.10, 0.28, 1.0], [0.50, 0.16, 0.42, 1.0]),
            scene: SceneStyle {
                padding: 0.07,
                corner_radius: 0.05,
                shadow: Some(Shadow {
                    dx: 0.0,
                    dy: 0.02,
                    blur: 0.04,
                    rgba: [0.0, 0.0, 0.0, 0.5],
                }),
                border: None,
            },
        },
        Preset {
            name: "Spotlight",
            background: gradient(90.0, [0.02, 0.02, 0.03, 1.0], [0.12, 0.12, 0.16, 1.0]),
            scene: SceneStyle {
                padding: 0.09,
                corner_radius: 0.06,
                shadow: Some(Shadow {
                    dx: 0.0,
                    dy: 0.03,
                    blur: 0.06,
                    rgba: [0.0, 0.0, 0.0, 0.6],
                }),
                border: None,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{Size, SourceMedia};
    use std::path::PathBuf;

    fn project() -> Project {
        Project {
            version: 1,
            source: SourceMedia {
                video_path: PathBuf::from("v"),
                event_log_path: PathBuf::from("e"),
                fps: 30,
                size: Size::new(640, 360),
            },
            canvas: Size::new(640, 360),
            background: Background::Solid([0.0, 0.0, 0.0, 1.0]),
            scene: SceneStyle::default(),
            modifiers: vec![],
        }
    }

    #[test]
    fn ships_at_least_three_presets() {
        assert!(builtin_presets().len() >= 3);
    }

    #[test]
    fn apply_sets_look_and_is_one_undo_step_per_field() {
        let mut p = project();
        let mut h = History::new();
        let preset = builtin_presets()[1].clone(); // Vibrant
        preset.apply(&mut h, &mut p);
        assert_eq!(p.background, preset.background);
        assert_eq!(p.scene, preset.scene);

        // Two commands (background + scene); undo both to restore.
        assert!(h.undo(&mut p));
        assert!(h.undo(&mut p));
        assert_eq!(p.background, Background::Solid([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(p.scene, SceneStyle::default());
    }
}
