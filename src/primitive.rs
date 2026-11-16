//! Parametric primitives: the editable description behind each generated mesh.

use crate::mesh::{self, Mesh};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Torus,
    Plane,
    Pyramid,
    Tube,
    Capsule,
    Landscape,
    Figure,
}

impl PrimitiveKind {
    pub const ALL: [PrimitiveKind; 11] = [
        PrimitiveKind::Cube,
        PrimitiveKind::Sphere,
        PrimitiveKind::Cylinder,
        PrimitiveKind::Cone,
        PrimitiveKind::Pyramid,
        PrimitiveKind::Tube,
        PrimitiveKind::Capsule,
        PrimitiveKind::Torus,
        PrimitiveKind::Plane,
        PrimitiveKind::Landscape,
        PrimitiveKind::Figure,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PrimitiveKind::Cube => "Cube",
            PrimitiveKind::Sphere => "Sphere",
            PrimitiveKind::Cylinder => "Cylinder",
            PrimitiveKind::Cone => "Cone",
            PrimitiveKind::Torus => "Torus",
            PrimitiveKind::Plane => "Plane",
            PrimitiveKind::Pyramid => "Pyramid",
            PrimitiveKind::Tube => "Tube",
            PrimitiveKind::Capsule => "Capsule",
            PrimitiveKind::Landscape => "Landscape",
            PrimitiveKind::Figure => "Figure",
        }
    }

    /// Default parameters, in centimetres.
    pub fn default_primitive(self) -> Primitive {
        match self {
            PrimitiveKind::Cube => Primitive::Cube {
                size: Vec3::splat(200.0),
                segments: [1, 1, 1],
            },
            PrimitiveKind::Sphere => Primitive::Sphere {
                radius: 100.0,
                segments: 36,
            },
            PrimitiveKind::Cylinder => Primitive::Cylinder {
                radius: 50.0,
                height: 200.0,
                segments: 36,
                hseg: 4,
                caps: true,
            },
            PrimitiveKind::Cone => Primitive::Cone {
                radius: 50.0,
                height: 100.0,
                segments: 36,
                hseg: 4,
                caps: true,
            },
            PrimitiveKind::Torus => Primitive::Torus {
                ring_radius: 100.0,
                pipe_radius: 25.0,
                ring_seg: 36,
                pipe_seg: 16,
            },
            PrimitiveKind::Plane => Primitive::Plane {
                width: 400.0,
                height: 400.0,
                wseg: 10,
                hseg: 10,
            },
            PrimitiveKind::Pyramid => Primitive::Pyramid {
                size: Vec3::splat(200.0),
                segments: 1,
            },
            PrimitiveKind::Tube => Primitive::Tube {
                inner_radius: 50.0,
                outer_radius: 100.0,
                height: 100.0,
                segments: 36,
                hseg: 1,
            },
            PrimitiveKind::Capsule => Primitive::Capsule {
                radius: 50.0,
                height: 200.0,
                segments: 36,
                hseg: 4,
            },
            PrimitiveKind::Landscape => Primitive::Landscape {
                size: Vec3::new(800.0, 120.0, 800.0),
                segments: [64, 64],
                roughness: 50.0,
                scale: 100.0,
                seed: 1,
                borders: true,
            },
            PrimitiveKind::Figure => Primitive::Figure { height: 200.0, segments: 16 },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    Cube { size: Vec3, segments: [u32; 3] },
    Sphere { radius: f32, segments: u32 },
    Cylinder { radius: f32, height: f32, segments: u32, hseg: u32, caps: bool },
    Cone { radius: f32, height: f32, segments: u32, hseg: u32, caps: bool },
    Torus { ring_radius: f32, pipe_radius: f32, ring_seg: u32, pipe_seg: u32 },
    Plane { width: f32, height: f32, wseg: u32, hseg: u32 },
    /// A square-based pyramid: base `size.x` by `size.z`, apex `size.y` up.
    Pyramid { size: Vec3, segments: u32 },
    /// A cylinder with a hole: the wall runs from `inner_radius` to `outer_radius`.
    Tube { inner_radius: f32, outer_radius: f32, height: f32, segments: u32, hseg: u32 },
    /// A cylinder with hemispherical ends. `height` is the full length, ends
    /// included, and is never less than the diameter.
    Capsule { radius: f32, height: f32, segments: u32, hseg: u32 },
    /// Fractal terrain: `size.x` by `size.z` wide, peaks up to `size.y`.
    Landscape {
        size: Vec3,
        segments: [u32; 2],
        /// 0-100 %: how much fine detail each octave keeps.
        roughness: f32,
        /// 1-1000 %: how large the hills are (bigger = broader).
        scale: f32,
        seed: u32,
        /// Flatten the edges down to zero height.
        borders: bool,
    },
    /// A standing human dummy, feet at the origin.
    Figure { height: f32, segments: u32 },
}

/// Hashable identity of a primitive's parameters (floats compared bitwise).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveKey(u8, [u32; 6]);

const MAX_SEGMENTS: u32 = 256;
const MIN_SIZE: f32 = 0.01;

fn size(v: f32) -> f32 {
    if v.is_finite() { v.max(MIN_SIZE) } else { MIN_SIZE }
}

fn segs(v: u32, min: u32) -> u32 {
    v.clamp(min, MAX_SEGMENTS)
}

impl Primitive {
    pub fn kind(&self) -> PrimitiveKind {
        match self {
            Primitive::Cube { .. } => PrimitiveKind::Cube,
            Primitive::Sphere { .. } => PrimitiveKind::Sphere,
            Primitive::Cylinder { .. } => PrimitiveKind::Cylinder,
            Primitive::Cone { .. } => PrimitiveKind::Cone,
            Primitive::Torus { .. } => PrimitiveKind::Torus,
            Primitive::Plane { .. } => PrimitiveKind::Plane,
            Primitive::Pyramid { .. } => PrimitiveKind::Pyramid,
            Primitive::Tube { .. } => PrimitiveKind::Tube,
            Primitive::Capsule { .. } => PrimitiveKind::Capsule,
            Primitive::Landscape { .. } => PrimitiveKind::Landscape,
            Primitive::Figure { .. } => PrimitiveKind::Figure,
        }
    }

    /// The primitive resized by `s` along X, Y and Z by changing its
    /// parameters (Model mode scaling). `None` when the shape can't take that
    /// scale, e.g. a sphere squashed along one axis.
    pub fn scaled(&self, s: Vec3) -> Option<Primitive> {
        if !s.is_finite() || s.min_element() <= 0.0 {
            return None;
        }
        let close = |a: f32, b: f32| (a - b).abs() <= 1e-3 * a.abs().max(b.abs()).max(1.0);
        let uniform = close(s.x, s.y) && close(s.y, s.z);
        let round = close(s.x, s.z);
        let p = match *self {
            Primitive::Cube { size, segments } => Primitive::Cube { size: size * s, segments },
            Primitive::Pyramid { size, segments } => Primitive::Pyramid { size: size * s, segments },
            Primitive::Landscape { size, segments, roughness, scale, seed, borders } => {
                Primitive::Landscape { size: size * s, segments, roughness, scale, seed, borders }
            }
            Primitive::Plane { width, height, wseg, hseg } => Primitive::Plane { width: width * s.x, height: height * s.z, wseg, hseg },
            Primitive::Sphere { radius, segments } if uniform => Primitive::Sphere { radius: radius * s.x, segments },
            Primitive::Torus { ring_radius, pipe_radius, ring_seg, pipe_seg } if uniform => {
                Primitive::Torus { ring_radius: ring_radius * s.x, pipe_radius: pipe_radius * s.x, ring_seg, pipe_seg }
            }
            Primitive::Figure { height, segments } if uniform => Primitive::Figure { height: height * s.y, segments },
            Primitive::Cylinder { radius, height, segments, hseg, caps } if round => {
                Primitive::Cylinder { radius: radius * s.x, height: height * s.y, segments, hseg, caps }
            }
            Primitive::Cone { radius, height, segments, hseg, caps } if round => {
                Primitive::Cone { radius: radius * s.x, height: height * s.y, segments, hseg, caps }
            }
            Primitive::Tube { inner_radius, outer_radius, height, segments, hseg } if round => Primitive::Tube {
                inner_radius: inner_radius * s.x,
                outer_radius: outer_radius * s.x,
                height: height * s.y,
                segments,
                hseg,
            },
            Primitive::Capsule { radius, height, segments, hseg } if round && height * s.y >= 2.0 * radius * s.x - 1e-3 => {
                Primitive::Capsule { radius: radius * s.x, height: height * s.y, segments, hseg }
            }
            _ => return None,
        };
        Some(p.sanitized())
    }

    /// Clamps every parameter to a range the mesh generators can handle, so
    /// dragging a field to an extreme value can never produce a degenerate or
    /// enormous mesh.
    pub fn sanitized(&self) -> Primitive {
        match *self {
            Primitive::Cube { size: s, segments: g } => Primitive::Cube {
                size: Vec3::new(size(s.x), size(s.y), size(s.z)),
                segments: [segs(g[0], 1), segs(g[1], 1), segs(g[2], 1)],
            },
            Primitive::Sphere { radius, segments } => Primitive::Sphere {
                radius: size(radius),
                segments: segs(segments, 3),
            },
            Primitive::Cylinder { radius, height, segments, hseg, caps } => Primitive::Cylinder {
                radius: size(radius),
                height: size(height),
                segments: segs(segments, 3),
                hseg: segs(hseg, 1),
                caps,
            },
            Primitive::Cone { radius, height, segments, hseg, caps } => Primitive::Cone {
                radius: size(radius),
                height: size(height),
                segments: segs(segments, 3),
                hseg: segs(hseg, 1),
                caps,
            },
            Primitive::Torus { ring_radius, pipe_radius, ring_seg, pipe_seg } => Primitive::Torus {
                ring_radius: size(ring_radius),
                pipe_radius: size(pipe_radius),
                ring_seg: segs(ring_seg, 3),
                pipe_seg: segs(pipe_seg, 3),
            },
            Primitive::Plane { width, height, wseg, hseg } => Primitive::Plane {
                width: size(width),
                height: size(height),
                wseg: segs(wseg, 1),
                hseg: segs(hseg, 1),
            },
            Primitive::Pyramid { size: s, segments } => Primitive::Pyramid {
                size: Vec3::new(size(s.x), size(s.y), size(s.z)),
                segments: segs(segments, 1),
            },
            Primitive::Tube { inner_radius, outer_radius, height, segments, hseg } => {
                let outer = size(outer_radius);
                // The hole always stays inside the wall.
                let inner = size(inner_radius).min(outer * 0.999);
                Primitive::Tube {
                    inner_radius: inner,
                    outer_radius: outer,
                    height: size(height),
                    segments: segs(segments, 3),
                    hseg: segs(hseg, 1),
                }
            }
            Primitive::Capsule { radius, height, segments, hseg } => {
                let radius = size(radius);
                Primitive::Capsule {
                    radius,
                    height: size(height).max(radius * 2.0),
                    segments: segs(segments, 3),
                    hseg: segs(hseg, 1),
                }
            }
            Primitive::Landscape { size: s, segments, roughness, scale, seed, borders } => Primitive::Landscape {
                size: Vec3::new(size(s.x), if s.y.is_finite() { s.y.clamp(-100_000.0, 100_000.0) } else { 0.0 }, size(s.z)),
                segments: [segs(segments[0], 1), segs(segments[1], 1)],
                roughness: if roughness.is_finite() { roughness.clamp(0.0, 100.0) } else { 50.0 },
                scale: if scale.is_finite() { scale.clamp(1.0, 1000.0) } else { 100.0 },
                seed,
                borders,
            },
            Primitive::Figure { height, segments } => Primitive::Figure { height: size(height), segments: segs(segments, 4) },
        }
    }

    pub fn key(&self) -> PrimitiveKey {
        let b = |f: f32| f.to_bits();
        let p = self.sanitized();
        match p {
            Primitive::Cube { size, segments } => PrimitiveKey(
                0,
                [b(size.x), b(size.y), b(size.z), segments[0], segments[1], segments[2]],
            ),
            Primitive::Sphere { radius, segments } => PrimitiveKey(1, [b(radius), segments, 0, 0, 0, 0]),
            Primitive::Cylinder { radius, height, segments, hseg, caps } => {
                PrimitiveKey(2, [b(radius), b(height), segments, hseg, caps as u32, 0])
            }
            Primitive::Cone { radius, height, segments, hseg, caps } => {
                PrimitiveKey(3, [b(radius), b(height), segments, hseg, caps as u32, 0])
            }
            Primitive::Torus { ring_radius, pipe_radius, ring_seg, pipe_seg } => {
                PrimitiveKey(4, [b(ring_radius), b(pipe_radius), ring_seg, pipe_seg, 0, 0])
            }
            Primitive::Plane { width, height, wseg, hseg } => {
                PrimitiveKey(5, [b(width), b(height), wseg, hseg, 0, 0])
            }
            Primitive::Pyramid { size, segments } => PrimitiveKey(6, [b(size.x), b(size.y), b(size.z), segments, 0, 0]),
            Primitive::Tube { inner_radius, outer_radius, height, segments, hseg } => {
                PrimitiveKey(7, [b(inner_radius), b(outer_radius), b(height), segments, hseg, 0])
            }
            Primitive::Capsule { radius, height, segments, hseg } => {
                PrimitiveKey(8, [b(radius), b(height), segments, hseg, 0, 0])
            }
            Primitive::Landscape { size, segments, roughness, scale, seed, borders } => {
                // Six slots: fold the extra values into the segment slots.
                PrimitiveKey(
                    9,
                    [b(size.x), b(size.y), b(size.z), segments[0] | segments[1] << 16, b(roughness) ^ b(scale).rotate_left(7) ^ seed.rotate_left(19), borders as u32],
                )
            }
            Primitive::Figure { height, segments } => PrimitiveKey(10, [b(height), segments, 0, 0, 0, 0]),
        }
    }

    pub fn build(&self) -> Mesh {
        match self.sanitized() {
            Primitive::Cube { size, segments } => mesh::cube(size, segments),
            Primitive::Sphere { radius, segments } => mesh::sphere(radius, segments),
            Primitive::Cylinder { radius, height, segments, hseg, caps } => {
                mesh::cylinder(radius, height, segments, hseg, caps)
            }
            Primitive::Cone { radius, height, segments, hseg, caps } => {
                mesh::cone(radius, height, segments, hseg, caps)
            }
            Primitive::Torus { ring_radius, pipe_radius, ring_seg, pipe_seg } => {
                mesh::torus(ring_radius, pipe_radius, ring_seg, pipe_seg)
            }
            Primitive::Plane { width, height, wseg, hseg } => mesh::plane(width, height, wseg, hseg),
            Primitive::Pyramid { size, segments } => mesh::pyramid(size, segments),
            Primitive::Tube { inner_radius, outer_radius, height, segments, hseg } => {
                mesh::tube(inner_radius, outer_radius, height, segments, hseg)
            }
            Primitive::Capsule { radius, height, segments, hseg } => mesh::capsule(radius, height, segments, hseg),
            Primitive::Landscape { size, segments, roughness, scale, seed, borders } => {
                mesh::landscape(size, segments, roughness, scale, seed, borders)
            }
            Primitive::Figure { height, segments } => mesh::figure(height, segments),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_build_valid_meshes() {
        for kind in PrimitiveKind::ALL {
            let m = kind.default_primitive().build();
            assert!(!m.vertices.is_empty(), "{kind:?}");
            assert!(!m.indices.is_empty(), "{kind:?}");
        }
    }

    /// Signed volume of a closed mesh (positive when faces point outwards).
    fn volume(m: &Mesh) -> f32 {
        m.indices
            .chunks_exact(3)
            .map(|t| m.position(t[0]).dot(m.position(t[1]).cross(m.position(t[2]))) / 6.0)
            .sum()
    }

    #[test]
    fn pyramid_is_closed_with_the_right_volume_and_outward_faces() {
        let m = Primitive::Pyramid { size: Vec3::new(200.0, 300.0, 100.0), segments: 3 }.build();
        // V = base area * height / 3.
        let expected = 200.0 * 100.0 * 300.0 / 3.0;
        assert!((volume(&m) - expected).abs() < expected * 1e-4, "{}", volume(&m));
        assert!((m.bounds_max.y - 150.0).abs() < 1e-3 && (m.bounds_min.y + 150.0).abs() < 1e-3);
        // Face normals agree with the winding (both point out).
        for t in m.indices.chunks_exact(3) {
            let (a, b, c) = (m.position(t[0]), m.position(t[1]), m.position(t[2]));
            let face = (b - a).cross(c - a);
            if face.length() > 1e-3 {
                let n = Vec3::from(m.vertices[t[0] as usize].normal);
                assert!(face.dot(n) > 0.0);
            }
        }
    }

    #[test]
    fn tube_volume_is_the_ring_area_times_height() {
        let m = Primitive::Tube { inner_radius: 50.0, outer_radius: 100.0, height: 80.0, segments: 256, hseg: 2 }.build();
        let expected = std::f32::consts::PI * (100.0f32.powi(2) - 50.0f32.powi(2)) * 80.0;
        assert!((volume(&m) - expected).abs() < expected * 2e-3, "{} vs {expected}", volume(&m));
        // An inner radius past the outer one is pulled back inside the wall.
        let odd = Primitive::Tube { inner_radius: 300.0, outer_radius: 100.0, height: 80.0, segments: 12, hseg: 1 }.sanitized();
        assert!(matches!(odd, Primitive::Tube { inner_radius, outer_radius, .. } if inner_radius < outer_radius));
        assert!(volume(&odd.build()) > 0.0);
    }

    #[test]
    fn capsule_is_a_cylinder_plus_a_sphere() {
        let (r, h) = (50.0f32, 300.0f32);
        let m = Primitive::Capsule { radius: r, height: h, segments: 256, hseg: 3 }.build();
        let pi = std::f32::consts::PI;
        let expected = pi * r * r * (h - 2.0 * r) + 4.0 / 3.0 * pi * r.powi(3);
        assert!((volume(&m) - expected).abs() < expected * 2e-3, "{} vs {expected}", volume(&m));
        assert!((m.bounds_max.y - h / 2.0).abs() < 1e-2 && (m.bounds_min.y + h / 2.0).abs() < 1e-2);
        // Normals point away from the axis segment everywhere.
        for v in &m.vertices {
            let p = Vec3::from(v.pos);
            let axis = Vec3::new(0.0, p.y.clamp(-(h / 2.0 - r), h / 2.0 - r), 0.0);
            assert!(Vec3::from(v.normal).dot(p - axis) > 0.0);
        }
        // Too short a height becomes a sphere, not something inside out.
        let short = Primitive::Capsule { radius: 50.0, height: 10.0, segments: 64, hseg: 1 }.build();
        assert!((short.bounds_max.y - 50.0).abs() < 1e-2 && volume(&short) > 0.0);
    }

    #[test]
    fn landscape_stays_in_its_box_and_is_repeatable() {
        let p = PrimitiveKind::Landscape.default_primitive();
        let a = p.build();
        let b = p.build();
        assert_eq!(a.vertices.len(), 65 * 65);
        assert!(a.vertices.iter().zip(&b.vertices).all(|(x, y)| x.pos == y.pos), "same seed, same terrain");
        assert!(a.bounds_min.y >= -1e-3 && a.bounds_max.y <= 120.0 + 1e-3, "{:?} {:?}", a.bounds_min, a.bounds_max);
        assert!(a.bounds_max.y > 40.0, "hills rise well above the floor");
        assert!((a.bounds_max.x - 400.0).abs() < 1e-3 && (a.bounds_min.z + 400.0).abs() < 1e-3);
        // Borders at sea level: every edge vertex sits at zero.
        for v in &a.vertices {
            let edge = (v.pos[0].abs() - 400.0).abs() < 1e-3 || (v.pos[2].abs() - 400.0).abs() < 1e-3;
            if edge {
                assert!(v.pos[1].abs() < 1e-3);
            }
            assert!(v.normal[1] > 0.0, "normals face up");
        }
        // Another seed gives other hills.
        let Primitive::Landscape { size, segments, roughness, scale, borders, .. } = p else { unreachable!() };
        let c = Primitive::Landscape { size, segments, roughness, scale, seed: 2, borders }.build();
        assert!(a.vertices.iter().zip(&c.vertices).any(|(x, y)| x.pos != y.pos));
    }

    #[test]
    fn figure_stands_on_the_floor_at_its_height() {
        let m = Primitive::Figure { height: 180.0, segments: 12 }.build();
        assert!(m.bounds_min.y.abs() < 1.0, "feet on the floor: {}", m.bounds_min.y);
        assert!((m.bounds_max.y - 180.0).abs() < 0.5, "top of the head: {}", m.bounds_max.y);
        // Arms and legs in pairs: the figure is left-right symmetric.
        assert!((m.bounds_max.x + m.bounds_min.x).abs() < 1e-2);
        assert!(m.bounds_max.x > 20.0 && m.bounds_max.x < 50.0, "shoulders and arms: {}", m.bounds_max.x);
        assert!(volume(&m) > 0.0, "parts face outwards");
    }

    #[test]
    fn extreme_parameters_are_clamped() {
        let wild = Primitive::Sphere { radius: -5.0, segments: 1_000_000 };
        let m = wild.build();
        // 256 segments, 128 rings: a bounded vertex count rather than a million.
        assert!(m.vertices.len() < 40_000);
        let nan = Primitive::Cylinder {
            radius: f32::NAN,
            height: f32::INFINITY,
            segments: 0,
            hseg: 0,
            caps: true,
        };
        let m = nan.build();
        assert!(m.vertices.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
    }

    #[test]
    fn keys_distinguish_parameters_but_ignore_clamped_noise() {
        let a = Primitive::Sphere { radius: 100.0, segments: 36 };
        let b = Primitive::Sphere { radius: 100.5, segments: 36 };
        assert_ne!(a.key(), b.key());
        assert_eq!(a.key(), a.key());
        let tiny1 = Primitive::Sphere { radius: -1.0, segments: 36 };
        let tiny2 = Primitive::Sphere { radius: -2.0, segments: 36 };
        assert_eq!(tiny1.key(), tiny2.key(), "both clamp to the same mesh");
    }
}
