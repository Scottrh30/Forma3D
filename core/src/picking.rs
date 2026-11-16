//! CPU ray picking against the scene's meshes.

use crate::mesh::Mesh;
use crate::scene::ObjectId;
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// World-space radius of the invisible sphere used to pick Null objects.
pub const NULL_PICK_RADIUS: f32 = 30.0;

pub enum PickShape {
    Mesh(Arc<Mesh>),
    /// Objects with no surface (Nulls): picked as a small sphere at their origin.
    Point,
    /// Curves (splines): picked near any of their line segments.
    Lines(Arc<Mesh>),
}

pub struct PickItem {
    pub id: ObjectId,
    pub world: Mat4,
    pub shape: PickShape,
}

/// Möller–Trumbore, two-sided. Returns the ray parameter `t` of the hit.
pub fn ray_triangle(ro: Vec3, rd: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    const EPS: f32 = 1e-9;
    let e1 = b - a;
    let e2 = c - a;
    let p = rd.cross(e2);
    let det = e1.dot(p);
    if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let s = ro - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = rd.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some(t)
}

/// Ray vs axis-aligned box (slab test). Direction components may be zero.
pub fn ray_aabb(ro: Vec3, rd: Vec3, min: Vec3, max: Vec3) -> bool {
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;
    for i in 0..3 {
        if rd[i].abs() < 1e-12 {
            if ro[i] < min[i] || ro[i] > max[i] {
                return false;
            }
        } else {
            let inv = 1.0 / rd[i];
            let (mut t0, mut t1) = ((min[i] - ro[i]) * inv, (max[i] - ro[i]) * inv);
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_min > t_max {
                return false;
            }
        }
    }
    t_max >= 0.0
}

pub fn ray_sphere(ro: Vec3, rd: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ro - center;
    let a = rd.dot(rd);
    let b = 2.0 * oc.dot(rd);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 || a <= 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t0 = (-b - sq) / (2.0 * a);
    let t1 = (-b + sq) / (2.0 * a);
    [t0, t1].into_iter().find(|t| *t > 1e-6)
}

fn pick_mesh(mesh: &Mesh, world: Mat4, ro: Vec3, rd: Vec3) -> Option<f32> {
    let inv = world.inverse();
    if !inv.is_finite() {
        return None;
    }
    // The direction is deliberately not renormalised: `t` then stays the same
    // parameter along the original world-space ray.
    let lo = inv.transform_point3(ro);
    let ld = inv.transform_vector3(rd);
    if !ray_aabb(lo, ld, mesh.bounds_min, mesh.bounds_max) {
        return None;
    }
    let mut best: Option<f32> = None;
    for tri in mesh.indices.chunks_exact(3) {
        if let Some(t) = ray_triangle(lo, ld, mesh.position(tri[0]), mesh.position(tri[1]), mesh.position(tri[2]))
            && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
    }
    best
}

/// The nearest object hit by the world-space ray, with its distance parameter.
/// Distance along the ray to the closest approach to segment `a`-`b`, and how
/// far apart they are there.
fn ray_segment(ro: Vec3, rd: Vec3, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
    let d = b - a;
    let w = ro - a;
    let (aa, bb, dd) = (rd.dot(rd), rd.dot(d), d.dot(d));
    let (cc, ee) = (rd.dot(w), d.dot(w));
    let denom = aa * dd - bb * bb;
    let s = if dd < 1e-12 {
        0.0
    } else if denom.abs() < 1e-12 {
        (ee / dd).clamp(0.0, 1.0)
    } else {
        ((aa * ee - bb * cc) / denom).clamp(0.0, 1.0)
    };
    let q = a + d * s;
    let t = (q - ro).dot(rd) / aa.max(1e-12);
    if t <= 0.0 {
        return None;
    }
    Some((t, (ro + rd * t).distance(q)))
}

/// Picks a line mesh: hit when the ray passes within about 0.7° of a segment
/// (a few pixels at any zoom), never less than a centimetre.
fn pick_lines(mesh: &Mesh, world: Mat4, ro: Vec3, rd: Vec3) -> Option<f32> {
    let rd = rd.normalize_or_zero();
    let mut best: Option<f32> = None;
    for e in mesh.edges.chunks_exact(2) {
        let a = world.transform_point3(mesh.position(e[0]));
        let b = world.transform_point3(mesh.position(e[1]));
        if let Some((t, gap)) = ray_segment(ro, rd, a, b)
            && gap <= (t * 0.012).max(1.0)
            && best.is_none_or(|bt| t < bt)
        {
            best = Some(t);
        }
    }
    best
}

pub fn pick(items: &[PickItem], ro: Vec3, rd: Vec3) -> Option<(ObjectId, f32)> {
    let mut best: Option<(ObjectId, f32)> = None;
    for item in items {
        let hit = match &item.shape {
            PickShape::Mesh(m) => pick_mesh(m, item.world, ro, rd),
            PickShape::Point => ray_sphere(ro, rd, item.world.w_axis.truncate(), NULL_PICK_RADIUS),
            PickShape::Lines(m) => pick_lines(m, item.world, ro, rd),
        };
        if let Some(t) = hit
            && best.is_none_or(|(_, bt)| t < bt) {
                best = Some((item.id, t));
            }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh;

    fn cube_item(id: ObjectId, at: Vec3) -> PickItem {
        PickItem {
            id,
            world: Mat4::from_translation(at),
            shape: PickShape::Mesh(Arc::new(mesh::cube(Vec3::splat(100.0), [1, 1, 1]))),
        }
    }

    #[test]
    fn triangle_hit_and_miss() {
        let (a, b, c) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 10.0, 0.0));
        let t = ray_triangle(Vec3::new(2.0, 2.0, -5.0), Vec3::Z, a, b, c).unwrap();
        assert!((t - 5.0).abs() < 1e-5);
        assert!(ray_triangle(Vec3::new(8.0, 8.0, -5.0), Vec3::Z, a, b, c).is_none());
        // Two-sided: hits from behind too.
        assert!(ray_triangle(Vec3::new(2.0, 2.0, 5.0), Vec3::NEG_Z, a, b, c).is_some());
        // Behind the ray origin does not count.
        assert!(ray_triangle(Vec3::new(2.0, 2.0, 5.0), Vec3::Z, a, b, c).is_none());
    }

    #[test]
    fn aabb_handles_axis_aligned_rays() {
        let (min, max) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(ray_aabb(Vec3::new(0.0, 0.0, -5.0), Vec3::Z, min, max));
        assert!(!ray_aabb(Vec3::new(3.0, 0.0, -5.0), Vec3::Z, min, max));
        assert!(!ray_aabb(Vec3::new(0.0, 0.0, 5.0), Vec3::Z, min, max), "box is behind the ray");
    }

    #[test]
    fn picks_the_nearest_of_overlapping_objects() {
        let items = vec![
            cube_item(1, Vec3::new(0.0, 0.0, 400.0)),
            cube_item(2, Vec3::new(0.0, 0.0, 100.0)),
            cube_item(3, Vec3::new(500.0, 0.0, 0.0)),
        ];
        let hit = pick(&items, Vec3::new(0.0, 0.0, -1000.0), Vec3::Z).unwrap();
        assert_eq!(hit.0, 2);
        assert!((hit.1 - (1000.0 + 100.0 - 50.0)).abs() < 1e-2, "t = {}", hit.1);
    }

    #[test]
    fn respects_object_transform() {
        let mut item = cube_item(1, Vec3::ZERO);
        // Scale the cube to 4x along X and rotate 90 degrees about Y: it now spans 400 along Z.
        item.world = Mat4::from_rotation_translation(glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), Vec3::ZERO)
            * Mat4::from_scale(Vec3::new(4.0, 1.0, 1.0));
        let items = vec![item];
        assert!(pick(&items, Vec3::new(0.0, 0.0, -1000.0), Vec3::Z).is_some());
        // 150 along Z is inside the stretched cube's reach but outside an unscaled one.
        assert!(pick(&items, Vec3::new(0.0, 0.0, 150.0 + 1000.0), Vec3::NEG_Z).is_some());
        assert!(pick(&items, Vec3::new(120.0, 0.0, -1000.0), Vec3::Z).is_none());
    }

    #[test]
    fn nulls_are_picked_as_small_spheres() {
        let items = vec![PickItem {
            id: 7,
            world: Mat4::from_translation(Vec3::new(0.0, 0.0, 200.0)),
            shape: PickShape::Point,
        }];
        assert_eq!(pick(&items, Vec3::new(10.0, 0.0, 0.0), Vec3::Z).map(|h| h.0), Some(7));
        assert!(pick(&items, Vec3::new(100.0, 0.0, 0.0), Vec3::Z).is_none());
    }

    #[test]
    fn splines_are_picked_along_their_line() {
        let s = crate::spline::Spline {
            points: vec![crate::spline::SplinePoint::corner(Vec3::ZERO), crate::spline::SplinePoint::corner(Vec3::X * 200.0)],
            ..Default::default()
        };
        let items = [PickItem { id: 3, world: Mat4::IDENTITY, shape: PickShape::Lines(Arc::new(s.line_mesh())) }];
        // Looking down at the middle of the line from 500 cm away: hit.
        assert_eq!(pick(&items, Vec3::new(100.0, 500.0, 0.0), Vec3::NEG_Y).map(|h| h.0), Some(3));
        // 3 cm to the side is within about 0.7° at that distance: still a hit.
        assert!(pick(&items, Vec3::new(100.0, 500.0, 3.0), Vec3::NEG_Y).is_some());
        // 30 cm to the side: a miss. Past the end: a miss.
        assert!(pick(&items, Vec3::new(100.0, 500.0, 30.0), Vec3::NEG_Y).is_none());
        assert!(pick(&items, Vec3::new(260.0, 500.0, 0.0), Vec3::NEG_Y).is_none());
    }

    #[test]
    fn degenerate_transform_does_not_panic() {
        let mut item = cube_item(1, Vec3::ZERO);
        item.world = Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0));
        assert!(pick(&[item], Vec3::new(0.0, 0.0, -10.0), Vec3::Z).is_none());
    }

    #[test]
    fn sphere_mesh_is_hit_in_the_middle_and_missed_at_the_corner() {
        let item = PickItem {
            id: 1,
            world: Mat4::IDENTITY,
            shape: PickShape::Mesh(Arc::new(mesh::sphere(100.0, 36))),
        };
        let items = [item];
        assert!(pick(&items, Vec3::new(0.0, 0.0, -500.0), Vec3::Z).is_some());
        // The bounding box corner region is empty space for a sphere.
        assert!(pick(&items, Vec3::new(90.0, 90.0, -500.0), Vec3::Z).is_none());
    }
}
