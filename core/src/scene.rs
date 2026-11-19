//! The scene graph: objects, hierarchy, transforms and materials.
//!
//! Conventions (chosen to match Cinema 4D):
//! * left-handed, Y-up, units are centimetres;
//! * rotations are stored as HPB Euler angles in degrees (Heading about Y,
//!   Pitch about X, Bank about Z) composed as `Ry(H) * Rx(P) * Rz(B)`.

use crate::deformer::{Deformer, DeformerKind};
use crate::editmesh::EditMesh;
use crate::generator::{Generator, GeneratorKind};
use crate::layers::{Layer, LayerFlag, LayerId};
use crate::primitive::{Primitive, PrimitiveKind};
use crate::takes::{ObjState, Take, TakeSystem};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

pub type ObjectId = u64;
pub type MaterialId = u64;

/// Maximum hierarchy depth we are willing to walk (guards against corrupt files).
const MAX_DEPTH: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Visibility {
    /// Inherit from the parent (visible at the top level).
    #[default]
    Default,
    On,
    Off,
}

impl Visibility {
    /// Click-cycle used by the Object Manager dots: default -> off -> on -> default.
    pub fn cycled(self) -> Self {
        match self {
            Visibility::Default => Visibility::Off,
            Visibility::Off => Visibility::On,
            Visibility::On => Visibility::Default,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObjectKind {
    Null,
    Primitive(Primitive),
    /// Builds its geometry from its child objects.
    Generator(Generator),
    /// Bends / twists its parent's geometry.
    Deformer(Deformer),
    /// An editable polygon mesh (points and polygons).
    Polygon(EditMesh),
    /// A curve drawn with the Spline Pen.
    Spline(crate::spline::Spline),
    /// A light (the Light menu).
    Light(crate::light::Light),
    /// A camera to render through (Create > Camera).
    Camera(crate::scene_camera::SceneCamera),
    /// A sky around the scene (Light > Sky, Physical Sky).
    Sky(crate::sky::Sky),
}

impl ObjectKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            ObjectKind::Null => "Null",
            ObjectKind::Primitive(p) => p.kind().name(),
            ObjectKind::Generator(g) => g.kind().name(),
            ObjectKind::Deformer(d) => d.kind().name(),
            ObjectKind::Polygon(_) => "Polygon",
            ObjectKind::Spline(_) => "Spline",
            ObjectKind::Light(l) => l.kind.name(),
            ObjectKind::Camera(c) => c.kind.name(),
            ObjectKind::Sky(s) => s.kind.name(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    pub name: String,
    pub kind: ObjectKind,
    /// Local position (cm).
    pub pos: Vec3,
    /// Local rotation as HPB degrees.
    pub rot: Vec3,
    pub scale: Vec3,
    pub parent: Option<ObjectId>,
    pub children: Vec<ObjectId>,
    pub vis_editor: Visibility,
    pub vis_render: Visibility,
    pub material: Option<MaterialId>,
    /// Generators and deformers can be switched off (the check mark in the
    /// Object Manager). A disabled generator acts like a Null; a disabled
    /// deformer does nothing.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// The layer the object belongs to (Layer Manager).
    #[serde(default)]
    pub layer: Option<LayerId>,
}

fn enabled_default() -> bool {
    true
}

impl Object {
    pub fn local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, hpb_to_quat(self.rot), self.pos)
    }
}

/// A material: its colour (shown in the viewport) and how it reflects,
/// lets light through and glows in final renders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    pub color: [f32; 3],
    /// 0 is a mirror finish, 1 is completely matte.
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    /// 0 is a dielectric (plastic, paint, glass), 1 is a metal: metals tint
    /// their reflections with the colour and have no diffuse part.
    #[serde(default)]
    pub metalness: f32,
    /// How strongly a dielectric reflects (0.5 is the usual 4 %).
    #[serde(default = "default_specular")]
    pub specular: f32,
    /// 0 is opaque, 1 lets light through (glass, water).
    #[serde(default)]
    pub transparency: f32,
    /// Index of refraction for transparent materials.
    #[serde(default = "default_ior")]
    pub ior: f32,
    /// Light the surface gives off: a colour and how strong it is.
    #[serde(default)]
    pub emission: [f32; 3],
    #[serde(default)]
    pub emission_strength: f32,
    /// An image for the colour, projected onto the object.
    #[serde(default)]
    pub texture: Option<crate::texture::Texture>,
}

fn default_roughness() -> f32 {
    0.5
}

fn default_specular() -> f32 {
    0.5
}

fn default_ior() -> f32 {
    1.5
}

impl Material {
    pub fn new(id: MaterialId, name: &str, color: [f32; 3]) -> Material {
        Material {
            id,
            name: name.to_owned(),
            color,
            roughness: default_roughness(),
            metalness: 0.0,
            specular: default_specular(),
            transparency: 0.0,
            ior: default_ior(),
            emission: [1.0, 1.0, 1.0],
            emission_strength: 0.0,
            texture: None,
        }
    }

    /// Values kept in range (and NaN-free).
    pub fn sanitized(&self) -> Material {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let c = |a: [f32; 3]| a.map(|x| f(x, 0.0, 1.0, 0.5));
        Material {
            id: self.id,
            name: self.name.clone(),
            color: c(self.color),
            roughness: f(self.roughness, 0.0, 1.0, 0.5),
            metalness: f(self.metalness, 0.0, 1.0, 0.0),
            specular: f(self.specular, 0.0, 1.0, 0.5),
            transparency: f(self.transparency, 0.0, 1.0, 0.0),
            ior: f(self.ior, 1.0, 3.0, 1.5),
            emission: c(self.emission),
            emission_strength: f(self.emission_strength, 0.0, 1000.0, 0.0),
            texture: self.texture.as_ref().map(|t| t.sanitized()),
        }
    }
}

/// Where a dragged object should land in the hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// Become the last child of this object.
    Into(ObjectId),
    /// Become the previous sibling of this object.
    Before(ObjectId),
    /// Become the next sibling of this object.
    After(ObjectId),
    /// Become the last top-level object.
    Root,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    next_object: ObjectId,
    next_material: MaterialId,
    objects: BTreeMap<ObjectId, Object>,
    roots: Vec<ObjectId>,
    materials: Vec<Material>,
    #[serde(default)]
    layers: Vec<Layer>,
    #[serde(default = "first_id")]
    next_layer: LayerId,
    /// The construction plane the pens draw on and the grid shows.
    #[serde(default)]
    pub workplane: Workplane,
    #[serde(default)]
    takes: TakeSystem,
    /// Render Settings (saved with the project).
    #[serde(default)]
    pub render: crate::render::RenderSettings,
    /// The camera renders go through (None: the active view's camera).
    #[serde(default)]
    pub render_camera: Option<ObjectId>,
}

fn first_id() -> u64 {
    1
}

/// The workplane (Cinema 4D's Workplane mode): a movable construction plane.
/// Its local XZ plane is the plane; Y is its normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Workplane {
    pub pos: Vec3,
    /// HPB degrees.
    pub rot: Vec3,
}

impl Workplane {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(hpb_to_quat(self.rot), self.pos)
    }

    pub fn normal(&self) -> Vec3 {
        hpb_to_quat(self.rot) * Vec3::Y
    }

    /// The floor at the origin.
    pub fn is_default(&self) -> bool {
        self.pos.length() < 1e-4 && self.rot.length() < 1e-4
    }
}

impl Default for Scene {
    fn default() -> Self {
        Scene::new()
    }
}

// ---------------------------------------------------------------------------
// Rotation helpers
// ---------------------------------------------------------------------------

pub fn hpb_to_quat(hpb_degrees: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        hpb_degrees.x.to_radians(),
        hpb_degrees.y.to_radians(),
        hpb_degrees.z.to_radians(),
    )
}

/// Converts a rotation back to HPB degrees, choosing the equivalent
/// representation closest to `reference` so values do not flip while a gizmo
/// is being dragged.
pub fn quat_to_hpb_near(q: Quat, reference: Vec3) -> Vec3 {
    let (h, p, b) = q.normalize().to_euler(EulerRot::YXZ);
    let reference = Vec3::new(
        reference.x.to_radians(),
        reference.y.to_radians(),
        reference.z.to_radians(),
    );
    let unwrap = |angle: f32, near: f32| angle + TAU * ((near - angle) / TAU).round();
    let fit = |c: Vec3| Vec3::new(unwrap(c.x, reference.x), unwrap(c.y, reference.y), unwrap(c.z, reference.z));
    let primary = fit(Vec3::new(h, p, b));
    // The same rotation can also be written as (h + 180, 180 - p, b + 180).
    let alternate = fit(Vec3::new(h + PI, PI - p, b + PI));
    let best = if (alternate - reference).length_squared() < (primary - reference).length_squared() {
        alternate
    } else {
        primary
    };
    Vec3::new(best.x.to_degrees(), best.y.to_degrees(), best.z.to_degrees())
}

/// The rotation part of a matrix, ignoring scale.
pub fn matrix_rotation(m: Mat4) -> Quat {
    let basis = Mat3::from_cols(
        m.x_axis.truncate().normalize_or_zero(),
        m.y_axis.truncate().normalize_or_zero(),
        m.z_axis.truncate().normalize_or_zero(),
    );
    Quat::from_mat3(&basis).normalize()
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

impl Scene {
    pub fn new() -> Self {
        Scene {
            next_object: 1,
            next_material: 1,
            objects: BTreeMap::new(),
            roots: Vec::new(),
            materials: Vec::new(),
            layers: Vec::new(),
            next_layer: 1,
            workplane: Workplane::default(),
            takes: TakeSystem::default(),
            render: Default::default(),
            render_camera: None,
        }
    }

    /// The starter scene shown on first launch. Shapes start with no material,
    /// so they all draw in the one default grey; the materials wait in the
    /// Material Manager to be dragged on.
    pub fn demo() -> Self {
        let mut s = Scene::new();
        s.add_material("Steel Blue", [0.36, 0.52, 0.72]);
        s.add_material("Terracotta", [0.80, 0.45, 0.32]);
        s.add_material("Sage", [0.52, 0.68, 0.50]);

        let cube = s.add_primitive(PrimitiveKind::Cube, None);
        s.set_position(cube, Vec3::new(-300.0, 100.0, 0.0));

        let sphere = s.add_primitive(PrimitiveKind::Sphere, None);
        s.set_position(sphere, Vec3::new(0.0, 100.0, 0.0));

        let torus = s.add_primitive(PrimitiveKind::Torus, None);
        s.set_position(torus, Vec3::new(300.0, 50.0, 0.0));
        s
    }

    // --- lookup -----------------------------------------------------------

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.objects.get(&id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.get_mut(&id)
    }

    pub fn contains(&self, id: ObjectId) -> bool {
        self.objects.contains_key(&id)
    }

    pub fn roots(&self) -> &[ObjectId] {
        &self.roots
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn materials(&self) -> &[Material] {
        &self.materials
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    pub fn material_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.iter_mut().find(|m| m.id == id)
    }

    /// Children of `parent`, or the top level when `None`.
    pub fn children_of(&self, parent: Option<ObjectId>) -> &[ObjectId] {
        match parent {
            Some(p) => self.objects.get(&p).map(|o| o.children.as_slice()).unwrap_or(&[]),
            None => &self.roots,
        }
    }

    fn sibling_list_mut(&mut self, parent: Option<ObjectId>) -> &mut Vec<ObjectId> {
        match parent {
            Some(p) if self.objects.contains_key(&p) => &mut self.objects.get_mut(&p).unwrap().children,
            _ => &mut self.roots,
        }
    }

    /// True if `id` is a strict descendant of `ancestor`.
    pub fn is_descendant(&self, id: ObjectId, ancestor: ObjectId) -> bool {
        let mut cur = self.objects.get(&id).and_then(|o| o.parent);
        for _ in 0..MAX_DEPTH {
            match cur {
                Some(c) if c == ancestor => return true,
                Some(c) => cur = self.objects.get(&c).and_then(|o| o.parent),
                None => return false,
            }
        }
        false
    }

    /// `id` and everything beneath it, parents before children.
    /// A copy holding only `id` and its descendants, with their ids kept
    /// and `id` made top-level (enough to build its mesh elsewhere).
    pub fn subtree_scene(&self, id: ObjectId) -> Scene {
        let keep: std::collections::HashSet<ObjectId> = self.subtree(id).into_iter().collect();
        let mut out = Scene { objects: BTreeMap::new(), roots: Vec::new(), ..self.clone_settings() };
        for (&k, o) in &self.objects {
            if keep.contains(&k) {
                let mut o = o.clone();
                if k == id {
                    o.parent = None;
                }
                out.objects.insert(k, o);
            }
        }
        if out.objects.contains_key(&id) {
            out.roots.push(id);
        }
        out
    }

    /// Everything but the objects (materials, layers, settings).
    fn clone_settings(&self) -> Scene {
        Scene {
            next_object: self.next_object,
            next_material: self.next_material,
            objects: BTreeMap::new(),
            roots: Vec::new(),
            materials: self.materials.clone(),
            layers: self.layers.clone(),
            next_layer: self.next_layer,
            workplane: self.workplane.clone(),
            takes: Default::default(),
            render: self.render.clone(),
            render_camera: None,
        }
    }

    pub fn subtree(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if let Some(o) = self.objects.get(&cur) {
                out.push(cur);
                stack.extend(o.children.iter().rev().copied());
            }
        }
        out
    }

    /// The given ids with duplicates, unknown ids and anything that already has
    /// a selected ancestor removed; original order is kept.
    pub fn top_level(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out: Vec<ObjectId> = Vec::new();
        for &id in ids {
            if !self.contains(id) || out.contains(&id) {
                continue;
            }
            if ids.iter().any(|&other| other != id && self.is_descendant(id, other)) {
                continue;
            }
            out.push(id);
        }
        out
    }

    // --- creation ---------------------------------------------------------

    fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.objects.values().any(|o| o.name == n);
        if !taken(base) {
            return base.to_owned();
        }
        (1..)
            .map(|i| format!("{base}.{i}"))
            .find(|n| !taken(n))
            .expect("unbounded range")
    }

    pub fn add_object(&mut self, name: &str, kind: ObjectKind, parent: Option<ObjectId>) -> ObjectId {
        let parent = parent.filter(|p| self.contains(*p));
        let id = self.next_object;
        self.next_object += 1;
        let name = self.unique_name(name);
        self.objects.insert(
            id,
            Object {
                id,
                name,
                kind,
                pos: Vec3::ZERO,
                rot: Vec3::ZERO,
                scale: Vec3::ONE,
                parent,
                children: Vec::new(),
                vis_editor: Visibility::Default,
                vis_render: Visibility::Default,
                material: None,
                enabled: true,
                layer: None,
            },
        );
        self.sibling_list_mut(parent).push(id);
        id
    }

    pub fn add_primitive(&mut self, kind: PrimitiveKind, parent: Option<ObjectId>) -> ObjectId {
        self.add_object(kind.name(), ObjectKind::Primitive(kind.default_primitive()), parent)
    }

    pub fn add_generator(&mut self, kind: GeneratorKind, parent: Option<ObjectId>) -> ObjectId {
        self.add_object(kind.name(), ObjectKind::Generator(kind.default_generator()), parent)
    }

    pub fn add_deformer(&mut self, kind: DeformerKind, parent: Option<ObjectId>) -> ObjectId {
        self.add_object(kind.name(), ObjectKind::Deformer(kind.default_deformer()), parent)
    }

    /// True when the object has at least one deformer directly under it.
    pub fn has_deformer(&self, id: ObjectId) -> bool {
        self.children_of(Some(id))
            .iter()
            .any(|&c| self.get(c).is_some_and(|o| o.enabled && matches!(o.kind, ObjectKind::Deformer(_))))
    }

    /// Generators and deformers carry an on/off switch; other objects don't.
    pub fn can_disable(&self, id: ObjectId) -> bool {
        matches!(self.get(id).map(|o| &o.kind), Some(ObjectKind::Generator(_) | ObjectKind::Deformer(_)))
    }

    /// Whether this object turns its whole subtree into one piece of geometry:
    /// generators always, and Nulls that carry a deformer.
    pub fn consumes_children(&self, id: ObjectId) -> bool {
        match self.get(id).map(|o| &o.kind) {
            Some(ObjectKind::Generator(_)) => self.get(id).is_some_and(|o| o.enabled),
            Some(ObjectKind::Null) => self.has_deformer(id),
            _ => false,
        }
    }

    /// The nearest ancestor whose output includes `id`'s geometry, if any.
    pub fn consumer_ancestor(&self, id: ObjectId) -> Option<ObjectId> {
        let mut cur = self.get(id)?.parent;
        while let Some(p) = cur {
            if self.consumes_children(p) {
                return Some(p);
            }
            cur = self.get(p)?.parent;
        }
        None
    }

    pub fn add_null(&mut self, parent: Option<ObjectId>) -> ObjectId {
        self.add_object("Null", ObjectKind::Null, parent)
    }

    pub fn add_material(&mut self, name: &str, color: [f32; 3]) -> MaterialId {
        let id = self.next_material;
        self.next_material += 1;
        self.materials.push(Material::new(id, name, color));
        id
    }

    pub fn remove_material(&mut self, id: MaterialId) {
        self.materials.retain(|m| m.id != id);
        for o in self.objects.values_mut() {
            if o.material == Some(id) {
                o.material = None;
            }
        }
    }

    // --- transforms -------------------------------------------------------

    pub fn world_matrix(&self, id: ObjectId) -> Mat4 {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(o) = self.objects.get(&c) else { break };
            chain.push(o.local_matrix());
            cur = o.parent;
            if chain.len() > MAX_DEPTH {
                break;
            }
        }
        chain.iter().rev().fold(Mat4::IDENTITY, |acc, m| acc * *m)
    }

    /// World matrix of the parent, identity for top-level objects.
    pub fn parent_world_matrix(&self, id: ObjectId) -> Mat4 {
        match self.objects.get(&id).and_then(|o| o.parent) {
            Some(p) => self.world_matrix(p),
            None => Mat4::IDENTITY,
        }
    }

    pub fn world_position(&self, id: ObjectId) -> Vec3 {
        self.world_matrix(id).w_axis.truncate()
    }

    pub fn set_position(&mut self, id: ObjectId, local: Vec3) {
        if let Some(o) = self.objects.get_mut(&id) {
            o.pos = local;
        }
    }

    /// Moves the object so its origin lands on `world`, whatever its ancestors do.
    pub fn set_world_position(&mut self, id: ObjectId, world: Vec3) {
        let local = self.parent_world_matrix(id).inverse().transform_point3(world);
        if local.is_finite()
            && let Some(o) = self.objects.get_mut(&id) {
                o.pos = local;
            }
    }

    /// The object's orientation in world space as HPB degrees.
    pub fn world_rotation_hpb(&self, id: ObjectId) -> Vec3 {
        let reference = self.objects.get(&id).map(|o| o.rot).unwrap_or(Vec3::ZERO);
        quat_to_hpb_near(matrix_rotation(self.world_matrix(id)), reference)
    }

    /// Orients the object so its world rotation becomes `hpb` (degrees).
    pub fn set_world_rotation_hpb(&mut self, id: ObjectId, hpb: Vec3) {
        let parent_rot = matrix_rotation(self.parent_world_matrix(id));
        let local = (parent_rot.inverse() * hpb_to_quat(hpb)).normalize();
        if let Some(o) = self.objects.get_mut(&id) {
            o.rot = quat_to_hpb_near(local, o.rot);
        }
    }

    /// Sets the object's local TRS so that its world matrix becomes `world`.
    pub fn set_world_matrix(&mut self, id: ObjectId, world: Mat4) {
        let local = self.parent_world_matrix(id).inverse() * world;
        let (scale, rot, pos) = local.to_scale_rotation_translation();
        if let Some(o) = self.objects.get_mut(&id) {
            o.pos = pos;
            o.scale = scale;
            o.rot = quat_to_hpb_near(rot, o.rot);
        }
    }

    /// Effective editor visibility, resolving "Default" through the parents.
    pub fn editor_visible(&self, id: ObjectId) -> bool {
        self.layer_shows(id) && self.vis_chain(id)
    }

    fn vis_chain(&self, id: ObjectId) -> bool {
        let mut cur = Some(id);
        for _ in 0..MAX_DEPTH {
            let Some(o) = cur.and_then(|c| self.objects.get(&c)) else { return true };
            match o.vis_editor {
                Visibility::On => return true,
                Visibility::Off => return false,
                Visibility::Default => cur = o.parent,
            }
        }
        true
    }

    /// Depth-first visit of every object with its world matrix.
    pub fn walk(&self, mut f: impl FnMut(&Object, Mat4)) {
        fn rec(scene: &Scene, id: ObjectId, parent_world: Mat4, depth: usize, f: &mut dyn FnMut(&Object, Mat4)) {
            if depth > MAX_DEPTH {
                return;
            }
            let Some(o) = scene.objects.get(&id) else { return };
            let world = parent_world * o.local_matrix();
            f(o, world);
            for &c in &o.children {
                rec(scene, c, world, depth + 1, f);
            }
        }
        for &r in &self.roots {
            rec(self, r, Mat4::IDENTITY, 0, &mut f);
        }
    }

    // --- hierarchy editing ------------------------------------------------

    fn detach(&mut self, id: ObjectId) {
        let parent = self.objects.get(&id).and_then(|o| o.parent);
        self.sibling_list_mut(parent).retain(|&c| c != id);
    }

    /// Moves `id` to `target`, keeping its world transform. Returns false (and
    /// changes nothing) if the move is invalid, e.g. into its own subtree.
    pub fn move_object(&mut self, id: ObjectId, target: DropTarget) -> bool {
        if !self.contains(id) {
            return false;
        }
        let (new_parent, anchor) = match target {
            DropTarget::Into(p) => (Some(p), None),
            DropTarget::Before(s) | DropTarget::After(s) => match self.objects.get(&s) {
                Some(sib) => (sib.parent, Some(s)),
                None => return false,
            },
            DropTarget::Root => (None, None),
        };
        if let Some(p) = new_parent
            && (!self.contains(p) || p == id || self.is_descendant(p, id)) {
                return false;
            }
        if anchor == Some(id) {
            return false;
        }

        let world = self.world_matrix(id);
        self.detach(id);
        let list = self.sibling_list_mut(new_parent);
        let index = match target {
            DropTarget::Before(s) => list.iter().position(|&c| c == s).unwrap_or(list.len()),
            DropTarget::After(s) => list.iter().position(|&c| c == s).map_or(list.len(), |i| i + 1),
            _ => list.len(),
        };
        list.insert(index, id);
        if let Some(o) = self.objects.get_mut(&id) {
            o.parent = new_parent;
        }
        self.set_world_matrix(id, world);
        true
    }

    /// Deletes the given objects and everything beneath them.
    pub fn remove(&mut self, ids: &[ObjectId]) {
        for top in self.top_level(ids) {
            self.detach(top);
            for id in self.subtree(top) {
                self.objects.remove(&id);
            }
        }
    }

    fn clone_subtree(&mut self, id: ObjectId, parent_field: Option<ObjectId>) -> Option<ObjectId> {
        let original = self.objects.get(&id)?.clone();
        let new_id = self.next_object;
        self.next_object += 1;
        let mut copy = original.clone();
        copy.id = new_id;
        copy.parent = parent_field;
        copy.children = Vec::new();
        self.objects.insert(new_id, copy);
        for child in original.children {
            if let Some(c) = self.clone_subtree(child, Some(new_id)) {
                self.objects.get_mut(&new_id).unwrap().children.push(c);
            }
        }
        Some(new_id)
    }

    /// Copies `id`'s subtree from `src` into this scene under `parent`,
    /// mapping material ids through `materials` (adding materials as needed).
    fn import_subtree(
        &mut self,
        src: &Scene,
        id: ObjectId,
        parent: Option<ObjectId>,
        maps: &mut ImportMaps,
    ) -> Option<ObjectId> {
        let original = src.objects.get(&id)?;
        let new_id = self.next_object;
        self.next_object += 1;
        let mut copy = original.clone();
        copy.id = new_id;
        copy.parent = parent;
        copy.children = Vec::new();
        let materials = &mut maps.materials;
        copy.material = original.material.and_then(|m| {
            if let Some(&mapped) = materials.get(&m) {
                return Some(mapped);
            }
            let mat = src.material(m)?;
            // Reuse an identical material instead of adding a second copy.
            let mapped = match self.materials.iter().find(|x| x.name == mat.name && x.color == mat.color) {
                Some(existing) => existing.id,
                None => self.add_material(&mat.name, mat.color),
            };
            materials.insert(m, mapped);
            Some(mapped)
        });
        // Layers are matched by name, as materials are.
        copy.layer = original.layer.and_then(|l| {
            if let Some(&mapped) = maps.layers.get(&l) {
                return Some(mapped);
            }
            let layer = src.layer(l)?;
            let mapped = match self.layers.iter().find(|x| x.name == layer.name) {
                Some(existing) => existing.id,
                None => {
                    let id = self.add_layer(&layer.name);
                    let mut copy = layer.clone();
                    copy.id = id;
                    *self.layer_mut(id).expect("just added") = copy;
                    id
                }
            };
            maps.layers.insert(l, mapped);
            Some(mapped)
        });
        self.objects.insert(new_id, copy);
        for &child in &original.children {
            if let Some(c) = self.import_subtree(src, child, Some(new_id), maps) {
                self.objects.get_mut(&new_id).unwrap().children.push(c);
            }
        }
        Some(new_id)
    }

    /// A standalone scene holding copies of the given objects (whole
    /// subtrees) and the materials they use. Each copied top object keeps its
    /// place in the world even if its parent isn't copied.
    pub fn extract(&self, ids: &[ObjectId]) -> Scene {
        let mut out = Scene::new();
        let mut maps = ImportMaps::default();
        for id in self.top_level(ids) {
            if let Some(new) = out.import_subtree(self, id, None, &mut maps) {
                out.roots.push(new);
                out.set_world_matrix(new, self.world_matrix(id));
            }
        }
        out
    }

    /// Adds everything in `other` to this scene with new ids, after the
    /// existing top-level objects. Materials identical to one already here
    /// (same name and colour) are shared. Returns the new top-level objects.
    pub fn merge(&mut self, other: &Scene) -> Vec<ObjectId> {
        let mut maps = ImportMaps::default();
        let mut added = Vec::new();
        for &root in &other.roots {
            if let Some(new) = self.import_subtree(other, root, None, &mut maps) {
                self.roots.push(new);
                added.push(new);
            }
        }
        // Materials nobody uses still come along (as with Merge Project in Cinema 4D).
        for m in &other.materials {
            if !maps.materials.contains_key(&m.id) && !self.materials.iter().any(|x| x.name == m.name && x.color == m.color) {
                self.add_material(&m.name, m.color);
            }
        }
        added
    }

    /// Duplicates whole subtrees, placing each copy right after its original.
    pub fn duplicate(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out = Vec::new();
        for id in self.top_level(ids) {
            let parent = self.objects.get(&id).and_then(|o| o.parent);
            if let Some(new) = self.clone_subtree(id, parent) {
                self.move_object(new, DropTarget::After(id));
                out.push(new);
            }
        }
        out
    }

    /// Wraps the given objects in a new Null placed at their centre. The Null
    /// takes the place of the first object in the hierarchy.
    pub fn group(&mut self, ids: &[ObjectId]) -> Option<ObjectId> {
        let tops = self.top_level(ids);
        let first = *tops.first()?;
        let parent = self.objects.get(&first).and_then(|o| o.parent);
        let centre = tops.iter().map(|&i| self.world_position(i)).sum::<Vec3>() / tops.len() as f32;

        let null = self.add_null(parent);
        self.move_object(null, DropTarget::Before(first));
        self.set_world_matrix(null, Mat4::from_translation(centre));
        for t in tops {
            self.move_object(t, DropTarget::Into(null));
        }
        Some(null)
    }

    // --- persistence ------------------------------------------------------

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        self.to_json_with(None)
    }

    /// The scene file, with the view panels saved alongside.
    pub fn to_json_with(&self, views: Option<&crate::views::SavedViews>) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&SceneFile {
            format: FORMAT_NAME.to_owned(),
            version: FORMAT_VERSION,
            scene: self.clone(),
            views: views.and_then(|v| serde_json::to_value(v).ok()),
        })
    }

    pub fn from_json(text: &str) -> Result<Scene, String> {
        Ok(Self::from_json_with(text)?.0)
    }

    /// The scene and, if the file has them, its saved view panels.
    pub fn from_json_with(text: &str) -> Result<(Scene, Option<crate::views::SavedViews>), String> {
        let file: SceneFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if file.format != FORMAT_NAME {
            return Err(format!("not a Forma3D scene (format \"{}\")", file.format));
        }
        if file.version > FORMAT_VERSION {
            return Err(format!(
                "scene was saved by a newer version (file v{}, this build reads up to v{})",
                file.version, FORMAT_VERSION
            ));
        }
        let views = file
            .views
            .and_then(|v| serde_json::from_value::<crate::views::SavedViews>(v).ok())
            .map(crate::views::SavedViews::sanitized);
        Ok((file.scene.validated()?, views))
    }

    /// Rejects files whose hierarchy is inconsistent, rather than crashing later.
    fn validated(self) -> Result<Scene, String> {
        let mut seen = std::collections::HashSet::new();
        let mut stack: Vec<(ObjectId, Option<ObjectId>, usize)> =
            self.roots.iter().map(|&r| (r, None, 0)).collect();
        while let Some((id, expected_parent, depth)) = stack.pop() {
            let o = self.objects.get(&id).ok_or_else(|| format!("missing object {id}"))?;
            if o.parent != expected_parent {
                return Err(format!("object {id} has an inconsistent parent"));
            }
            if depth > MAX_DEPTH || !seen.insert(id) {
                return Err(format!("object {id} is part of a cycle or nested too deeply"));
            }
            if let ObjectKind::Polygon(m) = &o.kind {
                m.validate().map_err(|e| format!("object {id}: {e}"))?;
            }
            stack.extend(o.children.iter().map(|&c| (c, Some(id), depth + 1)));
        }
        if seen.len() != self.objects.len() {
            return Err("scene contains objects that are not part of the hierarchy".to_owned());
        }
        if let Some(max) = self.objects.keys().max()
            && self.next_object <= *max {
                return Err("object id counter is behind existing objects".to_owned());
            }
        let mut s = self;
        s.takes.sanitize();
        if let Some(max) = s.layers.iter().map(|l| l.id).max()
            && s.next_layer <= max {
                s.next_layer = max + 1;
            }
        Ok(s)
    }
}

#[derive(Default)]
struct ImportMaps {
    materials: std::collections::HashMap<MaterialId, MaterialId>,
    layers: std::collections::HashMap<LayerId, LayerId>,
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

impl Scene {
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Adds a layer with the next colour of the palette.
    pub fn add_layer(&mut self, name: &str) -> LayerId {
        let id = self.next_layer;
        self.next_layer += 1;
        let taken = |n: &str, layers: &[Layer]| layers.iter().any(|l| l.name == n);
        let name = if taken(name, &self.layers) {
            (1..).map(|i| format!("{name} {i}")).find(|n| !taken(n, &self.layers)).expect("unbounded")
        } else {
            name.to_owned()
        };
        let color = crate::layers::PALETTE[self.layers.len() % crate::layers::PALETTE.len()];
        self.layers.push(Layer { id, name, color, ..Layer::default() });
        id
    }

    /// Deletes a layer; its objects keep existing, on no layer.
    pub fn remove_layer(&mut self, id: LayerId) {
        self.layers.retain(|l| l.id != id);
        for o in self.objects.values_mut() {
            if o.layer == Some(id) {
                o.layer = None;
            }
        }
    }

    /// Moves a layer up or down the list.
    pub fn move_layer(&mut self, id: LayerId, up: bool) {
        let Some(i) = self.layers.iter().position(|l| l.id == id) else { return };
        let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < self.layers.len()) };
        if let Some(j) = j {
            self.layers.swap(i, j);
        }
    }

    pub fn object_layer(&self, id: ObjectId) -> Option<&Layer> {
        self.get(id)?.layer.and_then(|l| self.layer(l))
    }

    fn layer_flag(&self, id: ObjectId, flag: LayerFlag) -> bool {
        self.object_layer(id).is_none_or(|l| flag.get(l))
    }

    /// Whether the object's layer lets it show in the viewport (View, Solo).
    pub fn layer_shows(&self, id: ObjectId) -> bool {
        if self.layers.iter().any(|l| l.solo) && !self.object_layer(id).is_some_and(|l| l.solo) {
            return false;
        }
        self.layer_flag(id, LayerFlag::View)
    }

    /// Whether the object shows in renders: its (and its parents') render
    /// visibility, and its layer's Render switch.
    pub fn render_visible(&self, id: ObjectId) -> bool {
        if !self.layer_flag(id, LayerFlag::Render) {
            return false;
        }
        let mut cur = Some(id);
        for _ in 0..MAX_DEPTH {
            let Some(o) = cur.and_then(|c| self.objects.get(&c)) else { return true };
            match o.vis_render {
                Visibility::On => return true,
                Visibility::Off => return false,
                Visibility::Default => cur = o.parent,
            }
        }
        true
    }

    /// The world matrix as the object acts: a light or camera with a Look At
    /// object turns to face it (a camera rig's own pose comes from
    /// `scene_camera::pose`).
    pub fn effective_world(&self, id: ObjectId) -> Mat4 {
        let world = self.world_matrix(id);
        let target = match self.objects.get(&id).map(|o| &o.kind) {
            Some(ObjectKind::Light(l)) => l.look_at,
            Some(ObjectKind::Camera(c)) => c.look_at,
            _ => None,
        };
        match target.filter(|t| *t != id && self.objects.contains_key(t)) {
            Some(t) => crate::scene_camera::aimed(world, self.world_position(t)),
            None => world,
        }
    }

    /// The cameras in the scene, in hierarchy order.
    pub fn cameras(&self) -> Vec<(ObjectId, String)> {
        let mut out = Vec::new();
        self.walk(|o, _| {
            if matches!(o.kind, ObjectKind::Camera(_)) {
                out.push((o.id, o.name.clone()));
            }
        });
        out
    }

    /// Objects on a locked layer can't be picked or changed in the viewport.
    pub fn is_locked(&self, id: ObjectId) -> bool {
        self.object_layer(id).is_some_and(|l| l.locked)
    }

    /// Whether the object is listed in the Object Manager.
    pub fn in_manager(&self, id: ObjectId) -> bool {
        self.layer_flag(id, LayerFlag::Manager)
    }

    /// Whether the object goes into the final render.
    pub fn renders(&self, id: ObjectId) -> bool {
        self.layer_flag(id, LayerFlag::Render)
    }

    /// Puts objects (with everything under them) on a layer, or on none.
    pub fn set_layer(&mut self, ids: &[ObjectId], layer: Option<LayerId>) {
        let layer = layer.filter(|l| self.layer(*l).is_some());
        for top in self.top_level(ids) {
            for id in self.subtree(top) {
                if let Some(o) = self.objects.get_mut(&id) {
                    o.layer = layer;
                }
            }
        }
    }

    pub fn layer_members(&self, layer: LayerId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        self.walk(|o, _| {
            if o.layer == Some(layer) {
                out.push(o.id);
            }
        });
        out
    }
}

// ---------------------------------------------------------------------------
// Takes
// ---------------------------------------------------------------------------

impl Scene {
    pub fn takes(&self) -> &TakeSystem {
        &self.takes
    }

    pub fn active_take(&self) -> usize {
        self.takes.active
    }

    /// Number of overrides a take has (the active one counted live).
    pub fn take_override_count(&self, take: usize) -> usize {
        if take == self.takes.active {
            self.takes.live_overrides(self.objects.values()).len()
        } else {
            self.takes.takes.get(take).map_or(0, |t| t.overrides.len())
        }
    }

    /// The overrides of a take, object by object (the active one live).
    pub fn take_overrides(&self, take: usize) -> Vec<(ObjectId, crate::takes::Override)> {
        if take == self.takes.active {
            self.takes.live_overrides(self.objects.values()).into_iter().collect()
        } else {
            self.takes.takes.get(take).map(|t| t.overrides.iter().map(|(k, v)| (*k, *v)).collect()).unwrap_or_default()
        }
    }

    /// Adds a take and makes it active. It starts with no overrides.
    pub fn add_take(&mut self, name: &str) -> usize {
        let taken = |n: &str, t: &TakeSystem| t.takes.iter().any(|x| x.name == n);
        let name = if taken(name, &self.takes) {
            (1..).map(|i| format!("{name}.{i}")).find(|n| !taken(n, &self.takes)).expect("unbounded")
        } else {
            name.to_owned()
        };
        self.takes.takes.push(Take { name, overrides: BTreeMap::new() });
        let i = self.takes.takes.len() - 1;
        self.set_active_take(i);
        i
    }

    /// A copy of a take (with its overrides), made active.
    pub fn clone_take(&mut self, take: usize) -> Option<usize> {
        if take == 0 || take >= self.takes.takes.len() {
            return None;
        }
        let overrides: BTreeMap<_, _> = self.take_overrides(take).into_iter().collect();
        let name = self.takes.takes[take].name.clone();
        let new = self.add_take(&name);
        // add_take switched to the (empty) new take: put the copied overrides on.
        self.takes.takes[new].overrides = overrides.clone();
        for (id, ov) in overrides {
            if let Some(o) = self.objects.get_mut(&id) {
                ov.apply(o);
            }
        }
        Some(new)
    }

    pub fn rename_take(&mut self, take: usize, name: &str) {
        if take > 0
            && let Some(t) = self.takes.takes.get_mut(take)
            && !name.trim().is_empty()
        {
            t.name = name.trim().to_owned();
        }
    }

    /// Deletes a take (Main can't be deleted). Deleting the active one goes back to Main.
    pub fn remove_take(&mut self, take: usize) {
        if take == 0 || take >= self.takes.takes.len() {
            return;
        }
        if self.takes.active == take {
            self.set_active_take(0);
        }
        self.takes.takes.remove(take);
        if self.takes.active > take {
            self.takes.active -= 1;
        }
    }

    /// Switches takes: records the active take's overrides, restores Main,
    /// then applies the new take's overrides.
    pub fn set_active_take(&mut self, take: usize) {
        if take == self.takes.active || take >= self.takes.takes.len() {
            return;
        }
        let from = self.takes.active;
        if from != 0 {
            let live = self.takes.live_overrides(self.objects.values());
            self.takes.takes[from].overrides = live;
            let base = std::mem::take(&mut self.takes.base);
            for (id, st) in base {
                if let Some(o) = self.objects.get_mut(&id) {
                    st.apply(o);
                }
            }
        }
        self.takes.active = take;
        if take != 0 {
            self.takes.base = self.objects.values().map(|o| (o.id, ObjState::of(o))).collect();
            let overrides = self.takes.takes[take].overrides.clone();
            for (id, ov) in overrides {
                if let Some(o) = self.objects.get_mut(&id) {
                    ov.apply(o);
                }
            }
        }
    }

    /// Removes the given objects' overrides from the active take (they go
    /// back to their Main state).
    pub fn clear_take_overrides(&mut self, ids: &[ObjectId]) {
        if self.takes.active == 0 {
            return;
        }
        for &id in ids {
            if let (Some(st), Some(o)) = (self.takes.base.get(&id).copied(), self.objects.get_mut(&id)) {
                st.apply(o);
            }
        }
    }
}

const FORMAT_NAME: &str = "forma3d-scene";
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct SceneFile {
    format: String,
    version: u32,
    scene: Scene,
    /// The view panels as they were when saved (layouts, cameras, options).
    /// Read loosely: views that don't parse are dropped, not the scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    views: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {

    #[test]
    fn layers_hide_solo_lock_and_travel_with_copies() {
        let mut s = Scene::new();
        let a = s.add_primitive(PrimitiveKind::Cube, None);
        let b = s.add_primitive(PrimitiveKind::Sphere, None);
        let la = s.add_layer("Props");
        let lb = s.add_layer("Props");
        assert_eq!(s.layer(lb).unwrap().name, "Props 1", "names stay unique");
        s.set_layer(&[a], Some(la));
        s.set_layer(&[b], Some(lb));
        assert!(s.editor_visible(a) && s.editor_visible(b));
        s.layer_mut(la).unwrap().view = false;
        assert!(!s.editor_visible(a) && s.editor_visible(b));
        s.layer_mut(la).unwrap().view = true;
        s.layer_mut(lb).unwrap().solo = true;
        assert!(!s.editor_visible(a) && s.editor_visible(b), "solo shows only soloed layers");
        s.layer_mut(lb).unwrap().solo = false;
        s.layer_mut(la).unwrap().locked = true;
        s.layer_mut(la).unwrap().manager = false;
        assert!(s.is_locked(a) && !s.in_manager(a) && s.in_manager(b));
        assert_eq!(s.layer_members(la), vec![a]);
        // A group goes onto a layer with everything in it.
        let g = s.add_null(None);
        let kid = s.add_primitive(PrimitiveKind::Cone, Some(g));
        s.set_layer(&[g], Some(lb));
        assert_eq!(s.get(kid).unwrap().layer, Some(lb));
        // Copies keep their layer (by name) in another scene.
        let clip = s.extract(&[a]);
        let mut other = Scene::new();
        let added = other.merge(&clip);
        let l = other.object_layer(added[0]).unwrap();
        assert_eq!(l.name, "Props");
        assert!(l.locked);
        // Deleting a layer leaves its objects on no layer.
        s.remove_layer(la);
        assert!(s.get(a).unwrap().layer.is_none() && !s.is_locked(a));
        // Old files without layers still load.
        let json = s.to_json().unwrap();
        assert_eq!(Scene::from_json(&json).unwrap(), s);
    }

    #[test]
    fn takes_record_overrides_and_restore_main() {
        let mut s = Scene::new();
        let cube = s.add_primitive(PrimitiveKind::Cube, None);
        let red = s.add_material("Red", [1.0, 0.0, 0.0]);
        let t = s.add_take("Wide");
        assert_eq!(s.active_take(), t);
        // Changes while the take is active are its overrides.
        s.get_mut(cube).unwrap().pos = Vec3::new(100.0, 0.0, 0.0);
        s.get_mut(cube).unwrap().material = Some(red);
        assert_eq!(s.take_override_count(t), 1);
        assert_eq!(s.take_overrides(t)[0].1.fields(), vec!["Position", "Material"]);
        // Back on Main the cube is where it was; the take keeps its overrides.
        s.set_active_take(0);
        assert_eq!(s.get(cube).unwrap().pos, Vec3::ZERO);
        assert_eq!(s.get(cube).unwrap().material, None);
        assert_eq!(s.take_override_count(t), 1);
        // Changing Main shows through takes that don't override it.
        s.get_mut(cube).unwrap().scale = Vec3::splat(2.0);
        s.set_active_take(t);
        let o = s.get(cube).unwrap();
        assert_eq!((o.pos, o.scale, o.material), (Vec3::new(100.0, 0.0, 0.0), Vec3::splat(2.0), Some(red)));
        // Clones copy overrides; removing overrides brings back Main's state.
        let c = s.clone_take(t).unwrap();
        assert_eq!(s.active_take(), c);
        assert_eq!(s.get(cube).unwrap().pos, Vec3::new(100.0, 0.0, 0.0));
        s.clear_take_overrides(&[cube]);
        assert_eq!(s.get(cube).unwrap().pos, Vec3::ZERO);
        assert_eq!(s.take_override_count(c), 0);
        // Saved and loaded while a take is active, it is still consistent.
        s.set_active_take(t);
        let back = Scene::from_json(&s.to_json().unwrap()).unwrap();
        assert_eq!(back, s);
        // Deleting the active take goes back to Main.
        s.remove_take(t);
        assert_eq!(s.active_take(), 0);
        assert_eq!(s.get(cube).unwrap().pos, Vec3::ZERO);
        assert_eq!(s.takes().takes.len(), 2);
    }

    #[test]
    fn extract_and_merge_copy_subtrees_with_materials() {
        let mut a = Scene::new();
        let red = a.add_material("Red", [1.0, 0.0, 0.0]);
        let null = a.add_null(None);
        a.get_mut(null).unwrap().pos = Vec3::new(100.0, 0.0, 0.0);
        let cube = a.add_primitive(PrimitiveKind::Cube, Some(null));
        a.get_mut(cube).unwrap().pos = Vec3::new(0.0, 50.0, 0.0);
        a.get_mut(cube).unwrap().material = Some(red);
        let _child = a.add_primitive(PrimitiveKind::Sphere, Some(cube));

        // Copy only the cube: it comes out at its world position, with its child.
        let clip = a.extract(&[cube]);
        assert_eq!(clip.roots().len(), 1);
        let top = clip.roots()[0];
        assert!((clip.world_position(top) - Vec3::new(100.0, 50.0, 0.0)).length() < 1e-4);
        assert_eq!(clip.children_of(Some(top)).len(), 1);
        assert_eq!(clip.materials().len(), 1);

        // Pasting back into the same scene shares the identical material.
        let before = a.materials().len();
        let pasted = a.merge(&clip);
        assert_eq!(pasted.len(), 1);
        assert_eq!(a.materials().len(), before);
        assert_eq!(a.get(pasted[0]).unwrap().material, Some(red));
        assert!((a.world_position(pasted[0]) - Vec3::new(100.0, 50.0, 0.0)).length() < 1e-4);
        assert_ne!(pasted[0], cube, "new ids");

        // Into another scene: the material comes along under a new id.
        let mut b = Scene::new();
        b.add_material("Blue", [0.0, 0.0, 1.0]);
        let added = b.merge(&clip);
        let m = b.get(added[0]).unwrap().material.unwrap();
        assert_eq!(b.material(m).unwrap().name, "Red");
        assert_eq!(b.materials().len(), 2);
    }

    #[test]
    fn merging_a_whole_scene_keeps_it_and_adds_the_other() {
        let mut a = Scene::demo();
        let count = a.objects.len();
        let b = Scene::demo();
        let added = a.merge(&b);
        assert_eq!(added.len(), b.roots().len());
        assert_eq!(a.objects.len(), count * 2);
        assert_eq!(a.materials().len(), b.materials().len(), "identical materials are shared");
        // Round trip through JSON (the system clipboard) works too.
        let text = a.extract(&added).to_json().unwrap();
        assert_eq!(Scene::from_json(&text).unwrap().roots().len(), added.len());
    }

    use super::*;

    fn approx(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn hpb_composition_order_is_heading_pitch_bank() {
        let q = hpb_to_quat(Vec3::new(30.0, 20.0, 10.0));
        let expected = Quat::from_rotation_y(30f32.to_radians())
            * Quat::from_rotation_x(20f32.to_radians())
            * Quat::from_rotation_z(10f32.to_radians());
        assert!(q.dot(expected).abs() > 0.99999);
    }

    #[test]
    fn hpb_round_trips_and_stays_near_reference() {
        for hpb in [
            Vec3::new(10.0, 20.0, 30.0),
            Vec3::new(-170.0, 45.0, 120.0),
            Vec3::new(350.0, -80.0, -200.0),
            Vec3::new(179.0, 10.0, 0.0),
        ] {
            let q = hpb_to_quat(hpb);
            let back = quat_to_hpb_near(q, hpb);
            assert!(hpb_to_quat(back).dot(q).abs() > 0.99999, "{hpb:?} -> {back:?}");
            assert!(approx(back, hpb), "{hpb:?} -> {back:?} should stay on the same branch");
        }
    }

    #[test]
    fn hpb_does_not_jump_across_the_wrap() {
        // Rotating heading from 179 to 181 degrees must give 181, not -179.
        let q = hpb_to_quat(Vec3::new(181.0, 0.0, 0.0));
        let back = quat_to_hpb_near(q, Vec3::new(179.0, 0.0, 0.0));
        assert!((back.x - 181.0).abs() < 1e-2, "{back:?}");
    }

    #[test]
    fn world_setters_account_for_the_parent() {
        let mut s = Scene::new();
        let parent = s.add_null(None);
        s.get_mut(parent).unwrap().pos = Vec3::new(100.0, 0.0, 0.0);
        s.get_mut(parent).unwrap().rot = Vec3::new(90.0, 0.0, 0.0);
        s.get_mut(parent).unwrap().scale = Vec3::splat(2.0);
        let child = s.add_null(Some(parent));
        s.set_world_position(child, Vec3::new(100.0, 40.0, 60.0));
        assert!(approx(s.world_position(child), Vec3::new(100.0, 40.0, 60.0)));

        s.set_world_rotation_hpb(child, Vec3::new(30.0, 20.0, 10.0));
        let got = s.world_rotation_hpb(child);
        assert!(approx(got, Vec3::new(30.0, 20.0, 10.0)), "{got:?}");
        // Rotating must not have moved the origin.
        assert!(approx(s.world_position(child), Vec3::new(100.0, 40.0, 60.0)));
    }

    #[test]
    fn names_are_made_unique() {
        let mut s = Scene::new();
        let a = s.add_primitive(PrimitiveKind::Cube, None);
        let b = s.add_primitive(PrimitiveKind::Cube, None);
        let c = s.add_primitive(PrimitiveKind::Cube, None);
        assert_eq!(s.get(a).unwrap().name, "Cube");
        assert_eq!(s.get(b).unwrap().name, "Cube.1");
        assert_eq!(s.get(c).unwrap().name, "Cube.2");
    }

    #[test]
    fn world_matrix_composes_through_parents() {
        let mut s = Scene::new();
        let parent = s.add_null(None);
        let child = s.add_primitive(PrimitiveKind::Cube, Some(parent));
        s.get_mut(parent).unwrap().pos = Vec3::new(100.0, 0.0, 0.0);
        s.get_mut(parent).unwrap().rot = Vec3::new(90.0, 0.0, 0.0); // heading
        s.get_mut(child).unwrap().pos = Vec3::new(0.0, 0.0, 50.0);
        // Heading +90 about Y sends local +Z to world +X.
        assert!(approx(s.world_position(child), Vec3::new(150.0, 0.0, 0.0)));
    }

    #[test]
    fn move_into_preserves_world_transform() {
        let mut s = Scene::new();
        let target = s.add_null(None);
        s.get_mut(target).unwrap().pos = Vec3::new(500.0, 20.0, -30.0);
        s.get_mut(target).unwrap().rot = Vec3::new(40.0, 10.0, 5.0);
        s.get_mut(target).unwrap().scale = Vec3::splat(2.0);
        let obj = s.add_primitive(PrimitiveKind::Sphere, None);
        s.get_mut(obj).unwrap().pos = Vec3::new(10.0, 200.0, 30.0);
        s.get_mut(obj).unwrap().rot = Vec3::new(20.0, 0.0, 70.0);
        let before = s.world_matrix(obj);

        assert!(s.move_object(obj, DropTarget::Into(target)));
        assert_eq!(s.get(obj).unwrap().parent, Some(target));
        assert!(s.world_matrix(obj).abs_diff_eq(before, 1e-2));

        assert!(s.move_object(obj, DropTarget::Root));
        assert!(s.world_matrix(obj).abs_diff_eq(before, 1e-2));
        assert_eq!(s.roots(), &[target, obj]);
    }

    #[test]
    fn reparenting_into_a_sheared_space_keeps_position_exactly() {
        // A rotated parent with non-uniform scale cannot express the child's old
        // world matrix with position/rotation/scale alone (it would need shear),
        // but the world *position* must still be preserved exactly.
        let mut s = Scene::new();
        let parent = s.add_null(None);
        s.get_mut(parent).unwrap().pos = Vec3::new(10.0, -5.0, 40.0);
        s.get_mut(parent).unwrap().rot = Vec3::new(35.0, 10.0, 0.0);
        s.get_mut(parent).unwrap().scale = Vec3::new(2.0, 1.0, 0.5);
        let obj = s.add_primitive(PrimitiveKind::Cube, None);
        s.get_mut(obj).unwrap().pos = Vec3::new(50.0, 30.0, -20.0);
        s.get_mut(obj).unwrap().rot = Vec3::new(20.0, 15.0, 5.0);
        let before = s.world_position(obj);
        assert!(s.move_object(obj, DropTarget::Into(parent)));
        assert!(approx(s.world_position(obj), before), "{:?} vs {before:?}", s.world_position(obj));
        let local = s.get(obj).unwrap();
        assert!(local.pos.is_finite() && local.rot.is_finite() && local.scale.is_finite());
    }

    #[test]
    fn cannot_move_object_into_its_own_subtree() {
        let mut s = Scene::new();
        let a = s.add_null(None);
        let b = s.add_null(Some(a));
        let c = s.add_null(Some(b));
        assert!(!s.move_object(a, DropTarget::Into(c)));
        assert!(!s.move_object(a, DropTarget::Into(a)));
        assert!(!s.move_object(a, DropTarget::Before(c)));
        assert_eq!(s.get(a).unwrap().parent, None);
        assert_eq!(s.get(c).unwrap().parent, Some(b));
    }

    #[test]
    fn reordering_siblings() {
        let mut s = Scene::new();
        let a = s.add_null(None);
        let b = s.add_null(None);
        let c = s.add_null(None);
        assert!(s.move_object(c, DropTarget::Before(a)));
        assert_eq!(s.roots(), &[c, a, b]);
        assert!(s.move_object(c, DropTarget::After(b)));
        assert_eq!(s.roots(), &[a, b, c]);
        assert!(!s.move_object(a, DropTarget::After(a)));
    }

    #[test]
    fn remove_deletes_whole_subtrees() {
        let mut s = Scene::new();
        let a = s.add_null(None);
        let b = s.add_null(Some(a));
        let _c = s.add_null(Some(b));
        let keep = s.add_null(None);
        s.remove(&[a, b]); // b is already covered by a
        assert_eq!(s.len(), 1);
        assert_eq!(s.roots(), &[keep]);
    }

    #[test]
    fn duplicate_copies_subtree_next_to_original() {
        let mut s = Scene::new();
        let parent = s.add_null(None);
        let child = s.add_primitive(PrimitiveKind::Cone, Some(parent));
        s.get_mut(child).unwrap().pos = Vec3::new(1.0, 2.0, 3.0);
        let other = s.add_null(None);
        let copies = s.duplicate(&[parent]);
        assert_eq!(copies.len(), 1);
        assert_eq!(s.roots(), &[parent, copies[0], other]);
        let copy_children = s.children_of(Some(copies[0])).to_vec();
        assert_eq!(copy_children.len(), 1);
        assert_ne!(copy_children[0], child);
        assert_eq!(s.get(copy_children[0]).unwrap().pos, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(s.get(copy_children[0]).unwrap().parent, Some(copies[0]));
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn group_centres_null_and_keeps_world_positions() {
        let mut s = Scene::new();
        let a = s.add_primitive(PrimitiveKind::Cube, None);
        let b = s.add_primitive(PrimitiveKind::Cube, None);
        s.get_mut(a).unwrap().pos = Vec3::new(-100.0, 0.0, 0.0);
        s.get_mut(b).unwrap().pos = Vec3::new(300.0, 40.0, 0.0);
        let g = s.group(&[a, b]).unwrap();
        assert!(approx(s.world_position(g), Vec3::new(100.0, 20.0, 0.0)));
        assert!(approx(s.world_position(a), Vec3::new(-100.0, 0.0, 0.0)));
        assert!(approx(s.world_position(b), Vec3::new(300.0, 40.0, 0.0)));
        assert_eq!(s.roots(), &[g]);
        assert_eq!(s.children_of(Some(g)), &[a, b]);
    }

    #[test]
    fn visibility_inherits_from_parent() {
        let mut s = Scene::new();
        let p = s.add_null(None);
        let c = s.add_null(Some(p));
        assert!(s.editor_visible(c));
        s.get_mut(p).unwrap().vis_editor = Visibility::Off;
        assert!(!s.editor_visible(c));
        s.get_mut(c).unwrap().vis_editor = Visibility::On;
        assert!(s.editor_visible(c), "an explicit On overrides the parent");
    }

    #[test]
    fn top_level_drops_descendants_and_duplicates() {
        let mut s = Scene::new();
        let a = s.add_null(None);
        let b = s.add_null(Some(a));
        let c = s.add_null(None);
        assert_eq!(s.top_level(&[b, a, c, a, 999]), vec![a, c]);
    }

    #[test]
    fn json_round_trip() {
        let s = Scene::demo();
        let text = s.to_json().unwrap();
        let back = Scene::from_json(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn json_rejects_garbage_and_inconsistent_files() {
        assert!(Scene::from_json("not json").is_err());
        assert!(Scene::from_json(r#"{"format":"other","version":1,"scene":{}}"#).is_err());

        let mut s = Scene::new();
        let a = s.add_null(None);
        let b = s.add_null(Some(a));
        // Corrupt: make `a` a child of `b` as well, forming a cycle.
        s.objects.get_mut(&b).unwrap().children.push(a);
        let text = s.to_json().unwrap();
        assert!(Scene::from_json(&text).is_err());
    }

    #[test]
    fn json_rejects_newer_versions() {
        let s = Scene::new();
        let text = s.to_json().unwrap().replace("\"version\": 1", "\"version\": 99");
        let err = Scene::from_json(&text).unwrap_err();
        assert!(err.contains("newer"), "{err}");
    }

    #[test]
    fn removing_a_material_unassigns_it() {
        let mut s = Scene::new();
        let m = s.add_material("M", [1.0, 0.0, 0.0]);
        let o = s.add_null(None);
        s.get_mut(o).unwrap().material = Some(m);
        s.remove_material(m);
        assert_eq!(s.get(o).unwrap().material, None);
        assert!(s.materials().is_empty());
    }
}
