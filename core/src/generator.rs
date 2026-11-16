//! Non-destructive generators: objects that build their geometry from their children.
//!
//! A generator consumes its child objects (which are no longer drawn on their own) and
//! produces one mesh, like Cinema 4D's Subdivision Surface, Array and Symmetry objects.
//! Results are recomputed from the scene each time it changes and cached by content hash.

use crate::csg::BoolMode;
use crate::mesh::{Mesh, Vertex};
use crate::primitive::Primitive;
use crate::scene::{ObjectId, ObjectKind, Scene, hpb_to_quat};
use glam::{Mat3, Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Generators never produce more triangles than this.
pub const MAX_TRIANGLES: usize = 1_500_000;
pub const MAX_LEVELS: u32 = 4;
pub const MAX_COPIES: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    Subdivision,
    Array,
    Symmetry,
    Boolean,
}

impl GeneratorKind {
    pub const ALL: [GeneratorKind; 4] =
        [GeneratorKind::Subdivision, GeneratorKind::Array, GeneratorKind::Symmetry, GeneratorKind::Boolean];

    pub fn name(self) -> &'static str {
        match self {
            GeneratorKind::Subdivision => "Subdivision Surface",
            GeneratorKind::Array => "Array",
            GeneratorKind::Symmetry => "Symmetry",
            GeneratorKind::Boolean => "Boolean",
        }
    }

    pub fn default_generator(self) -> Generator {
        match self {
            GeneratorKind::Subdivision => Generator::Subdivision { levels: 2 },
            GeneratorKind::Array => Generator::Array {
                mode: ArrayMode::Linear,
                count: 3,
                offset: Vec3::new(250.0, 0.0, 0.0),
                rotation: Vec3::ZERO,
                scale: Vec3::ONE,
                radius: 300.0,
            },
            GeneratorKind::Symmetry => Generator::Symmetry { plane: MirrorPlane::YZ },
            GeneratorKind::Boolean => Generator::Boolean { mode: BoolMode::Subtract },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrayMode {
    /// Each copy is offset / rotated / scaled a bit more than the previous one.
    Linear,
    /// Copies are spread around a circle in the XZ plane.
    Radial,
}

/// The plane an object is mirrored across.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorPlane {
    /// Mirrors X.
    YZ,
    /// Mirrors Y.
    XZ,
    /// Mirrors Z.
    XY,
}

impl MirrorPlane {
    pub const ALL: [MirrorPlane; 3] = [MirrorPlane::YZ, MirrorPlane::XZ, MirrorPlane::XY];

    pub fn label(self) -> &'static str {
        match self {
            MirrorPlane::YZ => "YZ (mirror X)",
            MirrorPlane::XZ => "XZ (mirror Y)",
            MirrorPlane::XY => "XY (mirror Z)",
        }
    }

    fn matrix(self) -> Mat4 {
        Mat4::from_scale(match self {
            MirrorPlane::YZ => Vec3::new(-1.0, 1.0, 1.0),
            MirrorPlane::XZ => Vec3::new(1.0, -1.0, 1.0),
            MirrorPlane::XY => Vec3::new(1.0, 1.0, -1.0),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Generator {
    Subdivision {
        levels: u32,
    },
    Array {
        mode: ArrayMode,
        count: u32,
        /// Linear: translation added per copy (cm).
        offset: Vec3,
        /// Linear: HPB rotation added per copy (degrees).
        rotation: Vec3,
        /// Linear: scale multiplier applied per copy.
        scale: Vec3,
        /// Radial: circle radius (cm).
        radius: f32,
    },
    Symmetry {
        plane: MirrorPlane,
    },
    /// First child is A, second child is B; further children are ignored.
    Boolean {
        mode: BoolMode,
    },
}

impl Generator {
    pub fn kind(&self) -> GeneratorKind {
        match self {
            Generator::Subdivision { .. } => GeneratorKind::Subdivision,
            Generator::Array { .. } => GeneratorKind::Array,
            Generator::Symmetry { .. } => GeneratorKind::Symmetry,
            Generator::Boolean { .. } => GeneratorKind::Boolean,
        }
    }

    /// Clamps parameters to values that cannot blow up memory or produce NaNs.
    pub fn sanitized(self) -> Generator {
        let finite = |v: Vec3| if v.is_finite() { v } else { Vec3::ZERO };
        match self {
            Generator::Subdivision { levels } => Generator::Subdivision { levels: levels.min(MAX_LEVELS) },
            Generator::Array { mode, count, offset, rotation, scale, radius } => Generator::Array {
                mode,
                count: count.clamp(1, MAX_COPIES),
                offset: finite(offset),
                rotation: finite(rotation),
                scale: if scale.is_finite() { scale.clamp(Vec3::splat(0.01), Vec3::splat(100.0)) } else { Vec3::ONE },
                radius: if radius.is_finite() { radius } else { 0.0 },
            },
            g @ (Generator::Symmetry { .. } | Generator::Boolean { .. }) => g,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw geometry
// ---------------------------------------------------------------------------

/// Plain geometry used while combining objects.
#[derive(Clone, Default)]
pub struct Geo {
    pub verts: Vec<Vertex>,
    pub tris: Vec<u32>,
    pub edges: Vec<u32>,
}

impl Geo {
    pub fn from_mesh(m: &Mesh) -> Geo {
        Geo { verts: m.vertices.clone(), tris: m.indices.clone(), edges: m.edges.clone() }
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len() / 3
    }

    /// Appends `other` transformed by `m`. Mirroring transforms get their winding flipped.
    pub fn append_transformed(&mut self, other: &Geo, m: Mat4) {
        let base = self.verts.len() as u32;
        let normal_m = Mat3::from_mat4(m).inverse().transpose();
        let normal_ok = normal_m.is_finite();
        for v in &other.verts {
            let p = m.transform_point3(Vec3::from(v.pos));
            let n = if normal_ok { (normal_m * Vec3::from(v.normal)).normalize_or_zero() } else { Vec3::from(v.normal) };
            self.verts.push(Vertex { pos: p.to_array(), normal: n.to_array() });
        }
        let flip = Mat3::from_mat4(m).determinant() < 0.0;
        for t in other.tris.chunks_exact(3) {
            if flip {
                self.tris.extend_from_slice(&[base + t[0], base + t[2], base + t[1]]);
            } else {
                self.tris.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            }
        }
        self.edges.extend(other.edges.iter().map(|e| base + e));
    }

    pub fn into_mesh(self) -> Mesh {
        Mesh::from_parts(self.verts, self.tris, self.edges)
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Geometry of object `id` in its own local space (its own transform excluded),
/// including the effect of deformers placed directly under it.
/// `build` supplies the mesh of a primitive (normally via a cache).
pub fn eval_geo(scene: &Scene, id: ObjectId, build: &mut dyn FnMut(&Primitive) -> Arc<Mesh>) -> Geo {
    let mut geo = base_geo(scene, id, build);
    if geo.verts.is_empty() {
        return geo;
    }
    for &c in scene.children_of(Some(id)) {
        if let Some(o) = scene.get(c)
            && let (ObjectKind::Deformer(d), true) = (&o.kind, o.enabled) {
                crate::deformer::apply(d, o.local_matrix(), &mut geo);
            }
    }
    geo
}

/// Geometry of `id` before its own deformers are applied.
pub fn base_geo(scene: &Scene, id: ObjectId, build: &mut dyn FnMut(&Primitive) -> Arc<Mesh>) -> Geo {
    let Some(obj) = scene.get(id) else { return Geo::default() };
    if !obj.enabled && matches!(obj.kind, ObjectKind::Generator(_)) {
        // A switched-off generator passes its children through, like a Null.
        return children_geo(scene, id, build);
    }
    match &obj.kind {
        ObjectKind::Primitive(p) => Geo::from_mesh(&build(p)),
        ObjectKind::Null => children_geo(scene, id, build),
        ObjectKind::Generator(Generator::Boolean { mode }) => {
            // Operands are the first two children that produce geometry.
            let mut operands = scene.children_of(Some(id)).iter().filter_map(|&c| {
                let g = eval_geo(scene, c, build);
                if g.verts.is_empty() {
                    return None;
                }
                let mut placed = Geo::default();
                placed.append_transformed(&g, scene.get(c)?.local_matrix());
                Some(placed)
            });
            let a = operands.next().unwrap_or_default();
            let b = operands.next().unwrap_or_default();
            crate::csg::boolean(&a, &b, *mode)
        }
        ObjectKind::Generator(g) => apply(&g.sanitized(), children_geo(scene, id, build)),
        ObjectKind::Deformer(_) => Geo::default(),
        ObjectKind::Polygon(m) => m.to_geo(),
        // Splines have no surface of their own (yet: no spline generators).
        ObjectKind::Spline(_) | ObjectKind::Light(_) | ObjectKind::Camera(_) | ObjectKind::Sky(_) => Geo::default(),
    }
}

fn children_geo(scene: &Scene, id: ObjectId, build: &mut dyn FnMut(&Primitive) -> Arc<Mesh>) -> Geo {
    let mut out = Geo::default();
    let children: Vec<ObjectId> = scene.children_of(Some(id)).to_vec();
    for c in children {
        let g = eval_geo(scene, c, build);
        if g.is_empty() {
            continue;
        }
        if out.triangle_count() + g.triangle_count() > MAX_TRIANGLES {
            break;
        }
        let local = scene.get(c).map(|o| o.local_matrix()).unwrap_or(Mat4::IDENTITY);
        out.append_transformed(&g, local);
    }
    out
}

fn apply(g: &Generator, input: Geo) -> Geo {
    if input.is_empty() {
        return input;
    }
    match *g {
        Generator::Subdivision { levels } => subdivide(&input, levels),
        Generator::Array { mode, count, offset, rotation, scale, radius } => {
            let max_copies = (MAX_TRIANGLES / input.triangle_count().max(1)).max(1) as u32;
            let count = count.min(max_copies);
            let mut out = Geo::default();
            for i in 0..count {
                let m = match mode {
                    ArrayMode::Linear => {
                        let k = i as f32;
                        Mat4::from_translation(offset * k)
                            * Mat4::from_quat(hpb_to_quat(rotation * k))
                            * Mat4::from_scale(Vec3::new(scale.x.powf(k), scale.y.powf(k), scale.z.powf(k)))
                    }
                    ArrayMode::Radial => {
                        let angle = std::f32::consts::TAU * i as f32 / count as f32;
                        Mat4::from_rotation_y(angle) * Mat4::from_translation(Vec3::new(radius, 0.0, 0.0))
                    }
                };
                out.append_transformed(&input, m);
            }
            out
        }
        // Booleans need their operands separately; see `base_geo`.
        Generator::Boolean { .. } => input,
        Generator::Symmetry { plane } => {
            if input.triangle_count() * 2 > MAX_TRIANGLES {
                return input;
            }
            let mut out = Geo::default();
            out.append_transformed(&input, Mat4::IDENTITY);
            out.append_transformed(&input, plane.matrix());
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Catmull-Clark subdivision
// ---------------------------------------------------------------------------

/// Re-forms the quads that primitives are built from: two consecutive triangles
/// `(a, b, c)` and `(a, c, d)` sharing the edge `a-c` become `(a, b, c, d)`.
/// Mirrored copies (reversed winding) are recognised too. Everything else stays a triangle.
pub fn polygons(geo: &Geo) -> Vec<Vec<u32>> {
    let tris: Vec<[u32; 3]> = geo.tris.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    let mut out = Vec::with_capacity(tris.len() / 2 + 1);
    let mut i = 0;
    while i < tris.len() {
        if i + 1 < tris.len()
            && let Some(q) = merge_pair(tris[i], tris[i + 1]) {
                out.push(q.to_vec());
                i += 2;
                continue;
            }
        out.push(tris[i].to_vec());
        i += 1;
    }
    out
}

fn merge_pair(t1: [u32; 3], t2: [u32; 3]) -> Option<[u32; 4]> {
    for r in 0..3 {
        let (a, b, c) = (t1[r], t1[(r + 1) % 3], t1[(r + 2) % 3]);
        for s in 0..3 {
            if t2[s] == a && t2[(s + 1) % 3] == c {
                let d = t2[(s + 2) % 3];
                if d != a && d != b && d != c {
                    return Some([a, b, c, d]);
                }
            }
        }
    }
    None
}

/// Merges vertices that share a position and drops faces that collapse.
pub fn weld(geo: &Geo, faces: Vec<Vec<u32>>) -> (Vec<Vec3>, Vec<Vec<u32>>) {
    let mut index: HashMap<[i64; 3], u32> = HashMap::new();
    let mut pos: Vec<Vec3> = Vec::new();
    let remap: Vec<u32> = geo
        .verts
        .iter()
        .map(|v| {
            let p = Vec3::from(v.pos);
            let key = [(p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64];
            *index.entry(key).or_insert_with(|| {
                pos.push(p);
                (pos.len() - 1) as u32
            })
        })
        .collect();
    let faces = faces
        .into_iter()
        .filter_map(|f| {
            let mut w: Vec<u32> = Vec::with_capacity(f.len());
            for i in f.into_iter().map(|i| remap[i as usize]) {
                if w.last() != Some(&i) {
                    w.push(i);
                }
            }
            while w.len() > 1 && w.first() == w.last() {
                w.pop();
            }
            let mut unique = w.clone();
            unique.sort_unstable();
            unique.dedup();
            (w.len() >= 3 && unique.len() == w.len()).then_some(w)
        })
        .collect();
    (pos, faces)
}

pub fn edge_key(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

/// One Catmull-Clark step. Works on any polygons; the result is all quads.
pub fn catmull_clark(pos: &[Vec3], faces: &[Vec<u32>]) -> (Vec<Vec3>, Vec<Vec<u32>>) {
    let face_pts: Vec<Vec3> = faces
        .iter()
        .map(|f| f.iter().map(|&i| pos[i as usize]).sum::<Vec3>() / f.len() as f32)
        .collect();

    // edge -> adjacent faces
    let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for k in 0..f.len() {
            edge_faces.entry(edge_key(f[k], f[(k + 1) % f.len()])).or_default().push(fi);
        }
    }

    // Per-vertex adjacency.
    let mut v_faces: Vec<Vec<usize>> = vec![Vec::new(); pos.len()];
    for (fi, f) in faces.iter().enumerate() {
        for &v in f {
            v_faces[v as usize].push(fi);
        }
    }
    let mut v_edges: Vec<Vec<(u32, u32)>> = vec![Vec::new(); pos.len()];
    for &e in edge_faces.keys() {
        v_edges[e.0 as usize].push(e);
        v_edges[e.1 as usize].push(e);
    }

    let mut out: Vec<Vec3> = Vec::with_capacity(pos.len() + edge_faces.len() + faces.len());
    for (i, &p) in pos.iter().enumerate() {
        let edges = &v_edges[i];
        let boundary: Vec<u32> = edges
            .iter()
            .filter(|e| edge_faces[e].len() != 2)
            .map(|e| if e.0 as usize == i { e.1 } else { e.0 })
            .collect();
        let np = if edges.is_empty() {
            p
        } else if boundary.is_empty() {
            let n = edges.len() as f32;
            let f = v_faces[i].iter().map(|&fi| face_pts[fi]).sum::<Vec3>() / v_faces[i].len() as f32;
            let r = edges.iter().map(|e| (pos[e.0 as usize] + pos[e.1 as usize]) * 0.5).sum::<Vec3>() / n;
            (f + r * 2.0 + p * (n - 3.0)) / n
        } else if boundary.len() == 2 {
            (p * 6.0 + pos[boundary[0] as usize] + pos[boundary[1] as usize]) / 8.0
        } else {
            p // corner or non-manifold: pinned
        };
        out.push(np);
    }

    let mut keys: Vec<(u32, u32)> = edge_faces.keys().copied().collect();
    keys.sort_unstable();
    let mut edge_pt: HashMap<(u32, u32), u32> = HashMap::with_capacity(keys.len());
    for k in keys {
        let adj = &edge_faces[&k];
        let mid = (pos[k.0 as usize] + pos[k.1 as usize]) * 0.5;
        let p = if adj.len() == 2 { (mid * 2.0 + face_pts[adj[0]] + face_pts[adj[1]]) * 0.25 } else { mid };
        edge_pt.insert(k, out.len() as u32);
        out.push(p);
    }

    let mut new_faces = Vec::with_capacity(faces.iter().map(Vec::len).sum());
    for (fi, f) in faces.iter().enumerate() {
        let fp = out.len() as u32;
        out.push(face_pts[fi]);
        let n = f.len();
        for k in 0..n {
            let (prev, cur, next) = (f[(k + n - 1) % n], f[k], f[(k + 1) % n]);
            new_faces.push(vec![cur, edge_pt[&edge_key(cur, next)], fp, edge_pt[&edge_key(prev, cur)]]);
        }
    }
    (out, new_faces)
}

fn subdivide(input: &Geo, levels: u32) -> Geo {
    if levels == 0 {
        return input.clone();
    }
    let (mut pos, mut faces) = weld(input, polygons(input));
    for _ in 0..levels {
        let next_quads: usize = faces.iter().map(Vec::len).sum();
        if next_quads * 2 > MAX_TRIANGLES {
            break;
        }
        let (p, f) = catmull_clark(&pos, &faces);
        pos = p;
        faces = f;
    }

    // Triangulate (fans) and compute smooth, area-weighted vertex normals.
    let mut tris: Vec<u32> = Vec::with_capacity(faces.len() * 6);
    for f in &faces {
        for k in 1..f.len() - 1 {
            tris.extend_from_slice(&[f[0], f[k], f[k + 1]]);
        }
    }
    let mut normals = vec![Vec3::ZERO; pos.len()];
    for t in tris.chunks_exact(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let n = (b - a).cross(c - a);
        for &i in t {
            normals[i as usize] += n;
        }
    }
    let verts = pos
        .iter()
        .zip(&normals)
        .map(|(p, n)| Vertex { pos: p.to_array(), normal: n.normalize_or_zero().to_array() })
        .collect();

    // Wire display: polygon edges only (no triangulation diagonals).
    let mut seen = std::collections::HashSet::new();
    let mut edges = Vec::new();
    for f in &faces {
        for k in 0..f.len() {
            let key = edge_key(f[k], f[(k + 1) % f.len()]);
            if seen.insert(key) {
                edges.extend_from_slice(&[key.0, key.1]);
            }
        }
    }
    Geo { verts, tris, edges }
}

// ---------------------------------------------------------------------------
// Cache support
// ---------------------------------------------------------------------------

/// Hash of everything that influences a generator's output.
pub fn content_hash(scene: &Scene, id: ObjectId) -> u64 {
    fn walk(scene: &Scene, id: ObjectId, h: &mut DefaultHasher) {
        if let Some(o) = scene.get(id) {
            match &o.kind {
                // Big meshes: hash the data directly instead of formatting it.
                ObjectKind::Polygon(m) => m.hash(h),
                k => format!("{k:?}").hash(h),
            }
            format!("{:?}|{:?}|{:?}|{}", o.pos, o.rot, o.scale, o.enabled).hash(h);
            for &c in &o.children {
                walk(scene, c, h);
            }
            o.children.len().hash(h);
        }
    }
    let mut h = DefaultHasher::new();
    walk(scene, id, &mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::PrimitiveKind;

    fn cube_geo() -> Geo {
        Geo::from_mesh(&PrimitiveKind::Cube.default_primitive().build())
    }

    fn signed_volume(g: &Geo) -> f32 {
        g.tris
            .chunks_exact(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(g.verts[i as usize].pos);
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }

    fn edge_face_counts(g: &Geo) -> HashMap<(u32, u32), usize> {
        let tris: Vec<Vec<u32>> = g.tris.chunks_exact(3).map(|t| t.to_vec()).collect();
        let (_, faces) = weld(g, tris);
        let mut m = HashMap::new();
        for f in &faces {
            for k in 0..f.len() {
                *m.entry(edge_key(f[k], f[(k + 1) % f.len()])).or_insert(0) += 1;
            }
        }
        m
    }

    fn bounds(g: &Geo) -> (Vec3, Vec3) {
        g.verts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos)))
        })
    }

    #[test]
    fn quads_are_recovered_from_primitives() {
        let cube = cube_geo();
        let polys = polygons(&cube);
        assert_eq!(polys.len(), 6);
        assert!(polys.iter().all(|p| p.len() == 4));
        // A mirrored copy (reversed winding) still pairs up.
        let mut mirrored = Geo::default();
        mirrored.append_transformed(&cube, MirrorPlane::YZ.matrix());
        assert!(polygons(&mirrored).iter().all(|p| p.len() == 4));
        // Sphere poles collapse to triangles after welding instead of degenerate quads.
        let sphere = Geo::from_mesh(&PrimitiveKind::Sphere.default_primitive().build());
        let (_, faces) = weld(&sphere, polygons(&sphere));
        assert!(faces.iter().any(|f| f.len() == 3) && faces.iter().any(|f| f.len() == 4));
    }

    #[test]
    fn subdividing_closed_primitives_keeps_them_watertight() {
        for kind in [PrimitiveKind::Sphere, PrimitiveKind::Cylinder, PrimitiveKind::Cone, PrimitiveKind::Torus] {
            let g = Geo::from_mesh(&kind.default_primitive().build());
            let sub = subdivide(&g, 1);
            let counts = edge_face_counts(&sub);
            assert!(counts.values().all(|&n| n == 2), "{kind:?} has open or non-manifold edges");
            assert!(signed_volume(&sub) > 0.0, "{kind:?} turned inside out");
        }
    }

    #[test]
    fn subdividing_a_cube_stays_closed_and_gets_rounder() {
        let cube = cube_geo();
        let sub = subdivide(&cube, 2);
        assert!(sub.triangle_count() == cube.triangle_count() * 16);
        assert!(edge_face_counts(&sub).values().all(|&n| n == 2), "mesh must stay watertight");
        let v0 = signed_volume(&cube);
        let v1 = signed_volume(&sub);
        assert!(v1 > 0.0 && v1 < v0, "volume {v1} should shrink below {v0}");
        // Symmetric input gives symmetric output (Loop on triangle diagonals did not).
        let (lo, hi) = bounds(&sub);
        assert!((lo + hi).length() < 1e-2, "{lo:?} {hi:?}");
        assert!((hi.x - hi.y).abs() < 1e-2 && (hi.y - hi.z).abs() < 1e-2);
        // Wireframe shows the quad cage only: 4 edges per quad shared by 2 = 2 per quad.
        assert_eq!(sub.edges.len() / 2, sub.triangle_count());
        // Normals are unit length and point outwards.
        for v in &sub.verts {
            let n = Vec3::from(v.normal);
            assert!((n.length() - 1.0).abs() < 1e-3);
            assert!(n.dot(Vec3::from(v.pos)) > 0.0);
        }
    }

    #[test]
    fn level_zero_is_a_pass_through_and_open_meshes_survive() {
        let cube = cube_geo();
        assert_eq!(subdivide(&cube, 0).tris, cube.tris);
        let plane = Geo::from_mesh(&PrimitiveKind::Plane.default_primitive().build());
        let sub = subdivide(&plane, 1);
        assert!(sub.verts.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
        assert_eq!(sub.triangle_count(), plane.triangle_count() * 4);
    }

    #[test]
    fn array_repeats_the_input() {
        let cube = cube_geo();
        let g = Generator::Array {
            mode: ArrayMode::Linear,
            count: 4,
            offset: Vec3::new(300.0, 0.0, 0.0),
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            radius: 0.0,
        };
        let out = apply(&g, cube.clone());
        assert_eq!(out.triangle_count(), cube.triangle_count() * 4);
        let max_x = out.verts.iter().map(|v| v.pos[0]).fold(f32::MIN, f32::max);
        assert!((max_x - (100.0 + 900.0)).abs() < 1e-2, "{max_x}");
        let expected = 4.0 * signed_volume(&cube);
        assert!((signed_volume(&out) - expected).abs() < expected * 1e-3, "{} vs {expected}", signed_volume(&out));
    }

    #[test]
    fn radial_array_places_copies_on_a_circle() {
        let g = Generator::Array {
            mode: ArrayMode::Radial,
            count: 4,
            offset: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            radius: 500.0,
        };
        let out = apply(&g, cube_geo());
        let max_z = out.verts.iter().map(|v| v.pos[2]).fold(f32::MIN, f32::max);
        assert!(max_z > 590.0 && max_z < 610.0, "{max_z}");
    }

    #[test]
    fn symmetry_mirrors_with_consistent_winding() {
        let mut cube = Geo::default();
        cube.append_transformed(&cube_geo(), Mat4::from_translation(Vec3::new(200.0, 0.0, 0.0)));
        let out = apply(&Generator::Symmetry { plane: MirrorPlane::YZ }, cube.clone());
        assert_eq!(out.triangle_count(), cube.triangle_count() * 2);
        let expected = 2.0 * signed_volume(&cube);
        assert!((signed_volume(&out) - expected).abs() < expected.abs() * 1e-3, "mirrored half must not be inside-out: {} vs {expected}", signed_volume(&out));
        let min_x = out.verts.iter().map(|v| v.pos[0]).fold(f32::MAX, f32::min);
        assert!((min_x + 300.0).abs() < 1e-2);
        // Mirrored normals still point away from the mirrored cube's centre.
        for v in out.verts.iter().skip(cube.verts.len()) {
            let centre = Vec3::new(-200.0, 0.0, 0.0);
            assert!(Vec3::from(v.normal).dot(Vec3::from(v.pos) - centre) > 0.0);
        }
    }

    #[test]
    fn generators_nest_and_respect_child_transforms() {
        let mut scene = Scene::new();
        let sym = scene.add_generator(GeneratorKind::Symmetry, None);
        let arr = scene.add_generator(GeneratorKind::Array, Some(sym));
        let cube = scene.add_primitive(PrimitiveKind::Cube, Some(arr));
        scene.get_mut(cube).unwrap().pos = Vec3::new(0.0, 0.0, 500.0);
        let geo = eval_geo(&scene, sym, &mut |p| Arc::new(p.build()));
        assert_eq!(geo.triangle_count(), 12 * 3 * 2);
        let min_z = geo.verts.iter().map(|v| v.pos[2]).fold(f32::MAX, f32::min);
        assert!((min_z - 400.0).abs() < 1e-2, "mirror across YZ must not move Z: {min_z}");
    }

    #[test]
    fn empty_generators_and_hostile_parameters_are_safe() {
        let mut scene = Scene::new();
        let g = scene.add_generator(GeneratorKind::Subdivision, None);
        assert!(eval_geo(&scene, g, &mut |p| Arc::new(p.build())).is_empty());
        let hostile = Generator::Array {
            mode: ArrayMode::Linear,
            count: u32::MAX,
            offset: Vec3::splat(f32::NAN),
            rotation: Vec3::splat(f32::INFINITY),
            scale: Vec3::ZERO,
            radius: f32::NAN,
        }
        .sanitized();
        let out = apply(&hostile, cube_geo());
        assert!(out.triangle_count() <= MAX_TRIANGLES);
        assert!(out.verts.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
        assert_eq!(Generator::Subdivision { levels: 99 }.sanitized(), Generator::Subdivision { levels: MAX_LEVELS });
    }

    #[test]
    fn boolean_generator_uses_its_first_two_children() {
        let mut scene = Scene::new();
        let b = scene.add_generator(GeneratorKind::Boolean, None);
        let big = scene.add_primitive(PrimitiveKind::Cube, Some(b));
        let small = scene.add_primitive(PrimitiveKind::Cube, Some(b));
        scene.get_mut(small).unwrap().pos = Vec3::splat(100.0);
        let geo = eval_geo(&scene, b, &mut |p| Arc::new(p.build()));
        let v = signed_volume(&geo);
        assert!((v - 7.0e6).abs() < 7.0e6 * 1e-4, "{v}");
        // Switching the mode re-evaluates.
        if let ObjectKind::Generator(Generator::Boolean { mode }) = &mut scene.get_mut(b).unwrap().kind {
            *mode = BoolMode::Intersect;
        }
        let v = signed_volume(&eval_geo(&scene, b, &mut |p| Arc::new(p.build())));
        assert!((v - 1.0e6).abs() < 1.0e6 * 1e-4, "{v}");
        // With one operand, A passes through.
        scene.remove(&[small]);
        if let ObjectKind::Generator(Generator::Boolean { mode }) = &mut scene.get_mut(b).unwrap().kind {
            *mode = BoolMode::Subtract;
        }
        let v = signed_volume(&eval_geo(&scene, b, &mut |p| Arc::new(p.build())));
        assert!((v - 8.0e6).abs() < 8.0e6 * 1e-4, "{v}");
        let _ = big;
    }

    #[test]
    fn hash_changes_with_content() {
        let mut scene = Scene::new();
        let g = scene.add_generator(GeneratorKind::Array, None);
        let c = scene.add_primitive(PrimitiveKind::Cube, Some(g));
        let a = content_hash(&scene, g);
        assert_eq!(a, content_hash(&scene, g));
        scene.get_mut(c).unwrap().pos.x += 1.0;
        assert_ne!(a, content_hash(&scene, g));
    }
}
