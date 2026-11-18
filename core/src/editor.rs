//! Editor state and the operations on it, independent of any UI toolkit.

use crate::camera::{Camera, ViewKind};
use crate::component::{CompSel, ComponentDrag, EditMode, mesh_of, mesh_of_mut};
use crate::deformer::{self, DeformerKind};
use crate::editmesh::EditMesh;
use crate::generator::{self, GeneratorKind};
use crate::gizmo::{AxisEnabled, CoordSystem, Drag, Tool};
use crate::history::History;
use crate::mesh::{self, Mesh};
use crate::picking::{PickItem, PickShape};
use crate::primitive::{Primitive, PrimitiveKey, PrimitiveKind};
use crate::renderer::{DisplayMode, ItemKind, SelectionState};
use crate::scene::{DropTarget, MaterialId, ObjectId, ObjectKind, Scene};
use glam::{Mat4, Vec3};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The one colour every shape starts with (no material assigned).
pub const DEFAULT_COLOR: [f32; 3] = [0.64, 0.64, 0.66];
pub const DEFORMER_COLOR: [f32; 3] = [0.62, 0.42, 0.98];
pub const DISABLED_COLOR: [f32; 3] = [0.45, 0.45, 0.48];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewLayout {
    Single,
    Quad,
}

/// What the viewports show (the viewport Options and Filter menus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewOptions {
    pub grid: bool,
    pub compass: bool,
    pub nulls: bool,
    pub deformers: bool,
    pub generators: bool,
    pub polygons: bool,
    pub primitives: bool,
    pub splines: bool,
    pub lights: bool,
    /// Cameras and skies.
    pub cameras: bool,
    /// Show only the selected objects.
    pub solo: bool,
    /// Render options: post effects, tessellation, shadows and so on.
    pub fx: crate::renderer::ViewEffects,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions { grid: true, compass: true, nulls: true, deformers: true, generators: true, polygons: true, primitives: true, splines: true, lights: true, cameras: true, solo: false, fx: Default::default() }
    }
}

/// Interactive modelling tools that replace the gizmo while active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeshTool {
    Extrude,
    Inset,
    LoopCut,
    Bevel,
    Knife,
}

impl MeshTool {
    pub fn label(self) -> &'static str {
        match self {
            MeshTool::Extrude => "Extrude",
            MeshTool::Inset => "Extrude Inner",
            MeshTool::LoopCut => "Loop Cut",
            MeshTool::Bevel => "Bevel",
            MeshTool::Knife => "Knife",
        }
    }
}

/// State of an Extrude / Extrude Inner drag: the mesh before the tool, so each
/// frame re-applies the operation with the current amount.
#[derive(Clone, Debug)]
pub struct ToolDrag {
    pub id: ObjectId,
    pub base: EditMesh,
    pub polys: BTreeSet<u32>,
    pub edges: BTreeSet<crate::editmesh::Edge>,
    pub start_x: f32,
    pub world_per_px: f32,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMode {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrTab {
    Basic,
    Coord,
    Object,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    New,
    Open,
    Quit,
    /// Close the current project.
    Close,
    /// Reload the current project from its file.
    Revert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    Open { path: String, error: Option<String> },
    SaveAs { path: String, error: Option<String> },
    /// Merge Project: add another scene file's objects to this one.
    Merge { path: String, error: Option<String> },
    ConfirmDiscard(Pending),
    Shortcuts,
    About,
}

pub struct Timeline {
    pub frame: i32,
    pub min: i32,
    pub max: i32,
    pub fps: f32,
    pub playing: bool,
    /// Playing backwards (the reverse play button).
    pub reverse: bool,
    pub carry: f32,
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline {
            frame: 0,
            min: 0,
            max: 90,
            fps: 30.0,
            playing: false,
            reverse: false,
            carry: 0.0,
        }
    }
}

impl Timeline {
    /// Advances playback by `dt` seconds, wrapping at either end.
    pub fn tick(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        self.carry += dt * self.fps;
        let steps = self.carry.floor() as i32;
        self.carry -= steps as f32;
        let span = (self.max - self.min + 1).max(1);
        let delta = if self.reverse { -steps } else { steps };
        self.frame = self.min + (self.frame - self.min + delta).rem_euclid(span);
    }

    pub fn set_range(&mut self, min: i32, max: i32) {
        self.min = min.min(max - 1);
        self.max = max.max(self.min + 1);
        self.frame = self.frame.clamp(self.min, self.max);
    }
}

/// Caches generated primitive meshes between frames.
pub struct MeshCache {
    map: HashMap<PrimitiveKey, (Arc<Mesh>, bool)>,
    generated: HashMap<u64, (Arc<Mesh>, bool)>,
    null: Arc<Mesh>,
    /// Builds slow meshes elsewhere (another thread); None builds them here.
    offload: Option<Box<dyn FnMut(MeshJob)>>,
    /// A clock in milliseconds, to learn which meshes are slow.
    pub clock: fn() -> f64,
    /// How long each object's mesh took to build here last time (ms).
    cost: HashMap<ObjectId, f64>,
    /// The newest mesh each object showed (kept while a new one is built).
    last: HashMap<ObjectId, Arc<Mesh>>,
    /// The content key being built elsewhere for each object.
    pending: HashMap<ObjectId, u64>,
}

/// A mesh to build elsewhere: the object's own part of the scene.
pub struct MeshJob {
    pub key: u64,
    pub object: ObjectId,
    pub scene: Scene,
}

/// Meshes slower than this (ms) are built elsewhere when that's possible.
pub const SLOW_MESH_MS: f64 = 20.0;

fn no_clock() -> f64 {
    0.0
}

impl Default for MeshCache {
    fn default() -> Self {
        MeshCache {
            map: HashMap::new(),
            generated: HashMap::new(),
            null: Arc::new(mesh::null_cross(28.0)),
            offload: None,
            clock: no_clock,
            cost: HashMap::new(),
            last: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

impl std::fmt::Debug for MeshCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshCache").field("meshes", &self.map.len()).field("generated", &self.generated.len()).field("pending", &self.pending.len()).finish()
    }
}

/// Kinds that are slow from the start (before their cost is measured).
fn heavy(scene: &Scene, id: ObjectId) -> bool {
    matches!(scene.get(id).map(|o| &o.kind), Some(ObjectKind::Generator(generator::Generator::Boolean { .. })))
}

/// Builds a job's mesh (on whatever thread runs it).
pub fn build_job_mesh(scene: &Scene, object: ObjectId) -> Option<Mesh> {
    let mut cache = MeshCache::default();
    let geo = generator::eval_geo(scene, object, &mut |p| cache.get(p));
    (!geo.is_empty()).then(|| geo.into_mesh())
}

impl MeshCache {
    pub fn get(&mut self, p: &Primitive) -> Arc<Mesh> {
        let entry = self
            .map
            .entry(p.key())
            .or_insert_with(|| (Arc::new(p.build()), true));
        entry.1 = true;
        entry.0.clone()
    }

    /// Mesh produced by a generator, cached by the hash of everything it depends on.
    /// `None` when the generator has no input geometry.
    pub fn generated(&mut self, scene: &Scene, id: ObjectId) -> Option<Arc<Mesh>> {
        let key = generator::content_hash(scene, id);
        if let Some(entry) = self.generated.get_mut(&key) {
            entry.1 = true;
            return Some(entry.0.clone());
        }
        let geo = generator::eval_geo(scene, id, &mut |p| self.get(p));
        if geo.is_empty() {
            return None;
        }
        let mesh = Arc::new(geo.into_mesh());
        self.generated.insert(key, (mesh.clone(), true));
        Some(mesh)
    }

    /// Hands slow meshes to `f` to build elsewhere; results come back
    /// through `finish`.
    pub fn set_offload(&mut self, f: Box<dyn FnMut(MeshJob)>, clock: fn() -> f64) {
        self.offload = Some(f);
        self.clock = clock;
    }

    pub fn offloading(&self) -> bool {
        self.offload.is_some()
    }

    /// Meshes being built elsewhere right now.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// For the viewport: like `generated`, but a slow mesh is built
    /// elsewhere while the object keeps showing its previous one.
    pub fn generated_view(&mut self, scene: &Scene, id: ObjectId) -> Option<Arc<Mesh>> {
        let key = generator::content_hash(scene, id);
        if let Some(entry) = self.generated.get_mut(&key) {
            entry.1 = true;
            let m = entry.0.clone();
            self.last.insert(id, m.clone());
            return Some(m);
        }
        let slow = self.cost.get(&id).is_some_and(|&c| c > SLOW_MESH_MS) || (!self.cost.contains_key(&id) && heavy(scene, id));
        if self.offload.is_some() && slow {
            // One job per object at a time: the newest state is sent when
            // the current one comes back.
            if !self.pending.contains_key(&id) {
                self.pending.insert(id, key);
                let job = MeshJob { key, object: id, scene: scene.subtree_scene(id) };
                if let Some(f) = self.offload.as_mut() {
                    f(job);
                }
            }
            return self.last.get(&id).cloned();
        }
        let t0 = (self.clock)();
        let m = self.generated(scene, id);
        let dt = (self.clock)() - t0;
        if dt > 0.0 {
            self.cost.insert(id, dt);
        }
        if let Some(m) = &m {
            self.last.insert(id, m.clone());
        }
        m
    }

    /// Whether this cache is waiting for that object's mesh in that state.
    pub fn waiting_for(&self, object: ObjectId, key: u64) -> bool {
        self.pending.get(&object) == Some(&key)
    }

    /// A mesh built elsewhere is back. Returns whether it is new to show.
    pub fn finish(&mut self, key: u64, object: ObjectId, mesh: Option<Mesh>, ms: f64) -> bool {
        if self.pending.get(&object) != Some(&key) {
            return false;
        }
        self.pending.remove(&object);
        self.cost.insert(object, ms.max(SLOW_MESH_MS + 1.0));
        if let Some(m) = mesh {
            let m = Arc::new(m);
            self.generated.insert(key, (m.clone(), true));
            self.last.insert(object, m);
        }
        true
    }

    /// The wire cage of a deformer, cached by its parameters.
    pub fn cage(&mut self, d: &deformer::Deformer) -> Arc<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("cage", format!("{d:?}")).hash(&mut h);
        let key = h.finish();
        let entry = self
            .generated
            .entry(key)
            .or_insert_with(|| (Arc::new(deformer::cage(d).into_mesh()), true));
        entry.1 = true;
        entry.0.clone()
    }

    /// The line drawing of a spline, cached by its points and settings.
    pub fn spline(&mut self, s: &crate::spline::Spline) -> Arc<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("spline", format!("{s:?}")).hash(&mut h);
        let entry = self
            .generated
            .entry(h.finish())
            .or_insert_with(|| (Arc::new(s.sanitized().line_mesh()), true));
        entry.1 = true;
        entry.0.clone()
    }

    /// A camera's viewport outline, cached by its settings.
    pub fn camera(&mut self, c: &crate::scene_camera::SceneCamera) -> Arc<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("camera", format!("{c:?}")).hash(&mut h);
        let entry = self.generated.entry(h.finish()).or_insert_with(|| (Arc::new(c.sanitized().gizmo()), true));
        entry.1 = true;
        entry.0.clone()
    }

    /// A sky's viewport dome, cached by its settings.
    pub fn sky(&mut self, s: &crate::sky::Sky) -> Arc<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("sky", format!("{s:?}")).hash(&mut h);
        let entry = self.generated.entry(h.finish()).or_insert_with(|| (Arc::new(s.sanitized().gizmo()), true));
        entry.1 = true;
        entry.0.clone()
    }

    /// A light's viewport outline, cached by its settings.
    pub fn light(&mut self, l: &crate::light::Light) -> Arc<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("light", format!("{l:?}")).hash(&mut h);
        let entry = self
            .generated
            .entry(h.finish())
            .or_insert_with(|| (Arc::new(l.sanitized().gizmo()), true));
        entry.1 = true;
        entry.0.clone()
    }

    pub fn null(&self) -> Arc<Mesh> {
        self.null.clone()
    }

    /// Drops meshes that were not requested since the previous call.
    pub fn end_frame(&mut self) {
        self.map.retain(|_, (_, used)| std::mem::replace(used, false));
        self.generated.retain(|_, (_, used)| std::mem::replace(used, false));
    }

    /// Forgets objects that are gone.
    pub fn forget_missing(&mut self, scene: &Scene) {
        self.last.retain(|id, _| scene.get(*id).is_some());
        self.cost.retain(|id, _| scene.get(*id).is_some());
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// One visible object, ready to draw and to pick.
pub struct RenderObject {
    pub id: ObjectId,
    pub world: Mat4,
    pub mesh: Arc<Mesh>,
    pub color: [f32; 3],
    /// The material it shows (its own, or the one it takes from its inputs).
    pub material: Option<crate::scene::MaterialId>,
    pub selection: SelectionState,
    pub kind: ItemKind,
    /// A spline: picked along its line rather than at its origin.
    pub curve: bool,
}

impl RenderObject {
    pub fn pick_item(&self) -> PickItem {
        PickItem {
            id: self.id,
            world: self.world,
            shape: match self.kind {
                ItemKind::Surface => PickShape::Mesh(self.mesh.clone()),
                ItemKind::LinesOnly if self.curve => PickShape::Lines(self.mesh.clone()),
                ItemKind::LinesOnly => PickShape::Point,
            },
        }
    }

    /// World-space bounding box of the object.
    pub fn world_bounds(&self) -> (Vec3, Vec3) {
        let (lo, hi) = (self.mesh.bounds_min, self.mesh.bounds_max);
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let w = self.world.transform_point3(corner);
            min = min.min(w);
            max = max.max(w);
        }
        (min, max)
    }
}

/// What an Axis-mode drag keeps where it was.
#[derive(Clone, Debug)]
pub struct AxisBase {
    pub id: ObjectId,
    pub start_world: Mat4,
    pub kind: ObjectKind,
    pub children: Vec<(ObjectId, Mat4)>,
}

pub struct Editor {
    pub scene: Scene,
    pub history: History,
    saved: Scene,
    pub file_path: Option<PathBuf>,

    /// Selected objects; the last one is the active object the gizmo attaches to.
    pub selection: Vec<ObjectId>,
    pub selected_material: Option<MaterialId>,

    pub tool: Tool,
    pub coord: CoordSystem,
    pub axes: AxisEnabled,
    pub display: DisplayMode,
    pub layout: ViewLayout,
    pub cameras: [Camera; 4],
    pub single_view: ViewKind,
    /// The view the pointer last interacted with; target of Frame commands.
    pub active_view: ViewKind,

    pub meshes: MeshCache,
    pub expanded: HashSet<ObjectId>,
    pub renaming: Option<ObjectId>,
    pub attr_tab: AttrTab,
    pub drag: Option<Drag>,
    pub nav: Option<NavMode>,

    pub view_opts: ViewOptions,
    /// The timeline strip under the viewport.
    pub show_timeline: bool,
    /// The customizable icon bar above the viewport.
    pub show_icon_bar: bool,
    /// Model, Points, Edges or Polygons.
    pub mode: EditMode,
    pub comp: CompSel,
    pub comp_drag: Option<ComponentDrag>,
    pub mesh_tool: Option<MeshTool>,
    pub tool_drag: Option<ToolDrag>,
    /// Screen position where a box selection started.
    pub box_start: Option<(ViewKind, [f32; 2])>,
    /// First click of the Knife tool.
    pub knife_start: Option<(ViewKind, [f32; 2])>,
    /// The Spline Pen or Polygon Pen, while one is in use.
    pub pen: Option<crate::pen::PenState>,
    /// Axis mode (Enable Axis): the tools move an object's axis, not its geometry.
    pub axis_mode: bool,
    /// Tweak mode: dragging on an object or component moves it directly.
    pub tweak: bool,
    pub snap: crate::snap::SnapSettings,
    pub quantize: crate::gizmo::Quantize,
    /// Where the current drag snapped (for the viewport marker).
    pub snap_hit: Option<crate::snap::SnapHit>,
    /// The geometry and children an Axis-mode drag keeps in place.
    pub axis_base: Option<AxisBase>,
    pub timeline: Timeline,
    /// The camera object the Perspective view looks through (Cameras >
    /// Look Through); navigating moves that camera.
    pub look_through: Option<ObjectId>,
    look_cache: Option<Camera>,
    pub dialog: Option<Dialog>,
    pub status: String,
    /// Label for the next recorded undo step.
    pub next_label: String,
    pub quit_requested: bool,
}

impl Editor {
    pub fn new(scene: Scene) -> Self {
        let history = History::new(&scene);
        let saved = scene.clone();
        let mut ed = Editor {
            scene,
            history,
            saved,
            file_path: None,
            selection: Vec::new(),
            selected_material: None,
            tool: Tool::Move,
            coord: CoordSystem::Object,
            axes: AxisEnabled::default(),
            display: DisplayMode::GouraudLines,
            layout: ViewLayout::Single,
            cameras: ViewKind::ALL.map(Camera::new),
            single_view: ViewKind::Perspective,
            active_view: ViewKind::Perspective,
            meshes: MeshCache::default(),
            expanded: HashSet::new(),
            renaming: None,
            attr_tab: AttrTab::Coord,
            drag: None,
            nav: None,
            view_opts: ViewOptions::default(),
            show_timeline: true,
            show_icon_bar: true,
            mode: EditMode::Model,
            comp: CompSel::default(),
            comp_drag: None,
            mesh_tool: None,
            tool_drag: None,
            box_start: None,
            knife_start: None,
            pen: None,
            axis_mode: false,
            tweak: false,
            snap: Default::default(),
            quantize: Default::default(),
            snap_hit: None,
            axis_base: None,
            timeline: Timeline::default(),
            look_through: None,
            look_cache: None,
            dialog: None,
            status: String::new(),
            next_label: "Edit".to_owned(),
            quit_requested: false,
        };
        ed.frame_all(ViewKind::Perspective);
        ed
    }

    // --- selection --------------------------------------------------------

    pub fn active(&self) -> Option<ObjectId> {
        self.selection.iter().rev().copied().find(|&id| self.scene.contains(id))
    }

    pub fn is_selected(&self, id: ObjectId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select_only(&mut self, id: ObjectId) {
        self.selection.clear();
        self.selection.push(id);
        self.selected_material = None;
    }

    /// Adds `id` (making it active) or, if already selected, removes it.
    pub fn toggle_selection(&mut self, id: ObjectId) {
        if let Some(i) = self.selection.iter().position(|&s| s == id) {
            self.selection.remove(i);
        } else {
            self.selection.push(id);
            self.selected_material = None;
        }
    }

    pub fn add_to_selection(&mut self, id: ObjectId) {
        self.selection.retain(|&s| s != id);
        self.selection.push(id);
        self.selected_material = None;
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn select_all(&mut self) {
        self.selection.clear();
        let mut all = Vec::new();
        self.scene.walk(|o, _| all.push(o.id));
        self.selection = all;
        self.selected_material = None;
    }

    /// Drops ids that no longer exist (after delete / undo / open).
    pub fn prune_selection(&mut self) {
        let scene = &self.scene;
        self.selection.retain(|&id| scene.contains(id));
        self.expanded.retain(|&id| scene.contains(id));
        if self.renaming.is_some_and(|id| !scene.contains(id)) {
            self.renaming = None;
        }
        if let Some(m) = self.selected_material
            && self.scene.material(m).is_none() {
                self.selected_material = None;
            }
        self.sync_components();
    }

    /// Inverts the selection: objects in Model mode, components otherwise.
    pub fn invert_selection(&mut self) {
        if self.mode.is_component() {
            let Some(mesh) = self.edit_target().and_then(|id| mesh_of(&self.scene, id)) else { return };
            match self.mode {
                EditMode::Points => {
                    self.comp.points = (0..mesh.points.len() as u32).filter(|p| !self.comp.points.contains(p)).collect()
                }
                EditMode::Edges => {
                    self.comp.edges = mesh.edges().into_iter().filter(|e| !self.comp.edges.contains(e)).collect()
                }
                EditMode::Polygons => {
                    self.comp.polys = (0..mesh.polys.len() as u32).filter(|p| !self.comp.polys.contains(p)).collect()
                }
                EditMode::Model | EditMode::Object | EditMode::Workplane => {}
            }
            return;
        }
        let mut all = Vec::new();
        self.scene.walk(|o, _| all.push(o.id));
        let new: Vec<ObjectId> = all.into_iter().filter(|id| !self.selection.contains(id)).collect();
        self.selection = new;
    }

    /// Edges mode: grows the selection to the full loops through the selected edges.
    pub fn loop_select(&mut self) {
        if self.mode != EditMode::Edges {
            self.status = "Loop Selection works in Edges mode (double-click an edge also selects its loop)".to_owned();
            return;
        }
        let Some(mesh) = self.edit_target().and_then(|id| mesh_of(&self.scene, id)) else { return };
        let mut all = self.comp.edges.clone();
        for &e in &self.comp.edges {
            all.extend(mesh.edge_loop(e));
        }
        self.comp.edges = all;
    }

    /// Switches the selected generators and deformers on or off.
    pub fn toggle_enabled(&mut self) {
        let ids: Vec<ObjectId> = self.selection.iter().copied().filter(|&s| self.scene.can_disable(s)).collect();
        for id in &ids {
            if let Some(o) = self.scene.get_mut(*id) {
                o.enabled = !o.enabled;
            }
        }
        if !ids.is_empty() {
            self.label("Enable / Disable");
        }
    }

    /// Reset PSR: zero position and rotation, unit scale, for the selected objects.
    pub fn reset_transform(&mut self) {
        for id in self.selection.clone() {
            if let Some(o) = self.scene.get_mut(id) {
                o.pos = Vec3::ZERO;
                o.rot = Vec3::ZERO;
                o.scale = Vec3::ONE;
            }
        }
        if !self.selection.is_empty() {
            self.label("Reset PSR");
        }
    }

    /// Adds every descendant of the selected objects to the selection.
    pub fn select_children(&mut self) {
        let mut add = Vec::new();
        for &id in &self.selection {
            add.extend(self.scene.subtree(id).into_iter().filter(|&d| d != id));
        }
        for id in add {
            if !self.selection.contains(&id) {
                self.selection.insert(0, id);
            }
        }
    }

    /// Objects the viewport filter lets through.
    pub fn passes_filter(&self, id: ObjectId) -> bool {
        let f = self.view_opts;
        let Some(o) = self.scene.get(id) else { return false };
        let kind_ok = match o.kind {
            ObjectKind::Null => f.nulls,
            ObjectKind::Deformer(_) => f.deformers,
            ObjectKind::Generator(_) => f.generators,
            ObjectKind::Polygon(_) => f.polygons,
            ObjectKind::Primitive(_) => f.primitives,
            ObjectKind::Spline(_) => f.splines,
            ObjectKind::Light(_) => f.lights,
            ObjectKind::Camera(_) | ObjectKind::Sky(_) => f.cameras,
        };
        kind_ok && (!f.solo || self.selection.iter().any(|&s| s == id || self.scene.is_descendant(id, s)))
    }

    /// Size of an object's own geometry (including its deformers) times its scale.
    pub fn object_size(&mut self, id: ObjectId) -> Option<Vec3> {
        let (lo, hi) = self.local_bounds(id)?;
        let s = self.scene.get(id)?.scale;
        Some((hi - lo) * s.abs())
    }

    fn local_bounds(&mut self, id: ObjectId) -> Option<(Vec3, Vec3)> {
        let meshes = &mut self.meshes;
        let geo = generator::eval_geo(&self.scene, id, &mut |p| meshes.get(p));
        if geo.verts.is_empty() {
            return None;
        }
        Some(geo.verts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos)))
        }))
    }

    /// Sets the object's scale so its geometry measures `size` (axes with no
    /// extent are left alone).
    pub fn set_object_size(&mut self, id: ObjectId, size: Vec3) {
        let Some((lo, hi)) = self.local_bounds(id) else { return };
        let extent = hi - lo;
        let Some(o) = self.scene.get_mut(id) else { return };
        for i in 0..3 {
            if extent[i] > 1e-4 && size[i].is_finite() && size[i] > 0.0 {
                // Keep a mirrored (negative) scale mirrored.
                let sign = if o.scale[i] < 0.0 { -1.0 } else { 1.0 };
                o.scale[i] = sign * size[i] / extent[i];
            }
        }
    }

    /// World-space bounds of the selected components.
    pub fn component_bounds(&self) -> Option<(Vec3, Vec3)> {
        let id = self.edit_target()?;
        let mesh = mesh_of(&self.scene, id)?;
        let world = self.scene.world_matrix(id);
        let pts = self.comp.affected_points(mesh, self.mode);
        if pts.is_empty() {
            return None;
        }
        Some(pts.iter().filter_map(|&p| mesh.points.get(p as usize)).fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), &p| {
                let w = world.transform_point3(p);
                (lo.min(w), hi.max(w))
            },
        ))
    }

    /// Moves / resizes the selected components so their world bounds get the
    /// given centre and size (zero-extent axes only move).
    pub fn set_component_bounds(&mut self, centre: Vec3, size: Vec3) {
        let Some((lo, hi)) = self.component_bounds() else { return };
        let Some(id) = self.edit_target() else { return };
        let world = self.scene.world_matrix(id);
        let inv = world.inverse();
        if !inv.is_finite() {
            return;
        }
        let (old_c, old_s) = ((lo + hi) * 0.5, hi - lo);
        let factor = Vec3::new(
            if old_s.x > 1e-5 { size.x / old_s.x } else { 1.0 },
            if old_s.y > 1e-5 { size.y / old_s.y } else { 1.0 },
            if old_s.z > 1e-5 { size.z / old_s.z } else { 1.0 },
        );
        if !factor.is_finite() || !centre.is_finite() {
            return;
        }
        let mode = self.mode;
        let Some(mesh) = mesh_of(&self.scene, id) else { return };
        let pts = self.comp.affected_points(mesh, mode);
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        for p in pts {
            if let Some(q) = mesh.points.get_mut(p as usize) {
                let w = world.transform_point3(*q);
                *q = inv.transform_point3(centre + (w - old_c) * factor);
            }
        }
        self.label(format!("Move {}", mode.label()));
    }

    // --- component editing ----------------------------------------------------

    /// The polygon object being edited in component modes (the active object).
    pub fn edit_target(&self) -> Option<ObjectId> {
        self.active().filter(|&id| mesh_of(&self.scene, id).is_some())
    }

    /// Keeps the component selection tied to the active polygon object.
    pub fn sync_components(&mut self) {
        let target = self.edit_target();
        if self.comp.object != target {
            self.comp.clear();
            self.comp.object = target;
            self.comp_drag = None;
            self.tool_drag = None;
        }
        if let Some(mesh) = target.and_then(|id| mesh_of(&self.scene, id)) {
            self.comp.prune(mesh);
        }
    }

    pub fn set_mode(&mut self, mode: EditMode) {
        self.mode = mode;
        self.comp_drag = None;
        self.tool_drag = None;
        self.box_start = None;
        if mode == EditMode::Model {
            self.mesh_tool = None;
        }
        if mode.is_component() && self.edit_target().is_none() {
            self.status = "Select a polygon object, or press C to make the selected object editable".to_owned();
        }
    }

    pub fn set_mesh_tool(&mut self, tool: MeshTool) {
        self.pen = None;
        self.mesh_tool = Some(tool);
        self.knife_start = None;
        match tool {
            MeshTool::Extrude | MeshTool::Inset if self.mode != EditMode::Polygons => self.set_mode(EditMode::Polygons),
            MeshTool::Bevel if self.mode != EditMode::Edges => self.set_mode(EditMode::Edges),
            MeshTool::LoopCut | MeshTool::Knife if !self.mode.is_component() => self.set_mode(EditMode::Edges),
            _ => {}
        }
        self.status = match tool {
            MeshTool::Extrude => "Extrude: drag left/right in the viewport to set the offset",
            MeshTool::Inset => "Extrude Inner: drag right in the viewport to set the inset",
            MeshTool::LoopCut => "Loop Cut: click an edge to cut a loop across it",
            MeshTool::Bevel => "Bevel: select edges, then drag right in the viewport to set the width",
            MeshTool::Knife => "Knife: click two points to cut along that line (selected polygons only, in Polygons mode)",
        }
        .to_owned();
    }

    /// Converts the active object into an editable polygon object (Cinema 4D's
    /// "Make Editable"). Generators bake their inputs; deformers stay live.
    pub fn make_editable(&mut self) -> bool {
        let Some(id) = self.active() else { return false };
        let Some(obj) = self.scene.get(id) else { return false };
        let convertible = match obj.kind {
            ObjectKind::Primitive(_) | ObjectKind::Generator(_) => true,
            ObjectKind::Null => self.scene.has_deformer(id),
            ObjectKind::Polygon(_) | ObjectKind::Deformer(_) | ObjectKind::Spline(_) | ObjectKind::Light(_) | ObjectKind::Camera(_) | ObjectKind::Sky(_) => false,
        };
        if !convertible {
            self.status = "Only primitives, generators and deformed Nulls can be made editable".to_owned();
            return false;
        }
        let consumes = self.scene.consumes_children(id);
        let meshes = &mut self.meshes;
        let geo = generator::base_geo(&self.scene, id, &mut |p| meshes.get(p));
        if geo.verts.is_empty() {
            self.status = "Nothing to convert: the object has no geometry".to_owned();
            return false;
        }
        let mesh = EditMesh::from_geo(&geo);
        if consumes {
            // The inputs are now baked in; deformers keep working on the result.
            let inputs: Vec<ObjectId> = self
                .scene
                .children_of(Some(id))
                .iter()
                .copied()
                .filter(|&c| !matches!(self.scene.get(c).map(|o| &o.kind), Some(ObjectKind::Deformer(_))))
                .collect();
            self.scene.remove(&inputs);
        }
        let (points, polys) = (mesh.points.len(), mesh.polys.len());
        if let Some(o) = self.scene.get_mut(id) {
            o.kind = ObjectKind::Polygon(mesh);
        }
        self.prune_selection();
        self.label("Make Editable");
        self.status = format!("Made editable: {points} points, {polys} polygons");
        true
    }

    pub fn comp_select_all(&mut self) {
        let Some(mesh) = self.edit_target().and_then(|id| mesh_of(&self.scene, id)) else { return };
        match self.mode {
            EditMode::Points => self.comp.points = (0..mesh.points.len() as u32).collect(),
            EditMode::Edges => self.comp.edges = mesh.edges().into_iter().collect(),
            EditMode::Polygons => self.comp.polys = (0..mesh.polys.len() as u32).collect(),
            EditMode::Model | EditMode::Object | EditMode::Workplane => {}
        }
    }

    /// Delete in a component mode: points and polygons are removed, edges dissolved.
    pub fn delete_components(&mut self) {
        let Some(id) = self.edit_target() else { return };
        let (mode, comp) = (self.mode, self.comp.clone());
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let msg = match mode {
            EditMode::Points if !comp.points.is_empty() => {
                mesh.delete_points(&comp.points);
                format!("Deleted {} point(s)", comp.points.len())
            }
            EditMode::Polygons if !comp.polys.is_empty() => {
                mesh.delete_polys(&comp.polys);
                format!("Deleted {} polygon(s)", comp.polys.len())
            }
            EditMode::Edges if !comp.edges.is_empty() => {
                let n = mesh.dissolve_edges(&comp.edges);
                format!("Dissolved {n} of {} edge(s) (border edges are kept)", comp.edges.len())
            }
            _ => return,
        };
        self.comp.clear();
        self.label("Delete");
        self.status = msg;
    }

    pub fn reverse_normals(&mut self) {
        let Some(id) = self.edit_target() else { return };
        let all = self.comp.polys.is_empty() || self.mode != EditMode::Polygons;
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let polys: BTreeSet<u32> = if all { (0..mesh.polys.len() as u32).collect() } else { self.comp.polys.clone() };
        mesh.reverse(&polys);
        self.label("Reverse Normals");
    }

    /// Applies a loop cut and selects the new edges.
    pub fn loop_cut(&mut self, edge: crate::editmesh::Edge, t: f32) {
        let Some(id) = self.edit_target() else { return };
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let cut = mesh.loop_cut(edge, t);
        if cut.is_empty() {
            self.status = "Loop Cut needs an edge between quads".to_owned();
            return;
        }
        self.comp.clear();
        self.comp.edges = cut.iter().copied().collect();
        if self.mode != EditMode::Edges {
            self.mode = EditMode::Edges;
        }
        self.label("Loop Cut");
        self.status = format!("Loop cut through {} polygon(s)", cut.len());
    }

    /// Knife cut along a world-space plane. In Polygons mode with a selection,
    /// only the selected polygons are cut.
    pub fn knife_cut(&mut self, world_point: Vec3, world_normal: Vec3) {
        let Some(id) = self.edit_target() else { return };
        let world = self.scene.world_matrix(id);
        let inv = world.inverse();
        if !inv.is_finite() {
            return;
        }
        let local_point = inv.transform_point3(world_point);
        let local_normal = glam::Mat3::from_mat4(world).transpose() * world_normal;
        let only = (self.mode == EditMode::Polygons && !self.comp.polys.is_empty()).then(|| self.comp.polys.clone());
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let cut = mesh.knife(local_point, local_normal, only.as_ref());
        if cut.is_empty() {
            self.status = "Knife: the line does not cross the object".to_owned();
            return;
        }
        self.comp.clear();
        if self.mode == EditMode::Edges {
            self.comp.edges = cut.iter().copied().collect();
        }
        self.label("Knife");
        self.status = format!("Knife: split {} polygon(s)", cut.len());
    }

    pub fn bridge(&mut self) {
        let Some(id) = self.edit_target() else { return };
        let (mode, comp) = (self.mode, self.comp.clone());
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let result = match mode {
            EditMode::Edges => mesh.bridge_edges(&comp.edges),
            EditMode::Polygons if comp.polys.len() == 2 => {
                let v: Vec<u32> = comp.polys.iter().copied().collect();
                mesh.bridge_polys(v[0], v[1])
            }
            EditMode::Polygons => Err("Bridge needs exactly two selected polygons".to_owned()),
            _ => Err("Bridge works in Edges mode (two border edges or loops) or Polygons mode (two polygons)".to_owned()),
        };
        match result {
            Ok(n) => {
                self.comp.clear();
                self.label("Bridge");
                self.status = format!("Bridge: {n} polygon(s) created");
            }
            Err(e) => self.status = e,
        }
    }

    pub fn weld(&mut self) {
        let Some(id) = self.edit_target() else { return };
        let Some(mesh) = mesh_of(&self.scene, id) else { return };
        let pts = self.comp.affected_points(mesh, self.mode);
        if pts.len() < 2 {
            self.status = "Weld needs at least two selected points (or edges / polygons)".to_owned();
            return;
        }
        if let Some(mesh) = mesh_of_mut(&mut self.scene, id) {
            mesh.weld(&pts);
        }
        self.comp.clear();
        self.label("Weld");
        self.status = format!("Welded {} points into one", pts.len());
    }

    pub fn optimize(&mut self) {
        let Some(id) = self.edit_target() else { return };
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        let n = mesh.optimize(0.01);
        self.comp.clear();
        self.label("Optimize");
        self.status = format!("Optimize: merged {n} point(s) closer than 0.01 cm");
    }

    /// Smooth subdivides the whole object; linear subdivides the selected
    /// polygons (or all of them when none are selected).
    pub fn subdivide(&mut self, smooth: bool) {
        let Some(id) = self.edit_target() else { return };
        let sel = if self.mode == EditMode::Polygons { self.comp.polys.clone() } else { Default::default() };
        let Some(mesh) = mesh_of_mut(&mut self.scene, id) else { return };
        if smooth {
            mesh.subdivide_smooth();
            self.comp.clear();
        } else {
            let target = if sel.is_empty() { (0..mesh.polys.len() as u32).collect() } else { sel };
            let new = mesh.subdivide_linear(&target);
            self.comp.clear();
            if self.mode == EditMode::Polygons {
                self.comp.polys = new;
            }
        }
        let n = mesh_of(&self.scene, id).map_or(0, |m| m.polys.len());
        self.label("Subdivide");
        self.status = format!("Subdivided: {n} polygons");
    }

    /// Re-applies an Extrude / Inset drag with `amount`.
    pub fn apply_tool_drag(&mut self, amount: f32) {
        let Some(mut td) = self.tool_drag.take() else { return };
        td.amount = amount;
        let mut mesh = td.base.clone();
        match self.mesh_tool {
            Some(MeshTool::Extrude) => mesh.extrude(&td.polys, amount),
            Some(MeshTool::Inset) => mesh.inset(&td.polys, amount.max(0.0)),
            Some(MeshTool::Bevel) => {
                mesh.bevel_edges(&td.edges, amount.max(0.0));
            }
            _ => {}
        }
        if let Some(m) = mesh_of_mut(&mut self.scene, td.id) {
            *m = mesh;
        }
        self.label(self.mesh_tool.map(MeshTool::label).unwrap_or("Edit"));
        self.tool_drag = Some(td);
    }

    pub fn label(&mut self, text: impl Into<String>) {
        self.next_label = text.into();
    }

    // --- scene operations -------------------------------------------------

    pub fn create_primitive(&mut self, kind: PrimitiveKind) -> ObjectId {
        let id = self.scene.add_primitive(kind, None);
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        self.status = format!("Created {}", kind.name());
        id
    }

    /// Adds a light, placed to light the scene from above and in front.
    pub fn create_light(&mut self, kind: crate::light::LightKind) -> ObjectId {
        let id = self.scene.add_object(kind.name(), ObjectKind::Light(kind.default_light()), None);
        let cam = self.cameras[self.active_view.index()];
        let (pos, rot) = kind.default_placement(cam.target, cam.eye() - cam.target);
        if let Some(o) = self.scene.get_mut(id) {
            o.pos = pos;
            o.rot = rot;
        }
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        self.status = format!("Created {}: the viewport now uses the scene's lights", kind.name());
        id
    }

    /// The visible lights, ready for the viewport (skies count as lights).
    pub fn scene_lights(&self) -> crate::light::SceneLights {
        let mut found = Vec::new();
        self.scene.walk(|o, _| {
            if !self.scene.editor_visible(o.id) {
                return;
            }
            match &o.kind {
                ObjectKind::Light(l) => found.push((l.clone(), self.scene.effective_world(o.id))),
                ObjectKind::Sky(s) => found.extend(sky_lights(s)),
                _ => {}
            }
        });
        crate::light::SceneLights::gather(found.iter().map(|(l, w)| (l, *w)))
    }

    /// Keeps the Perspective view and the looked-through camera together:
    /// when the view was navigated since the last call, the camera object
    /// moves to it; then the view takes the camera's pose. `aspect` is the
    /// view's width / height. Returns whether the view changed.
    pub fn follow_look_through(&mut self, aspect: f32) -> bool {
        use crate::scene_camera::CameraKind;
        let slot = ViewKind::Perspective.index();
        let Some(id) = self.look_through.filter(|id| matches!(self.scene.get(*id).map(|o| &o.kind), Some(ObjectKind::Camera(_)))) else {
            self.look_through = None;
            self.look_cache = None;
            return false;
        };
        let view = self.cameras[slot];
        if self.look_cache.is_some_and(|c| c != view) {
            let kind = match self.scene.get(id).map(|o| &o.kind) {
                Some(ObjectKind::Camera(c)) => c.kind,
                _ => CameraKind::Standard,
            };
            // Rigs place the camera themselves: only plain cameras follow
            // fully; a Target camera moves but keeps aiming at its target.
            match kind {
                CameraKind::Standard | CameraKind::Stereo | CameraKind::Motion => {
                    self.scene.set_world_position(id, view.eye());
                    self.scene.set_world_rotation_hpb(id, crate::light::aim_hpb(view.forward()));
                }
                CameraKind::Target => self.scene.set_world_position(id, view.eye()),
                CameraKind::Morph | CameraKind::Crane => {}
            }
        }
        let Some(pose) = crate::scene_camera::pose(&self.scene, id, self.time()) else { return false };
        let mut cam = pose.to_editor(aspect, view.distance);
        cam.kind = ViewKind::Perspective;
        let changed = cam != view;
        self.cameras[slot] = cam;
        self.look_cache = Some(cam);
        changed
    }

    /// The scene time in seconds (the timeline's frame).
    pub fn time(&self) -> f32 {
        self.timeline.frame as f32 / self.timeline.fps.max(1.0)
    }

    /// Adds a camera where the active view looks from, seeing what it sees.
    /// A Target camera gets a Null to aim at; a Camera Morph blends the two
    /// selected cameras.
    pub fn create_camera(&mut self, kind: crate::scene_camera::CameraKind) -> ObjectId {
        use crate::scene_camera::{CameraKind, SceneCamera};
        let picked: Vec<ObjectId> = self.selection.iter().copied().filter(|&s| matches!(self.scene.get(s).map(|o| &o.kind), Some(ObjectKind::Camera(_)))).collect();
        let mut cam = SceneCamera::new(kind);
        let view = self.cameras[ViewKind::Perspective.index()];
        if kind == CameraKind::Morph {
            cam.morph_from = picked.first().copied();
            cam.morph_to = picked.get(1).copied();
        }
        cam.focus_distance = view.distance;
        let id = self.scene.add_object(kind.name(), ObjectKind::Camera(cam), None);
        let eye = view.eye();
        let hpb = crate::light::aim_hpb(view.forward());
        if let Some(o) = self.scene.get_mut(id) {
            if kind == CameraKind::Crane {
                // The base on the floor, the boom reaching out to where the view is.
                let flat = Vec3::new(view.forward().x, 0.0, view.forward().z).normalize_or(Vec3::Z);
                o.pos = Vec3::new(eye.x, 0.0, eye.z) - flat * 400.0 * 15f32.to_radians().cos();
                o.rot = Vec3::new(hpb.x, 0.0, 0.0);
            } else {
                o.pos = eye;
                o.rot = hpb;
            }
        }
        if kind == CameraKind::Target {
            let t = self.scene.add_null(None);
            if let Some(o) = self.scene.get_mut(t) {
                o.name = "Camera Target".to_owned();
                o.pos = view.target;
            }
            if let Some(ObjectKind::Camera(c)) = self.scene.get_mut(id).map(|o| &mut o.kind) {
                c.look_at = Some(t);
            }
        }
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        self.status = match kind {
            CameraKind::Morph if picked.len() < 2 => "Camera Morph: set Morph From and Morph To to two cameras".to_owned(),
            _ => format!("Created {}: Cameras > Look Through shows its view", kind.name()),
        };
        id
    }

    /// Adds a Sky or Physical Sky.
    pub fn create_sky(&mut self, kind: crate::sky::SkyKind) -> ObjectId {
        let id = self.scene.add_object(kind.name(), ObjectKind::Sky(crate::sky::Sky::new(kind)), None);
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        self.status = format!("Created {}: it lights the scene and shows behind it in renders", kind.name());
        id
    }

    /// Adds a Spot light aimed at a new Null (a Target light).
    pub fn create_target_light(&mut self) -> ObjectId {
        let id = self.create_light(crate::light::LightKind::Spot);
        let target = self.cameras[self.active_view.index()].target;
        let t = self.scene.add_null(None);
        if let Some(o) = self.scene.get_mut(t) {
            o.name = "Light Target".to_owned();
            o.pos = target;
        }
        if let Some(o) = self.scene.get_mut(id) {
            o.name = "Target Light".to_owned();
            if let ObjectKind::Light(l) = &mut o.kind {
                l.look_at = Some(t);
            }
        }
        self.select_only(id);
        self.label("Create Target Light");
        self.status = "Created Target Light: move \"Light Target\" to aim it".to_owned();
        id
    }

    pub fn create_null(&mut self) -> ObjectId {
        let id = self.scene.add_null(None);
        self.select_only(id);
        self.label("Create Null");
        id
    }

    /// Creates a generator. Selected objects become its children (keeping their world
    /// position), which is the usual Cinema 4D workflow.
    pub fn create_generator(&mut self, kind: GeneratorKind) -> ObjectId {
        let id = self.scene.add_generator(kind, None);
        let inputs = self.scene.top_level(&self.selection);
        let mut adopted = 0;
        for obj in inputs {
            if obj != id && self.scene.move_object(obj, DropTarget::Into(id)) {
                adopted += 1;
            }
        }
        self.expanded.insert(id);
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        self.status = if adopted > 0 {
            format!("Created {} with {adopted} child object(s)", kind.name())
        } else {
            format!("Created {}: drag objects into it in the Object Manager", kind.name())
        };
        id
    }

    /// Adds a deformer under the active object (the object it will deform) and fits
    /// its box to that object. With nothing selected it is created at the top level.
    pub fn create_deformer(&mut self, kind: DeformerKind) -> ObjectId {
        let parent = self.active().filter(|&p| !matches!(self.scene.get(p).map(|o| &o.kind), Some(ObjectKind::Deformer(_))));
        let id = self.scene.add_deformer(kind, parent);
        if let Some(p) = parent {
            self.expanded.insert(p);
            self.fit_deformer(id);
            self.status = format!("Added {} to {}", kind.name(), self.scene.get(p).map(|o| o.name.as_str()).unwrap_or(""));
        } else {
            self.status = format!("Created {}: drag it onto the object it should deform", kind.name());
        }
        self.select_only(id);
        self.label(format!("Create {}", kind.name()));
        id
    }

    /// Resizes and places a deformer's box around its parent's undeformed geometry.
    /// Returns false when there is nothing to fit to.
    pub fn fit_deformer(&mut self, id: ObjectId) -> bool {
        let Some(parent) = self.scene.get(id).and_then(|o| o.parent) else { return false };
        let meshes = &mut self.meshes;
        let geo = generator::base_geo(&self.scene, parent, &mut |p| meshes.get(p));
        if geo.verts.is_empty() {
            return false;
        }
        let (lo, hi) = geo.verts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos)))
        });
        let Some(o) = self.scene.get_mut(id) else { return false };
        let ObjectKind::Deformer(d) = &mut o.kind else { return false };
        d.set_size((hi - lo).max(Vec3::splat(1.0)));
        o.pos = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5);
        o.rot = Vec3::ZERO;
        o.scale = Vec3::ONE;
        self.label("Fit to Parent");
        true
    }

    pub fn delete_selection(&mut self) {
        let doomed = self.scene.top_level(&self.selection);
        if doomed.is_empty() {
            return;
        }
        self.scene.remove(&doomed);
        self.prune_selection();
        self.label("Delete");
        self.status = format!("Deleted {} object(s)", doomed.len());
    }

    pub fn duplicate_selection(&mut self) {
        let copies = self.scene.duplicate(&self.selection.clone());
        if copies.is_empty() {
            return;
        }
        self.selection = copies;
        self.label("Duplicate");
    }

    /// A copy of the selected objects for the clipboard (whole subtrees and
    /// their materials). `None` when nothing is selected.
    pub fn copy_selection(&mut self) -> Option<Scene> {
        if self.selection.is_empty() {
            self.status = "Nothing selected to copy".to_owned();
            return None;
        }
        let clip = self.scene.extract(&self.selection);
        self.status = format!("Copied {} object(s)", clip.roots().len());
        Some(clip)
    }

    /// Copies the selection, then deletes it.
    pub fn cut_selection(&mut self) -> Option<Scene> {
        let clip = self.copy_selection()?;
        self.delete_selection();
        self.label("Cut");
        self.status = format!("Cut {} object(s)", clip.roots().len());
        Some(clip)
    }

    /// Adds the clipboard's objects (at the top level, where they were in the
    /// world) and selects them.
    pub fn paste(&mut self, clip: &Scene) -> Vec<ObjectId> {
        let added = self.scene.merge(clip);
        if added.is_empty() {
            self.status = "The clipboard has no objects".to_owned();
            return added;
        }
        self.selection = added.clone();
        self.label("Paste");
        self.status = format!("Pasted {} object(s)", added.len());
        added
    }

    /// Merge Project: adds every object and material of another scene file.
    pub fn merge_from(&mut self, path: &Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let other = Scene::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let added = self.scene.merge(&other);
        self.selection = added.clone();
        self.label("Merge Project");
        self.status = format!("Merged {} object(s) from {}", added.len(), path.display());
        Ok(added.len())
    }

    /// Revert: reloads the scene from its file, dropping unsaved changes.
    pub fn revert(&mut self) -> Result<(), String> {
        let path = self.file_path.clone().ok_or("This project has never been saved")?;
        self.open_from(&path)?;
        self.status = format!("Reverted to the saved {}", path.display());
        Ok(())
    }

    /// Never saved, unchanged, with nothing undoable: Open can replace it.
    pub fn is_pristine(&self) -> bool {
        self.file_path.is_none() && !self.is_dirty() && !self.history.can_undo()
    }

    /// The project's name, as the Window menu lists it.
    pub fn project_name(&self) -> String {
        self.file_path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    pub fn group_selection(&mut self) {
        if let Some(null) = self.scene.group(&self.selection.clone()) {
            self.expanded.insert(null);
            self.select_only(null);
            self.label("Group Objects");
        }
    }

    pub fn new_material(&mut self) -> MaterialId {
        let n = self.scene.materials().len() + 1;
        let hue = (n as f32 * 0.17).fract();
        let id = self.scene.add_material(&format!("Mat.{n}"), hsv_to_rgb(hue, 0.45, 0.85));
        self.selected_material = Some(id);
        self.label("New Material");
        id
    }

    pub fn assign_material(&mut self, ids: &[ObjectId], material: Option<MaterialId>) {
        for &id in ids {
            if let Some(o) = self.scene.get_mut(id) {
                o.material = material;
            }
        }
        self.label("Assign Material");
    }

    pub fn undo(&mut self) {
        if let Some(label) = self.history.undo(&mut self.scene) {
            self.prune_selection();
            self.drag = None;
            self.status = format!("Undo: {label}");
        }
    }

    pub fn redo(&mut self) {
        if let Some(label) = self.history.redo(&mut self.scene) {
            self.prune_selection();
            self.drag = None;
            self.status = format!("Redo: {label}");
        }
    }

    // --- modes: axis, model scaling, workplane ----------------------------

    /// Starts keeping `id`'s geometry and children in place while its axis
    /// moves (Axis mode). Returns false for objects whose geometry is made
    /// from parameters (primitives): their axis can't move alone.
    pub fn begin_axis_drag(&mut self, id: ObjectId) -> bool {
        let Some(o) = self.scene.get(id) else { return false };
        if matches!(o.kind, ObjectKind::Primitive(_)) {
            self.status = format!("Axis mode: \"{}\" is parametric; press C to make it editable first", o.name);
            return false;
        }
        let kind = o.kind.clone();
        let children = self.scene.children_of(Some(id)).iter().map(|&c| (c, self.scene.world_matrix(c))).collect();
        self.axis_base = Some(AxisBase { id, start_world: self.scene.world_matrix(id), kind, children });
        true
    }

    /// Puts the geometry and children back where they were before the axis moved.
    pub fn apply_axis_compensation(&mut self) {
        let Some(base) = self.axis_base.clone() else { return };
        let now = self.scene.world_matrix(base.id);
        let m = now.inverse() * base.start_world;
        if !m.is_finite() {
            return;
        }
        if let Some(o) = self.scene.get_mut(base.id) {
            match (&mut o.kind, &base.kind) {
                (ObjectKind::Polygon(mesh), ObjectKind::Polygon(orig)) => {
                    for (p, q) in mesh.points.iter_mut().zip(&orig.points) {
                        *p = m.transform_point3(*q);
                    }
                }
                (ObjectKind::Spline(sp), ObjectKind::Spline(orig)) => {
                    for (p, q) in sp.points.iter_mut().zip(&orig.points) {
                        p.pos = m.transform_point3(q.pos);
                        p.tan_in = m.transform_vector3(q.tan_in);
                        p.tan_out = m.transform_vector3(q.tan_out);
                    }
                }
                _ => {}
            }
        }
        for (c, w) in base.children {
            self.scene.set_world_matrix(c, w);
        }
    }

    /// Ends a gizmo drag. In Model mode a scale drag is baked into the
    /// object's geometry, so its scale stays as it was.
    pub fn finish_drag(&mut self) {
        let Some(d) = self.drag.take() else { return };
        self.axis_base = None;
        self.snap_hit = None;
        if d.workplane || self.axis_mode || d.tool != Tool::Scale || self.mode != EditMode::Model {
            return;
        }
        let (_, _, start_scale) = d.start_local();
        self.bake_scale(d.id, start_scale);
    }

    /// Moves the scale gained since `start_scale` from the object into its
    /// geometry (parameters, points) and its children's positions.
    pub fn bake_scale(&mut self, id: ObjectId, start_scale: Vec3) -> bool {
        let Some(o) = self.scene.get(id) else { return false };
        let ratio = o.scale / start_scale;
        if !ratio.is_finite() || (ratio - Vec3::ONE).abs().max_element() < 1e-6 {
            return false;
        }
        let baked = match &o.kind {
            ObjectKind::Primitive(p) => p.scaled(ratio).map(ObjectKind::Primitive),
            ObjectKind::Polygon(m) => {
                let mut m = m.clone();
                m.points.iter_mut().for_each(|p| *p *= ratio);
                Some(ObjectKind::Polygon(m))
            }
            ObjectKind::Spline(sp) => {
                let mut sp = sp.clone();
                for p in &mut sp.points {
                    p.pos *= ratio;
                    p.tan_in *= ratio;
                    p.tan_out *= ratio;
                }
                Some(ObjectKind::Spline(sp))
            }
            _ => None,
        };
        let Some(kind) = baked else {
            // Nulls, lights, generators and squashed round shapes keep the scale.
            return false;
        };
        let children: Vec<ObjectId> = self.scene.children_of(Some(id)).to_vec();
        if let Some(o) = self.scene.get_mut(id) {
            o.kind = kind;
            o.scale = start_scale;
        }
        for c in children {
            if let Some(child) = self.scene.get_mut(c) {
                child.pos *= ratio;
            }
        }
        true
    }

    /// Workplane > Align to an axis: the plane through the origin facing X, Y or Z.
    pub fn align_workplane_to_axis(&mut self, axis: usize) {
        self.scene.workplane = crate::scene::Workplane {
            pos: Vec3::ZERO,
            // The plane's normal (local Y) onto world X, Y or Z.
            rot: match axis {
                0 => Vec3::new(0.0, 0.0, -90.0),
                2 => Vec3::new(0.0, 90.0, 0.0),
                _ => Vec3::ZERO,
            },
        };
        self.label("Align Workplane");
    }

    /// Workplane > Align to Selection: onto the active object's axis.
    pub fn align_workplane_to_selection(&mut self) {
        let Some(id) = self.active() else {
            self.status = "Align Workplane to Selection: select an object".to_owned();
            return;
        };
        self.scene.workplane = crate::scene::Workplane { pos: self.scene.world_position(id), rot: self.scene.world_rotation_hpb(id) };
        self.label("Align Workplane");
    }

    /// Moves and turns the selected objects onto the workplane.
    pub fn align_selection_to_workplane(&mut self) {
        let wp = self.scene.workplane;
        for id in self.scene.top_level(&self.selection) {
            self.scene.set_world_position(id, wp.pos);
            self.scene.set_world_rotation_hpb(id, wp.rot);
        }
        self.label("Align to Workplane");
    }

    /// Axis mode helper: moves the object's axis to the centre of its geometry
    /// (or of its children), leaving everything where it is.
    pub fn center_axis(&mut self) {
        let Some(id) = self.active() else { return };
        let centre = match self.scene.get(id).map(|o| &o.kind) {
            Some(ObjectKind::Polygon(m)) if !m.points.is_empty() => {
                let (lo, hi) = m.points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                Some(self.scene.world_matrix(id).transform_point3((lo + hi) * 0.5))
            }
            Some(ObjectKind::Spline(sp)) if !sp.points.is_empty() => {
                let (lo, hi) = sp.points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(p.pos), hi.max(p.pos)));
                Some(self.scene.world_matrix(id).transform_point3((lo + hi) * 0.5))
            }
            _ => {
                let kids = self.scene.children_of(Some(id));
                (!kids.is_empty()).then(|| kids.iter().map(|&c| self.scene.world_position(c)).sum::<Vec3>() / kids.len() as f32)
            }
        };
        let Some(centre) = centre else {
            self.status = "Center Axis: the object has no points or children".to_owned();
            return;
        };
        if !self.begin_axis_drag(id) {
            return;
        }
        self.scene.set_world_position(id, centre);
        self.apply_axis_compensation();
        self.axis_base = None;
        self.label("Center Axis");
    }

    /// Call once per frame after all edits.
    pub fn commit_history(&mut self, gesture_in_progress: bool) {
        let label = self.next_label.clone();
        if self.history.update(&self.scene, gesture_in_progress, &label) {
            self.next_label = "Edit".to_owned();
        }
    }

    // --- rendering helpers ------------------------------------------------

    /// Every visible object with its mesh, world matrix and display colour.
    pub fn render_objects(&mut self) -> Vec<RenderObject> {
        self.collect_objects(false)
    }

    /// The surfaces a final render shows: objects visible in renders (the
    /// render dot and the layer's Render switch), without helpers such as
    /// lights, cameras, nulls and splines.
    pub fn final_objects(&mut self) -> Vec<RenderObject> {
        let mut v = self.collect_objects(true);
        v.retain(|o| o.kind == ItemKind::Surface && !o.mesh.indices.is_empty());
        for o in &mut v {
            o.selection = SelectionState::None;
        }
        v
    }

    fn collect_objects(&mut self, final_render: bool) -> Vec<RenderObject> {
        let active = self.active();
        let time = self.time();
        let mut out = Vec::new();
        let (scene, meshes, selection) = (&self.scene, &mut self.meshes, &self.selection);
        scene.walk(|o, world| {
            // Cameras sit where their rig puts them; aimed lights face their target.
            let world = match &o.kind {
                ObjectKind::Camera(_) => crate::scene_camera::pose(scene, o.id, time).map_or(world, |p| p.matrix()),
                ObjectKind::Light(l) if l.look_at.is_some() => scene.effective_world(o.id),
                _ => world,
            };
            // Objects inside a generator (or a deformed Null) only exist through its output;
            // deformers stay visible as their cage.
            let is_deformer = matches!(o.kind, ObjectKind::Deformer(_));
            let visible = if final_render { scene.render_visible(o.id) } else { scene.editor_visible(o.id) };
            if !visible || (!is_deformer && scene.consumer_ancestor(o.id).is_some()) {
                return;
            }
            let deformed = scene.has_deformer(o.id);
            let (mesh, kind) = match &o.kind {
                ObjectKind::Null if deformed => match meshes.generated(scene, o.id) {
                    Some(m) => (m, ItemKind::Surface),
                    None => (meshes.null(), ItemKind::LinesOnly),
                },
                ObjectKind::Null => (meshes.null(), ItemKind::LinesOnly),
                ObjectKind::Primitive(p) if deformed => match meshes.generated(scene, o.id) {
                    Some(m) => (m, ItemKind::Surface),
                    None => (meshes.get(p), ItemKind::Surface),
                },
                ObjectKind::Primitive(p) => (meshes.get(p), ItemKind::Surface),
                // Switched off: its children are drawn on their own; show a marker.
                ObjectKind::Generator(_) if !o.enabled => (meshes.null(), ItemKind::LinesOnly),
                ObjectKind::Generator(_) => match if final_render { meshes.generated(scene, o.id) } else { meshes.generated_view(scene, o.id) } {
                    Some(m) => (m, ItemKind::Surface),
                    None => (meshes.null(), ItemKind::LinesOnly),
                },
                ObjectKind::Deformer(d) => (meshes.cage(d), ItemKind::LinesOnly),
                ObjectKind::Spline(s) => (meshes.spline(s), ItemKind::LinesOnly),
                ObjectKind::Light(l) => (meshes.light(l), ItemKind::LinesOnly),
                ObjectKind::Camera(c) => (meshes.camera(c), ItemKind::LinesOnly),
                ObjectKind::Sky(s) => (meshes.sky(s), ItemKind::LinesOnly),
                ObjectKind::Polygon(_) => match meshes.generated(scene, o.id) {
                    Some(m) => (m, ItemKind::Surface),
                    None => (meshes.null(), ItemKind::LinesOnly),
                },
            };
            // A generator (or deformed Null) without its own material shows its first input's.
            // A Boolean takes its colour from A (the first child) only.
            let material = o.material.or_else(|| {
                if !scene.consumes_children(o.id) {
                    return None;
                }
                let root = match o.kind {
                    ObjectKind::Generator(generator::Generator::Boolean { .. }) => *scene.children_of(Some(o.id)).first()?,
                    _ => o.id,
                };
                scene.subtree(root).into_iter().find_map(|d| scene.get(d)?.material)
            });
            let light_color = match &o.kind {
                // Lights are drawn in their own colour, lifted so dark ones stay visible.
                ObjectKind::Light(l) => Some(Vec3::from(l.color).lerp(Vec3::new(1.0, 0.85, 0.35), 0.35).max(Vec3::splat(0.35)).to_array()),
                ObjectKind::Camera(_) => Some([0.62, 0.78, 1.0]),
                ObjectKind::Sky(s) => Some(s.radiance(Vec3::new(0.0, 0.6, 0.8)).max(Vec3::splat(0.4)).to_array()),
                _ => None,
            };
            let color = light_color.or_else(|| material.and_then(|m| scene.material(m)).map(|m| m.color)).unwrap_or(match kind {
                    _ if is_deformer && !o.enabled => DISABLED_COLOR,
                    _ if is_deformer => DEFORMER_COLOR,
                    ItemKind::Surface => DEFAULT_COLOR,
                    ItemKind::LinesOnly => [0.85, 0.85, 0.88],
                });
            let selection = if Some(o.id) == active {
                SelectionState::Active
            } else if selection.contains(&o.id) {
                SelectionState::Selected
            } else {
                SelectionState::None
            };
            out.push(RenderObject {
                id: o.id,
                world,
                mesh,
                color,
                material: if light_color.is_some() { None } else { material },
                selection,
                kind,
                curve: matches!(o.kind, ObjectKind::Spline(_) | ObjectKind::Light(_) | ObjectKind::Camera(_) | ObjectKind::Sky(_)),
            });
        });
        out
    }

    /// Bounding sphere (center, radius) of the objects chosen by `include`.
    pub fn bounds_sphere(objects: &[RenderObject], include: impl Fn(&RenderObject) -> bool) -> Option<(Vec3, f32)> {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        let mut any = false;
        for o in objects.iter().filter(|o| include(o)) {
            let (lo, hi) = o.world_bounds();
            min = min.min(lo);
            max = max.max(hi);
            any = true;
        }
        any.then(|| ((min + max) * 0.5, (max - min).length() * 0.5))
    }

    pub fn frame_selected(&mut self, view: ViewKind) {
        let objs = self.render_objects();
        let sel = self.selection.clone();
        if let Some((c, r)) = Self::bounds_sphere(&objs, |o| sel.contains(&o.id)) {
            self.cameras[view.index()].frame(c, r);
        } else {
            self.frame_all(view);
        }
    }

    pub fn frame_all(&mut self, view: ViewKind) {
        let objs = self.render_objects();
        let (c, r) = Self::bounds_sphere(&objs, |_| true).unwrap_or((Vec3::new(0.0, 60.0, 0.0), 400.0));
        self.cameras[view.index()].frame(c, r);
    }

    // --- files ------------------------------------------------------------

    pub fn is_dirty(&self) -> bool {
        self.scene != self.saved
    }

    pub fn title(&self) -> String {
        let name = self
            .file_path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());
        format!("{}{} - Forma3D", name, if self.is_dirty() { " *" } else { "" })
    }

    pub fn new_scene(&mut self) {
        self.replace_scene(Scene::new(), None);
        self.status = "New scene".to_owned();
    }

    fn replace_scene(&mut self, scene: Scene, path: Option<PathBuf>) {
        self.scene = scene;
        self.saved = self.scene.clone();
        self.history.reset(&self.scene);
        self.file_path = path;
        self.selection.clear();
        self.selected_material = None;
        self.expanded.clear();
        self.renaming = None;
        self.drag = None;
        self.frame_all(ViewKind::Perspective);
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let json = self.scene.to_json().map_err(|e| e.to_string())?;
        // Write to a temporary file first so a failed write cannot destroy the old save.
        let tmp = path.with_extension("tmp-save");
        std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })?;
        self.saved = self.scene.clone();
        self.file_path = Some(path.to_path_buf());
        self.status = format!("Saved {}", path.display());
        Ok(())
    }

    /// The scene as file text, marked as saved to `path`. For hosts that
    /// write the file themselves (the Tauri app).
    pub fn save_json(&mut self, path: PathBuf) -> Result<String, String> {
        let json = self.scene.to_json().map_err(|e| e.to_string())?;
        self.saved = self.scene.clone();
        self.status = format!("Saved {}", path.display());
        self.file_path = Some(path);
        Ok(json)
    }

    /// Opens scene file text read by the host from `path`.
    pub fn open_json(&mut self, text: &str, path: PathBuf) -> Result<(), String> {
        let scene = Scene::from_json(text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.status = format!("Opened {}", path.display());
        self.replace_scene(scene, Some(path));
        Ok(())
    }

    pub fn open_from(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let scene = Scene::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.replace_scene(scene, Some(path.to_path_buf()));
        self.status = format!("Opened {}", path.display());
        Ok(())
    }

    pub fn default_path(&self) -> String {
        match &self.file_path {
            Some(p) => p.display().to_string(),
            None => std::env::current_dir()
                .unwrap_or_default()
                .join("scene.forma3d.json")
                .display()
                .to_string(),
        }
    }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match (i as i32).rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn cut_copy_paste_and_merge_and_revert() {
        let dir = std::env::temp_dir().join(format!("forma3d-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ed = Editor::new(Scene::demo());
        let cube = ed.scene.roots()[0];
        let count = ed.scene.roots().len();

        // Copy leaves the scene alone; paste adds a copy and selects it.
        ed.select_only(cube);
        let clip = ed.copy_selection().unwrap();
        assert_eq!(ed.scene.roots().len(), count);
        let pasted = ed.paste(&clip);
        assert_eq!(ed.scene.roots().len(), count + 1);
        assert_eq!(ed.selection, pasted);
        // Cut removes; paste brings it back; both are undoable steps.
        ed.select_only(cube);
        let clip = ed.cut_selection().unwrap();
        assert!(ed.scene.get(cube).is_none());
        ed.paste(&clip);
        assert_eq!(ed.scene.roots().len(), count + 1);
        // Nothing selected: nothing copied.
        ed.selection.clear();
        assert!(ed.copy_selection().is_none());

        // Save, change, revert.
        let path = dir.join("a.forma3d.json");
        ed.save_to(&path).unwrap();
        let saved_count = ed.scene.roots().len();
        ed.create_primitive(crate::primitive::PrimitiveKind::Torus);
        assert!(ed.is_dirty());
        ed.revert().unwrap();
        assert_eq!(ed.scene.roots().len(), saved_count);
        assert!(!ed.is_dirty());

        // Merge the saved file into a fresh project.
        let mut other = Editor::new(Scene::new());
        assert!(other.is_pristine());
        assert_eq!(other.merge_from(&path).unwrap(), saved_count);
        assert_eq!(other.scene.roots().len(), saved_count);
        assert!(other.file_path.is_none(), "merging doesn't change which file the project is");
        assert!(!other.is_pristine());
        // Reverting a never-saved project explains why it can't.
        assert!(Editor::new(Scene::new()).revert().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn slow_meshes_are_built_elsewhere_and_shown_when_back() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let mut scene = Scene::new();
        let b = scene.add_generator(GeneratorKind::Boolean, None);
        scene.add_primitive(crate::primitive::PrimitiveKind::Cube, Some(b));
        let s = scene.add_primitive(crate::primitive::PrimitiveKind::Sphere, Some(b));
        scene.get_mut(s).unwrap().pos = Vec3::new(60.0, 60.0, 0.0);
        let mut ed = Editor::new(scene);
        let jobs: Rc<RefCell<Vec<MeshJob>>> = Rc::default();
        let sink = jobs.clone();
        ed.meshes.set_offload(Box::new(move |j| sink.borrow_mut().push(j)), || 0.0);
        let drawn = |ed: &mut Editor| ed.render_objects().into_iter().find(|o| o.id == b).map(|o| o.kind);
        // A Boolean that isn't built yet goes elsewhere at once.
        ed.scene.get_mut(s).unwrap().pos = Vec3::new(70.0, 60.0, 0.0);
        drawn(&mut ed);
        assert_eq!(jobs.borrow().len(), 1);
        // Asking again doesn't send it twice.
        drawn(&mut ed);
        assert_eq!(jobs.borrow().len(), 1);
        let job = jobs.borrow_mut().remove(0);
        assert_eq!(job.scene.roots(), &[b][..]);
        let mesh = build_job_mesh(&job.scene, job.object);
        let tris = mesh.as_ref().map(|m| m.indices.len()).unwrap_or(0);
        assert!(tris > 0);
        assert!(ed.meshes.finish(job.key, job.object, mesh, 50.0));
        let shown = ed.render_objects().into_iter().find(|o| o.id == b).unwrap();
        assert_eq!(shown.mesh.indices.len(), tris);
        // A change: the previous mesh stays up while the new one is built.
        ed.scene.get_mut(s).unwrap().pos = Vec3::new(90.0, 60.0, 0.0);
        let shown = ed.render_objects().into_iter().find(|o| o.id == b).unwrap();
        assert_eq!(shown.mesh.indices.len(), tris);
        assert_eq!(jobs.borrow().len(), 1);
        // A result for an older state is ignored.
        assert!(!ed.meshes.finish(job.key, job.object, None, 50.0));
        // Final renders never wait: they build it here.
        let finals = ed.final_objects();
        assert!(finals.iter().any(|o| o.id == b));
    }

    fn editor() -> Editor {
        Editor::new(Scene::demo())
    }

    #[test]
    fn model_mode_scaling_resizes_geometry_and_object_mode_scales_the_object() {
        use crate::gizmo::{AxisEnabled, DragView, GizmoFrame, Handle};
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        let child = ed.scene.add_primitive(PrimitiveKind::Sphere, Some(cube));
        ed.scene.get_mut(child).unwrap().pos = Vec3::new(0.0, 100.0, 0.0);
        let start = |ed: &Editor| {
            let frame = GizmoFrame::new(&ed.scene, cube, true, 1.0);
            let view = DragView { forward: Vec3::Z, right: Vec3::X, up: Vec3::Y };
            Drag::begin(&ed.scene, cube, Tool::Scale, Handle::Free, frame, view, AxisEnabled::default(), (Vec3::new(0.0, 0.0, -500.0), Vec3::Z), glam::Vec2::ZERO).unwrap()
        };
        // Model mode: the drag doubles the scale, release bakes it into the size.
        ed.drag = Some(start(&ed));
        ed.scene.get_mut(cube).unwrap().scale = Vec3::splat(2.0);
        ed.finish_drag();
        let o = ed.scene.get(cube).unwrap();
        assert_eq!(o.scale, Vec3::ONE);
        match o.kind {
            ObjectKind::Primitive(Primitive::Cube { size, .. }) => assert!((size - Vec3::splat(400.0)).length() < 1e-3),
            _ => panic!("still a cube"),
        }
        assert!((ed.scene.get(child).unwrap().pos.y - 200.0).abs() < 1e-3, "children move with the geometry");
        // Object mode: the scale stays on the object.
        ed.set_mode(EditMode::Object);
        ed.drag = Some(start(&ed));
        ed.scene.get_mut(cube).unwrap().scale = Vec3::splat(0.5);
        ed.finish_drag();
        assert_eq!(ed.scene.get(cube).unwrap().scale, Vec3::splat(0.5));
        // A sphere squashed in Model mode can't take it as parameters: the scale stays.
        let sphere = ed.create_primitive(PrimitiveKind::Sphere);
        ed.scene.get_mut(sphere).unwrap().scale = Vec3::new(1.0, 0.5, 1.0);
        assert!(!ed.bake_scale(sphere, Vec3::ONE));
        assert_eq!(ed.scene.get(sphere).unwrap().scale, Vec3::new(1.0, 0.5, 1.0));
    }

    #[test]
    fn axis_mode_moves_the_axis_but_not_the_points_or_children() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.create_primitive(PrimitiveKind::Cube);
        assert!(!ed.begin_axis_drag(id), "parametric objects must be made editable first");
        assert!(ed.make_editable());
        let id = ed.active().unwrap();
        let kid = ed.scene.add_null(Some(id));
        ed.scene.get_mut(kid).unwrap().pos = Vec3::new(50.0, 0.0, 0.0);
        let world_points = |ed: &Editor| {
            let w = ed.scene.world_matrix(id);
            mesh_of(&ed.scene, id).unwrap().points.iter().map(|p| w.transform_point3(*p)).collect::<Vec<_>>()
        };
        let before = world_points(&ed);
        let kid_before = ed.scene.world_position(kid);
        assert!(ed.begin_axis_drag(id));
        ed.scene.get_mut(id).unwrap().pos = Vec3::new(100.0, 40.0, 0.0);
        ed.scene.get_mut(id).unwrap().rot = Vec3::new(30.0, 0.0, 0.0);
        ed.apply_axis_compensation();
        for (a, b) in before.iter().zip(world_points(&ed)) {
            assert!((*a - b).length() < 1e-3);
        }
        assert!((ed.scene.world_position(kid) - kid_before).length() < 1e-3);
        // Center Axis puts it back in the middle of the points.
        ed.axis_base = None;
        ed.center_axis();
        assert!(ed.scene.world_position(id).length() < 1e-3);
        for (a, b) in before.iter().zip(world_points(&ed)) {
            assert!((*a - b).length() < 1e-3);
        }
    }

    #[test]
    fn workplane_aligns_to_axes_and_selection() {
        let mut ed = Editor::new(Scene::new());
        for (axis, normal) in [(0, Vec3::X), (1, Vec3::Y), (2, Vec3::Z)] {
            ed.align_workplane_to_axis(axis);
            assert!((ed.scene.workplane.normal().abs() - normal).length() < 1e-4, "axis {axis}: {:?}", ed.scene.workplane.normal());
        }
        let id = ed.create_primitive(PrimitiveKind::Cube);
        ed.scene.get_mut(id).unwrap().pos = Vec3::new(10.0, 20.0, 30.0);
        ed.align_workplane_to_selection();
        assert_eq!(ed.scene.workplane.pos, Vec3::new(10.0, 20.0, 30.0));
        ed.align_workplane_to_axis(1);
        assert!(ed.scene.workplane.is_default());
    }

    #[test]
    fn selection_active_is_last_and_survives_pruning() {
        let mut ed = editor();
        let ids: Vec<_> = ed.scene.roots().to_vec();
        ed.select_only(ids[0]);
        ed.add_to_selection(ids[1]);
        assert_eq!(ed.active(), Some(ids[1]));
        ed.scene.remove(&[ids[1]]);
        ed.prune_selection();
        assert_eq!(ed.active(), Some(ids[0]));
        ed.toggle_selection(ids[0]);
        assert_eq!(ed.active(), None);
    }

    #[test]
    fn create_delete_roundtrip_with_history() {
        let mut ed = Editor::new(Scene::new());
        ed.create_primitive(PrimitiveKind::Cube);
        ed.commit_history(false);
        assert_eq!(ed.scene.len(), 1);
        assert_eq!(ed.selection.len(), 1);
        ed.delete_selection();
        ed.commit_history(false);
        assert!(ed.scene.is_empty() && ed.selection.is_empty());
        ed.undo();
        assert_eq!(ed.scene.len(), 1);
        ed.undo();
        assert!(ed.scene.is_empty());
        ed.redo();
        ed.redo();
        assert!(ed.scene.is_empty());
    }

    #[test]
    fn render_objects_respect_visibility_and_selection() {
        let mut ed = editor();
        let ids = ed.scene.roots().to_vec();
        ed.select_only(ids[1]);
        ed.scene.get_mut(ids[0]).unwrap().vis_editor = crate::scene::Visibility::Off;
        let objs = ed.render_objects();
        assert_eq!(objs.len(), 2, "hidden object must not be drawn");
        let sel = objs.iter().find(|o| o.id == ids[1]).unwrap();
        assert_eq!(sel.selection, SelectionState::Active);
        assert!(objs.iter().filter(|o| o.selection != SelectionState::None).count() == 1);
    }

    #[test]
    fn look_through_follows_and_moves_the_camera() {
        let mut ed = Editor::new(Scene::new());
        let cam = ed.create_camera(crate::scene_camera::CameraKind::Standard);
        ed.scene.get_mut(cam).unwrap().pos = Vec3::new(0.0, 0.0, -500.0);
        ed.scene.get_mut(cam).unwrap().rot = Vec3::ZERO;
        ed.look_through = Some(cam);
        ed.follow_look_through(1.5);
        assert!((ed.cameras[0].eye() - Vec3::new(0.0, 0.0, -500.0)).length() < 0.5);
        assert!(!ed.follow_look_through(1.5), "steady when nothing changes");
        // Navigating the view moves the camera object.
        ed.cameras[0].target += Vec3::new(100.0, 0.0, 0.0);
        ed.follow_look_through(1.5);
        assert!((ed.scene.world_position(cam).x - 100.0).abs() < 0.5);
        ed.scene.remove(&[cam]);
        assert!(!ed.follow_look_through(1.5));
        assert_eq!(ed.look_through, None);
    }

    #[test]
    fn material_colour_overrides_default() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.scene.add_primitive(PrimitiveKind::Cube, None);
        assert_eq!(ed.render_objects()[0].color, DEFAULT_COLOR);
        let m = ed.new_material();
        ed.assign_material(&[id], Some(m));
        let expected = ed.scene.material(m).unwrap().color;
        assert_eq!(ed.render_objects()[0].color, expected);
    }

    #[test]
    fn mesh_cache_reuses_and_prunes() {
        let mut cache = MeshCache::default();
        let p = PrimitiveKind::Sphere.default_primitive();
        let a = cache.get(&p);
        let b = cache.get(&p);
        assert!(Arc::ptr_eq(&a, &b));
        cache.end_frame();
        assert_eq!(cache.len(), 1, "used this frame, so it stays");
        cache.end_frame();
        assert_eq!(cache.len(), 0, "unused for a frame, so it is dropped");
    }

    #[test]
    fn frame_all_encloses_the_scene() {
        let mut ed = editor();
        ed.frame_all(ViewKind::Perspective);
        let cam = ed.cameras[0];
        let objs = ed.render_objects();
        for o in &objs {
            let (lo, hi) = o.world_bounds();
            for corner in [lo, hi] {
                let ndc = cam.project(corner, 1.0).unwrap();
                assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{corner:?} off screen: {ndc:?}");
            }
        }
    }

    #[test]
    fn frame_selected_centres_on_the_selection() {
        let mut ed = editor();
        let ids = ed.scene.roots().to_vec();
        ed.select_only(ids[2]);
        ed.frame_selected(ViewKind::Front);
        let target = ed.cameras[ViewKind::Front.index()].target;
        let expected = ed.scene.world_position(ids[2]);
        assert!((target - expected).length() < 60.0, "{target:?} vs {expected:?}");
    }

    #[test]
    fn save_and_open_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("forma3d-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.json");
        let mut ed = editor();
        ed.group_selection();
        ed.select_all();
        ed.group_selection();
        ed.save_to(&path).unwrap();
        assert!(!ed.is_dirty());
        let expected = ed.scene.clone();

        let mut other = Editor::new(Scene::new());
        other.open_from(&path).unwrap();
        assert_eq!(other.scene, expected);
        assert!(!other.is_dirty());
        assert_eq!(other.file_path.as_deref(), Some(path.as_path()));
        assert!(other.title().starts_with("scene.json - "));

        // Garbage on disk is rejected without touching the open scene.
        std::fs::write(&path, "{ definitely not a scene").unwrap();
        let before = other.scene.clone();
        assert!(other.open_from(&path).is_err());
        assert_eq!(other.scene, before);
        assert!(other.open_from(&dir.join("missing.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generator_consumes_selected_objects() {
        let mut ed = editor();
        let ids = ed.scene.roots().to_vec();
        let before = ed.scene.world_position(ids[1]);
        let clay = ed.scene.materials()[1].id;
        ed.scene.get_mut(ids[1]).unwrap().material = Some(clay);
        ed.select_only(ids[1]);
        let gen_id = ed.create_generator(GeneratorKind::Array);
        assert_eq!(ed.scene.get(ids[1]).unwrap().parent, Some(gen_id));
        assert!((ed.scene.world_position(ids[1]) - before).length() < 1e-3);
        let objs = ed.render_objects();
        assert!(objs.iter().all(|o| o.id != ids[1]), "inputs are not drawn on their own");
        let drawn = objs.iter().find(|o| o.id == gen_id).unwrap();
        assert_eq!(drawn.kind, ItemKind::Surface);
        assert!(!drawn.mesh.indices.is_empty());
        let sphere_mat = ed.scene.get(ids[1]).unwrap().material.unwrap();
        assert_eq!(drawn.color, ed.scene.material(sphere_mat).unwrap().color, "inherits the input's material");
        // Undo-friendly: an edit to the child changes the generated mesh, via a new cache key.
        let first = drawn.mesh.id;
        ed.scene.get_mut(ids[1]).unwrap().pos.y += 10.0;
        let second = ed.render_objects().iter().find(|o| o.id == gen_id).unwrap().mesh.id;
        assert_ne!(first, second);
    }

    #[test]
    fn deformer_bends_its_parent_and_shows_a_cage() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        if let Some(ObjectKind::Primitive(crate::primitive::Primitive::Cube { segments, .. })) =
            ed.scene.get_mut(cube).map(|o| &mut o.kind)
        {
            *segments = [1, 10, 1];
        }
        let plain = ed.render_objects()[0].world_bounds();
        let bend = ed.create_deformer(DeformerKind::Bend);
        assert_eq!(ed.scene.get(bend).unwrap().parent, Some(cube));
        // Fitted: box spans the cube, origin at its bottom.
        let d = ed.scene.get(bend).unwrap();
        assert!((d.pos - Vec3::new(0.0, -100.0, 0.0)).length() < 1e-3);
        let ObjectKind::Deformer(def) = d.kind else { panic!() };
        assert!((def.size() - Vec3::splat(200.0)).length() < 1e-3);

        let objs = ed.render_objects();
        assert_eq!(objs.len(), 2);
        let bent = objs.iter().find(|o| o.id == cube).unwrap();
        assert_eq!(bent.kind, ItemKind::Surface);
        assert!(bent.world_bounds().1.x > plain.1.x + 20.0, "a 90 degree bend swings the top towards +X");
        let cage = objs.iter().find(|o| o.id == bend).unwrap();
        assert_eq!(cage.kind, ItemKind::LinesOnly);
        assert_eq!(cage.color, DEFORMER_COLOR);
    }

    #[test]
    fn deformer_under_a_null_consumes_its_children() {
        let mut ed = Editor::new(Scene::new());
        let null = ed.create_null();
        let a = ed.scene.add_primitive(PrimitiveKind::Sphere, Some(null));
        let twist = ed.create_deformer(DeformerKind::Twist);
        assert_eq!(ed.scene.get(twist).unwrap().parent, Some(null));
        let objs = ed.render_objects();
        assert!(objs.iter().all(|o| o.id != a), "children are drawn through the Null");
        assert_eq!(objs.iter().find(|o| o.id == null).unwrap().kind, ItemKind::Surface);
        // Deleting the deformer restores normal drawing.
        ed.select_only(twist);
        ed.delete_selection();
        let objs = ed.render_objects();
        assert!(objs.iter().any(|o| o.id == a));
    }

    #[test]
    fn deformers_feed_into_generators() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        ed.create_deformer(DeformerKind::Twist);
        ed.select_only(cube);
        let sds = ed.create_generator(GeneratorKind::Subdivision);
        let objs = ed.render_objects();
        assert!(objs.iter().any(|o| o.id == sds && o.kind == ItemKind::Surface));
        assert!(objs.iter().all(|o| o.id != cube));
    }

    #[test]
    fn boolean_takes_its_colour_from_a_only() {
        let mut ed = Editor::new(Scene::demo());
        let a = ed.create_primitive(PrimitiveKind::Cube);
        let sphere = ed.scene.roots()[1];
        ed.add_to_selection(sphere);
        let b = ed.create_generator(GeneratorKind::Boolean);
        assert_eq!(ed.scene.children_of(Some(b)), &[a, sphere]);
        let drawn = ed.render_objects().into_iter().find(|o| o.id == b).unwrap();
        assert_eq!(drawn.color, DEFAULT_COLOR, "A has no material, so B's must not leak in");
    }

    #[test]
    fn make_editable_converts_and_keeps_deformers() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        let bend = ed.create_deformer(DeformerKind::Bend);
        ed.commit_history(false);
        ed.select_only(cube);
        assert!(ed.make_editable());
        let m = mesh_of(&ed.scene, cube).unwrap();
        assert_eq!((m.points.len(), m.polys.len()), (8, 6));
        assert_eq!(ed.scene.get(bend).unwrap().parent, Some(cube), "deformers stay live");
        // Undo restores the primitive.
        ed.commit_history(false);
        ed.undo();
        assert!(matches!(ed.scene.get(cube).unwrap().kind, ObjectKind::Primitive(_)));
    }

    #[test]
    fn make_editable_bakes_generators() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        let arr = ed.create_generator(GeneratorKind::Array);
        assert!(ed.make_editable());
        assert!(!ed.scene.contains(cube), "inputs are baked in");
        let m = mesh_of(&ed.scene, arr).unwrap();
        assert_eq!(m.polys.len(), 18);
        assert!(!ed.make_editable(), "already editable");
    }

    #[test]
    fn component_selection_follows_the_active_object_and_survives_undo() {
        let mut ed = Editor::new(Scene::new());
        ed.create_primitive(PrimitiveKind::Cube);
        ed.make_editable();
        ed.commit_history(false);
        ed.set_mode(EditMode::Polygons);
        ed.prune_selection();
        ed.comp_select_all();
        assert_eq!(ed.comp.polys.len(), 6);
        ed.delete_components();
        ed.commit_history(false);
        assert_eq!(mesh_of(&ed.scene, ed.active().unwrap()).unwrap().polys.len(), 0);
        ed.undo();
        ed.prune_selection();
        assert_eq!(mesh_of(&ed.scene, ed.active().unwrap()).unwrap().polys.len(), 6);
        // A different active object clears the component selection.
        ed.comp_select_all();
        ed.create_primitive(PrimitiveKind::Sphere);
        ed.prune_selection();
        assert!(ed.comp.polys.is_empty() && ed.edit_target().is_none());
    }

    #[test]
    fn extrude_drag_reapplies_from_the_base_mesh() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.create_primitive(PrimitiveKind::Cube);
        ed.make_editable();
        ed.set_mesh_tool(MeshTool::Extrude);
        assert_eq!(ed.mode, EditMode::Polygons);
        ed.prune_selection();
        ed.comp.polys.insert(0);
        let base = mesh_of(&ed.scene, id).unwrap().clone();
        ed.tool_drag = Some(ToolDrag { id, base, polys: ed.comp.polys.clone(), edges: BTreeSet::new(), start_x: 0.0, world_per_px: 1.0, amount: 0.0 });
        ed.apply_tool_drag(10.0);
        ed.apply_tool_drag(40.0);
        let m = mesh_of(&ed.scene, id).unwrap();
        assert_eq!(m.polys.len(), 10, "one extrusion, not two");
    }

    #[test]
    fn polygon_objects_save_and_reject_broken_meshes() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.create_primitive(PrimitiveKind::Cube);
        ed.make_editable();
        let json = ed.scene.to_json().unwrap();
        let back = Scene::from_json(&json).unwrap();
        assert_eq!(back, ed.scene);
        // A polygon pointing at a missing point is refused on load.
        if let Some(m) = mesh_of_mut(&mut ed.scene, id) {
            m.polys[0][0] = 999;
        }
        let err = Scene::from_json(&ed.scene.to_json().unwrap()).unwrap_err();
        assert!(err.contains("missing point"), "{err}");
    }

    #[test]
    fn mesh_commands_through_the_editor() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.create_primitive(PrimitiveKind::Cube);
        ed.make_editable();
        ed.prune_selection();
        // Bevel drag on one edge.
        ed.set_mesh_tool(MeshTool::Bevel);
        assert_eq!(ed.mode, EditMode::Edges);
        let e = mesh_of(&ed.scene, id).unwrap().edges()[0];
        let base = mesh_of(&ed.scene, id).unwrap().clone();
        ed.tool_drag = Some(ToolDrag { id, base, polys: BTreeSet::new(), edges: BTreeSet::from([e]), start_x: 0.0, world_per_px: 1.0, amount: 0.0 });
        ed.apply_tool_drag(10.0);
        ed.apply_tool_drag(20.0);
        assert_eq!(mesh_of(&ed.scene, id).unwrap().polys.len(), 7);
        ed.tool_drag = None;
        // Knife through the middle, in world space (object moved up 100 cm).
        ed.scene.get_mut(id).unwrap().pos.y = 100.0;
        ed.knife_cut(Vec3::new(0.0, 100.0, 0.0), Vec3::Y);
        assert!(ed.status.starts_with("Knife: split"), "{}", ed.status);
        // Subdivide and weld.
        let before = mesh_of(&ed.scene, id).unwrap().polys.len();
        ed.subdivide(true);
        assert!(mesh_of(&ed.scene, id).unwrap().polys.len() > before);
        ed.set_mode(EditMode::Points);
        ed.comp.points = BTreeSet::from([0, 1]);
        ed.weld();
        assert!(ed.status.starts_with("Welded"));
        // Bridge without a valid selection explains itself instead of failing silently.
        ed.set_mode(EditMode::Polygons);
        ed.comp.polys = BTreeSet::from([0]);
        ed.bridge();
        assert!(ed.status.contains("two"), "{}", ed.status);
    }

    #[test]
    fn switched_off_generators_and_deformers() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        let sds = ed.create_generator(GeneratorKind::Subdivision);
        ed.scene.get_mut(sds).unwrap().enabled = false;
        let objs = ed.render_objects();
        assert!(objs.iter().any(|o| o.id == cube), "the input is drawn on its own");
        assert_eq!(objs.iter().find(|o| o.id == sds).unwrap().kind, ItemKind::LinesOnly);
        ed.scene.get_mut(sds).unwrap().enabled = true;
        assert!(ed.render_objects().iter().all(|o| o.id != cube));

        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        let bend = ed.create_deformer(DeformerKind::Bend);
        let bent = ed.object_size(cube).unwrap();
        ed.scene.get_mut(bend).unwrap().enabled = false;
        assert!((ed.object_size(cube).unwrap() - Vec3::splat(200.0)).length() < 1e-3);
        assert!(bent.x > 200.5, "the bend changed the bounds while on: {bent:?}");
        let cage = ed.render_objects().into_iter().find(|o| o.id == bend).unwrap();
        assert_eq!(cage.color, DISABLED_COLOR);
        // Old files without the flag load as enabled.
        fn strip(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(m) => {
                    m.remove("enabled");
                    m.values_mut().for_each(strip);
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(strip),
                _ => {}
            }
        }
        let mut value: serde_json::Value = serde_json::from_str(&ed.scene.to_json().unwrap()).unwrap();
        strip(&mut value);
        assert!(!value.to_string().contains("enabled"));
        let back = Scene::from_json(&value.to_string()).unwrap();
        assert!(back.get(bend).unwrap().enabled);
    }

    #[test]
    fn coordinate_manager_size_helpers() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(PrimitiveKind::Cube);
        assert!((ed.object_size(cube).unwrap() - Vec3::splat(200.0)).length() < 1e-3);
        ed.set_object_size(cube, Vec3::new(400.0, 100.0, 200.0));
        assert!((ed.scene.get(cube).unwrap().scale - Vec3::new(2.0, 0.5, 1.0)).length() < 1e-5);
        assert!((ed.object_size(cube).unwrap() - Vec3::new(400.0, 100.0, 200.0)).length() < 1e-2);

        ed.make_editable();
        ed.set_mode(EditMode::Points);
        ed.prune_selection();
        ed.comp_select_all();
        let (lo, hi) = ed.component_bounds().unwrap();
        assert!((hi - lo - Vec3::new(400.0, 100.0, 200.0)).length() < 1e-2);
        ed.set_component_bounds(Vec3::new(10.0, 0.0, 0.0), Vec3::new(200.0, 100.0, 200.0));
        let (lo, hi) = ed.component_bounds().unwrap();
        assert!((hi - lo - Vec3::new(200.0, 100.0, 200.0)).length() < 1e-2);
        assert!(((lo + hi) * 0.5 - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-2);
    }

    #[test]
    fn viewport_filter_and_solo() {
        let mut ed = Editor::new(Scene::demo());
        let null = ed.create_null();
        assert!(ed.passes_filter(null));
        ed.view_opts.nulls = false;
        assert!(!ed.passes_filter(null));
        ed.view_opts = ViewOptions { solo: true, ..ViewOptions::default() };
        let roots = ed.scene.roots().to_vec();
        ed.select_only(roots[0]);
        assert!(ed.passes_filter(roots[0]) && !ed.passes_filter(roots[1]));
    }

    #[test]
    fn invert_and_child_selection() {
        let mut ed = Editor::new(Scene::demo());
        let roots = ed.scene.roots().to_vec();
        ed.select_only(roots[0]);
        ed.invert_selection();
        assert_eq!(ed.selection.len(), roots.len() - 1);
        assert!(!ed.is_selected(roots[0]));
        ed.select_only(roots[0]);
        let g = ed.create_generator(GeneratorKind::Array);
        ed.select_only(g);
        ed.select_children();
        assert!(ed.is_selected(roots[0]) && ed.active() == Some(g), "the active object stays active");
    }

    #[test]
    fn timeline_playback_wraps_both_ways() {
        let mut t = Timeline { playing: true, ..Timeline::default() };
        t.frame = 89;
        t.tick(2.0 / 30.0);
        assert_eq!(t.frame, 0, "89 -> 90 -> wraps to 0");
        t.reverse = true;
        t.tick(1.0 / 30.0);
        assert_eq!(t.frame, 90);
        t.set_range(10, 5);
        assert!(t.min < t.max && (t.min..=t.max).contains(&t.frame));
    }

    #[test]
    fn empty_generator_draws_as_a_marker() {
        let mut ed = Editor::new(Scene::new());
        ed.create_generator(GeneratorKind::Symmetry);
        let objs = ed.render_objects();
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].kind, ItemKind::LinesOnly);
    }

    #[test]
    fn dirty_flag_tracks_edits() {
        let mut ed = editor();
        assert!(!ed.is_dirty());
        ed.create_null();
        assert!(ed.is_dirty());
        assert!(ed.title().contains('*'));
    }

    #[test]
    fn hsv_conversion_hits_the_primaries() {
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(near(hsv_to_rgb(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]));
        assert!(near(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), [0.0, 1.0, 0.0]));
        assert!(near(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), [0.0, 0.0, 1.0]));
    }
}

/// A sky as the lights the viewport understands: ambient light (a Dome) and,
/// for a physical sky, its sun.
pub fn sky_lights(s: &crate::sky::Sky) -> Vec<(crate::light::Light, Mat4)> {
    use crate::light::{Light, LightKind};
    let s = s.sanitized();
    let amb = s.ambient();
    let peak = amb.max_element().max(1e-4);
    let mut out = vec![(
        Light { kind: LightKind::Dome, color: (amb / peak).to_array(), intensity: s.intensity * peak * 0.8, ..LightKind::Dome.default_light() },
        Mat4::IDENTITY,
    )];
    if s.kind == crate::sky::SkyKind::Physical && s.sun && s.sun_direction().y > -0.05 {
        let dir = -s.sun_direction();
        let rot = crate::scene::hpb_to_quat(crate::light::aim_hpb(dir));
        out.push((Light { kind: LightKind::Sun, intensity: s.sun_intensity * 1.1, ..LightKind::Sun.default_light() }, Mat4::from_quat(rot)));
    }
    out
}
