//! CPU-side triangle meshes and the procedural generators behind the primitives.
//!
//! Every primitive is assembled from *parametric surfaces*: a grid of
//! `(nu + 1) * (nv + 1)` vertices sampled from `f(u, v)` with `u, v` in `0..=1`.
//! The convention is that `df/du x df/dv` points out of the solid, which keeps
//! triangle winding consistent. The iso-lines of each grid double as the
//! quad-style edge list that is drawn in "Gouraud Shading (Lines)" mode.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicU64, Ordering};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
}

static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

/// An immutable triangle mesh plus a line list of its "quad" edges.
#[derive(Debug)]
pub struct Mesh {
    /// Process-unique id, used as the GPU buffer cache key.
    pub id: u64,
    pub vertices: Vec<Vertex>,
    /// Triangle list.
    pub indices: Vec<u32>,
    /// Line list (pairs of vertex indices).
    pub edges: Vec<u32>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl Mesh {
    pub fn position(&self, index: u32) -> Vec3 {
        Vec3::from(self.vertices[index as usize].pos)
    }

    pub fn bounds_center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }

    pub fn bounds_radius(&self) -> f32 {
        (self.bounds_max - self.bounds_min).length() * 0.5
    }
}

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    edges: Vec<u32>,
}

impl MeshBuilder {
    fn push_vertex(&mut self, pos: Vec3, normal: Vec3) -> u32 {
        self.vertices.push(Vertex {
            pos: pos.to_array(),
            normal: normal.to_array(),
        });
        (self.vertices.len() - 1) as u32
    }

    fn push_edge(&mut self, a: u32, b: u32) {
        let pa = Vec3::from(self.vertices[a as usize].pos);
        let pb = Vec3::from(self.vertices[b as usize].pos);
        // Collapsed rows (sphere poles, cap centres) would draw as dots.
        if pa.distance_squared(pb) > 1e-10 {
            self.edges.push(a);
            self.edges.push(b);
        }
    }

    /// Adds a parametric surface. `f(u, v)` returns `(position, normal)`.
    fn surface(&mut self, nu: u32, nv: u32, f: impl Fn(f32, f32) -> (Vec3, Vec3)) {
        let nu = nu.max(1);
        let nv = nv.max(1);
        let base = self.vertices.len() as u32;
        let stride = nu + 1;
        for j in 0..=nv {
            for i in 0..=nu {
                let (p, n) = f(i as f32 / nu as f32, j as f32 / nv as f32);
                self.push_vertex(p, n);
            }
        }
        let at = |i: u32, j: u32| base + j * stride + i;
        for j in 0..nv {
            for i in 0..nu {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                self.indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        for j in 0..=nv {
            for i in 0..nu {
                self.push_edge(at(i, j), at(i + 1, j));
            }
        }
        for i in 0..=nu {
            for j in 0..nv {
                self.push_edge(at(i, j), at(i, j + 1));
            }
        }
    }

    /// A flat rectangle: `origin + u * du + v * dv` with a constant normal.
    fn flat(&mut self, origin: Vec3, du: Vec3, dv: Vec3, nu: u32, nv: u32) {
        let normal = du.cross(dv).normalize_or_zero();
        self.surface(nu, nv, |u, v| (origin + du * u + dv * v, normal));
    }

    fn finish(self) -> Mesh {
        Mesh::from_parts(self.vertices, self.indices, self.edges)
    }
}

impl Mesh {
    /// Builds a mesh from raw buffers, computing its bounds.
    pub fn from_parts(vertices: Vec<Vertex>, indices: Vec<u32>, edges: Vec<u32>) -> Mesh {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &vertices {
            min = min.min(Vec3::from(v.pos));
            max = max.max(Vec3::from(v.pos));
        }
        if vertices.is_empty() {
            min = Vec3::ZERO;
            max = Vec3::ZERO;
        }
        Mesh {
            id: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed),
            vertices,
            indices,
            edges,
            bounds_min: min,
            bounds_max: max,
        }
    }
}

pub fn cube(size: Vec3, seg: [u32; 3]) -> Mesh {
    let mut b = MeshBuilder::default();
    let h = size * 0.5;
    let (sx, sy, sz) = (size.x, size.y, size.z);
    let [gx, gy, gz] = seg;
    // Each face is (origin, u axis, v axis) with u x v pointing outwards.
    b.flat(Vec3::new(h.x, -h.y, -h.z), Vec3::Y * sy, Vec3::Z * sz, gy, gz); // +X
    b.flat(Vec3::new(-h.x, -h.y, -h.z), Vec3::Z * sz, Vec3::Y * sy, gz, gy); // -X
    b.flat(Vec3::new(-h.x, h.y, -h.z), Vec3::Z * sz, Vec3::X * sx, gz, gx); // +Y
    b.flat(Vec3::new(-h.x, -h.y, -h.z), Vec3::X * sx, Vec3::Z * sz, gx, gz); // -Y
    b.flat(Vec3::new(-h.x, -h.y, h.z), Vec3::X * sx, Vec3::Y * sy, gx, gy); // +Z
    b.flat(Vec3::new(-h.x, -h.y, -h.z), Vec3::Y * sy, Vec3::X * sx, gy, gx); // -Z
    b.finish()
}

/// A plane in the XZ plane facing +Y, `width` along X and `height` along Z.
pub fn plane(width: f32, height: f32, wseg: u32, hseg: u32) -> Mesh {
    let mut b = MeshBuilder::default();
    b.flat(
        Vec3::new(-width * 0.5, 0.0, -height * 0.5),
        Vec3::Z * height,
        Vec3::X * width,
        hseg,
        wseg,
    );
    b.finish()
}

pub fn sphere(radius: f32, segments: u32) -> Mesh {
    let segments = segments.max(3);
    let rings = (segments / 2).max(2);
    let mut b = MeshBuilder::default();
    b.surface(segments, rings, |u, v| {
        let (phi, theta) = (TAU * u, PI * v);
        let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
        (n * radius, n)
    });
    b.finish()
}

/// Axis-aligned cap disc. `up == true` faces +Y, otherwise -Y.
fn cap(b: &mut MeshBuilder, radius: f32, y: f32, segments: u32, up: bool) {
    let normal = if up { Vec3::Y } else { Vec3::NEG_Y };
    b.surface(segments, 1, |u, v| {
        let phi = TAU * u;
        let r = if up { radius * v } else { radius * (1.0 - v) };
        (Vec3::new(r * phi.cos(), y, r * phi.sin()), normal)
    });
}

pub fn cylinder(radius: f32, height: f32, segments: u32, hseg: u32, caps: bool) -> Mesh {
    let segments = segments.max(3);
    let mut b = MeshBuilder::default();
    let half = height * 0.5;
    b.surface(segments, hseg, |u, v| {
        let phi = TAU * u;
        let dir = Vec3::new(phi.cos(), 0.0, phi.sin());
        (dir * radius + Vec3::Y * (half - v * height), dir)
    });
    if caps {
        cap(&mut b, radius, half, segments, true);
        cap(&mut b, radius, -half, segments, false);
    }
    b.finish()
}

pub fn cone(radius: f32, height: f32, segments: u32, hseg: u32, caps: bool) -> Mesh {
    let segments = segments.max(3);
    let mut b = MeshBuilder::default();
    let half = height * 0.5;
    b.surface(segments, hseg, |u, v| {
        let phi = TAU * u;
        let dir = Vec3::new(phi.cos(), 0.0, phi.sin());
        let pos = dir * (radius * v) + Vec3::Y * (half - v * height);
        let normal = (dir * height + Vec3::Y * radius).normalize_or_zero();
        (pos, normal)
    });
    if caps {
        cap(&mut b, radius, -half, segments, false);
    }
    b.finish()
}

/// A pyramid with a `size.x` by `size.z` base at `-size.y / 2` and its apex at
/// `+size.y / 2`. Each face is split into `segments` rows and columns.
pub fn pyramid(size: Vec3, segments: u32) -> Mesh {
    let n = segments.max(1);
    let h = size * 0.5;
    let mut b = MeshBuilder::default();
    let apex = Vec3::new(0.0, h.y, 0.0);
    let base = [
        Vec3::new(-h.x, -h.y, -h.z),
        Vec3::new(h.x, -h.y, -h.z),
        Vec3::new(h.x, -h.y, h.z),
        Vec3::new(-h.x, -h.y, h.z),
    ];
    for k in 0..4 {
        let (mut a, mut c) = (base[k], base[(k + 1) % 4]);
        let mut normal = (c - a).cross(apex - a).normalize_or_zero();
        // Keep every side facing out, whatever order the corners come in.
        let centre = (a + c + apex) / 3.0;
        if normal.dot(centre) < 0.0 {
            std::mem::swap(&mut a, &mut c);
            normal = -normal;
        }
        b.surface(n, n, |u, v| (a.lerp(c, u).lerp(apex, v), normal));
    }
    // The base, facing down (X then Z, as for the cube's -Y face).
    b.flat(base[0], Vec3::X * size.x, Vec3::Z * size.z, n, n);
    b.finish()
}

/// A tube: outer and inner walls plus flat rings at both ends.
pub fn tube(inner: f32, outer: f32, height: f32, segments: u32, hseg: u32) -> Mesh {
    let segments = segments.max(3);
    let half = height * 0.5;
    let mut b = MeshBuilder::default();
    let dir = |u: f32| {
        let phi = TAU * u;
        Vec3::new(phi.cos(), 0.0, phi.sin())
    };
    // Outer wall, facing out (top to bottom, as for the cylinder).
    b.surface(segments, hseg, |u, v| (dir(u) * outer + Vec3::Y * (half - v * height), dir(u)));
    // Inner wall, facing the hole (bottom to top flips the winding).
    b.surface(segments, hseg, |u, v| (dir(u) * inner + Vec3::Y * (-half + v * height), -dir(u)));
    // Top ring (inside to outside faces up), bottom ring (outside to inside faces down).
    b.surface(segments, 1, |u, v| (dir(u) * (inner + (outer - inner) * v) + Vec3::Y * half, Vec3::Y));
    b.surface(segments, 1, |u, v| (dir(u) * (outer - (outer - inner) * v) - Vec3::Y * half, Vec3::NEG_Y));
    b.finish()
}

/// A capsule along Y: a cylinder of `radius` with half-sphere ends, `height`
/// long overall (at least the diameter).
pub fn capsule(radius: f32, height: f32, segments: u32, hseg: u32) -> Mesh {
    let segments = segments.max(3);
    let half = (height * 0.5 - radius).max(0.0);
    let cap_rings = (segments / 4).max(2);
    // The profile from the top pole to the bottom pole: (radius, y, normal in the (r, y) plane).
    let mut profile: Vec<(f32, f32, f32, f32)> = Vec::new();
    for i in 0..=cap_rings {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / cap_rings as f32; // 0 at the pole
        profile.push((radius * a.sin(), half + radius * a.cos(), a.sin(), a.cos()));
    }
    for i in 1..hseg.max(1) {
        let y = half - 2.0 * half * i as f32 / hseg.max(1) as f32;
        profile.push((radius, y, 1.0, 0.0));
    }
    for i in 0..=cap_rings {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / cap_rings as f32; // 0 at the equator
        profile.push((radius * a.cos(), -half - radius * a.sin(), a.cos(), -a.sin()));
    }
    let rows = (profile.len() - 1) as u32;
    let mut b = MeshBuilder::default();
    // Top to bottom, like the cylinder wall, so the faces point out.
    b.surface(segments, rows, |u, v| {
        let (r, y, nr, ny) = profile[((v * rows as f32).round() as usize).min(profile.len() - 1)];
        let phi = TAU * u;
        let dir = Vec3::new(phi.cos(), 0.0, phi.sin());
        (dir * r + Vec3::Y * y, (dir * nr + Vec3::Y * ny).normalize_or_zero())
    });
    b.finish()
}

/// Combines meshes, each moved by its transform, into one mesh.
pub fn merge(parts: &[(Mesh, Mat4)]) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut edges = Vec::new();
    for (m, t) in parts {
        let base = vertices.len() as u32;
        let normal_m = glam::Mat3::from_mat4(*t).inverse().transpose();
        vertices.extend(m.vertices.iter().map(|v| Vertex {
            pos: t.transform_point3(Vec3::from(v.pos)).to_array(),
            normal: (normal_m * Vec3::from(v.normal)).normalize_or_zero().to_array(),
        }));
        indices.extend(m.indices.iter().map(|i| i + base));
        edges.extend(m.edges.iter().map(|i| i + base));
    }
    Mesh::from_parts(vertices, indices, edges)
}

/// Smooth value noise in 0..1 on an integer lattice (repeatable per seed).
fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    fn hash(x: i32, z: i32, seed: u32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32
    }
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sz) = (s(fx), s(fz));
    let (xi, zi) = (xi as i32, zi as i32);
    let a = hash(xi, zi, seed) + (hash(xi + 1, zi, seed) - hash(xi, zi, seed)) * sx;
    let b = hash(xi, zi + 1, seed) + (hash(xi + 1, zi + 1, seed) - hash(xi, zi + 1, seed)) * sx;
    a + (b - a) * sz
}

/// Fractal terrain: `size.x` by `size.z`, heights from 0 to `size.y`.
pub fn landscape(size: Vec3, segments: [u32; 2], roughness: f32, scale: f32, seed: u32, borders: bool) -> Mesh {
    let (nx, nz) = (segments[0].max(1) as usize, segments[1].max(1) as usize);
    let gain = 0.25 + 0.5 * (roughness / 100.0).clamp(0.0, 1.0);
    // Base frequency: about three hills across at 100 %.
    let freq = 3.0 / (scale / 100.0).max(0.01);
    let mut h = vec![0.0f32; (nx + 1) * (nz + 1)];
    for j in 0..=nz {
        for i in 0..=nx {
            let (u, v) = (i as f32 / nx as f32, j as f32 / nz as f32);
            let (mut amp, mut f, mut sum) = (1.0, freq, 0.0);
            for octave in 0..6u32 {
                sum += amp * value_noise(u * f, v * f, seed.wrapping_add(octave * 1013));
                amp *= gain;
                f *= 2.0;
            }
            h[j * (nx + 1) + i] = sum;
        }
    }
    // Stretch to the full 0..1 range, then to the requested height.
    let (lo, hi) = h.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    let span = (hi - lo).max(1e-6);
    for j in 0..=nz {
        for i in 0..=nx {
            let k = j * (nx + 1) + i;
            let mut t = (h[k] - lo) / span;
            if borders {
                // Fade to zero over the outer fifth on every side.
                let (u, v) = (i as f32 / nx as f32, j as f32 / nz as f32);
                let edge = u.min(1.0 - u).min(v).min(1.0 - v);
                let w = (edge / 0.2).clamp(0.0, 1.0);
                t *= w * w * (3.0 - 2.0 * w);
            }
            h[k] = t * size.y;
        }
    }
    let at = |i: usize, j: usize| h[j.min(nz) * (nx + 1) + i.min(nx)];
    let (dx, dz) = (size.x / nx as f32, size.z / nz as f32);
    let mut b = MeshBuilder::default();
    // u runs along Z and v along X, as for the plane, so the faces point up.
    b.surface(nz as u32, nx as u32, |u, v| {
        let (j, i) = ((u * nz as f32).round() as usize, (v * nx as f32).round() as usize);
        let pos = Vec3::new(-size.x * 0.5 + i as f32 * dx, at(i, j), -size.z * 0.5 + j as f32 * dz);
        // Central differences (one-sided at the borders).
        let (i0, i1) = (i.saturating_sub(1), (i + 1).min(nx));
        let (j0, j1) = (j.saturating_sub(1), (j + 1).min(nz));
        let gx = (at(i1, j) - at(i0, j)) / (dx * (i1 - i0).max(1) as f32);
        let gz = (at(i, j1) - at(i, j0)) / (dz * (j1 - j0).max(1) as f32);
        (pos, Vec3::new(-gx, 1.0, -gz).normalize_or_zero())
    });
    b.finish()
}

/// A standing human dummy built from capsules and spheres, feet at y = 0 and
/// the top of the head at `height`.
pub fn figure(height: f32, segments: u32) -> Mesh {
    use glam::Quat;
    let seg = segments.max(4);
    let k = height / 188.0;
    let part = |m: Mesh, centre: Vec3, rot: Quat, scale: Vec3| (m, Mat4::from_scale_rotation_translation(scale * k, rot, centre * k));
    let capsule_y = |r: f32, len: f32| capsule(r, len, seg, 2);
    let sphere_r = |r: f32| sphere(r, seg);
    let z = |deg: f32| Quat::from_rotation_z(deg.to_radians());
    let flat = Quat::IDENTITY;
    let mut parts = vec![
        part(sphere_r(11.5), Vec3::new(0.0, 176.5, 0.0), flat, Vec3::new(0.92, 1.0, 1.0)),
        part(capsule_y(5.0, 16.0), Vec3::new(0.0, 160.0, 0.0), flat, Vec3::ONE),
        part(capsule_y(17.0, 46.0), Vec3::new(0.0, 133.0, 0.0), flat, Vec3::new(1.0, 1.0, 0.62)),
        part(capsule_y(15.0, 26.0), Vec3::new(0.0, 100.0, 0.0), flat, Vec3::new(1.0, 1.0, 0.68)),
    ];
    for side in [1.0f32, -1.0] {
        parts.extend([
            part(capsule_y(5.5, 32.0), Vec3::new(23.0 * side, 138.0, 0.0), z(8.0 * side), Vec3::ONE),
            part(capsule_y(4.6, 30.0), Vec3::new(26.5 * side, 107.0, 0.0), z(5.0 * side), Vec3::ONE),
            part(sphere_r(5.5), Vec3::new(28.0 * side, 85.0, 0.0), flat, Vec3::new(0.6, 1.2, 0.9)),
            part(capsule_y(8.0, 46.0), Vec3::new(9.0 * side, 66.0, 0.0), flat, Vec3::ONE),
            part(capsule_y(6.0, 42.0), Vec3::new(9.5 * side, 24.0, 0.0), flat, Vec3::ONE),
            part(capsule_y(4.0, 24.0), Vec3::new(9.5 * side, 4.0, 4.5), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), Vec3::ONE),
        ]);
    }
    merge(&parts)
}

pub fn torus(ring_radius: f32, pipe_radius: f32, ring_seg: u32, pipe_seg: u32) -> Mesh {
    let (ring_seg, pipe_seg) = (ring_seg.max(3), pipe_seg.max(3));
    let mut b = MeshBuilder::default();
    b.surface(ring_seg, pipe_seg, |u, v| {
        let phi = TAU * u;
        let theta = -TAU * v;
        let radial = Vec3::new(phi.cos(), 0.0, phi.sin());
        let normal = radial * theta.cos() + Vec3::Y * theta.sin();
        let pos = radial * ring_radius + normal * pipe_radius;
        (pos, normal)
    });
    b.finish()
}

/// Three crossing axis lines, used to draw Null objects (no triangles).
pub fn null_cross(half: f32) -> Mesh {
    let mut b = MeshBuilder::default();
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        let a = b.push_vertex(-axis * half, axis);
        let c = b.push_vertex(axis * half, axis);
        b.push_edge(a, c);
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_meshes() -> Vec<(&'static str, Mesh)> {
        vec![
            ("cube", cube(Vec3::new(200.0, 100.0, 300.0), [1, 2, 3])),
            ("plane", plane(400.0, 300.0, 4, 3)),
            ("sphere", sphere(100.0, 24)),
            ("cylinder", cylinder(50.0, 200.0, 16, 3, true)),
            ("cone", cone(50.0, 200.0, 16, 2, true)),
            ("torus", torus(100.0, 25.0, 24, 12)),
        ]
    }

    #[test]
    fn indices_are_in_range() {
        for (name, m) in all_meshes() {
            let n = m.vertices.len() as u32;
            assert!(m.indices.iter().all(|&i| i < n), "{name}: triangle index out of range");
            assert!(m.edges.iter().all(|&i| i < n), "{name}: edge index out of range");
            assert_eq!(m.indices.len() % 3, 0, "{name}");
            assert_eq!(m.edges.len() % 2, 0, "{name}");
            assert!(!m.edges.is_empty(), "{name}: no edges");
        }
    }

    #[test]
    fn normals_are_unit_length() {
        for (name, m) in all_meshes() {
            for v in &m.vertices {
                let len = Vec3::from(v.normal).length();
                assert!((len - 1.0).abs() < 1e-3, "{name}: normal length {len}");
            }
        }
    }

    #[test]
    fn winding_agrees_with_vertex_normals() {
        for (name, m) in all_meshes() {
            let mut checked = 0;
            for t in m.indices.chunks(3) {
                let (a, b, c) = (m.position(t[0]), m.position(t[1]), m.position(t[2]));
                let geometric = (b - a).cross(c - a);
                if geometric.length() < 1e-4 {
                    continue; // degenerate triangle at a pole / apex
                }
                let avg = Vec3::from(m.vertices[t[0] as usize].normal)
                    + Vec3::from(m.vertices[t[1] as usize].normal)
                    + Vec3::from(m.vertices[t[2] as usize].normal);
                assert!(geometric.dot(avg) > 0.0, "{name}: triangle wound against its normal");
                checked += 1;
            }
            assert!(checked > 0, "{name}");
        }
    }

    #[test]
    fn bounds_match_dimensions() {
        let m = cube(Vec3::new(200.0, 100.0, 300.0), [1, 1, 1]);
        assert!((m.bounds_max - Vec3::new(100.0, 50.0, 150.0)).length() < 1e-4);
        assert!((m.bounds_min + Vec3::new(100.0, 50.0, 150.0)).length() < 1e-4);
        let s = sphere(100.0, 32);
        // 32 segments reach the poles and all four equatorial extremes exactly,
        // so the box is 200 wide on every axis: half-diagonal = 100 * sqrt(3).
        assert!((s.bounds_radius() - 173.205).abs() < 0.01, "{}", s.bounds_radius());
        assert!((s.bounds_max.y - 100.0).abs() < 1e-3);
    }

    #[test]
    fn closed_solids_enclose_their_volume() {
        // Signed volume via the divergence theorem is positive iff winding is outward.
        for (name, m) in all_meshes() {
            if name == "plane" {
                continue;
            }
            let mut vol = 0.0f32;
            for t in m.indices.chunks(3) {
                let (a, b, c) = (m.position(t[0]), m.position(t[1]), m.position(t[2]));
                vol += a.dot(b.cross(c)) / 6.0;
            }
            assert!(vol > 0.0, "{name}: negative signed volume {vol}");
        }
    }

    #[test]
    fn null_cross_has_edges_but_no_triangles() {
        let m = null_cross(25.0);
        assert!(m.indices.is_empty());
        assert_eq!(m.edges.len(), 6);
    }
}
