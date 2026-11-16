//! Component editing: which points / edges / polygons are selected, picking
//! them on screen, and transforming them with the gizmo.

use crate::editmesh::{Edge, EditMesh};
use crate::generator::edge_key;
use crate::gizmo::{Axis, AxisEnabled, DragView, GizmoFrame, Handle, Tool, closest_param_on_line, ray_plane, segment_distance, wrap_angle};
use crate::picking::ray_triangle;
use crate::scene::{ObjectId, ObjectKind, Scene};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditMode {
    /// Whole objects; scaling resizes their geometry (parameters or points),
    /// leaving the object's own scale at 1 where it can.
    Model,
    /// Whole objects; scaling changes the object's scale, so its children
    /// are scaled with it.
    Object,
    Points,
    Edges,
    Polygons,
    /// The tools move and rotate the workplane.
    Workplane,
}

impl EditMode {
    pub const ALL: [EditMode; 6] =
        [EditMode::Model, EditMode::Object, EditMode::Points, EditMode::Edges, EditMode::Polygons, EditMode::Workplane];

    pub fn label(self) -> &'static str {
        match self {
            EditMode::Model => "Model",
            EditMode::Object => "Object",
            EditMode::Points => "Points",
            EditMode::Edges => "Edges",
            EditMode::Polygons => "Polygons",
            EditMode::Workplane => "Workplane",
        }
    }

    pub fn is_component(self) -> bool {
        matches!(self, EditMode::Points | EditMode::Edges | EditMode::Polygons)
    }
}

/// How a click changes the selection (Cinema 4D: Shift adds, Ctrl removes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectOp {
    Replace,
    Add,
    Remove,
}

/// The component selection of the object being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompSel {
    pub object: Option<ObjectId>,
    pub points: BTreeSet<u32>,
    pub edges: BTreeSet<Edge>,
    pub polys: BTreeSet<u32>,
}

impl CompSel {
    pub fn clear(&mut self) {
        self.points.clear();
        self.edges.clear();
        self.polys.clear();
    }

    pub fn is_empty_for(&self, mode: EditMode) -> bool {
        match mode {
            EditMode::Model | EditMode::Object | EditMode::Workplane => true,
            EditMode::Points => self.points.is_empty(),
            EditMode::Edges => self.edges.is_empty(),
            EditMode::Polygons => self.polys.is_empty(),
        }
    }

    pub fn count(&self, mode: EditMode) -> usize {
        match mode {
            EditMode::Model | EditMode::Object | EditMode::Workplane => 0,
            EditMode::Points => self.points.len(),
            EditMode::Edges => self.edges.len(),
            EditMode::Polygons => self.polys.len(),
        }
    }

    /// Points affected by a transform in the given mode.
    pub fn affected_points(&self, mesh: &EditMesh, mode: EditMode) -> BTreeSet<u32> {
        match mode {
            EditMode::Model | EditMode::Object | EditMode::Workplane => BTreeSet::new(),
            EditMode::Points => self.points.clone(),
            EditMode::Edges => EditMesh::points_of_edges(&self.edges),
            EditMode::Polygons => mesh.points_of_polys(&self.polys),
        }
    }

    /// Drops indices that no longer exist (after undo, delete, ...).
    pub fn prune(&mut self, mesh: &EditMesh) {
        let np = mesh.points.len() as u32;
        let npoly = mesh.polys.len() as u32;
        self.points.retain(|&p| p < np);
        self.polys.retain(|&p| p < npoly);
        if !self.edges.is_empty() {
            let all: BTreeSet<Edge> = mesh.edges().into_iter().collect();
            self.edges.retain(|e| all.contains(e));
        }
    }

    pub fn apply_points(&mut self, picked: impl IntoIterator<Item = u32>, op: SelectOp) {
        apply(&mut self.points, picked, op);
    }

    pub fn apply_edges(&mut self, picked: impl IntoIterator<Item = Edge>, op: SelectOp) {
        apply(&mut self.edges, picked.into_iter().map(|(a, b)| edge_key(a, b)), op);
    }

    pub fn apply_polys(&mut self, picked: impl IntoIterator<Item = u32>, op: SelectOp) {
        apply(&mut self.polys, picked, op);
    }
}

fn apply<T: Ord + Copy>(set: &mut BTreeSet<T>, picked: impl IntoIterator<Item = T>, op: SelectOp) {
    if op == SelectOp::Replace {
        set.clear();
    }
    for x in picked {
        if op == SelectOp::Remove {
            set.remove(&x);
        } else {
            set.insert(x);
        }
    }
}

/// The editable mesh of `id`, if it is a polygon object.
pub fn mesh_of(scene: &Scene, id: ObjectId) -> Option<&EditMesh> {
    match &scene.get(id)?.kind {
        ObjectKind::Polygon(m) => Some(m),
        _ => None,
    }
}

pub fn mesh_of_mut(scene: &mut Scene, id: ObjectId) -> Option<&mut EditMesh> {
    match &mut scene.get_mut(id)?.kind {
        ObjectKind::Polygon(m) => Some(m),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Picking
// ---------------------------------------------------------------------------

/// Everything picking needs to know about the view and the object.
pub struct PickView<'a> {
    pub mesh: &'a EditMesh,
    pub world: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub ortho: bool,
    /// World point -> screen pixel.
    pub project: &'a dyn Fn(Vec3) -> Option<Vec2>,
}

impl PickView<'_> {
    /// Front-facing polygons. Points and edges count as visible when a
    /// polygon around them is (back-face culling, not full occlusion).
    pub fn facing(&self) -> Vec<bool> {
        let normal_m = glam::Mat3::from_mat4(self.world).inverse().transpose();
        (0..self.mesh.polys.len())
            .map(|i| {
                let c = self.world.transform_point3(self.mesh.poly_center(i));
                let n = normal_m * self.mesh.poly_area_normal(i);
                let to_eye = if self.ortho { -self.forward } else { self.eye - c };
                n.dot(to_eye) > 0.0
            })
            .collect()
    }

    pub fn visible_points(&self, facing: &[bool]) -> Vec<bool> {
        let mut vis = vec![false; self.mesh.points.len()];
        let mut used = vec![false; self.mesh.points.len()];
        for (i, poly) in self.mesh.polys.iter().enumerate() {
            for &p in poly {
                used[p as usize] = true;
                if facing[i] {
                    vis[p as usize] = true;
                }
            }
        }
        // Loose points are always shown.
        for (v, u) in vis.iter_mut().zip(&used) {
            if !*u {
                *v = true;
            }
        }
        vis
    }

    pub fn visible_edges(&self, facing: &[bool]) -> Vec<Edge> {
        let mut out = BTreeSet::new();
        for (i, poly) in self.mesh.polys.iter().enumerate() {
            if facing[i] {
                for k in 0..poly.len() {
                    out.insert(edge_key(poly[k], poly[(k + 1) % poly.len()]));
                }
            }
        }
        out.into_iter().collect()
    }

    pub fn screen(&self, p: u32) -> Option<Vec2> {
        (self.project)(self.world.transform_point3(self.mesh.points[p as usize]))
    }

    pub fn pick_point(&self, mouse: Vec2, radius: f32) -> Option<u32> {
        let facing = self.facing();
        let vis = self.visible_points(&facing);
        (0..self.mesh.points.len() as u32)
            .filter(|&p| vis[p as usize])
            .filter_map(|p| Some((p, (self.screen(p)? - mouse).length())))
            .filter(|&(_, d)| d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p)
    }

    pub fn pick_edge(&self, mouse: Vec2, radius: f32) -> Option<(Edge, f32)> {
        let facing = self.facing();
        self.visible_edges(&facing)
            .into_iter()
            .filter_map(|e| {
                let (a, b) = (self.screen(e.0)?, self.screen(e.1)?);
                let d = segment_distance(mouse, a, b);
                // Fraction along the edge, for loop cuts.
                let ab = b - a;
                let t = if ab.length_squared() > 0.0 { ((mouse - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.5 };
                Some((e, t, d))
            })
            .filter(|&(_, _, d)| d <= radius)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(e, t, _)| (e, t))
    }

    /// Nearest polygon under the world-space ray.
    pub fn pick_poly(&self, ray: (Vec3, Vec3)) -> Option<u32> {
        let inv = self.world.inverse();
        if !inv.is_finite() {
            return None;
        }
        let ro = inv.transform_point3(ray.0);
        let rd = inv.transform_vector3(ray.1);
        let mut best: Option<(u32, f32)> = None;
        for i in 0..self.mesh.polys.len() {
            let poly = &self.mesh.polys[i];
            for t in self.mesh.triangulate(i) {
                let p = |k: usize| self.mesh.points[poly[k] as usize];
                if let Some(d) = ray_triangle(ro, rd, p(t[0]), p(t[1]), p(t[2]))
                    && best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((i as u32, d));
                    }
            }
        }
        best.map(|(i, _)| i)
    }

    /// Box selection: visible components entirely inside the screen rectangle.
    pub fn in_rect(&self, mode: EditMode, min: Vec2, max: Vec2) -> (Vec<u32>, Vec<Edge>, Vec<u32>) {
        let inside = |p: u32| self.screen(p).is_some_and(|s| s.x >= min.x && s.y >= min.y && s.x <= max.x && s.y <= max.y);
        let facing = self.facing();
        match mode {
            EditMode::Points => {
                let vis = self.visible_points(&facing);
                ((0..self.mesh.points.len() as u32).filter(|&p| vis[p as usize] && inside(p)).collect(), vec![], vec![])
            }
            EditMode::Edges => (vec![], self.visible_edges(&facing).into_iter().filter(|e| inside(e.0) && inside(e.1)).collect(), vec![]),
            EditMode::Polygons => (
                vec![],
                vec![],
                (0..self.mesh.polys.len())
                    .filter(|&i| facing[i] && self.mesh.polys[i].iter().all(|&p| inside(p)))
                    .map(|i| i as u32)
                    .collect(),
            ),
            EditMode::Model | EditMode::Object | EditMode::Workplane => (vec![], vec![], vec![]),
        }
    }
}

// ---------------------------------------------------------------------------
// Transforming components with the gizmo
// ---------------------------------------------------------------------------

/// World-space centre of the affected points (the gizmo position).
pub fn selection_center(mesh: &EditMesh, world: Mat4, points: &BTreeSet<u32>) -> Option<Vec3> {
    if points.is_empty() {
        return None;
    }
    let sum: Vec3 = points.iter().filter_map(|&p| mesh.points.get(p as usize)).map(|&p| world.transform_point3(p)).sum();
    Some(sum / points.len() as f32)
}

/// An in-progress move / rotate / scale of components. Like the object gizmo,
/// results are computed from the start state, so nothing drifts.
#[derive(Clone, Debug)]
pub struct ComponentDrag {
    pub id: ObjectId,
    pub tool: Tool,
    pub handle: Handle,
    frame: GizmoFrame,
    view: DragView,
    enabled: AxisEnabled,
    world: Mat4,
    pub quantize: crate::gizmo::Quantize,
    /// (point index, world position at the start)
    start: Vec<(u32, Vec3)>,
    start_param: f32,
    start_point: Vec3,
    start_mouse: Vec2,
}

impl ComponentDrag {
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        scene: &Scene,
        id: ObjectId,
        points: &BTreeSet<u32>,
        tool: Tool,
        handle: Handle,
        frame: GizmoFrame,
        view: DragView,
        enabled: AxisEnabled,
        ray: (Vec3, Vec3),
        mouse: Vec2,
    ) -> Option<ComponentDrag> {
        if tool == Tool::Select || points.is_empty() {
            return None;
        }
        let mesh = mesh_of(scene, id)?;
        let world = scene.world_matrix(id);
        if !world.inverse().is_finite() {
            return None;
        }
        let start = points.iter().filter_map(|&p| Some((p, world.transform_point3(*mesh.points.get(p as usize)?)))).collect();
        let mut d = ComponentDrag {
            id,
            tool,
            handle,
            frame,
            view,
            enabled,
            world,
            quantize: Default::default(),
            start,
            start_param: 0.0,
            start_point: frame.origin,
            start_mouse: mouse,
        };
        let (ro, rd) = ray;
        match (tool, handle) {
            (Tool::Move | Tool::Scale, Handle::Axis(a)) => d.start_param = closest_param_on_line(frame.origin, frame.axis(a), ro, rd)?,
            (Tool::Move, Handle::Free) => {
                let t = ray_plane(ro, rd, frame.origin, view.forward)?;
                d.start_point = ro + rd * t;
            }
            (Tool::Rotate, Handle::Axis(a)) => {
                let t = ray_plane(ro, rd, frame.origin, frame.axis(a))?;
                d.start_param = d.angle_of(a, ro + rd * t);
            }
            _ => {}
        }
        Some(d)
    }

    fn angle_of(&self, a: Axis, point: Vec3) -> f32 {
        let (u, v) = self.frame.ring_basis(a);
        let rel = point - self.frame.origin;
        rel.dot(v).atan2(rel.dot(u))
    }

    /// The world-space transform for the current pointer position.
    pub fn transform(&self, ray: (Vec3, Vec3), mouse: Vec2) -> Option<Mat4> {
        let (ro, rd) = ray;
        let o = self.frame.origin;
        let about = |m: Mat4| Mat4::from_translation(o) * m * Mat4::from_translation(-o);
        // Scale along the gizmo's axes: build in the gizmo basis.
        let basis = glam::Mat3::from_cols(self.frame.axes[0], self.frame.axes[1], self.frame.axes[2]);
        let scale_in_basis = |s: Vec3| {
            let b = Mat4::from_mat3(basis);
            about(b * Mat4::from_scale(s) * b.inverse())
        };
        match (self.tool, self.handle) {
            (Tool::Move, Handle::Axis(a)) => {
                let ax = self.frame.axis(a);
                let s = closest_param_on_line(o, ax, ro, rd)?;
                Some(Mat4::from_translation(ax * self.quantize.distance(s - self.start_param)))
            }
            (Tool::Move, Handle::Free) => {
                let t = ray_plane(ro, rd, o, self.view.forward)?;
                let delta = ro + rd * t - self.start_point;
                let mut allowed = Vec3::ZERO;
                for a in Axis::ALL {
                    if self.enabled.get(a) {
                        allowed += self.frame.axis(a) * self.quantize.distance(delta.dot(self.frame.axis(a)));
                    }
                }
                Some(Mat4::from_translation(allowed))
            }
            (Tool::Scale, Handle::Axis(a)) => {
                let s = closest_param_on_line(o, self.frame.axis(a), ro, rd)?;
                let min_start = 20.0 * self.frame.world_per_px;
                let denom = if self.start_param.abs() < min_start { min_start.copysign(self.start_param) } else { self.start_param };
                let mut v = Vec3::ONE;
                v[a.index()] = self.quantize.factor(s / denom);
                Some(scale_in_basis(v))
            }
            (Tool::Scale, Handle::Free) => {
                let f = self.quantize.factor(1.0 + (mouse.x - self.start_mouse.x) / 150.0);
                let mut v = Vec3::ONE;
                for a in Axis::ALL {
                    if self.enabled.get(a) {
                        v[a.index()] = f;
                    }
                }
                Some(scale_in_basis(v))
            }
            (Tool::Rotate, Handle::Axis(a)) => {
                let t = ray_plane(ro, rd, o, self.frame.axis(a))?;
                let delta = self.quantize.angle(wrap_angle(self.angle_of(a, ro + rd * t) - self.start_param));
                Some(about(Mat4::from_quat(Quat::from_axis_angle(self.frame.axis(a), delta))))
            }
            (Tool::Rotate, Handle::Free) => {
                let d = mouse - self.start_mouse;
                let k = 0.012;
                let q = Quat::from_axis_angle(self.view.up, self.quantize.angle(-d.x * k))
                    * Quat::from_axis_angle(self.view.right, self.quantize.angle(-d.y * k));
                Some(about(Mat4::from_quat(q)))
            }
            (Tool::Select, _) => None,
        }
    }

    pub fn update(&self, scene: &mut Scene, ray: (Vec3, Vec3), mouse: Vec2) {
        if let Some(t) = self.transform(ray, mouse) {
            self.apply(scene, t);
        }
    }

    /// The gizmo origin at the start (the selection's centre).
    pub fn origin(&self) -> Vec3 {
        self.frame.origin
    }

    /// Moves the selection so its centre lands on `world`, keeping to the
    /// handle's axis or the enabled axes (used by snapping).
    pub fn move_to(&self, scene: &mut Scene, world: Vec3) {
        let delta = world - self.frame.origin;
        let allowed: Vec3 = match self.handle {
            Handle::Axis(a) => self.frame.axis(a) * delta.dot(self.frame.axis(a)),
            Handle::Free => Axis::ALL
                .iter()
                .filter(|a| self.enabled.get(**a))
                .map(|a| self.frame.axis(*a) * delta.dot(self.frame.axis(*a)))
                .sum(),
        };
        self.apply(scene, Mat4::from_translation(allowed));
    }

    fn apply(&self, scene: &mut Scene, t: Mat4) {
        let inv = self.world.inverse();
        let Some(mesh) = mesh_of_mut(scene, self.id) else { return };
        for &(p, w) in &self.start {
            let local = inv.transform_point3(t.transform_point3(w));
            if let (true, Some(slot)) = (local.is_finite(), mesh.points.get_mut(p as usize)) {
                *slot = local;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, ViewKind};
    use crate::generator::Geo;
    use crate::primitive::PrimitiveKind;

    const VIEWPORT: Vec2 = Vec2::new(900.0, 600.0);

    fn setup() -> (Scene, ObjectId, Camera) {
        let mut scene = Scene::new();
        let mesh = EditMesh::from_geo(&Geo::from_mesh(&PrimitiveKind::Cube.default_primitive().build()));
        let id = scene.add_object("Poly", ObjectKind::Polygon(mesh), None);
        let mut cam = Camera::new(ViewKind::Perspective);
        cam.frame(Vec3::ZERO, 200.0);
        (scene, id, cam)
    }

    fn project(cam: &Camera) -> impl Fn(Vec3) -> Option<Vec2> + '_ {
        move |p| {
            let ndc = cam.project(p, VIEWPORT.x / VIEWPORT.y)?;
            Some(Vec2::new((ndc.x * 0.5 + 0.5) * VIEWPORT.x, (1.0 - (ndc.y * 0.5 + 0.5)) * VIEWPORT.y))
        }
    }

    fn view<'a>(scene: &'a Scene, id: ObjectId, cam: &Camera, proj: &'a dyn Fn(Vec3) -> Option<Vec2>) -> PickView<'a> {
        PickView {
            mesh: mesh_of(scene, id).unwrap(),
            world: scene.world_matrix(id),
            eye: cam.eye(),
            forward: cam.forward(),
            ortho: false,
            project: proj,
        }
    }

    #[test]
    fn picking_points_edges_and_polygons() {
        let (scene, id, cam) = setup();
        let proj = project(&cam);
        let v = view(&scene, id, &cam, &proj);
        let facing = v.facing();
        assert_eq!(facing.iter().filter(|&&f| f).count(), 3, "a corner-on view of a cube sees three faces");

        // The nearest corner to the camera is visible and pickable.
        let mesh = mesh_of(&scene, id).unwrap();
        let near = (0..8u32).min_by(|&a, &b| {
            let d = |p: u32| (mesh.points[p as usize] - cam.eye()).length();
            d(a).total_cmp(&d(b))
        }).unwrap();
        let s = v.screen(near).unwrap();
        assert_eq!(v.pick_point(s + Vec2::new(3.0, -2.0), 10.0), Some(near));
        // The farthest corner is hidden behind the cube.
        let far = (0..8u32).max_by(|&a, &b| {
            let d = |p: u32| (mesh.points[p as usize] - cam.eye()).length();
            d(a).total_cmp(&d(b))
        }).unwrap();
        assert!(!v.visible_points(&facing)[far as usize]);

        // Edge: the middle of a visible edge.
        let e = v.visible_edges(&facing)[0];
        let m = (v.screen(e.0).unwrap() + v.screen(e.1).unwrap()) * 0.5;
        let (picked, t) = v.pick_edge(m, 6.0).unwrap();
        assert_eq!(picked, e);
        assert!((t - 0.5).abs() < 0.05);

        // Polygon: a ray through the centre of the top face hits it.
        let top = (0..6).find(|&i| mesh.poly_normal(i).y > 0.9).unwrap();
        let c = mesh.poly_center(top);
        let ray = (cam.eye(), (c - cam.eye()).normalize());
        assert_eq!(v.pick_poly(ray), Some(top as u32));
    }

    #[test]
    fn box_select_and_select_ops() {
        let (scene, id, cam) = setup();
        let proj = project(&cam);
        let v = view(&scene, id, &cam, &proj);
        let (pts, _, _) = v.in_rect(EditMode::Points, Vec2::ZERO, VIEWPORT);
        assert_eq!(pts.len(), 7, "all but the hidden back corner");
        let (_, _, polys) = v.in_rect(EditMode::Polygons, Vec2::ZERO, VIEWPORT);
        assert_eq!(polys.len(), 3);

        let mut sel = CompSel::default();
        sel.apply_points([1, 2], SelectOp::Replace);
        sel.apply_points([3], SelectOp::Add);
        sel.apply_points([1], SelectOp::Remove);
        assert_eq!(sel.points, BTreeSet::from([2, 3]));
        sel.apply_edges([(5, 4)], SelectOp::Add);
        assert!(sel.edges.contains(&(4, 5)), "edges are stored normalised");
    }

    #[test]
    fn dragging_components_moves_only_them() {
        let (mut scene, id, cam) = setup();
        scene.get_mut(id).unwrap().pos = Vec3::new(100.0, 0.0, 0.0);
        scene.get_mut(id).unwrap().scale = Vec3::splat(2.0);
        let mesh = mesh_of(&scene, id).unwrap().clone();
        let top = (0..6).find(|&i| mesh.poly_normal(i).y > 0.9).unwrap() as u32;
        let pts = mesh.points_of_polys(&BTreeSet::from([top]));
        let world = scene.world_matrix(id);
        let centre = selection_center(&mesh, world, &pts).unwrap();
        assert!((centre - Vec3::new(100.0, 200.0, 0.0)).length() < 1e-3);

        let proj = project(&cam);
        let wpp = cam.world_per_pixel(centre, VIEWPORT.y);
        let frame = GizmoFrame { origin: centre, axes: [Vec3::X, Vec3::Y, Vec3::Z], world_per_px: wpp };
        let ray_at = |px: Vec2| cam.ray(Vec2::new(px.x / VIEWPORT.x * 2.0 - 1.0, 1.0 - px.y / VIEWPORT.y * 2.0), VIEWPORT.x / VIEWPORT.y);
        let start = proj(centre + Vec3::Y * frame.axis_len() * 0.7).unwrap();
        let view = DragView { forward: cam.forward(), right: cam.right(), up: cam.up() };
        let drag = ComponentDrag::begin(&scene, id, &pts, Tool::Move, Handle::Axis(Axis::Y), frame, view, AxisEnabled::default(), ray_at(start), start).unwrap();
        // Drag 50 cm (world) up the Y axis.
        let target = proj(centre + Vec3::Y * (frame.axis_len() * 0.7 + 50.0)).unwrap();
        drag.update(&mut scene, ray_at(target), target);
        let after = mesh_of(&scene, id).unwrap();
        for (i, (&a, &b)) in mesh.points.iter().zip(&after.points).enumerate() {
            if pts.contains(&(i as u32)) {
                // World +50 is local +25 under a scale of 2.
                assert!((b - a - Vec3::new(0.0, 25.0, 0.0)).length() < 0.05, "{a:?} -> {b:?}");
            } else {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn scale_and_rotate_transforms_pivot_on_the_selection() {
        let (scene, id, cam) = setup();
        let centre = Vec3::new(0.0, 100.0, 0.0);
        let frame = GizmoFrame { origin: centre, axes: [Vec3::X, Vec3::Y, Vec3::Z], world_per_px: 1.0 };
        let view = DragView { forward: cam.forward(), right: cam.right(), up: cam.up() };
        let pts = BTreeSet::from([0u32]);
        let ray = (cam.eye(), (centre - cam.eye()).normalize());
        let d = ComponentDrag::begin(&scene, id, &pts, Tool::Scale, Handle::Free, frame, view, AxisEnabled::default(), ray, Vec2::ZERO).unwrap();
        let t = d.transform(ray, Vec2::new(150.0, 0.0)).unwrap();
        assert!((t.transform_point3(centre) - centre).length() < 1e-4, "the pivot stays put");
        assert!((t.transform_point3(centre + Vec3::X) - (centre + Vec3::X * 2.0)).length() < 1e-4);
    }

    #[test]
    fn prune_drops_stale_indices() {
        let (scene, id, _) = setup();
        let mesh = mesh_of(&scene, id).unwrap();
        let mut sel = CompSel { points: BTreeSet::from([1, 99]), polys: BTreeSet::from([0, 42]), edges: BTreeSet::from([(0, 7)]), ..Default::default() };
        sel.prune(mesh);
        assert_eq!(sel.points, BTreeSet::from([1]));
        assert_eq!(sel.polys, BTreeSet::from([0]));
        assert!(sel.edges.iter().all(|e| mesh.edges().contains(e)));
    }
}
