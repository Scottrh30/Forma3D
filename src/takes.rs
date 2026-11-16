//! Takes (Cinema 4D's Take Manager): variations of one scene. The Main take
//! is the scene itself; every other take records overrides of objects'
//! position, rotation, scale, visibility, material, layer and on/off switch.
//!
//! Recording works like Cinema 4D's Auto Take: while a take other than Main
//! is active, whatever you change becomes that take's override. Switching
//! takes stores the active take's differences from Main and restores Main
//! before the next take's overrides go on.

use crate::layers::LayerId;
use crate::scene::{MaterialId, Object, ObjectId, Visibility};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The overridable state of one object.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjState {
    pub pos: Vec3,
    pub rot: Vec3,
    pub scale: Vec3,
    pub vis_editor: Visibility,
    pub vis_render: Visibility,
    pub material: Option<MaterialId>,
    pub layer: Option<LayerId>,
    pub enabled: bool,
}

impl ObjState {
    pub fn of(o: &Object) -> ObjState {
        ObjState {
            pos: o.pos,
            rot: o.rot,
            scale: o.scale,
            vis_editor: o.vis_editor,
            vis_render: o.vis_render,
            material: o.material,
            layer: o.layer,
            enabled: o.enabled,
        }
    }

    pub fn apply(&self, o: &mut Object) {
        o.pos = self.pos;
        o.rot = self.rot;
        o.scale = self.scale;
        o.vis_editor = self.vis_editor;
        o.vis_render = self.vis_render;
        o.material = self.material;
        o.layer = self.layer;
        o.enabled = self.enabled;
    }
}

/// The parts of an object a take changes (unset = as in Main).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Override {
    pub pos: Option<Vec3>,
    pub rot: Option<Vec3>,
    pub scale: Option<Vec3>,
    pub vis_editor: Option<Visibility>,
    pub vis_render: Option<Visibility>,
    pub material: Option<Option<MaterialId>>,
    pub layer: Option<Option<LayerId>>,
    pub enabled: Option<bool>,
}

impl Override {
    /// What differs between `base` and `now` (None when nothing does).
    pub fn diff(base: &ObjState, now: &ObjState) -> Option<Override> {
        fn ne<T: PartialEq + Copy>(a: T, b: T) -> Option<T> {
            (a != b).then_some(b)
        }
        let o = Override {
            pos: ne(base.pos, now.pos),
            rot: ne(base.rot, now.rot),
            scale: ne(base.scale, now.scale),
            vis_editor: ne(base.vis_editor, now.vis_editor),
            vis_render: ne(base.vis_render, now.vis_render),
            material: ne(base.material, now.material),
            layer: ne(base.layer, now.layer),
            enabled: ne(base.enabled, now.enabled),
        };
        (o != Override::default()).then_some(o)
    }

    pub fn apply(&self, o: &mut Object) {
        if let Some(v) = self.pos {
            o.pos = v;
        }
        if let Some(v) = self.rot {
            o.rot = v;
        }
        if let Some(v) = self.scale {
            o.scale = v;
        }
        if let Some(v) = self.vis_editor {
            o.vis_editor = v;
        }
        if let Some(v) = self.vis_render {
            o.vis_render = v;
        }
        if let Some(v) = self.material {
            o.material = v;
        }
        if let Some(v) = self.layer {
            o.layer = v;
        }
        if let Some(v) = self.enabled {
            o.enabled = v;
        }
    }

    /// Names of the overridden parameters, for the Take Manager.
    pub fn fields(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.pos.is_some() {
            v.push("Position");
        }
        if self.rot.is_some() {
            v.push("Rotation");
        }
        if self.scale.is_some() {
            v.push("Scale");
        }
        if self.vis_editor.is_some() {
            v.push("Visible in Editor");
        }
        if self.vis_render.is_some() {
            v.push("Visible in Renderer");
        }
        if self.material.is_some() {
            v.push("Material");
        }
        if self.layer.is_some() {
            v.push("Layer");
        }
        if self.enabled.is_some() {
            v.push("Enabled");
        }
        v
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub name: String,
    /// Overrides as last recorded. The active take's live overrides are the
    /// scene's differences from `TakeSystem::base`.
    pub overrides: BTreeMap<ObjectId, Override>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TakeSystem {
    /// Index 0 is the Main take, which has no overrides.
    pub takes: Vec<Take>,
    pub active: usize,
    /// While a take other than Main is active: every object's Main state, as
    /// it was when the take was entered.
    pub base: BTreeMap<ObjectId, ObjState>,
}

impl Default for TakeSystem {
    fn default() -> Self {
        TakeSystem { takes: vec![Take { name: "Main".to_owned(), overrides: BTreeMap::new() }], active: 0, base: BTreeMap::new() }
    }
}

impl TakeSystem {
    /// The active take's overrides as they stand (differences from Main).
    pub fn live_overrides<'a>(&self, objects: impl Iterator<Item = &'a Object>) -> BTreeMap<ObjectId, Override> {
        if self.active == 0 {
            return BTreeMap::new();
        }
        objects
            .filter_map(|o| {
                let base = self.base.get(&o.id)?;
                Some((o.id, Override::diff(base, &ObjState::of(o))?))
            })
            .collect()
    }

    pub fn sanitize(&mut self) {
        if self.takes.is_empty() {
            *self = TakeSystem::default();
        }
        self.takes[0].overrides.clear();
        if self.active >= self.takes.len() {
            self.active = 0;
        }
        if self.active == 0 {
            self.base.clear();
        }
    }
}
