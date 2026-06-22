//! Undo/redo command history over a [`Project`] (spec EDT-10).
//!
//! Editing goes through [`EditCommand`]s rather than mutating the project
//! directly. [`History`] applies each command, recording its exact inverse, so
//! undo and redo are lossless and symmetric — applying a command's inverse
//! yields the original command, which is what makes redo fall out for free.

use chroma_core_api::{Background, ModifierSpec, Project, SceneStyle, Size};

/// A single reversible edit to a [`Project`].
///
/// `Insert`/`Remove`/`Update` carry an index into [`Project::modifiers`]; the
/// `Set*` commands replace a whole field. [`Noop`](EditCommand::Noop) is the
/// inverse produced for an out-of-range edit, so the history stays total.
#[derive(Debug, Clone, PartialEq)]
pub enum EditCommand {
    /// Replace the background.
    SetBackground(Background),
    /// Replace the scene styling.
    SetScene(SceneStyle),
    /// Replace the output canvas size.
    SetCanvas(Size),
    /// Append a modifier to the end of the lane list.
    AddModifier(ModifierSpec),
    /// Insert a modifier at `index` (the inverse of a remove).
    InsertModifier { index: usize, spec: ModifierSpec },
    /// Remove the modifier at `index`.
    RemoveModifier { index: usize },
    /// Replace the modifier at `index`.
    UpdateModifier { index: usize, spec: ModifierSpec },
    /// Does nothing; its own inverse.
    Noop,
}

/// Applies `cmd` to `project` and returns the command that exactly undoes it.
fn apply_to(project: &mut Project, cmd: EditCommand) -> EditCommand {
    match cmd {
        EditCommand::SetBackground(bg) => {
            EditCommand::SetBackground(std::mem::replace(&mut project.background, bg))
        }
        EditCommand::SetScene(scene) => {
            EditCommand::SetScene(std::mem::replace(&mut project.scene, scene))
        }
        EditCommand::SetCanvas(size) => {
            EditCommand::SetCanvas(std::mem::replace(&mut project.canvas, size))
        }
        EditCommand::AddModifier(spec) => {
            project.modifiers.push(spec);
            EditCommand::RemoveModifier {
                index: project.modifiers.len() - 1,
            }
        }
        EditCommand::InsertModifier { index, spec } => {
            let i = index.min(project.modifiers.len());
            project.modifiers.insert(i, spec);
            EditCommand::RemoveModifier { index: i }
        }
        EditCommand::RemoveModifier { index } => {
            if index >= project.modifiers.len() {
                return EditCommand::Noop;
            }
            let spec = project.modifiers.remove(index);
            EditCommand::InsertModifier { index, spec }
        }
        EditCommand::UpdateModifier { index, spec } => {
            if index >= project.modifiers.len() {
                return EditCommand::Noop;
            }
            let old = std::mem::replace(&mut project.modifiers[index], spec);
            EditCommand::UpdateModifier { index, spec: old }
        }
        EditCommand::Noop => EditCommand::Noop,
    }
}

/// A bounded-free undo/redo stack bound to a project's edits.
///
/// The caller owns the [`Project`]; the history holds only the commands needed
/// to step it backward and forward.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<EditCommand>,
    redo: Vec<EditCommand>,
}

impl History {
    /// A fresh, empty history.
    pub fn new() -> Self {
        History::default()
    }

    /// Applies `cmd` to `project`, recording it for undo and clearing the redo
    /// stack (a new edit forks the timeline).
    pub fn apply(&mut self, project: &mut Project, cmd: EditCommand) {
        let inverse = apply_to(project, cmd);
        self.undo.push(inverse);
        self.redo.clear();
    }

    /// Reverts the most recent applied command. Returns `false` if there is
    /// nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> bool {
        match self.undo.pop() {
            Some(inverse) => {
                let forward = apply_to(project, inverse);
                self.redo.push(forward);
                true
            }
            None => false,
        }
    }

    /// Re-applies the most recently undone command. Returns `false` if there is
    /// nothing to redo.
    pub fn redo(&mut self, project: &mut Project) -> bool {
        match self.redo.pop() {
            Some(forward) => {
                let inverse = apply_to(project, forward);
                self.undo.push(inverse);
                true
            }
            None => false,
        }
    }

    /// Whether an [`undo`](Self::undo) would do something.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a [`redo`](Self::redo) would do something.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::{
        ModifierKind, ModifierParams, Project, Rect, SceneStyle, Size, SourceMedia, TimeRange,
        TimeStamp,
    };
    use std::path::PathBuf;

    fn empty_project() -> Project {
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

    fn text_spec(content: &str) -> ModifierSpec {
        ModifierSpec {
            kind: ModifierKind::Overlay,
            range: TimeRange::new(TimeStamp::ZERO, TimeStamp::from_nanos(10)),
            params: ModifierParams::Text {
                content: content.to_owned(),
                rect: Rect::FULL,
                rgba: [1.0; 4],
            },
        }
    }

    #[test]
    fn add_then_undo_redo() {
        let mut p = empty_project();
        let mut h = History::new();
        h.apply(&mut p, EditCommand::AddModifier(text_spec("a")));
        h.apply(&mut p, EditCommand::AddModifier(text_spec("b")));
        assert_eq!(p.modifiers.len(), 2);

        assert!(h.undo(&mut p));
        assert_eq!(p.modifiers.len(), 1);
        assert!(h.undo(&mut p));
        assert_eq!(p.modifiers.len(), 0);
        assert!(!h.undo(&mut p), "nothing left to undo");

        assert!(h.redo(&mut p));
        assert!(h.redo(&mut p));
        assert_eq!(p.modifiers.len(), 2);
    }

    #[test]
    fn set_background_is_reversible() {
        let mut p = empty_project();
        let mut h = History::new();
        let new_bg = Background::Solid([1.0, 0.0, 0.0, 1.0]);
        h.apply(&mut p, EditCommand::SetBackground(new_bg.clone()));
        assert_eq!(p.background, new_bg);
        h.undo(&mut p);
        assert_eq!(p.background, Background::Solid([0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn remove_and_update_round_trip() {
        let mut p = empty_project();
        let mut h = History::new();
        h.apply(&mut p, EditCommand::AddModifier(text_spec("first")));
        h.apply(
            &mut p,
            EditCommand::UpdateModifier {
                index: 0,
                spec: text_spec("edited"),
            },
        );
        h.apply(&mut p, EditCommand::RemoveModifier { index: 0 });
        assert!(p.modifiers.is_empty());

        h.undo(&mut p); // un-remove
        assert_eq!(p.modifiers.len(), 1);
        h.undo(&mut p); // un-edit → back to "first"
        match &p.modifiers[0].params {
            ModifierParams::Text { content, .. } => assert_eq!(content, "first"),
            _ => panic!("wrong params"),
        }
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut p = empty_project();
        let mut h = History::new();
        h.apply(&mut p, EditCommand::AddModifier(text_spec("a")));
        h.undo(&mut p);
        assert!(h.can_redo());
        h.apply(&mut p, EditCommand::AddModifier(text_spec("b")));
        assert!(!h.can_redo(), "a fresh edit forks the timeline");
    }

    #[test]
    fn out_of_range_remove_is_a_noop() {
        let mut p = empty_project();
        let mut h = History::new();
        h.apply(&mut p, EditCommand::RemoveModifier { index: 9 });
        assert!(p.modifiers.is_empty());
        // The recorded inverse is a Noop; undo does nothing harmful.
        assert!(h.undo(&mut p));
        assert!(p.modifiers.is_empty());
    }
}
