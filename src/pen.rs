//! The Spline Pen and Polygon Pen: editing rules, independent of the UI.
//!
//! Positions come in as world-space points (the viewport works out where a
//! click lands); they are stored in the target object's local space, so the
//! pens work on objects that have been moved, rotated or scaled.

use crate::editmesh::EditMesh;
use crate::editor::Editor;
use crate::scene::{ObjectId, ObjectKind};
use crate::spline::{Spline, SplinePoint};
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PenKind {
    Spline,
    Polygon,
}

impl PenKind {
    pub fn label(self) -> &'static str {
        match self {
            PenKind::Spline => "Spline Pen",
            PenKind::Polygon => "Polygon Pen",
        }
    }
}

/// What a drag with the Spline Pen is changing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PenDrag {
    /// A point just placed: dragging pulls out its tangents (both sides).
    NewPoint(usize),
    MovePoint(usize),
    /// A tangent handle (`out` = the outgoing one). Shift breaks the mirror.
    Tangent { index: usize, out: bool },
}

/// A corner of a polygon being drawn with the Polygon Pen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PenVertex {
    /// An existing point of the target mesh (snapped to).
    Existing(u32),
    /// A new point, in the target's local space.
    New(Vec3),
}

#[derive(Clone, Debug)]
pub struct PenState {
    pub kind: PenKind,
    /// The spline or polygon object being drawn into (created on first use).
    pub target: Option<ObjectId>,
    pub drag: Option<PenDrag>,
    /// Polygon Pen: corners placed so far.
    pub pending: Vec<PenVertex>,
}

impl PenState {
    pub fn new(kind: PenKind, target: Option<ObjectId>) -> Self {
        PenState { kind, target, drag: None, pending: Vec::new() }
    }
}

fn to_local(world: Mat4, p: Vec3) -> Vec3 {
    let inv = world.inverse();
    if inv.is_finite() { inv.transform_point3(p) } else { p }
}

fn to_local_vector(world: Mat4, v: Vec3) -> Vec3 {
    let inv = world.inverse();
    if inv.is_finite() { inv.transform_vector3(v) } else { v }
}

impl Editor {
    /// Turns a pen on. It draws into the active object when that is the right
    /// kind (a spline for the Spline Pen, a polygon object for the Polygon
    /// Pen); otherwise the first click creates a new object.
    pub fn start_pen(&mut self, kind: PenKind) {
        let target = self.active().filter(|&id| {
            matches!(
                (kind, self.scene.get(id).map(|o| &o.kind)),
                (PenKind::Spline, Some(ObjectKind::Spline(_))) | (PenKind::Polygon, Some(ObjectKind::Polygon(_)))
            )
        });
        self.mesh_tool = None;
        self.tool_drag = None;
        self.pen = Some(PenState::new(kind, target));
        self.status = match kind {
            PenKind::Spline => "Spline Pen: click to add points, drag to pull out curves; click the first point to close; Enter or Esc to finish",
            PenKind::Polygon => "Polygon Pen: click corners (snaps to existing points); click the first corner or press Enter to make the polygon",
        }
        .to_owned();
    }

    pub fn stop_pen(&mut self) {
        self.pen = None;
    }

    fn pen_target(&self, kind: PenKind) -> Option<ObjectId> {
        let id = self.pen.as_ref()?.target?;
        match (kind, &self.scene.get(id)?.kind) {
            (PenKind::Spline, ObjectKind::Spline(_)) | (PenKind::Polygon, ObjectKind::Polygon(_)) => Some(id),
            _ => None,
        }
    }

    /// The spline the Spline Pen draws into, and its world matrix.
    pub fn pen_spline(&self) -> Option<(ObjectId, &Spline, Mat4)> {
        let id = self.pen_target(PenKind::Spline)?;
        match &self.scene.get(id)?.kind {
            ObjectKind::Spline(s) => Some((id, s, self.scene.world_matrix(id))),
            _ => None,
        }
    }

    /// The mesh the Polygon Pen draws into, and its world matrix.
    pub fn pen_mesh(&self) -> Option<(ObjectId, &EditMesh, Mat4)> {
        let id = self.pen_target(PenKind::Polygon)?;
        match &self.scene.get(id)?.kind {
            ObjectKind::Polygon(m) => Some((id, m, self.scene.world_matrix(id))),
            _ => None,
        }
    }

    fn pen_spline_mut(&mut self) -> Option<(&mut Spline, Mat4)> {
        let id = self.pen_target(PenKind::Spline)?;
        let world = self.scene.world_matrix(id);
        match &mut self.scene.get_mut(id)?.kind {
            ObjectKind::Spline(s) => Some((s, world)),
            _ => None,
        }
    }

    /// Spline Pen: adds a corner point at `world`, creating the spline on the
    /// first click. Returns the new point's index.
    pub fn pen_add_spline_point(&mut self, world: Vec3) -> usize {
        if self.pen_target(PenKind::Spline).is_none() {
            let id = self.scene.add_object("Spline", ObjectKind::Spline(Spline::default()), None);
            self.select_only(id);
            if let Some(p) = self.pen.as_mut() {
                p.target = Some(id);
            }
        }
        self.label("Spline Pen");
        let Some((s, m)) = self.pen_spline_mut() else { return 0 };
        s.points.push(SplinePoint::corner(to_local(m, world)));
        s.points.len() - 1
    }

    /// Spline Pen: point `index`'s outgoing tangent reaches to `world`; the
    /// incoming one mirrors it.
    pub fn pen_pull_tangents(&mut self, index: usize, world: Vec3) {
        if let Some((s, m)) = self.pen_spline_mut()
            && let Some(p) = s.points.get_mut(index)
        {
            let out = to_local(m, world) - p.pos;
            *p = SplinePoint::smooth(p.pos, out);
        }
    }

    pub fn pen_move_spline_point(&mut self, index: usize, world: Vec3) {
        if let Some((s, m)) = self.pen_spline_mut()
            && let Some(p) = s.points.get_mut(index)
        {
            p.pos = to_local(m, world);
        }
    }

    /// Moves a tangent handle to `world`. Unless `broken`, the opposite handle
    /// turns with it (keeping its own length), so the curve stays smooth.
    pub fn pen_move_tangent(&mut self, index: usize, out: bool, world: Vec3, broken: bool) {
        if let Some((s, m)) = self.pen_spline_mut()
            && let Some(p) = s.points.get_mut(index)
        {
            let v = to_local(m, world) - p.pos;
            let (this, other) = if out { (&mut p.tan_out, &mut p.tan_in) } else { (&mut p.tan_in, &mut p.tan_out) };
            *this = v;
            if !broken {
                let len = other.length();
                *other = -v.normalize_or_zero() * if len > 1e-4 { len } else { v.length() };
            }
        }
    }

    /// Splits segment `segment` at `t`, keeping the curve. Returns the new index.
    pub fn pen_insert_spline_point(&mut self, segment: usize, t: f32) -> Option<usize> {
        self.label("Spline Pen");
        let (s, _) = self.pen_spline_mut()?;
        (segment < s.segment_count()).then(|| s.insert(segment, t))
    }

    pub fn pen_close_spline(&mut self) {
        if let Some((s, _)) = self.pen_spline_mut()
            && s.points.len() >= 3
        {
            s.closed = true;
            self.label("Close Spline");
            self.status = "Spline closed".to_owned();
        }
    }

    /// Removes the last point (Backspace while drawing).
    pub fn pen_remove_last_spline_point(&mut self) {
        if let Some((s, _)) = self.pen_spline_mut() {
            s.points.pop();
            if s.points.len() < 3 {
                s.closed = false;
            }
            self.label("Spline Pen");
        }
    }

    /// Polygon Pen: adds a corner. Snapped points are kept as they are; new
    /// ones are given in world space.
    pub fn pen_add_corner(&mut self, corner: PenVertex) {
        let world = self.pen_mesh().map(|(_, _, m)| m).unwrap_or(Mat4::IDENTITY);
        let Some(pen) = self.pen.as_mut() else { return };
        let corner = match corner {
            PenVertex::New(w) => PenVertex::New(to_local(world, w)),
            v => v,
        };
        if pen.pending.last() != Some(&corner) {
            pen.pending.push(corner);
        }
    }

    /// Polygon Pen: turns the placed corners into a polygon. `toward_viewer`
    /// is a world direction towards the camera: a polygon with no neighbours
    /// is wound to face it; one that shares an edge follows its neighbour.
    /// Returns false (and keeps the corners) when there are fewer than three.
    pub fn pen_commit_polygon(&mut self, toward_viewer: Vec3) -> bool {
        let Some(pen) = self.pen.as_ref() else { return false };
        let pending = pen.pending.clone();
        let distinct = {
            let mut v = pending.clone();
            v.dedup();
            if v.len() > 1 && v.first() == v.last() {
                v.pop();
            }
            v
        };
        if distinct.len() < 3 {
            self.status = "Polygon Pen: place at least three corners".to_owned();
            return false;
        }
        if self.pen_target(PenKind::Polygon).is_none() {
            let id = self.scene.add_object("Polygon", ObjectKind::Polygon(EditMesh::default()), None);
            self.select_only(id);
            if let Some(p) = self.pen.as_mut() {
                p.target = Some(id);
            }
        }
        let Some(id) = self.pen_target(PenKind::Polygon) else { return false };
        let world = self.scene.world_matrix(id);
        let toward_local = to_local_vector(world, toward_viewer);
        let Some(ObjectKind::Polygon(mesh)) = self.scene.get_mut(id).map(|o| &mut o.kind) else { return false };
        let mut poly: Vec<u32> = distinct
            .iter()
            .map(|v| match *v {
                PenVertex::Existing(i) => i,
                PenVertex::New(p) => {
                    mesh.points.push(p);
                    (mesh.points.len() - 1) as u32
                }
            })
            .collect();
        poly.dedup();
        orient(mesh, &mut poly, toward_local);
        mesh.polys.push(poly);
        if let Some(p) = self.pen.as_mut() {
            p.pending.clear();
        }
        self.label("Polygon Pen");
        self.status = "Polygon Pen: polygon created; keep clicking for the next one".to_owned();
        true
    }
}

/// Winds `poly` like its neighbours (a shared edge runs the other way in a
/// consistent mesh), or, with none, to face `toward`.
fn orient(mesh: &EditMesh, poly: &mut [u32], toward: Vec3) {
    let n = poly.len();
    let directed = |a: u32, b: u32| {
        mesh.polys.iter().any(|p| (0..p.len()).any(|k| p[k] == a && p[(k + 1) % p.len()] == b))
    };
    let mut same = 0;
    let mut opposite = 0;
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        if directed(a, b) {
            same += 1;
        } else if directed(b, a) {
            opposite += 1;
        }
    }
    let flip = if same + opposite > 0 {
        same > opposite
    } else {
        // Newell normal of the new polygon.
        let mut normal = Vec3::ZERO;
        for k in 0..n {
            let (p, q) = (mesh.points[poly[k] as usize], mesh.points[poly[(k + 1) % n] as usize]);
            normal += Vec3::new((p.y - q.y) * (p.z + q.z), (p.z - q.z) * (p.x + q.x), (p.x - q.x) * (p.y + q.y));
        }
        normal.dot(toward) < 0.0
    };
    if flip {
        poly.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    fn spline_of(ed: &Editor) -> Spline {
        ed.pen_spline().map(|(_, s, _)| s.clone()).unwrap()
    }

    #[test]
    fn spline_pen_creates_draws_curves_and_closes() {
        let mut ed = Editor::new(Scene::new());
        ed.start_pen(PenKind::Spline);
        assert!(ed.pen_spline().is_none(), "nothing until the first click");
        ed.pen_add_spline_point(Vec3::ZERO);
        let i = ed.pen_add_spline_point(Vec3::X * 100.0);
        ed.pen_pull_tangents(i, Vec3::new(100.0, 0.0, 40.0));
        ed.pen_add_spline_point(Vec3::new(100.0, 0.0, 100.0));
        let s = spline_of(&ed);
        assert_eq!(s.points.len(), 3);
        assert_eq!(s.points[1].tan_out, Vec3::Z * 40.0);
        assert_eq!(s.points[1].tan_in, -Vec3::Z * 40.0);
        ed.pen_close_spline();
        assert!(spline_of(&ed).closed);
        // The new spline is the selected object.
        let (id, _, _) = ed.pen_spline().unwrap();
        assert_eq!(ed.active(), Some(id));
        // Backspace drops the last point (and with it the closing).
        ed.pen_remove_last_spline_point();
        assert!(!spline_of(&ed).closed);
    }

    #[test]
    fn spline_pen_stores_points_in_the_objects_own_space() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.scene.add_object("Spline", ObjectKind::Spline(Spline::default()), None);
        ed.scene.get_mut(id).unwrap().pos = Vec3::new(50.0, 0.0, 0.0);
        ed.select_only(id);
        ed.start_pen(PenKind::Spline);
        ed.pen_add_spline_point(Vec3::new(70.0, 10.0, 0.0));
        assert_eq!(spline_of(&ed).points[0].pos, Vec3::new(20.0, 10.0, 0.0));
        ed.pen_move_spline_point(0, Vec3::new(50.0, 0.0, 0.0));
        assert_eq!(spline_of(&ed).points[0].pos, Vec3::ZERO);
    }

    #[test]
    fn moving_a_handle_turns_its_partner_unless_broken() {
        let mut ed = Editor::new(Scene::new());
        ed.start_pen(PenKind::Spline);
        let i = ed.pen_add_spline_point(Vec3::ZERO);
        ed.pen_pull_tangents(i, Vec3::X * 30.0);
        ed.pen_move_tangent(i, true, Vec3::Z * 10.0, false);
        let p = spline_of(&ed).points[0];
        assert_eq!(p.tan_out, Vec3::Z * 10.0);
        assert!((p.tan_in - Vec3::NEG_Z * 30.0).length() < 1e-4, "keeps its own length, turns to mirror");
        ed.pen_move_tangent(i, false, Vec3::X * 5.0, true);
        let p = spline_of(&ed).points[0];
        assert_eq!(p.tan_in, Vec3::X * 5.0);
        assert_eq!(p.tan_out, Vec3::Z * 10.0, "broken: the other handle stays");
    }

    #[test]
    fn polygon_pen_makes_a_new_object_facing_the_camera() {
        let mut ed = Editor::new(Scene::new());
        ed.start_pen(PenKind::Polygon);
        for p in [Vec3::ZERO, Vec3::X * 100.0, Vec3::new(100.0, 0.0, 100.0), Vec3::Z * 100.0] {
            ed.pen_add_corner(PenVertex::New(p));
        }
        assert!(ed.pen_commit_polygon(Vec3::Y));
        let (_, mesh, _) = ed.pen_mesh().unwrap();
        assert_eq!((mesh.points.len(), mesh.polys.len()), (4, 1));
        assert!(mesh.poly_normal(0).y > 0.9, "faces up, towards the camera above");
        // Two corners are not a polygon; they are kept for more clicks.
        ed.pen_add_corner(PenVertex::Existing(0));
        ed.pen_add_corner(PenVertex::Existing(1));
        assert!(!ed.pen_commit_polygon(Vec3::Y));
        assert_eq!(ed.pen.as_ref().unwrap().pending.len(), 2);
    }

    #[test]
    fn polygon_pen_snaps_and_follows_its_neighbour() {
        let mut ed = Editor::new(Scene::new());
        ed.start_pen(PenKind::Polygon);
        for p in [Vec3::ZERO, Vec3::X * 100.0, Vec3::new(100.0, 0.0, 100.0), Vec3::Z * 100.0] {
            ed.pen_add_corner(PenVertex::New(p));
        }
        ed.pen_commit_polygon(Vec3::Y);
        // A second quad sharing the edge 1-2, drawn in the "wrong" order and
        // with the camera below: it must still match the first one's side.
        ed.pen_add_corner(PenVertex::Existing(1));
        ed.pen_add_corner(PenVertex::Existing(2));
        ed.pen_add_corner(PenVertex::New(Vec3::new(200.0, 0.0, 100.0)));
        ed.pen_add_corner(PenVertex::New(Vec3::new(200.0, 0.0, 0.0)));
        assert!(ed.pen_commit_polygon(Vec3::NEG_Y));
        let (_, mesh, _) = ed.pen_mesh().unwrap();
        assert_eq!(mesh.points.len(), 6, "two points reused, two new");
        assert!(mesh.poly_normal(1).y > 0.9);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn polygon_pen_adds_to_the_selected_polygon_object() {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.create_primitive(crate::primitive::PrimitiveKind::Cube);
        ed.select_only(cube);
        ed.make_editable();
        ed.start_pen(PenKind::Polygon);
        let (id, mesh, _) = ed.pen_mesh().expect("draws into the editable cube");
        assert_eq!(id, cube);
        let before = mesh.polys.len();
        for p in [Vec3::new(0.0, 300.0, 0.0), Vec3::new(50.0, 300.0, 0.0), Vec3::new(0.0, 300.0, 50.0)] {
            ed.pen_add_corner(PenVertex::New(p));
        }
        ed.pen_commit_polygon(Vec3::Y);
        assert_eq!(ed.pen_mesh().unwrap().1.polys.len(), before + 1);
    }
}
