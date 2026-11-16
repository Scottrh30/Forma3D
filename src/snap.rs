//! Snapping (Cinema 4D's Snap settings): while a move or a pen click is
//! near a point, an edge, a surface or a grid point, it lands exactly on it.
//!
//! Everything works in screen space with a pixel radius, so what snaps is
//! what you see near the cursor. Nothing here touches egui.

use crate::mesh::Mesh;
use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapMode {
    /// Snap to anything in 3D space.
    Snap3D,
    /// Snap, then drop the result onto the workplane.
    Snap25D,
    /// Snap in the orthographic views only, keeping the dragged depth.
    Snap2D,
}

impl SnapMode {
    pub const ALL: [SnapMode; 3] = [SnapMode::Snap3D, SnapMode::Snap25D, SnapMode::Snap2D];

    pub fn label(self) -> &'static str {
        match self {
            SnapMode::Snap3D => "3D Snapping",
            SnapMode::Snap25D => "2.5D Snapping (onto the workplane)",
            SnapMode::Snap2D => "2D Snapping (orthographic views)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapKind {
    Vertex,
    Edge,
    Midpoint,
    Polygon,
    Axis,
    Grid,
}

impl SnapKind {
    pub const ALL: [SnapKind; 6] = [SnapKind::Vertex, SnapKind::Edge, SnapKind::Midpoint, SnapKind::Polygon, SnapKind::Axis, SnapKind::Grid];

    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Vertex => "Vertex",
            SnapKind::Edge => "Edge",
            SnapKind::Midpoint => "Edge Midpoint",
            SnapKind::Polygon => "Polygon",
            SnapKind::Axis => "Object Axis",
            SnapKind::Grid => "Grid Point",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            SnapKind::Vertex => "Snap to points and mesh vertices",
            SnapKind::Edge => "Snap onto the nearest edge or spline segment",
            SnapKind::Midpoint => "Snap to the middle of edges",
            SnapKind::Polygon => "Snap onto the surface under the cursor",
            SnapKind::Axis => "Snap to objects' origins",
            SnapKind::Grid => "Snap to the grid (the workplane grid in the Perspective view)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapSettings {
    pub enabled: bool,
    pub mode: SnapMode,
    /// How close (pixels) the cursor must come.
    pub radius: f32,
    pub vertex: bool,
    pub edge: bool,
    pub midpoint: bool,
    pub polygon: bool,
    pub axis: bool,
    pub grid: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings {
            enabled: false,
            mode: SnapMode::Snap3D,
            radius: 12.0,
            vertex: true,
            edge: true,
            midpoint: true,
            polygon: false,
            axis: true,
            grid: false,
        }
    }
}

impl SnapSettings {
    pub fn get(&self, k: SnapKind) -> bool {
        match k {
            SnapKind::Vertex => self.vertex,
            SnapKind::Edge => self.edge,
            SnapKind::Midpoint => self.midpoint,
            SnapKind::Polygon => self.polygon,
            SnapKind::Axis => self.axis,
            SnapKind::Grid => self.grid,
        }
    }

    pub fn toggle(&mut self, k: SnapKind) {
        let f = match k {
            SnapKind::Vertex => &mut self.vertex,
            SnapKind::Edge => &mut self.edge,
            SnapKind::Midpoint => &mut self.midpoint,
            SnapKind::Polygon => &mut self.polygon,
            SnapKind::Axis => &mut self.axis,
            SnapKind::Grid => &mut self.grid,
        };
        *f = !*f;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapHit {
    pub pos: Vec3,
    pub kind: SnapKind,
}

/// Geometry that can be snapped to.
pub struct Source<'a> {
    pub world: Mat4,
    pub mesh: &'a Mesh,
    /// Has a surface (for Polygon snapping).
    pub surface: bool,
    /// Its origin attracts (Object Axis snapping).
    pub origin: bool,
}

/// Where the cursor is and how the view sees the world.
pub struct Query<'a> {
    pub project: &'a dyn Fn(Vec3) -> Option<Vec2>,
    pub mouse: Vec2,
    pub ray: (Vec3, Vec3),
    /// The view direction of an orthographic view (None in Perspective).
    pub ortho_forward: Option<Vec3>,
    pub workplane: Mat4,
    /// Grid spacing in centimetres.
    pub grid_unit: f32,
    /// Where the dragged point started (2D snapping keeps its depth).
    pub start: Vec3,
    /// Extra points to snap to (e.g. the unmoved points of a mesh being edited).
    pub extra_points: &'a [Vec3],
}

/// The best snap target near the cursor, if any.
pub fn find(s: &SnapSettings, sources: &[Source], q: &Query) -> Option<SnapHit> {
    if !s.enabled || (s.mode == SnapMode::Snap2D && q.ortho_forward.is_none()) {
        return None;
    }
    let hit = find_raw(s, sources, q)?;
    let pos = match s.mode {
        SnapMode::Snap3D => hit.pos,
        SnapMode::Snap25D => {
            let o = q.workplane.w_axis.truncate();
            let n = q.workplane.transform_vector3(Vec3::Y).normalize_or_zero();
            hit.pos - n * (hit.pos - o).dot(n)
        }
        SnapMode::Snap2D => {
            let f = q.ortho_forward.unwrap_or(Vec3::Z).normalize_or_zero();
            hit.pos + f * (q.start - hit.pos).dot(f)
        }
    };
    pos.is_finite().then_some(SnapHit { pos, kind: hit.kind })
}

fn find_raw(s: &SnapSettings, sources: &[Source], q: &Query) -> Option<SnapHit> {
    let r = s.radius.max(1.0);
    // 1. Points: vertices, edge midpoints, object origins (closest on screen wins).
    let mut best: Option<(f32, SnapHit)> = None;
    let mut consider = |p: Vec3, kind: SnapKind| {
        if let Some(sp) = (q.project)(p) {
            let d = sp.distance(q.mouse);
            if d <= r && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, SnapHit { pos: p, kind }));
            }
        }
    };
    for src in sources {
        if s.vertex {
            for v in &src.mesh.vertices {
                consider(src.world.transform_point3(Vec3::from(v.pos)), SnapKind::Vertex);
            }
        }
        if s.midpoint {
            for e in src.mesh.edges.chunks_exact(2) {
                let (a, b) = (src.mesh.position(e[0]), src.mesh.position(e[1]));
                consider(src.world.transform_point3((a + b) * 0.5), SnapKind::Midpoint);
            }
        }
        if s.axis && src.origin {
            consider(src.world.w_axis.truncate(), SnapKind::Axis);
        }
    }
    if s.vertex {
        for &p in q.extra_points {
            consider(p, SnapKind::Vertex);
        }
    }
    if let Some((_, hit)) = best {
        return Some(hit);
    }

    // 2. Edges: the point of the nearest edge under the cursor.
    if s.edge {
        let mut best: Option<(f32, Vec3)> = None;
        for src in sources {
            for e in src.mesh.edges.chunks_exact(2) {
                let a = src.world.transform_point3(src.mesh.position(e[0]));
                let b = src.world.transform_point3(src.mesh.position(e[1]));
                let (Some(sa), Some(sb)) = ((q.project)(a), (q.project)(b)) else { continue };
                let ab = sb - sa;
                let len2 = ab.length_squared();
                let t = if len2 < 1e-9 { 0.0 } else { ((q.mouse - sa).dot(ab) / len2).clamp(0.0, 1.0) };
                let d = q.mouse.distance(sa + ab * t);
                if d <= r && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, a.lerp(b, t)));
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(SnapHit { pos: p, kind: SnapKind::Edge });
        }
    }

    // 3. Polygons: the surface under the cursor.
    if s.polygon {
        let (ro, rd) = q.ray;
        let mut best: Option<f32> = None;
        for src in sources.iter().filter(|s| s.surface) {
            let m = src.mesh;
            for t in m.indices.chunks_exact(3) {
                let p = |i: u32| src.world.transform_point3(m.position(i));
                if let Some(d) = crate::picking::ray_triangle(ro, rd, p(t[0]), p(t[1]), p(t[2]))
                    && d > 0.0
                    && best.is_none_or(|b| d < b)
                {
                    best = Some(d);
                }
            }
        }
        if let Some(t) = best {
            return Some(SnapHit { pos: ro + rd * t, kind: SnapKind::Polygon });
        }
    }

    // 4. Grid points.
    if s.grid {
        return grid_point(q).map(|pos| SnapHit { pos, kind: SnapKind::Grid });
    }
    None
}

/// The grid point nearest where the cursor ray meets the grid plane.
pub fn grid_point(q: &Query) -> Option<Vec3> {
    let (ro, rd) = q.ray;
    let unit = q.grid_unit.max(1e-3);
    let round = |v: f32| (v / unit).round() * unit;
    match q.ortho_forward {
        Some(f) => {
            // The grid of an orthographic view lies in the plane through the origin.
            let t = crate::gizmo::ray_plane(ro, rd, Vec3::ZERO, f)?;
            let p = ro + rd * t;
            let snapped = Vec3::new(round(p.x), round(p.y), round(p.z));
            // Keep the component along the view direction.
            let f = f.normalize_or_zero();
            Some(snapped + f * (p - snapped).dot(f))
        }
        None => {
            let o = q.workplane.w_axis.truncate();
            let n = q.workplane.transform_vector3(Vec3::Y).normalize_or_zero();
            let t = crate::gizmo::ray_plane(ro, rd, o, n)?;
            if t <= 0.0 {
                return None;
            }
            let local = q.workplane.inverse().transform_point3(ro + rd * t);
            Some(q.workplane.transform_point3(Vec3::new(round(local.x), 0.0, round(local.z))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, ViewKind};
    use crate::primitive::PrimitiveKind;

    const VIEW: Vec2 = Vec2::new(800.0, 600.0);

    fn screen(cam: &Camera, p: Vec3) -> Option<Vec2> {
        let ndc = cam.project(p, VIEW.x / VIEW.y)?;
        Some(Vec2::new((ndc.x * 0.5 + 0.5) * VIEW.x, (1.0 - (ndc.y * 0.5 + 0.5)) * VIEW.y))
    }

    fn query<'a>(cam: &'a Camera, project: &'a dyn Fn(Vec3) -> Option<Vec2>, mouse: Vec2) -> Query<'a> {
        let ndc = Vec2::new(mouse.x / VIEW.x * 2.0 - 1.0, 1.0 - mouse.y / VIEW.y * 2.0);
        Query {
            project,
            mouse,
            ray: cam.ray(ndc, VIEW.x / VIEW.y),
            ortho_forward: cam.kind.is_ortho().then(|| cam.forward()),
            workplane: Mat4::IDENTITY,
            grid_unit: 100.0,
            start: Vec3::ZERO,
            extra_points: &[],
        }
    }

    #[test]
    fn snaps_to_the_nearest_vertex_then_edges_then_grid() {
        let cam = Camera::new(ViewKind::Front);
        let project = |p: Vec3| screen(&cam, p);
        let cube = PrimitiveKind::Cube.default_primitive().build();
        let world = Mat4::from_translation(Vec3::new(300.0, 0.0, 0.0));
        let sources = [Source { world, mesh: &cube, surface: true, origin: true }];
        let on = SnapSettings { enabled: true, ..SnapSettings::default() };

        // Near a corner of the cube (300 +/- 100).
        let corner = Vec3::new(400.0, 100.0, -100.0);
        let near = project(corner).unwrap() + Vec2::new(4.0, -3.0);
        let hit = find(&on, &sources, &query(&cam, &project, near)).unwrap();
        assert_eq!(hit.kind, SnapKind::Vertex);
        assert!((hit.pos.truncate() - corner.truncate()).length() < 1e-3);

        // Partway down the right edge: an Edge hit on x = 400.
        let side = project(Vec3::new(400.0, 30.0, -100.0)).unwrap() + Vec2::new(3.0, 0.0);
        let hit = find(&on, &sources, &query(&cam, &project, side)).unwrap();
        assert_eq!(hit.kind, SnapKind::Edge);
        assert!((hit.pos.x - 400.0).abs() < 1e-2);

        // Far from everything: nothing, unless grid snapping is on.
        let empty = project(Vec3::new(-420.0, 260.0, 0.0)).unwrap();
        assert!(find(&on, &sources, &query(&cam, &project, empty)).is_none());
        let grid = SnapSettings { grid: true, ..on };
        let hit = find(&grid, &sources, &query(&cam, &project, empty)).unwrap();
        assert_eq!(hit.kind, SnapKind::Grid);
        assert!((hit.pos.x + 400.0).abs() < 1e-3 && (hit.pos.y - 300.0).abs() < 1e-3);

        // Off: never snaps.
        assert!(find(&SnapSettings::default(), &sources, &query(&cam, &project, near)).is_none());
    }

    #[test]
    fn two_and_a_half_d_drops_onto_the_workplane_and_2d_keeps_depth() {
        let cam = Camera::new(ViewKind::Front);
        let project = |p: Vec3| screen(&cam, p);
        let cube = PrimitiveKind::Cube.default_primitive().build();
        let sources = [Source { world: Mat4::IDENTITY, mesh: &cube, surface: true, origin: true }];
        let corner = Vec3::new(100.0, 100.0, -100.0);
        let mouse = project(corner).unwrap();
        let s25 = SnapSettings { enabled: true, mode: SnapMode::Snap25D, ..SnapSettings::default() };
        let hit = find(&s25, &sources, &query(&cam, &project, mouse)).unwrap();
        assert!(hit.pos.y.abs() < 1e-3, "dropped onto the floor workplane: {:?}", hit.pos);
        let s2 = SnapSettings { enabled: true, mode: SnapMode::Snap2D, ..SnapSettings::default() };
        let mut q = query(&cam, &project, mouse);
        q.start = Vec3::new(0.0, 0.0, 250.0);
        let hit = find(&s2, &sources, &q).unwrap();
        assert!((hit.pos.z - 250.0).abs() < 1e-3, "kept the dragged depth: {:?}", hit.pos);
        // 2D snapping does nothing in the Perspective view.
        let persp = Camera::new(ViewKind::Perspective);
        let pp = |p: Vec3| screen(&persp, p);
        assert!(find(&s2, &sources, &query(&persp, &pp, Vec2::new(400.0, 300.0))).is_none());
    }
}
