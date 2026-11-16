//! Mesh booleans (union, subtract, intersect) with binary space partitioning trees,
//! the classic "constructive solid geometry" approach. Computation is in f64 and
//! every tree walk is iterative, so dense meshes cannot overflow the stack.
//!
//! Inputs should be closed solids. The result is visually correct but contains
//! T-junctions (split polygons are not re-stitched), so it is meant for display,
//! not as watertight input for further booleans of the same mesh.

use crate::generator::Geo;
use crate::mesh::Vertex;
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolMode {
    /// A minus B.
    Subtract,
    /// A plus B.
    Union,
    /// Only where A and B overlap.
    Intersect,
}

impl BoolMode {
    pub const ALL: [BoolMode; 3] = [BoolMode::Subtract, BoolMode::Union, BoolMode::Intersect];

    pub fn label(self) -> &'static str {
        match self {
            BoolMode::Subtract => "A subtract B",
            BoolMode::Union => "A union B",
            BoolMode::Intersect => "A intersect B",
        }
    }
}

/// Booleans above this many input polygons are skipped (A is returned as is),
/// so a dense mesh cannot freeze the editor.
pub const MAX_INPUT_POLYGONS: usize = 60_000;

#[derive(Clone, Copy, Debug)]
struct V {
    p: DVec3,
    n: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct Plane {
    n: DVec3,
    w: f64,
}

impl Plane {
    fn flipped(self) -> Plane {
        Plane { n: -self.n, w: -self.w }
    }

    fn dist(&self, p: DVec3) -> f64 {
        self.n.dot(p) - self.w
    }
}

#[derive(Clone, Debug)]
struct Poly {
    verts: Vec<V>,
    plane: Plane,
    from_a: bool,
}

impl Poly {
    /// Newell's method: robust for slightly non-planar or nearly degenerate input.
    fn new(verts: Vec<V>, from_a: bool) -> Option<Poly> {
        if verts.len() < 3 {
            return None;
        }
        let mut n = DVec3::ZERO;
        let mut centre = DVec3::ZERO;
        for i in 0..verts.len() {
            let (a, b) = (verts[i].p, verts[(i + 1) % verts.len()].p);
            n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
            centre += a;
        }
        let len = n.length();
        if len < 1e-12 || !len.is_finite() {
            return None;
        }
        let n = n / len;
        let centre = centre / verts.len() as f64;
        Some(Poly { plane: Plane { n, w: n.dot(centre) }, verts, from_a })
    }

    fn flip(&mut self) {
        self.verts.reverse();
        for v in &mut self.verts {
            v.n = -v.n;
        }
        self.plane = self.plane.flipped();
    }
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

/// Where the pieces of a split polygon go.
#[derive(Default)]
struct Split {
    coplanar_front: Vec<Poly>,
    coplanar_back: Vec<Poly>,
    front: Vec<Poly>,
    back: Vec<Poly>,
}

fn split_polygon(plane: &Plane, poly: Poly, eps: f64, out: &mut Split) {
    let types: Vec<u8> = poly
        .verts
        .iter()
        .map(|v| {
            let t = plane.dist(v.p);
            if t < -eps {
                BACK
            } else if t > eps {
                FRONT
            } else {
                COPLANAR
            }
        })
        .collect();
    let kind = types.iter().fold(0, |acc, t| acc | t);
    match kind {
        COPLANAR => {
            if plane.n.dot(poly.plane.n) > 0.0 {
                out.coplanar_front.push(poly);
            } else {
                out.coplanar_back.push(poly);
            }
        }
        FRONT => out.front.push(poly),
        BACK => out.back.push(poly),
        _ => {
            let (mut f, mut b) = (Vec::new(), Vec::new());
            let n = poly.verts.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let (ti, tj) = (types[i], types[j]);
                let (vi, vj) = (poly.verts[i], poly.verts[j]);
                if ti != BACK {
                    f.push(vi);
                }
                if ti != FRONT {
                    b.push(vi);
                }
                if (ti | tj) == SPANNING {
                    let di = plane.dist(vi.p);
                    let dj = plane.dist(vj.p);
                    let t = di / (di - dj);
                    let v = V { p: vi.p.lerp(vj.p, t), n: vi.n.lerp(vj.n, t as f32).normalize_or_zero() };
                    f.push(v);
                    b.push(v);
                }
            }
            // Keep the parent's plane: re-deriving it from slivers only adds error.
            if f.len() >= 3 {
                out.front.push(Poly { verts: f, plane: poly.plane, from_a: poly.from_a });
            }
            if b.len() >= 3 {
                out.back.push(Poly { verts: b, plane: poly.plane, from_a: poly.from_a });
            }
        }
    }
}

#[derive(Clone, Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<usize>,
    back: Option<usize>,
    polys: Vec<Poly>,
}

/// A BSP tree stored in an arena; node 0 is the root.
#[derive(Clone)]
struct Bsp {
    nodes: Vec<Node>,
    eps: f64,
}

impl Bsp {
    fn new(polys: Vec<Poly>, eps: f64) -> Bsp {
        let mut t = Bsp { nodes: vec![Node::default()], eps };
        t.build(polys);
        t
    }

    fn child(&mut self, node: usize, front: bool) -> usize {
        let existing = if front { self.nodes[node].front } else { self.nodes[node].back };
        existing.unwrap_or_else(|| {
            self.nodes.push(Node::default());
            let idx = self.nodes.len() - 1;
            if front {
                self.nodes[node].front = Some(idx);
            } else {
                self.nodes[node].back = Some(idx);
            }
            idx
        })
    }

    fn build(&mut self, polys: Vec<Poly>) {
        let mut stack = vec![(0usize, polys)];
        while let Some((idx, polys)) = stack.pop() {
            if polys.is_empty() {
                continue;
            }
            let plane = *self.nodes[idx].plane.get_or_insert(polys[0].plane);
            let mut s = Split::default();
            for p in polys {
                split_polygon(&plane, p, self.eps, &mut s);
            }
            self.nodes[idx].polys.extend(s.coplanar_front);
            self.nodes[idx].polys.extend(s.coplanar_back);
            if !s.front.is_empty() {
                let c = self.child(idx, true);
                stack.push((c, s.front));
            }
            if !s.back.is_empty() {
                let c = self.child(idx, false);
                stack.push((c, s.back));
            }
        }
    }

    fn invert(&mut self) {
        for n in &mut self.nodes {
            for p in &mut n.polys {
                p.flip();
            }
            n.plane = n.plane.map(Plane::flipped);
            std::mem::swap(&mut n.front, &mut n.back);
        }
    }

    /// Removes the parts of `polys` that are inside this tree's solid.
    fn clip_polygons(&self, polys: Vec<Poly>) -> Vec<Poly> {
        let mut out = Vec::new();
        let mut stack = vec![(0usize, polys)];
        while let Some((idx, polys)) = stack.pop() {
            let node = &self.nodes[idx];
            let Some(plane) = node.plane else {
                out.extend(polys);
                continue;
            };
            let mut s = Split::default();
            for p in polys {
                split_polygon(&plane, p, self.eps, &mut s);
            }
            let mut front = s.front;
            front.extend(s.coplanar_front);
            let mut back = s.back;
            back.extend(s.coplanar_back);
            match node.front {
                Some(c) => stack.push((c, front)),
                None => out.extend(front),
            }
            if let Some(c) = node.back {
                stack.push((c, back));
            }
            // Without a back child, `back` is inside the solid and dropped.
        }
        out
    }

    fn clip_to(&mut self, other: &Bsp) {
        for i in 0..self.nodes.len() {
            let polys = std::mem::take(&mut self.nodes[i].polys);
            self.nodes[i].polys = other.clip_polygons(polys);
        }
    }

    fn all_polygons(&self) -> Vec<Poly> {
        self.nodes.iter().flat_map(|n| n.polys.iter().cloned()).collect()
    }

    /// Removes the parts of line segments that are inside this tree's solid.
    fn clip_segments(&self, segs: Vec<(DVec3, DVec3)>) -> Vec<(DVec3, DVec3)> {
        let eps = self.eps;
        let mut out = Vec::new();
        let mut stack = vec![(0usize, segs)];
        while let Some((idx, segs)) = stack.pop() {
            let node = &self.nodes[idx];
            let Some(plane) = node.plane else {
                out.extend(segs);
                continue;
            };
            let (mut front, mut back) = (Vec::new(), Vec::new());
            for (a, b) in segs {
                let (da, db) = (plane.dist(a), plane.dist(b));
                if da >= -eps && db >= -eps {
                    front.push((a, b));
                } else if da <= eps && db <= eps {
                    back.push((a, b));
                } else {
                    let m = a.lerp(b, da / (da - db));
                    if da > 0.0 {
                        front.push((a, m));
                        back.push((m, b));
                    } else {
                        back.push((a, m));
                        front.push((m, b));
                    }
                }
            }
            match node.front {
                Some(c) => stack.push((c, front)),
                None => out.extend(front),
            }
            if let Some(c) = node.back {
                stack.push((c, back));
            }
        }
        out
    }

    /// True when `p` lies on one of the tree's polygons.
    fn on_surface(&self, p: DVec3) -> bool {
        let eps = self.eps;
        let mut stack = vec![0usize];
        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx];
            let Some(plane) = node.plane else { continue };
            let d = plane.dist(p);
            if d.abs() <= eps {
                if node.polys.iter().any(|poly| inside_polygon(poly, p, eps)) {
                    return true;
                }
                stack.extend(node.front);
                stack.extend(node.back);
            } else if d > 0.0 {
                stack.extend(node.front);
            } else {
                stack.extend(node.back);
            }
        }
        false
    }
}

fn inside_polygon(poly: &Poly, p: DVec3, eps: f64) -> bool {
    if poly.plane.dist(p).abs() > eps {
        return false;
    }
    let n = poly.verts.len();
    (0..n).all(|i| {
        let (a, b) = (poly.verts[i].p, poly.verts[(i + 1) % n].p);
        let edge = b - a;
        let len = edge.length();
        len < 1e-12 || edge.cross(p - a).dot(poly.plane.n) >= -eps * len
    })
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn to_polys(geo: &Geo, from_a: bool, eps: f64) -> Vec<Poly> {
    let v = |i: u32| {
        let vert = geo.verts[i as usize];
        V { p: Vec3::from(vert.pos).as_dvec3(), n: Vec3::from(vert.normal) }
    };
    let mut out = Vec::new();
    for face in crate::generator::polygons(geo) {
        let verts: Vec<V> = face.iter().map(|&i| v(i)).collect();
        if let Some(poly) = Poly::new(verts.clone(), from_a) {
            // Non-planar quads (e.g. after a twist) are split back into triangles.
            if poly.verts.iter().all(|vv| poly.plane.dist(vv.p).abs() <= eps) {
                out.push(poly);
                continue;
            }
        }
        for k in 1..verts.len() - 1 {
            out.extend(Poly::new(vec![verts[0], verts[k], verts[k + 1]], from_a));
        }
    }
    out
}

fn segments(geo: &Geo) -> Vec<(DVec3, DVec3)> {
    let p = |i: u32| Vec3::from(geo.verts[i as usize].pos).as_dvec3();
    geo.edges.chunks_exact(2).map(|e| (p(e[0]), p(e[1]))).collect()
}

fn bounds(geo: &Geo) -> (Vec3, Vec3) {
    geo.verts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
        (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos)))
    })
}

// ---------------------------------------------------------------------------
// The operation
// ---------------------------------------------------------------------------

pub fn boolean(a: &Geo, b: &Geo, mode: BoolMode) -> Geo {
    if a.is_empty() && b.is_empty() {
        return Geo::default();
    }
    if b.is_empty() {
        return if mode == BoolMode::Intersect { Geo::default() } else { a.clone() };
    }
    if a.is_empty() {
        return if mode == BoolMode::Union { b.clone() } else { Geo::default() };
    }
    if a.triangle_count() + b.triangle_count() > MAX_INPUT_POLYGONS * 2 {
        return a.clone();
    }
    // Tolerance relative to the size of the scene being cut.
    let (alo, ahi) = bounds(a);
    let (blo, bhi) = bounds(b);
    let extent = (ahi.max(bhi) - alo.min(blo)).max_element().max(1.0) as f64;
    let eps = extent * 1e-7;

    let a_orig = Bsp::new(to_polys(a, true, eps), eps);
    let b_orig = Bsp::new(to_polys(b, false, eps), eps);
    let (mut ta, mut tb) = (a_orig.clone(), b_orig.clone());

    match mode {
        BoolMode::Union => {
            ta.clip_to(&tb);
            tb.clip_to(&ta);
            tb.invert();
            tb.clip_to(&ta);
            tb.invert();
            ta.build(tb.all_polygons());
        }
        BoolMode::Subtract => {
            ta.invert();
            ta.clip_to(&tb);
            tb.clip_to(&ta);
            tb.invert();
            tb.clip_to(&ta);
            tb.invert();
            ta.build(tb.all_polygons());
            ta.invert();
        }
        BoolMode::Intersect => {
            ta.invert();
            tb.clip_to(&ta);
            tb.invert();
            ta.clip_to(&tb);
            tb.clip_to(&ta);
            ta.build(tb.all_polygons());
            ta.invert();
        }
    }
    let result = ta.all_polygons();

    // --- wire edges: the inputs' own edges where they survive, plus the cut curve.
    let inverted = |t: &Bsp| {
        let mut t = t.clone();
        t.invert();
        t
    };
    let (a_keep, b_keep) = match mode {
        BoolMode::Union => (b_orig.clip_segments(segments(a)), a_orig.clip_segments(segments(b))),
        BoolMode::Subtract => (b_orig.clip_segments(segments(a)), inverted(&a_orig).clip_segments(segments(b))),
        BoolMode::Intersect => (
            inverted(&b_orig).clip_segments(segments(a)),
            inverted(&a_orig).clip_segments(segments(b)),
        ),
    };
    let mut lines: Vec<(DVec3, DVec3)> = a_keep.into_iter().chain(b_keep).collect();
    // Cut curve: result edges from one operand that lie on the other's surface.
    for poly in &result {
        let other = if poly.from_a { &b_orig } else { &a_orig };
        let n = poly.verts.len();
        for i in 0..n {
            let (p, q) = (poly.verts[i].p, poly.verts[(i + 1) % n].p);
            if (q - p).length() > eps
                && other.on_surface(p)
                && other.on_surface(q)
                && other.on_surface((p + q) * 0.5)
            {
                lines.push((p, q));
            }
        }
    }

    // --- assemble
    let mut out = Geo::default();
    for poly in &result {
        let base = out.verts.len() as u32;
        for v in &poly.verts {
            let n = if v.n.length_squared() > 0.0 { v.n } else { poly.plane.n.as_vec3() };
            out.verts.push(Vertex { pos: v.p.as_vec3().to_array(), normal: n.to_array() });
        }
        for k in 1..poly.verts.len() as u32 - 1 {
            out.tris.extend_from_slice(&[base, base + k, base + k + 1]);
        }
    }
    for (p, q) in lines {
        let base = out.verts.len() as u32;
        for x in [p, q] {
            out.verts.push(Vertex { pos: x.as_vec3().to_array(), normal: [0.0, 1.0, 0.0] });
        }
        out.edges.extend_from_slice(&[base, base + 1]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::{Primitive, PrimitiveKind};
    use glam::Mat4;

    fn cube_at(size: f32, at: Vec3) -> Geo {
        let mut g = Geo::default();
        let m = Primitive::Cube { size: Vec3::splat(size), segments: [1, 1, 1] }.build();
        g.append_transformed(&Geo::from_mesh(&m), Mat4::from_translation(at));
        g
    }

    fn volume(g: &Geo) -> f64 {
        g.tris
            .chunks_exact(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(g.verts[i as usize].pos).as_dvec3();
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= b.abs().max(1.0) * 1e-4
    }

    // Two 200 cm cubes, the second shifted by (100, 100, 100): they overlap in a 100 cm cube.
    fn pair() -> (Geo, Geo) {
        (cube_at(200.0, Vec3::ZERO), cube_at(200.0, Vec3::splat(100.0)))
    }

    #[test]
    fn subtract_union_intersect_volumes() {
        let (a, b) = pair();
        let full = 8.0e6;
        let overlap = 1.0e6;
        let s = boolean(&a, &b, BoolMode::Subtract);
        assert!(close(volume(&s), full - overlap), "{}", volume(&s));
        let u = boolean(&a, &b, BoolMode::Union);
        assert!(close(volume(&u), 2.0 * full - overlap), "{}", volume(&u));
        let i = boolean(&a, &b, BoolMode::Intersect);
        assert!(close(volume(&i), overlap), "{}", volume(&i));
    }

    #[test]
    fn coplanar_faces_are_handled() {
        // Same size, shifted along X only: top/bottom/front/back faces are coplanar.
        let a = cube_at(200.0, Vec3::ZERO);
        let b = cube_at(200.0, Vec3::new(100.0, 0.0, 0.0));
        assert!(close(volume(&boolean(&a, &b, BoolMode::Subtract)), 4.0e6));
        assert!(close(volume(&boolean(&a, &b, BoolMode::Union)), 12.0e6));
        assert!(close(volume(&boolean(&a, &b, BoolMode::Intersect)), 4.0e6));
    }

    #[test]
    fn disjoint_and_missing_operands() {
        let a = cube_at(100.0, Vec3::ZERO);
        let far = cube_at(100.0, Vec3::splat(1000.0));
        assert!(close(volume(&boolean(&a, &far, BoolMode::Subtract)), 1.0e6));
        assert!(boolean(&a, &far, BoolMode::Intersect).is_empty());
        assert!(close(volume(&boolean(&a, &far, BoolMode::Union)), 2.0e6));
        let empty = Geo::default();
        assert!(close(volume(&boolean(&a, &empty, BoolMode::Subtract)), 1.0e6));
        assert!(boolean(&a, &empty, BoolMode::Intersect).is_empty());
        assert!(close(volume(&boolean(&empty, &a, BoolMode::Union)), 1.0e6));
    }

    #[test]
    fn sphere_bite_out_of_a_cube() {
        let cube = cube_at(200.0, Vec3::ZERO);
        let mut sphere = Geo::default();
        let s = PrimitiveKind::Sphere.default_primitive().build();
        sphere.append_transformed(&Geo::from_mesh(&s), Mat4::from_translation(Vec3::new(100.0, 100.0, 100.0)));
        let out = boolean(&cube, &sphere, BoolMode::Subtract);
        let v = volume(&out);
        // The sphere (r = 100, faceted) covers about an eighth of itself inside the cube.
        let bite = 4.0 / 3.0 * std::f64::consts::PI * 1.0e6 / 8.0;
        assert!(v < 8.0e6 - bite * 0.9 && v > 8.0e6 - bite * 1.1, "{v}");
        assert!(out.verts.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
        // Normals stay unit length; the carved hollow faces into the bite (towards the sphere centre).
        assert!(out.verts.iter().filter(|v| v.normal != [0.0, 1.0, 0.0]).all(|v| (Vec3::from(v.normal).length() - 1.0).abs() < 1e-3));
    }

    #[test]
    fn wire_shows_surviving_edges_and_the_cut() {
        let (a, b) = pair();
        let out = boolean(&a, &b, BoolMode::Subtract);
        let segs: Vec<(Vec3, Vec3)> = out
            .edges
            .chunks_exact(2)
            .map(|e| (Vec3::from(out.verts[e[0] as usize].pos), Vec3::from(out.verts[e[1] as usize].pos)))
            .collect();
        assert!(!segs.is_empty());
        // No surviving edge runs through the removed corner region.
        for (p, q) in &segs {
            let m = (*p + *q) * 0.5;
            assert!(!(m.x > 1.0 && m.y > 1.0 && m.z > 1.0 && m.max_element() < 99.0), "edge inside the cut: {m:?}");
        }
        // The cut curve is drawn: e.g. where A's top face (y = 100) meets B's side (x = 0).
        assert!(segs.iter().any(|(p, q)| {
            let m = (*p + *q) * 0.5;
            p.x.abs() < 1e-3 && q.x.abs() < 1e-3 && (m.y - 100.0).abs() < 1e-3 && m.z > 1.0
        }));
    }

    #[test]
    fn deep_trees_do_not_overflow() {
        // A dense subdivided sphere would recurse thousands of levels in a naive BSP.
        let mut sphere = Geo::default();
        let s = Primitive::Sphere { radius: 100.0, segments: 96 }.build();
        sphere.append_transformed(&Geo::from_mesh(&s), Mat4::IDENTITY);
        let cube = cube_at(120.0, Vec3::new(80.0, 0.0, 0.0));
        let out = boolean(&sphere, &cube, BoolMode::Subtract);
        assert!(out.triangle_count() > 0);
    }
}
