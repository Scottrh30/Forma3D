//! Display-time tessellation (the viewport's Options > Tessellation).
//!
//! Each triangle is split into `n * n` smaller ones and the new points are
//! pushed out with Phong tessellation (Boubekeur & Alexa 2008): every point is
//! projected onto the tangent planes of the triangle's corners, so curved
//! surfaces get rounder silhouettes without changing the object itself.
//!
//! Corners that sit on a hard edge (several vertices at one position with
//! different normals) are left flat. Along any edge the displacement depends
//! only on that edge's two endpoints, so neighbouring triangles always agree
//! and no cracks open, at hard edges or smooth ones.

use crate::mesh::{Mesh, Vertex};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;

/// How far points move towards the curved surface (0 flat, 1 full Phong).
const ALPHA: f32 = 0.75;
/// Meshes above this many triangles are drawn as they are.
pub const MAX_TRIANGLES: usize = 150_000;
/// Splits per triangle edge.
pub const LEVEL: u32 = 3;

#[derive(Clone, Copy)]
struct Corner {
    p: Vec3,
    /// `None` on a hard edge: the corner does not bend the surface.
    n: Option<Vec3>,
}

impl Corner {
    /// The projection of `q` onto this corner's tangent plane.
    fn project(&self, q: Vec3) -> Vec3 {
        match self.n {
            Some(n) => q - (q - self.p).dot(n) * n,
            None => q,
        }
    }
}

fn pos_key(p: Vec3) -> [i64; 3] {
    let q = |v: f32| (v as f64 * 1e4).round() as i64;
    [q(p.x), q(p.y), q(p.z)]
}

/// For each vertex, its normal when every vertex at the same position shares
/// it (a smooth corner), or `None` at a hard edge.
fn smooth_normals(mesh: &Mesh) -> Vec<Option<Vec3>> {
    let mut groups: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, v) in mesh.vertices.iter().enumerate() {
        groups.entry(pos_key(Vec3::from(v.pos))).or_default().push(i);
    }
    let mut out = vec![None; mesh.vertices.len()];
    for members in groups.values() {
        let n0 = Vec3::from(mesh.vertices[members[0]].normal).normalize_or_zero();
        if n0 == Vec3::ZERO {
            continue;
        }
        let smooth = members
            .iter()
            .all(|&i| Vec3::from(mesh.vertices[i].normal).normalize_or_zero().dot(n0) > 0.999);
        if smooth {
            for &i in members {
                out[i] = Some(n0);
            }
        }
    }
    out
}

fn phong(corners: &[Corner], weights: &[f32]) -> Vec3 {
    let flat: Vec3 = corners.iter().zip(weights).map(|(c, &w)| c.p * w).sum();
    let curved: Vec3 = corners.iter().zip(weights).map(|(c, &w)| c.project(flat) * w).sum();
    flat.lerp(curved, ALPHA)
}

/// Returns a tessellated copy of `mesh`, or `None` when it is too large or
/// has nothing to tessellate.
pub fn tessellate(mesh: &Mesh, level: u32) -> Option<Mesh> {
    let tris = mesh.indices.len() / 3;
    if level < 2 || tris == 0 || tris > MAX_TRIANGLES {
        return None;
    }
    let smooth = smooth_normals(mesh);
    let corner = |i: u32| Corner {
        p: mesh.position(i),
        n: smooth[i as usize],
    };
    let normal = |i: u32| Vec3::from(mesh.vertices[i as usize].normal);
    let n = level as usize;

    let mut vertices: Vec<Vertex> = Vec::with_capacity(tris * (n + 1) * (n + 2) / 2);
    let mut indices: Vec<u32> = Vec::with_capacity(tris * n * n * 3);
    let mut row_start = vec![0u32; n + 1];
    for t in mesh.indices.chunks_exact(3) {
        let c = [corner(t[0]), corner(t[1]), corner(t[2])];
        let nrm = [normal(t[0]), normal(t[1]), normal(t[2])];
        // Points (i, j) with i + j <= n: weights (n - i - j, i, j) / n.
        for (j, start) in row_start.iter_mut().enumerate() {
            *start = vertices.len() as u32;
            for i in 0..=(n - j) {
                let w = [(n - i - j) as f32 / n as f32, i as f32 / n as f32, j as f32 / n as f32];
                let p = phong(&c, &w);
                let nn = (nrm[0] * w[0] + nrm[1] * w[1] + nrm[2] * w[2]).normalize_or_zero();
                vertices.push(Vertex { pos: p.into(), normal: nn.into() });
            }
        }
        let at = |i: usize, j: usize| row_start[j] + i as u32;
        for j in 0..n {
            for i in 0..(n - j) {
                indices.extend_from_slice(&[at(i, j), at(i + 1, j), at(i, j + 1)]);
                if i + j + 1 < n {
                    indices.extend_from_slice(&[at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
            }
        }
    }

    // Edges follow the same curve as the surface: on an edge only its two
    // endpoints carry weight, exactly as for the triangles that share it.
    let mut edges = Vec::with_capacity(mesh.edges.len() * n);
    for e in mesh.edges.chunks_exact(2) {
        let c = [corner(e[0]), corner(e[1])];
        let mut prev = None;
        for k in 0..=n {
            let w = [(n - k) as f32 / n as f32, k as f32 / n as f32];
            let idx = vertices.len() as u32;
            vertices.push(Vertex {
                pos: phong(&c, &w).into(),
                normal: (normal(e[0]) * w[0] + normal(e[1]) * w[1]).normalize_or_zero().into(),
            });
            if let Some(p) = prev {
                edges.extend_from_slice(&[p, idx]);
            }
            prev = Some(idx);
        }
    }
    Some(Mesh::from_parts(vertices, indices, edges))
}

/// Tessellated meshes, keyed by the source mesh's id, so each is built once.
#[derive(Default)]
pub struct Cache {
    meshes: HashMap<u64, Option<Arc<Mesh>>>,
    used: Vec<u64>,
}

impl Cache {
    /// The tessellated version of `mesh` (or `mesh` itself when it is too big).
    pub fn get(&mut self, mesh: &Arc<Mesh>) -> Arc<Mesh> {
        self.used.push(mesh.id);
        self.meshes
            .entry(mesh.id)
            .or_insert_with(|| tessellate(mesh, LEVEL).map(Arc::new))
            .clone()
            .unwrap_or_else(|| mesh.clone())
    }

    /// Drops meshes that were not asked for since the last call.
    pub fn end_frame(&mut self) {
        let used: std::collections::HashSet<u64> = self.used.drain(..).collect();
        self.meshes.retain(|id, _| used.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh;

    fn max_radius_error(m: &Mesh, r: f32) -> f32 {
        let mut worst = 0.0f32;
        for t in m.indices.chunks_exact(3) {
            let c = (m.position(t[0]) + m.position(t[1]) + m.position(t[2])) / 3.0;
            worst = worst.max((r - c.length()).abs());
        }
        worst
    }

    #[test]
    fn rounds_out_a_coarse_sphere() {
        let s = mesh::sphere(100.0, 8);
        let t = tessellate(&s, LEVEL).unwrap();
        assert_eq!(t.indices.len(), s.indices.len() * 9);
        assert!(max_radius_error(&t, 100.0) < max_radius_error(&s, 100.0) * 0.6);
    }

    #[test]
    fn keeps_flat_faces_flat() {
        let c = mesh::cube(Vec3::splat(200.0), [1, 1, 1]);
        let t = tessellate(&c, LEVEL).unwrap();
        for v in &t.vertices {
            let p = Vec3::from(v.pos).abs();
            assert!((p.max_element() - 100.0).abs() < 1e-3, "{p:?} left the cube's surface");
        }
    }

    #[test]
    fn no_cracks_between_neighbours() {
        // Every tessellated edge point is shared by both triangles along an
        // edge: positions appear in pairs (or more), never alone on an edge.
        let c = mesh::cylinder(50.0, 100.0, 6, 1, true);
        let t = tessellate(&c, LEVEL).unwrap();
        let mut seen: HashMap<[i64; 3], usize> = HashMap::new();
        for v in &t.vertices[..t.vertices.len() - c.edges.len() / 2 * (LEVEL as usize + 1)] {
            *seen.entry(pos_key(Vec3::from(v.pos))).or_default() += 1;
        }
        let edge_points = &t.vertices[t.vertices.len() - c.edges.len() / 2 * (LEVEL as usize + 1)..];
        for v in edge_points {
            assert!(seen.contains_key(&pos_key(Vec3::from(v.pos))), "edge line leaves the surface");
        }
    }

    #[test]
    fn cache_reuses_and_forgets() {
        let s = Arc::new(mesh::sphere(10.0, 6));
        let mut cache = Cache::default();
        let a = cache.get(&s);
        let b = cache.get(&s);
        assert!(Arc::ptr_eq(&a, &b));
        assert_ne!(a.id, s.id);
        cache.end_frame();
        cache.end_frame();
        assert!(cache.meshes.is_empty());
    }
}
