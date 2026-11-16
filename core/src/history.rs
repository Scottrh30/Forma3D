//! Undo / redo.
//!
//! Instead of every editing operation having to describe its own inverse, the
//! history watches the scene: whenever it differs from the last *committed*
//! state and no gesture (mouse drag, text editing) is in progress, the change is
//! recorded as one undo step. A whole gizmo drag or a typed-in value therefore
//! becomes a single step automatically. Steps are full scene snapshots, which is
//! cheap while scenes hold parametric primitives rather than raw mesh data.

use crate::scene::Scene;

/// Default number of undo steps kept (Preferences > Undo Steps changes it).
pub const DEFAULT_STEPS: usize = 200;

struct Step {
    label: String,
    /// The scene as it was on the other side of this step.
    scene: Scene,
}

pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    committed: Scene,
    limit: usize,
}

impl History {
    pub fn new(scene: &Scene) -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            committed: scene.clone(),
            limit: DEFAULT_STEPS,
        }
    }

    /// Forgets all steps and treats `scene` as the new baseline (New / Open).
    pub fn reset(&mut self, scene: &Scene) {
        self.undo.clear();
        self.redo.clear();
        self.committed = scene.clone();
    }

    /// Call once per frame after all edits. Records a step if the scene changed
    /// and no gesture is in progress. Returns true if a step was recorded.
    pub fn update(&mut self, scene: &Scene, gesture_in_progress: bool, label: &str) -> bool {
        if gesture_in_progress || *scene == self.committed {
            return false;
        }
        self.undo.push(Step {
            label: label.to_owned(),
            scene: std::mem::replace(&mut self.committed, scene.clone()),
        });
        self.trim();
        self.redo.clear();
        true
    }

    /// How many undo steps to keep (at least 1). Extra old steps are dropped.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        self.trim();
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    fn trim(&mut self) {
        if self.undo.len() > self.limit {
            let extra = self.undo.len() - self.limit;
            self.undo.drain(..extra);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// True while there are unrecorded edits (a gesture is still in progress).
    pub fn has_pending_changes(&self, scene: &Scene) -> bool {
        *scene != self.committed
    }

    pub fn undo(&mut self, scene: &mut Scene) -> Option<String> {
        // Edits still pending (mid-gesture) are folded into the step being undone.
        let step = self.undo.pop()?;
        self.redo.push(Step {
            label: step.label.clone(),
            scene: scene.clone(),
        });
        *scene = step.scene;
        self.committed = scene.clone();
        Some(step.label)
    }

    pub fn redo(&mut self, scene: &mut Scene) -> Option<String> {
        let step = self.redo.pop()?;
        self.undo.push(Step {
            label: step.label.clone(),
            scene: scene.clone(),
        });
        *scene = step.scene;
        self.committed = scene.clone();
        Some(step.label)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_undo_limit_can_be_changed() {
        let mut scene = Scene::new();
        let mut h = History::new(&scene);
        for _ in 0..30 {
            scene.add_null(None);
            h.update(&scene, false, "Add");
        }
        h.set_limit(10);
        assert_eq!(h.limit(), 10);
        let mut undone = 0;
        while h.undo(&mut scene).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 10);
        h.set_limit(0);
        assert_eq!(h.limit(), 1, "at least one step");
    }

    use super::*;
    use crate::primitive::PrimitiveKind;

    #[test]
    fn unchanged_scene_records_nothing() {
        let scene = Scene::demo();
        let mut h = History::new(&scene);
        assert!(!h.update(&scene, false, "x"));
        assert!(h.undo_label().is_none());
    }

    #[test]
    fn change_is_recorded_once_and_can_be_undone_and_redone() {
        let mut scene = Scene::new();
        let mut h = History::new(&scene);
        scene.add_primitive(PrimitiveKind::Cube, None);
        assert!(h.update(&scene, false, "Create Cube"));
        assert!(!h.update(&scene, false, "again"), "same state must not record twice");
        assert_eq!(h.undo_label(), Some("Create Cube"));

        assert_eq!(h.undo(&mut scene).as_deref(), Some("Create Cube"));
        assert!(scene.is_empty());
        assert!(h.undo_label().is_none());
        assert_eq!(h.redo_label(), Some("Create Cube"));

        assert!(h.redo(&mut scene).is_some());
        assert_eq!(scene.len(), 1);
        assert!(!h.update(&scene, false, "x"), "undo/redo must leave no pending change");
    }

    #[test]
    fn a_whole_gesture_is_one_step() {
        let mut scene = Scene::new();
        let id = scene.add_primitive(PrimitiveKind::Sphere, None);
        let mut h = History::new(&scene);
        // Simulate a drag: many frames of change while the mouse is down.
        for i in 1..=30 {
            scene.get_mut(id).unwrap().pos.x = i as f32;
            assert!(!h.update(&scene, true, "Move"));
        }
        assert!(h.has_pending_changes(&scene));
        // Mouse released.
        assert!(h.update(&scene, false, "Move"));
        assert!(h.undo(&mut scene).is_some());
        assert_eq!(scene.get(id).unwrap().pos.x, 0.0, "one undo restores the pre-drag state");
        assert!(h.undo(&mut scene).is_none());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut scene = Scene::new();
        let mut h = History::new(&scene);
        scene.add_null(None);
        h.update(&scene, false, "A");
        h.undo(&mut scene);
        assert!(h.redo_label().is_some());
        scene.add_primitive(PrimitiveKind::Cone, None);
        h.update(&scene, false, "B");
        assert!(h.redo_label().is_none());
    }

    #[test]
    fn history_is_bounded() {
        let mut scene = Scene::new();
        let mut h = History::new(&scene);
        for _ in 0..(DEFAULT_STEPS + 50) {
            scene.add_null(None);
            h.update(&scene, false, "n");
        }
        let mut count = 0;
        while h.undo(&mut scene).is_some() {
            count += 1;
        }
        assert_eq!(count, DEFAULT_STEPS);
    }

    #[test]
    fn reset_drops_everything() {
        let mut scene = Scene::new();
        let mut h = History::new(&scene);
        scene.add_null(None);
        h.update(&scene, false, "A");
        h.reset(&scene);
        assert!(h.undo_label().is_none() && h.redo_label().is_none());
        assert!(!h.update(&scene, false, "x"));
    }
}
