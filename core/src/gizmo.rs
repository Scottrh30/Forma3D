//! Transform gizmos (Move / Rotate / Scale): geometry, hit testing and drag
//! maths. Nothing here touches egui or wgpu, so it is all unit-testable.

use crate::scene::{ObjectId, Scene, matrix_rotation, quat_to_hpb_near};
use glam::{Quat, Vec2, Vec3};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordSystem {
    /// Gizmo axes follow the object's orientation.
    Object,
    /// Gizmo axes are the world axes.
    World,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Axis(Axis),
    /// Dragging anywhere that is not an axis handle.
    Free,
}

/// Which axes the tools may affect (Cinema 4D's X/Y/Z lock toggles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisEnabled(pub [bool; 3]);

impl Default for AxisEnabled {
    fn default() -> Self {
        AxisEnabled([true; 3])
    }
}

impl AxisEnabled {
    pub fn get(&self, a: Axis) -> bool {
        self.0[a.index()]
    }
}

/// Quantizing (Cinema 4D's Quantize): moves, rotations and scales go in
/// fixed steps while it is on.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Quantize {
    pub enabled: bool,
    /// Move step in centimetres.
    pub move_step: f32,
    /// Rotation step in degrees.
    pub rotate_step: f32,
    /// Scale step in percent.
    pub scale_step: f32,
}

impl Default for Quantize {
    fn default() -> Self {
        Quantize { enabled: false, move_step: 10.0, rotate_step: 10.0, scale_step: 10.0 }
    }
}

impl Quantize {
    fn step(v: f32, step: f32) -> f32 {
        if step > 1e-6 && v.is_finite() { (v / step).round() * step } else { v }
    }

    /// A distance moved, in steps.
    pub fn distance(&self, d: f32) -> f32 {
        if self.enabled { Self::step(d, self.move_step) } else { d }
    }

    /// An angle in radians, in steps.
    pub fn angle(&self, a: f32) -> f32 {
        if self.enabled { Self::step(a.to_degrees(), self.rotate_step).to_radians() } else { a }
    }

    /// A scale factor (1 = unchanged), in steps.
    pub fn factor(&self, f: f32) -> f32 {
        if self.enabled {
            let q = Self::step((f - 1.0) * 100.0, self.scale_step) / 100.0 + 1.0;
            if q <= 0.0 { (self.scale_step / 100.0).max(0.001) } else { q }
        } else {
            f
        }
    }
}

pub const AXIS_LENGTH_PX: f32 = 110.0;
pub const RING_RADIUS_PX: f32 = 88.0;
pub const HIT_RADIUS_PX: f32 = 8.0;
const RING_SEGMENTS: usize = 72;

/// Where the gizmo sits and how big it is, in world space.
#[derive(Clone, Copy, Debug)]
pub struct GizmoFrame {
    pub origin: Vec3,
    pub axes: [Vec3; 3],
    pub world_per_px: f32,
}

impl GizmoFrame {
    /// `use_object_axes` selects the object's own orientation over world axes.
    pub fn new(scene: &Scene, id: ObjectId, use_object_axes: bool, world_per_px: f32) -> Self {
        let world = scene.world_matrix(id);
        let axes = if use_object_axes {
            let rot = matrix_rotation(world);
            [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        GizmoFrame {
            origin: world.w_axis.truncate(),
            axes,
            world_per_px,
        }
    }

    /// A gizmo on the workplane (Workplane mode).
    pub fn workplane(wp: &crate::scene::Workplane, use_object_axes: bool, world_per_px: f32) -> Self {
        let axes = if use_object_axes {
            let rot = crate::scene::hpb_to_quat(wp.rot);
            [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        GizmoFrame { origin: wp.pos, axes, world_per_px }
    }

    pub fn axis_len(&self) -> f32 {
        AXIS_LENGTH_PX * self.world_per_px
    }

    pub fn ring_radius(&self) -> f32 {
        RING_RADIUS_PX * self.world_per_px
    }

    pub fn axis(&self, a: Axis) -> Vec3 {
        self.axes[a.index()]
    }

    /// Two unit vectors spanning the plane perpendicular to `a`.
    pub fn ring_basis(&self, a: Axis) -> (Vec3, Vec3) {
        perpendicular_basis(self.axis(a))
    }

    /// World-space points of the rotation ring around `a`.
    pub fn ring_points(&self, a: Axis) -> Vec<Vec3> {
        let (u, v) = self.ring_basis(a);
        let r = self.ring_radius();
        (0..=RING_SEGMENTS)
            .map(|i| {
                let t = TAU * i as f32 / RING_SEGMENTS as f32;
                self.origin + (u * t.cos() + v * t.sin()) * r
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Two unit vectors `(u, v)` with `u x v == n` (for unit `n`).
pub fn perpendicular_basis(n: Vec3) -> (Vec3, Vec3) {
    let n = n.normalize_or_zero();
    let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = helper.cross(n).normalize_or_zero();
    let v = n.cross(u);
    (u, v)
}

/// Parameter `s` of the point on the line `o + s*d` that is closest to the ray
/// `ro + t*rd`. `None` when the two are (nearly) parallel.
pub fn closest_param_on_line(o: Vec3, d: Vec3, ro: Vec3, rd: Vec3) -> Option<f32> {
    let w0 = o - ro;
    let a = d.dot(d);
    let b = d.dot(rd);
    let c = rd.dot(rd);
    let dd = d.dot(w0);
    let e = rd.dot(w0);
    let denom = a * c - b * b;
    // `denom / (a*c)` is sin^2 of the angle between the directions.
    if a * c <= 0.0 || denom / (a * c) < 1e-4 {
        return None;
    }
    Some((b * e - c * dd) / denom)
}

/// Distance `t` along the ray at which it meets the plane, if it does.
pub fn ray_plane(ro: Vec3, rd: Vec3, plane_point: Vec3, plane_normal: Vec3) -> Option<f32> {
    let denom = rd.dot(plane_normal);
    if denom.abs() < 1e-5 {
        return None;
    }
    let t = (plane_point - ro).dot(plane_normal) / denom;
    (t.is_finite()).then_some(t)
}

pub fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-9 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

pub fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

/// Finds the gizmo handle under `mouse` (screen pixels). `project` maps world
/// points to screen pixels.
pub fn hit_test(
    tool: Tool,
    frame: &GizmoFrame,
    project: &dyn Fn(Vec3) -> Option<Vec2>,
    mouse: Vec2,
    enabled: AxisEnabled,
) -> Option<Handle> {
    let mut best: Option<(f32, Handle)> = None;
    let mut consider = |dist: f32, handle: Handle| {
        if dist <= HIT_RADIUS_PX && best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, handle));
        }
    };
    match tool {
        Tool::Select => return None,
        Tool::Move | Tool::Scale => {
            let origin = project(frame.origin)?;
            for a in Axis::ALL {
                if !enabled.get(a) {
                    continue;
                }
                if let Some(tip) = project(frame.origin + frame.axis(a) * frame.axis_len()) {
                    consider(segment_distance(mouse, origin, tip), Handle::Axis(a));
                }
            }
        }
        Tool::Rotate => {
            for a in Axis::ALL {
                if !enabled.get(a) {
                    continue;
                }
                let pts: Vec<Option<Vec2>> = frame.ring_points(a).into_iter().map(project).collect();
                for w in pts.windows(2) {
                    if let [Some(p), Some(q)] = w {
                        consider(segment_distance(mouse, *p, *q), Handle::Axis(a));
                    }
                }
            }
        }
    }
    best.map(|(_, h)| h)
}

// ---------------------------------------------------------------------------
// Dragging
// ---------------------------------------------------------------------------

/// Everything about the camera a drag needs.
#[derive(Clone, Copy, Debug)]
pub struct DragView {
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

/// An in-progress gizmo drag. All results are computed from the state captured
/// at the start, never accumulated, so they cannot drift.
#[derive(Clone, Debug)]
pub struct Drag {
    pub tool: Tool,
    pub handle: Handle,
    pub id: ObjectId,
    /// Moves the workplane instead of an object.
    pub workplane: bool,
    pub quantize: Quantize,
    frame: GizmoFrame,
    view: DragView,
    enabled: AxisEnabled,
    start_pos: Vec3,
    start_rot: Vec3,
    start_scale: Vec3,
    start_world_pos: Vec3,
    start_world_rot: Quat,
    start_param: f32,
    start_point: Vec3,
    start_mouse: Vec2,
}

impl Drag {
    /// Starts a drag. Returns `None` if the object is gone or the ray cannot
    /// be related to the handle (e.g. an axis pointing straight at the camera).
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        scene: &Scene,
        id: ObjectId,
        tool: Tool,
        handle: Handle,
        frame: GizmoFrame,
        view: DragView,
        enabled: AxisEnabled,
        ray: (Vec3, Vec3),
        mouse: Vec2,
    ) -> Option<Drag> {
        if tool == Tool::Select {
            return None;
        }
        let obj = scene.get(id)?;
        let world = scene.world_matrix(id);
        let mut drag = Drag {
            tool,
            handle,
            id,
            workplane: false,
            quantize: Quantize::default(),
            frame,
            view,
            enabled,
            start_pos: obj.pos,
            start_rot: obj.rot,
            start_scale: obj.scale,
            start_world_pos: world.w_axis.truncate(),
            start_world_rot: matrix_rotation(world),
            start_param: 0.0,
            start_point: frame.origin,
            start_mouse: mouse,
        };
        let (ro, rd) = ray;
        match (tool, handle) {
            (Tool::Move | Tool::Scale, Handle::Axis(a)) => {
                drag.start_param = closest_param_on_line(frame.origin, frame.axis(a), ro, rd)?;
            }
            (Tool::Move, Handle::Free) => {
                let t = ray_plane(ro, rd, frame.origin, view.forward)?;
                drag.start_point = ro + rd * t;
            }
            (Tool::Rotate, Handle::Axis(a)) => {
                let t = ray_plane(ro, rd, frame.origin, frame.axis(a))?;
                drag.start_param = drag.angle_of(a, ro + rd * t);
            }
            // Free scale / rotate only need the start mouse position.
            _ => {}
        }
        Some(drag)
    }

    /// Starts a drag of the workplane (Workplane mode). Scaling does nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_workplane(scene: &Scene, tool: Tool, handle: Handle, frame: GizmoFrame, view: DragView, enabled: AxisEnabled, ray: (Vec3, Vec3), mouse: Vec2) -> Option<Drag> {
        if matches!(tool, Tool::Select | Tool::Scale) {
            return None;
        }
        let wp = scene.workplane;
        let mut drag = Drag {
            tool,
            handle,
            id: 0,
            workplane: true,
            quantize: Quantize::default(),
            frame,
            view,
            enabled,
            start_pos: wp.pos,
            start_rot: wp.rot,
            start_scale: Vec3::ONE,
            start_world_pos: wp.pos,
            start_world_rot: crate::scene::hpb_to_quat(wp.rot),
            start_param: 0.0,
            start_point: frame.origin,
            start_mouse: mouse,
        };
        let (ro, rd) = ray;
        match (tool, handle) {
            (Tool::Move, Handle::Axis(a)) => drag.start_param = closest_param_on_line(frame.origin, frame.axis(a), ro, rd)?,
            (Tool::Move, Handle::Free) => drag.start_point = ro + rd * ray_plane(ro, rd, frame.origin, view.forward)?,
            (Tool::Rotate, Handle::Axis(a)) => {
                let t = ray_plane(ro, rd, frame.origin, frame.axis(a))?;
                drag.start_param = drag.angle_of(a, ro + rd * t);
            }
            _ => {}
        }
        Some(drag)
    }

    /// Where a Move drag started (world).
    pub fn start_world_pos(&self) -> Vec3 {
        self.start_world_pos
    }

    /// Moves the dragged thing so its origin is at `world`, keeping to the
    /// handle's axis or the enabled axes (used by snapping).
    pub fn move_to(&self, scene: &mut Scene, world: Vec3) {
        let delta = world - self.start_world_pos;
        let allowed = match self.handle {
            Handle::Axis(a) => self.frame.axis(a) * delta.dot(self.frame.axis(a)),
            Handle::Free => Axis::ALL
                .iter()
                .filter(|a| self.enabled.get(**a))
                .map(|a| self.frame.axis(*a) * delta.dot(self.frame.axis(*a)))
                .sum(),
        };
        self.set_world_position(scene, self.start_world_pos + allowed);
    }

    fn angle_of(&self, a: Axis, point: Vec3) -> f32 {
        let (u, v) = self.frame.ring_basis(a);
        let rel = point - self.frame.origin;
        rel.dot(v).atan2(rel.dot(u))
    }

    /// Applies the drag for the current pointer position to `scene`.
    pub fn update(&self, scene: &mut Scene, ray: (Vec3, Vec3), mouse: Vec2) {
        if !self.workplane && !scene.contains(self.id) {
            return;
        }
        let (ro, rd) = ray;
        match (self.tool, self.handle) {
            (Tool::Move, Handle::Axis(a)) => {
                let ax = self.frame.axis(a);
                if let Some(s) = closest_param_on_line(self.frame.origin, ax, ro, rd) {
                    let d = self.quantize.distance(s - self.start_param);
                    self.set_world_position(scene, self.start_world_pos + ax * d);
                }
            }
            (Tool::Move, Handle::Free) => {
                if let Some(t) = ray_plane(ro, rd, self.frame.origin, self.view.forward) {
                    let delta = ro + rd * t - self.start_point;
                    // Keep only the components along enabled axes.
                    let mut allowed = Vec3::ZERO;
                    for a in Axis::ALL {
                        if self.enabled.get(a) {
                            allowed += self.frame.axis(a) * self.quantize.distance(delta.dot(self.frame.axis(a)));
                        }
                    }
                    self.set_world_position(scene, self.start_world_pos + allowed);
                }
            }
            (Tool::Scale, Handle::Axis(a)) => {
                let ax = self.frame.axis(a);
                if let Some(s) = closest_param_on_line(self.frame.origin, ax, ro, rd) {
                    // Never divide by a start value near zero (grabbed at the pivot).
                    let min_start = 20.0 * self.frame.world_per_px;
                    let denom = if self.start_param.abs() < min_start {
                        min_start.copysign(self.start_param)
                    } else {
                        self.start_param
                    };
                    let factor = self.quantize.factor((s / denom).max(0.001));
                    let mut scale = self.start_scale;
                    scale[a.index()] = self.start_scale[a.index()] * factor;
                    self.set_scale(scene, scale);
                }
            }
            (Tool::Scale, Handle::Free) => {
                let factor = self.quantize.factor((1.0 + (mouse.x - self.start_mouse.x) / 150.0).max(0.001));
                let mut scale = self.start_scale;
                for a in Axis::ALL {
                    if self.enabled.get(a) {
                        scale[a.index()] *= factor;
                    }
                }
                self.set_scale(scene, scale);
            }
            (Tool::Rotate, Handle::Axis(a)) => {
                if let Some(t) = ray_plane(ro, rd, self.frame.origin, self.frame.axis(a)) {
                    let now = self.angle_of(a, ro + rd * t);
                    let delta = self.quantize.angle(wrap_angle(now - self.start_param));
                    let q = Quat::from_axis_angle(self.frame.axis(a), delta);
                    self.set_world_rotation(scene, q * self.start_world_rot);
                }
            }
            (Tool::Rotate, Handle::Free) => {
                let d = mouse - self.start_mouse;
                let k = 0.012;
                // Trackball: horizontal drag spins about the view-up, vertical about view-right.
                let q = Quat::from_axis_angle(self.view.up, self.quantize.angle(-d.x * k))
                    * Quat::from_axis_angle(self.view.right, self.quantize.angle(-d.y * k));
                self.set_world_rotation(scene, q * self.start_world_rot);
            }
            (Tool::Select, _) => {}
        }
    }

    fn set_world_position(&self, scene: &mut Scene, world: Vec3) {
        if self.workplane {
            if world.is_finite() {
                scene.workplane.pos = world;
            }
            return;
        }
        let parent = scene.parent_world_matrix(self.id);
        let local = parent.inverse().transform_point3(world);
        if local.is_finite()
            && let Some(o) = scene.get_mut(self.id) {
                o.pos = local;
            }
    }

    fn set_scale(&self, scene: &mut Scene, scale: Vec3) {
        if self.workplane {
            return;
        }
        if scale.is_finite()
            && let Some(o) = scene.get_mut(self.id) {
                o.scale = scale;
            }
    }

    fn set_world_rotation(&self, scene: &mut Scene, world_rot: Quat) {
        if self.workplane {
            scene.workplane.rot = quat_to_hpb_near(world_rot.normalize(), self.start_rot);
            return;
        }
        let parent_rot = matrix_rotation(scene.parent_world_matrix(self.id));
        let local = (parent_rot.inverse() * world_rot).normalize();
        if let Some(o) = scene.get_mut(self.id) {
            o.rot = quat_to_hpb_near(local, self.start_rot);
        }
    }

    pub fn start_local(&self) -> (Vec3, Vec3, Vec3) {
        (self.start_pos, self.start_rot, self.start_scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, ViewKind};
    use crate::primitive::PrimitiveKind;
    use crate::scene::hpb_to_quat;

    const ASPECT: f32 = 1.5;
    const VIEWPORT: Vec2 = Vec2::new(900.0, 600.0);

    fn to_screen(cam: &Camera, p: Vec3) -> Option<Vec2> {
        let ndc = cam.project(p, ASPECT)?;
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * VIEWPORT.x,
            (1.0 - (ndc.y * 0.5 + 0.5)) * VIEWPORT.y,
        ))
    }

    fn ray_at(cam: &Camera, px: Vec2) -> (Vec3, Vec3) {
        let ndc = Vec2::new(px.x / VIEWPORT.x * 2.0 - 1.0, 1.0 - px.y / VIEWPORT.y * 2.0);
        cam.ray(ndc, ASPECT)
    }

    fn view_of(cam: &Camera) -> DragView {
        DragView {
            forward: cam.forward(),
            right: cam.right(),
            up: cam.up(),
        }
    }

    fn setup() -> (Scene, ObjectId, Camera) {
        let mut scene = Scene::new();
        let id = scene.add_primitive(PrimitiveKind::Cube, None);
        scene.get_mut(id).unwrap().pos = Vec3::new(50.0, 30.0, -20.0);
        let mut cam = Camera::new(ViewKind::Perspective);
        cam.target = Vec3::new(50.0, 30.0, -20.0);
        (scene, id, cam)
    }

    fn frame_for(scene: &Scene, id: ObjectId, cam: &Camera, object_axes: bool) -> GizmoFrame {
        let origin = scene.world_position(id);
        GizmoFrame::new(scene, id, object_axes, cam.world_per_pixel(origin, VIEWPORT.y))
    }

    #[test]
    fn line_closest_point_finds_intersections() {
        // Ray from above looking straight down at x = 5 meets the X axis at s = 5.
        let s = closest_param_on_line(Vec3::ZERO, Vec3::X, Vec3::new(5.0, 10.0, 0.0), Vec3::NEG_Y).unwrap();
        assert!((s - 5.0).abs() < 1e-5);
        // Skew lines: closest approach is still found.
        let s = closest_param_on_line(Vec3::ZERO, Vec3::X, Vec3::new(3.0, 5.0, -4.0), Vec3::Z).unwrap();
        assert!((s - 3.0).abs() < 1e-5);
        // Parallel lines have no unique answer.
        assert!(closest_param_on_line(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 1.0, 0.0), Vec3::X).is_none());
    }

    #[test]
    fn plane_intersection() {
        let t = ray_plane(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, Vec3::ZERO, Vec3::Y).unwrap();
        assert!((t - 5.0).abs() < 1e-6);
        assert!(ray_plane(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 1.0, 0.0), Vec3::Y).is_none());
    }

    #[test]
    fn perpendicular_basis_is_orthonormal_and_right_handed() {
        for n in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 2.0, 3.0).normalize()] {
            let (u, v) = perpendicular_basis(n);
            assert!(u.dot(v).abs() < 1e-5 && u.dot(n).abs() < 1e-5 && v.dot(n).abs() < 1e-5);
            assert!((u.length() - 1.0).abs() < 1e-5 && (v.length() - 1.0).abs() < 1e-5);
            assert!((u.cross(v) - n).length() < 1e-5);
        }
    }

    #[test]
    fn hit_test_picks_the_axis_under_the_cursor() {
        let (scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, false);
        let project = |p: Vec3| to_screen(&cam, p);
        let origin = project(frame.origin).unwrap();
        for a in Axis::ALL {
            let tip = project(frame.origin + frame.axis(a) * frame.axis_len()).unwrap();
            let mid = origin.lerp(tip, 0.6);
            let hit = hit_test(Tool::Move, &frame, &project, mid, AxisEnabled::default());
            assert_eq!(hit, Some(Handle::Axis(a)), "axis {a:?}");
        }
        // Far from every handle there is no hit.
        assert_eq!(
            hit_test(Tool::Move, &frame, &project, Vec2::new(5.0, 5.0), AxisEnabled::default()),
            None
        );
        // Disabled axes cannot be grabbed.
        let tip_x = project(frame.origin + frame.axis(Axis::X) * frame.axis_len()).unwrap();
        let mid_x = origin.lerp(tip_x, 0.6);
        let only_yz = AxisEnabled([false, true, true]);
        assert_ne!(
            hit_test(Tool::Move, &frame, &project, mid_x, only_yz),
            Some(Handle::Axis(Axis::X))
        );
    }

    #[test]
    fn hit_test_finds_rotation_rings() {
        let (scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, false);
        let project = |p: Vec3| to_screen(&cam, p);
        for a in Axis::ALL {
            let pts = frame.ring_points(a);
            let on_ring = project(pts[RING_SEGMENTS / 8]).unwrap();
            let hit = hit_test(Tool::Rotate, &frame, &project, on_ring, AxisEnabled::default());
            assert!(hit.is_some(), "ring {a:?} not hittable");
        }
    }

    #[test]
    fn dragging_a_move_handle_moves_the_object_along_its_axis() {
        let (mut scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, false);
        let start_world = scene.world_position(id);
        let grab_world = start_world + Vec3::X * (frame.axis_len() * 0.5);
        let grab_px = to_screen(&cam, grab_world).unwrap();
        let drag = Drag::begin(
            &scene,
            id,
            Tool::Move,
            Handle::Axis(Axis::X),
            frame,
            view_of(&cam),
            AxisEnabled::default(),
            ray_at(&cam, grab_px),
            grab_px,
        )
        .unwrap();
        // Drag so the handle point lands where world x is +120 further.
        let target_world = grab_world + Vec3::X * 120.0;
        let target_px = to_screen(&cam, target_world).unwrap();
        drag.update(&mut scene, ray_at(&cam, target_px), target_px);
        let moved = scene.world_position(id) - start_world;
        assert!((moved - Vec3::X * 120.0).length() < 0.5, "{moved:?}");
    }

    #[test]
    fn move_drag_works_inside_a_rotated_scaled_parent() {
        let (mut scene, _, cam) = setup();
        let parent = scene.add_null(None);
        scene.get_mut(parent).unwrap().rot = Vec3::new(35.0, 10.0, 0.0);
        scene.get_mut(parent).unwrap().scale = Vec3::new(2.0, 1.0, 0.5);
        scene.get_mut(parent).unwrap().pos = Vec3::new(10.0, 0.0, 0.0);
        let id = scene.add_primitive(PrimitiveKind::Cube, Some(parent));
        scene.get_mut(id).unwrap().pos = Vec3::new(30.0, 20.0, -10.0);

        let mut cam = cam;
        cam.target = scene.world_position(id);
        let frame = frame_for(&scene, id, &cam, false);
        let start = scene.world_position(id);
        let grab = start + Vec3::Y * frame.axis_len() * 0.5;
        let grab_px = to_screen(&cam, grab).unwrap();
        let drag = Drag::begin(
            &scene, id, Tool::Move, Handle::Axis(Axis::Y), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, grab_px), grab_px,
        ).unwrap();
        let end_px = to_screen(&cam, grab + Vec3::Y * 75.0).unwrap();
        drag.update(&mut scene, ray_at(&cam, end_px), end_px);
        let moved = scene.world_position(id) - start;
        assert!((moved - Vec3::Y * 75.0).length() < 0.5, "world motion {moved:?}");
    }

    #[test]
    fn free_move_respects_axis_locks() {
        let (mut scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, false);
        let start = scene.world_position(id);
        let start_px = to_screen(&cam, start).unwrap();
        let enabled = AxisEnabled([true, false, true]); // Y locked
        let drag = Drag::begin(
            &scene, id, Tool::Move, Handle::Free, frame, view_of(&cam), enabled,
            ray_at(&cam, start_px), start_px,
        ).unwrap();
        let end_px = start_px + Vec2::new(120.0, -90.0);
        drag.update(&mut scene, ray_at(&cam, end_px), end_px);
        let moved = scene.world_position(id) - start;
        assert!(moved.y.abs() < 1e-3, "locked Y axis moved by {}", moved.y);
        assert!(moved.length() > 1.0, "should still move on the other axes");
    }

    #[test]
    fn rotate_handle_rotates_by_the_dragged_angle() {
        let (mut scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, false);
        let (u, v) = frame.ring_basis(Axis::Y);
        let r = frame.ring_radius();
        let o = frame.origin;
        let a0 = o + u * r;
        let a1 = o + (u * 30f32.to_radians().cos() + v * 30f32.to_radians().sin()) * r;
        let p0 = to_screen(&cam, a0).unwrap();
        let p1 = to_screen(&cam, a1).unwrap();
        let drag = Drag::begin(
            &scene, id, Tool::Rotate, Handle::Axis(Axis::Y), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, p0), p0,
        ).unwrap();
        drag.update(&mut scene, ray_at(&cam, p1), p1);
        let q = hpb_to_quat(scene.get(id).unwrap().rot);
        let expected = Quat::from_axis_angle(Vec3::Y, 30f32.to_radians());
        assert!(q.dot(expected).abs() > 0.9995, "got {:?}, hpb {:?}", q, scene.get(id).unwrap().rot);
        // Position is untouched by rotation.
        assert!((scene.world_position(id) - Vec3::new(50.0, 30.0, -20.0)).length() < 1e-3);
    }

    #[test]
    fn scale_handle_scales_proportionally() {
        let (mut scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, true);
        let o = frame.origin;
        let grab = o + frame.axis(Axis::X) * frame.axis_len();
        let grab_px = to_screen(&cam, grab).unwrap();
        let drag = Drag::begin(
            &scene, id, Tool::Scale, Handle::Axis(Axis::X), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, grab_px), grab_px,
        ).unwrap();
        let end = o + frame.axis(Axis::X) * frame.axis_len() * 2.0;
        let end_px = to_screen(&cam, end).unwrap();
        drag.update(&mut scene, ray_at(&cam, end_px), end_px);
        let s = scene.get(id).unwrap().scale;
        assert!((s.x - 2.0).abs() < 0.02, "{s:?}");
        assert!((s.y - 1.0).abs() < 1e-6 && (s.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn scale_never_collapses_to_zero_or_negative() {
        let (mut scene, id, cam) = setup();
        let frame = frame_for(&scene, id, &cam, true);
        let grab = frame.origin + frame.axis(Axis::X) * frame.axis_len();
        let grab_px = to_screen(&cam, grab).unwrap();
        let drag = Drag::begin(
            &scene, id, Tool::Scale, Handle::Axis(Axis::X), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, grab_px), grab_px,
        ).unwrap();
        // Drag far past the pivot to the other side.
        let past = frame.origin - frame.axis(Axis::X) * frame.axis_len() * 3.0;
        let past_px = to_screen(&cam, past).unwrap();
        drag.update(&mut scene, ray_at(&cam, past_px), past_px);
        assert!(scene.get(id).unwrap().scale.x >= 0.001);
    }

    #[test]
    fn rotation_stays_continuous_across_the_euler_wrap() {
        let (mut scene, id, cam) = setup();
        scene.get_mut(id).unwrap().rot = Vec3::new(175.0, 0.0, 0.0);
        let frame = frame_for(&scene, id, &cam, false);
        let (u, v) = frame.ring_basis(Axis::Y);
        let r = frame.ring_radius();
        let at = |deg: f32| frame.origin + (u * deg.to_radians().cos() + v * deg.to_radians().sin()) * r;
        let p0 = to_screen(&cam, at(0.0)).unwrap();
        let drag = Drag::begin(
            &scene, id, Tool::Rotate, Handle::Axis(Axis::Y), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, p0), p0,
        ).unwrap();
        let p1 = to_screen(&cam, at(20.0)).unwrap();
        drag.update(&mut scene, ray_at(&cam, p1), p1);
        let h = scene.get(id).unwrap().rot.x;
        assert!((h - 195.0).abs() < 0.5 || (h + 165.0).abs() < 0.5, "heading {h}");
        // It must stay on the branch nearest the start (195), not jump to -165.
        assert!((h - 195.0).abs() < 0.5, "heading flipped to {h}");
    }

    #[test]
    fn quantize_steps_moves_angles_and_scales() {
        let q = Quantize { enabled: true, move_step: 10.0, rotate_step: 15.0, scale_step: 25.0 };
        assert_eq!(q.distance(23.0), 20.0);
        assert_eq!(q.distance(-26.0), -30.0);
        assert!((q.angle(20f32.to_radians()).to_degrees() - 15.0).abs() < 1e-4);
        assert!((q.factor(1.4) - 1.5).abs() < 1e-6);
        assert!(q.factor(0.05) > 0.0, "never collapses to zero");
        let off = Quantize::default();
        assert_eq!(off.distance(23.0), 23.0);
    }

    #[test]
    fn dragging_the_workplane_moves_and_turns_it() {
        let (mut scene, _, cam) = setup();
        let frame = GizmoFrame::workplane(&scene.workplane, true, 1.0);
        let view = view_of(&cam);
        let start = Vec2::new(450.0, 300.0);
        let d = Drag::begin_workplane(&scene, Tool::Move, Handle::Free, frame, view, AxisEnabled::default(), ray_at(&cam, start), start).unwrap();
        let end = start + Vec2::new(60.0, 0.0);
        d.update(&mut scene, ray_at(&cam, end), end);
        assert!(scene.workplane.pos.length() > 1.0, "moved: {:?}", scene.workplane.pos);
        assert!(Drag::begin_workplane(&scene, Tool::Scale, Handle::Free, frame, view, AxisEnabled::default(), ray_at(&cam, start), start).is_none());
    }

    #[test]
    fn edge_on_axis_cannot_start_a_drag() {
        let (scene, id, mut cam) = setup();
        // Look straight down the world X axis: the X handle points at the camera.
        cam.kind = ViewKind::Right;
        let frame = frame_for(&scene, id, &cam, false);
        let px = Vec2::new(450.0, 300.0);
        let drag = Drag::begin(
            &scene, id, Tool::Move, Handle::Axis(Axis::X), frame, view_of(&cam),
            AxisEnabled::default(), ray_at(&cam, px), px,
        );
        assert!(drag.is_none());
    }
}
