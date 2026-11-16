//! Deformers: objects that bend or twist the geometry of their parent, in the
//! Cinema 4D way. A deformer is placed *under* the object it deforms. Under a
//! Null or a generator it deforms everything inside that object.
//!
//! The deformer's box sits on its local origin: X and Z are centred, Y runs
//! from 0 to `size.y`. Every deformer works along local +Y:
//!
//! - **Bend** curls the object over towards a direction.
//! - **Twist** rotates each slice around Y, more the higher it is.
//! - **Taper** scales each slice in X and Z, narrowing (or widening) towards the top.
//! - **Shear** slides each slice sideways, more the higher it is.
//!
//! Taper and Shear have a Curvature (0-100 %): 0 changes evenly with height,
//! 100 keeps the bottom nearly untouched and does most of the change near the
//! top, giving a rounded profile.

use crate::generator::Geo;
use crate::mesh::Vertex;
use glam::{Mat3, Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeformerKind {
    Bend,
    Twist,
    Taper,
    Shear,
}

impl DeformerKind {
    pub const ALL: [DeformerKind; 4] = [DeformerKind::Bend, DeformerKind::Twist, DeformerKind::Taper, DeformerKind::Shear];

    pub fn name(self) -> &'static str {
        match self {
            DeformerKind::Bend => "Bend",
            DeformerKind::Twist => "Twist",
            DeformerKind::Taper => "Taper",
            DeformerKind::Shear => "Shear",
        }
    }

    pub fn default_deformer(self) -> Deformer {
        let size = Vec3::new(250.0, 250.0, 250.0);
        match self {
            DeformerKind::Bend => Deformer::Bend { size, strength: 90.0, direction: 0.0, limited: true },
            DeformerKind::Twist => Deformer::Twist { size, angle: 90.0, limited: true },
            DeformerKind::Taper => Deformer::Taper { size, strength: 50.0, curvature: 0.0, limited: true },
            DeformerKind::Shear => Deformer::Shear { size, strength: 50.0, direction: 0.0, curvature: 0.0, limited: true },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Deformer {
    Bend {
        /// Box size in cm; the bend happens over `size.y`.
        size: Vec3,
        /// Total bend angle in degrees.
        strength: f32,
        /// Which way it bends, as a rotation around local Y in degrees (0 = towards +X).
        direction: f32,
        /// Limited: only the part inside the box height bends, the rest continues straight.
        limited: bool,
    },
    Twist {
        size: Vec3,
        /// Total twist in degrees over `size.y`.
        angle: f32,
        limited: bool,
    },
    Taper {
        size: Vec3,
        /// How much narrower the top is, in percent: 100 pinches it to a point,
        /// negative values widen it.
        strength: f32,
        /// 0-100 %: how rounded the profile is.
        curvature: f32,
        /// Limited: above the box the object keeps the top's scale.
        limited: bool,
    },
    Shear {
        size: Vec3,
        /// How far the top slides, in percent of the box height (100 = 45°).
        strength: f32,
        /// Which way it slides, as a rotation around local Y in degrees (0 = towards +X).
        direction: f32,
        /// 0-100 %: how curved the slide is.
        curvature: f32,
        /// Limited: above the box the object keeps the top's offset.
        limited: bool,
    },
}

/// Height in the box as 0..1 (clamped when limited), then eased by curvature.
fn profile(y: f32, length: f32, curvature: f32, limited: bool) -> f32 {
    let t = y / length;
    let t = if limited { t.clamp(0.0, 1.0) } else { t };
    let c = (curvature / 100.0).clamp(0.0, 1.0);
    (1.0 - c) * t + c * t * t.abs()
}

impl Deformer {
    pub fn kind(&self) -> DeformerKind {
        match self {
            Deformer::Bend { .. } => DeformerKind::Bend,
            Deformer::Twist { .. } => DeformerKind::Twist,
            Deformer::Taper { .. } => DeformerKind::Taper,
            Deformer::Shear { .. } => DeformerKind::Shear,
        }
    }

    pub fn size(&self) -> Vec3 {
        match self {
            Deformer::Bend { size, .. }
            | Deformer::Twist { size, .. }
            | Deformer::Taper { size, .. }
            | Deformer::Shear { size, .. } => *size,
        }
    }

    pub fn set_size(&mut self, s: Vec3) {
        match self {
            Deformer::Bend { size, .. }
            | Deformer::Twist { size, .. }
            | Deformer::Taper { size, .. }
            | Deformer::Shear { size, .. } => *size = s,
        }
    }

    pub fn sanitized(self) -> Deformer {
        let size = |s: Vec3| if s.is_finite() { s.clamp(Vec3::splat(0.1), Vec3::splat(1.0e6)) } else { Vec3::splat(250.0) };
        let angle = |a: f32| if a.is_finite() { a.clamp(-3600.0, 3600.0) } else { 0.0 };
        let percent = |v: f32| if v.is_finite() { v.clamp(-1000.0, 1000.0) } else { 0.0 };
        let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 100.0) } else { 0.0 };
        match self {
            Deformer::Bend { size: s, strength, direction, limited } => Deformer::Bend {
                size: size(s),
                strength: angle(strength),
                direction: angle(direction),
                limited,
            },
            Deformer::Twist { size: s, angle: a, limited } => Deformer::Twist { size: size(s), angle: angle(a), limited },
            Deformer::Taper { size: s, strength, curvature, limited } => Deformer::Taper {
                size: size(s),
                strength: percent(strength),
                curvature: unit(curvature),
                limited,
            },
            Deformer::Shear { size: s, strength, direction, curvature, limited } => Deformer::Shear {
                size: size(s),
                strength: percent(strength),
                direction: angle(direction),
                curvature: unit(curvature),
                limited,
            },
        }
    }

    /// Deforms one point given in the deformer's local space.
    pub fn deform_point(&self, p: Vec3) -> Vec3 {
        match *self {
            Deformer::Bend { size, strength, direction, limited } => {
                let length = size.y;
                let theta_total = strength.to_radians();
                if length <= 0.0 || theta_total.abs() < 1e-6 {
                    return p;
                }
                // Rotate so the bend happens in the (u, y) plane, u along `direction`.
                let rot = Mat3::from_rotation_y(-direction.to_radians());
                let q = rot * p;
                let (u, y, w) = (q.x, q.y, q.z);
                let k = theta_total / length; // curvature
                let r = 1.0 / k;
                let t = if limited { y.clamp(0.0, length) } else { y };
                let theta = k * t;
                let (s, c) = theta.sin_cos();
                // Point on the bent spine plus the offset `u` along the bent normal.
                let mut nu = r - (r - u) * c;
                let mut ny = (r - u) * s;
                // Beyond the bent region the object continues straight along the tangent.
                let rest = y - t;
                nu += rest * s;
                ny += rest * c;
                rot.transpose() * Vec3::new(nu, ny, w)
            }
            Deformer::Twist { size, angle, limited } => {
                let length = size.y;
                if length <= 0.0 || angle.abs() < 1e-6 {
                    return p;
                }
                let t = if limited { p.y.clamp(0.0, length) } else { p.y };
                let theta = angle.to_radians() * t / length;
                Mat3::from_rotation_y(theta) * p
            }
            Deformer::Taper { size, strength, curvature, limited } => {
                let length = size.y;
                if length <= 0.0 || strength.abs() < 1e-6 {
                    return p;
                }
                let scale = 1.0 - strength / 100.0 * profile(p.y, length, curvature, limited);
                Vec3::new(p.x * scale, p.y, p.z * scale)
            }
            Deformer::Shear { size, strength, direction, curvature, limited } => {
                let length = size.y;
                if length <= 0.0 || strength.abs() < 1e-6 {
                    return p;
                }
                let offset = strength / 100.0 * length * profile(p.y, length, curvature, limited);
                p + Mat3::from_rotation_y(direction.to_radians()) * Vec3::X * offset
            }
        }
    }
}

/// Applies `d`, whose local matrix (relative to the geometry's space) is `local`, to `geo`.
/// Normals follow the deformation through its numerically evaluated Jacobian.
pub fn apply(d: &Deformer, local: Mat4, geo: &mut Geo) {
    let d = d.sanitized();
    if local.determinant().abs() < 1e-12 || !local.is_finite() {
        return;
    }
    let inv = local.inverse();
    let f = |p: Vec3| local.transform_point3(d.deform_point(inv.transform_point3(p)));
    let scale = d.size().max_element().max(1.0);
    let eps = scale * 1e-4;
    for v in &mut geo.verts {
        let p = Vec3::from(v.pos);
        let fp = f(p);
        let j = Mat3::from_cols(
            (f(p + Vec3::X * eps) - f(p - Vec3::X * eps)) / (2.0 * eps),
            (f(p + Vec3::Y * eps) - f(p - Vec3::Y * eps)) / (2.0 * eps),
            (f(p + Vec3::Z * eps) - f(p - Vec3::Z * eps)) / (2.0 * eps),
        );
        let n = Vec3::from(v.normal);
        let jn = j.inverse().transpose() * n;
        let n2 = if jn.is_finite() { jn.normalize_or_zero() } else { n };
        if fp.is_finite() {
            *v = Vertex { pos: fp.to_array(), normal: n2.to_array() };
        }
    }
}

/// The wire box shown for a deformer, already deformed, in its own local space.
pub fn cage(d: &Deformer) -> Geo {
    let d = d.sanitized();
    let s = d.size();
    let (hx, hz, h) = (s.x * 0.5, s.z * 0.5, s.y);
    let corners = [
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.0, -hz),
        Vec3::new(hx, 0.0, hz),
        Vec3::new(-hx, 0.0, hz),
        Vec3::new(-hx, h, -hz),
        Vec3::new(hx, h, -hz),
        Vec3::new(hx, h, hz),
        Vec3::new(-hx, h, hz),
    ];
    const EDGES: [(usize, usize); 12] =
        [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    const STEPS: u32 = 16;
    let mut geo = Geo::default();
    for (a, b) in EDGES {
        let base = geo.verts.len() as u32;
        for i in 0..=STEPS {
            let p = corners[a].lerp(corners[b], i as f32 / STEPS as f32);
            geo.verts.push(Vertex { pos: d.deform_point(p).to_array(), normal: [0.0, 1.0, 0.0] });
        }
        for i in 0..STEPS {
            geo.edges.extend_from_slice(&[base + i, base + i + 1]);
        }
    }
    geo
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn near(a: Vec3, b: Vec3, tol: f32) -> bool {
        (a - b).length() < tol
    }

    fn bend(strength: f32, direction: f32, limited: bool) -> Deformer {
        Deformer::Bend { size: Vec3::new(100.0, 200.0, 100.0), strength, direction, limited }
    }

    #[test]
    fn quarter_bend_moves_the_top_of_the_spine_onto_the_circle() {
        let d = bend(90.0, 0.0, true);
        let r = 200.0 / FRAC_PI_2;
        assert!(near(d.deform_point(Vec3::new(0.0, 200.0, 0.0)), Vec3::new(r, r, 0.0), 1e-2));
        assert!(near(d.deform_point(Vec3::ZERO), Vec3::ZERO, 1e-4));
        // Below the box nothing bends.
        assert!(near(d.deform_point(Vec3::new(10.0, -50.0, 5.0)), Vec3::new(10.0, -50.0, 5.0), 1e-3));
    }

    #[test]
    fn bending_keeps_the_spine_length() {
        let d = bend(140.0, 30.0, true);
        let mut length = 0.0;
        let mut prev = d.deform_point(Vec3::ZERO);
        for i in 1..=400 {
            let p = d.deform_point(Vec3::new(0.0, 200.0 * i as f32 / 400.0, 0.0));
            length += (p - prev).length();
            prev = p;
        }
        assert!((length - 200.0).abs() < 0.1, "{length}");
    }

    #[test]
    fn limited_bend_continues_straight_but_unlimited_keeps_curling() {
        let limited = bend(90.0, 0.0, true);
        let unlimited = bend(90.0, 0.0, false);
        let r = 200.0 / FRAC_PI_2;
        // 50 cm past the end, the limited one continues along the tangent (+X).
        assert!(near(limited.deform_point(Vec3::new(0.0, 250.0, 0.0)), Vec3::new(r + 50.0, r, 0.0), 1e-2));
        let u = unlimited.deform_point(Vec3::new(0.0, 250.0, 0.0));
        assert!(u.y < r - 1.0, "unlimited keeps curving: {u:?}");
    }

    #[test]
    fn bend_direction_turns_the_bend_around_y() {
        let d = bend(90.0, 90.0, true);
        let top = d.deform_point(Vec3::new(0.0, 200.0, 0.0));
        let r = 200.0 / FRAC_PI_2;
        assert!(top.x.abs() < 1e-2 && (top.y - r).abs() < 1e-2 && (top.z.abs() - r).abs() < 1e-2, "{top:?}");
    }

    #[test]
    fn twist_rotates_proportionally_to_height() {
        let d = Deformer::Twist { size: Vec3::splat(200.0), angle: 90.0, limited: true };
        assert!(near(d.deform_point(Vec3::new(50.0, 0.0, 0.0)), Vec3::new(50.0, 0.0, 0.0), 1e-4));
        let top = d.deform_point(Vec3::new(50.0, 200.0, 0.0));
        assert!((Vec3::new(top.x, 0.0, top.z).length() - 50.0).abs() < 1e-3 && top.x.abs() < 1e-3);
        let mid = d.deform_point(Vec3::new(50.0, 100.0, 0.0));
        assert!((mid.x - 50.0 * (std::f32::consts::FRAC_PI_4).cos()).abs() < 1e-3);
        // Above a limited twist the rotation stays at the full angle.
        let above = d.deform_point(Vec3::new(50.0, 300.0, 0.0));
        assert!(near(above, Vec3::new(top.x, 300.0, top.z), 1e-3));
    }

    #[test]
    fn zero_strength_and_degenerate_sizes_are_identity_and_finite() {
        let p = Vec3::new(12.0, 34.0, 56.0);
        assert_eq!(bend(0.0, 0.0, true).deform_point(p), p);
        let flat = Deformer::Bend { size: Vec3::ZERO, strength: 90.0, direction: 0.0, limited: true }.sanitized();
        assert!(flat.deform_point(p).is_finite());
        let nan = Deformer::Twist { size: Vec3::splat(f32::NAN), angle: f32::INFINITY, limited: false }.sanitized();
        assert!(nan.deform_point(p).is_finite());
    }

    #[test]
    fn apply_works_in_parent_space_and_bends_normals() {
        // A thin, tall, well segmented box standing on the deformer.
        let mesh = crate::primitive::Primitive::Cube { size: Vec3::new(20.0, 200.0, 20.0), segments: [1, 20, 1] }.build();
        let mut geo = Geo::from_mesh(&mesh);
        let d = bend(90.0, 0.0, true);
        // The cube is centred on its origin, so the deformer sits 100 cm lower.
        let local = Mat4::from_translation(Vec3::new(0.0, -100.0, 0.0));
        apply(&d, local, &mut geo);
        let r = 200.0 / FRAC_PI_2;
        let max_x = geo.verts.iter().map(|v| v.pos[0]).fold(f32::MIN, f32::max);
        // After a quarter bend the top cap lies in the plane x = r.
        assert!((max_x - r).abs() < 0.5, "top end swung out to +X: {max_x}");
        for v in &geo.verts {
            let n = Vec3::from(v.normal);
            assert!((n.length() - 1.0).abs() < 1e-3);
        }
        // The top cap (originally +Y) now faces +X.
        let top_cap = geo.verts.iter().filter(|v| v.pos[0] > r - 0.5).map(|v| Vec3::from(v.normal)).find(|n| n.x > 0.9);
        assert!(top_cap.is_some());
    }

    #[test]
    fn taper_scales_slices_by_height() {
        let d = Deformer::Taper { size: Vec3::splat(200.0), strength: 50.0, curvature: 0.0, limited: true };
        // The base is untouched, the top is half as wide, the middle three quarters.
        assert!(near(d.deform_point(Vec3::new(40.0, 0.0, -20.0)), Vec3::new(40.0, 0.0, -20.0), 1e-4));
        assert!(near(d.deform_point(Vec3::new(40.0, 200.0, -20.0)), Vec3::new(20.0, 200.0, -10.0), 1e-3));
        assert!(near(d.deform_point(Vec3::new(40.0, 100.0, 0.0)), Vec3::new(30.0, 100.0, 0.0), 1e-3));
        // Limited: above the box the top's scale carries on.
        assert!(near(d.deform_point(Vec3::new(40.0, 300.0, 0.0)), Vec3::new(20.0, 300.0, 0.0), 1e-3));
        // Unlimited keeps narrowing.
        let u = Deformer::Taper { size: Vec3::splat(200.0), strength: 50.0, curvature: 0.0, limited: false };
        assert!(near(u.deform_point(Vec3::new(40.0, 300.0, 0.0)), Vec3::new(10.0, 300.0, 0.0), 1e-3));
        // 100 % pinches the top to a point; negative strength widens.
        let pinch = Deformer::Taper { size: Vec3::splat(200.0), strength: 100.0, curvature: 0.0, limited: true };
        assert!(near(pinch.deform_point(Vec3::new(40.0, 200.0, 40.0)), Vec3::new(0.0, 200.0, 0.0), 1e-3));
        let flare = Deformer::Taper { size: Vec3::splat(200.0), strength: -50.0, curvature: 0.0, limited: true };
        assert!((flare.deform_point(Vec3::new(40.0, 200.0, 0.0)).x - 60.0).abs() < 1e-3);
    }

    #[test]
    fn curvature_rounds_the_profile_but_keeps_both_ends() {
        let straight = Deformer::Taper { size: Vec3::splat(200.0), strength: 50.0, curvature: 0.0, limited: true };
        let curved = Deformer::Taper { size: Vec3::splat(200.0), strength: 50.0, curvature: 100.0, limited: true };
        let p = |d: &Deformer, y: f32| d.deform_point(Vec3::new(40.0, y, 0.0)).x;
        assert!((p(&curved, 0.0) - 40.0).abs() < 1e-4 && (p(&curved, 200.0) - 20.0).abs() < 1e-3);
        // Halfway up, the curved taper has narrowed less (a rounded shoulder).
        assert!(p(&curved, 100.0) > p(&straight, 100.0) + 4.0);
    }

    #[test]
    fn shear_slides_slices_sideways() {
        let d = Deformer::Shear { size: Vec3::splat(200.0), strength: 50.0, direction: 0.0, curvature: 0.0, limited: true };
        assert!(near(d.deform_point(Vec3::new(10.0, 0.0, 5.0)), Vec3::new(10.0, 0.0, 5.0), 1e-4));
        // The top moves 50 % of the height (100 cm) towards +X; heights and depth stay.
        assert!(near(d.deform_point(Vec3::new(10.0, 200.0, 5.0)), Vec3::new(110.0, 200.0, 5.0), 1e-3));
        assert!(near(d.deform_point(Vec3::new(10.0, 100.0, 5.0)), Vec3::new(60.0, 100.0, 5.0), 1e-3));
        // Limited: above the box the offset stays; unlimited keeps leaning.
        assert!(near(d.deform_point(Vec3::new(0.0, 300.0, 0.0)), Vec3::new(100.0, 300.0, 0.0), 1e-3));
        let u = Deformer::Shear { size: Vec3::splat(200.0), strength: 50.0, direction: 0.0, curvature: 0.0, limited: false };
        assert!(near(u.deform_point(Vec3::new(0.0, 300.0, 0.0)), Vec3::new(150.0, 300.0, 0.0), 1e-3));
        // Direction turns it around Y, the same way as Bend's angle.
        let side = Deformer::Shear { size: Vec3::splat(200.0), strength: 50.0, direction: 90.0, curvature: 0.0, limited: true };
        let bend_side = bend(90.0, 90.0, true).deform_point(Vec3::new(0.0, 200.0, 0.0));
        let top = side.deform_point(Vec3::new(0.0, 200.0, 0.0));
        assert!(top.x.abs() < 1e-3 && (top.z.abs() - 100.0).abs() < 1e-3 && top.z.signum() == bend_side.z.signum(), "{top:?}");
    }

    #[test]
    fn new_deformers_are_safe_with_bad_input() {
        let p = Vec3::new(12.0, 34.0, 56.0);
        for d in [
            Deformer::Taper { size: Vec3::splat(f32::NAN), strength: f32::INFINITY, curvature: f32::NAN, limited: false },
            Deformer::Shear { size: Vec3::ZERO, strength: 1e9, direction: f32::NAN, curvature: -5.0, limited: true },
        ] {
            assert!(d.sanitized().deform_point(p).is_finite());
        }
        let zero = Deformer::Taper { size: Vec3::splat(200.0), strength: 0.0, curvature: 50.0, limited: true };
        assert_eq!(zero.deform_point(p), p);
    }

    #[test]
    fn shear_and_taper_keep_normals_unit_length() {
        let mesh = crate::primitive::Primitive::Cube { size: Vec3::new(100.0, 200.0, 100.0), segments: [2, 10, 2] }.build();
        for d in [DeformerKind::Taper, DeformerKind::Shear] {
            let mut geo = Geo::from_mesh(&mesh);
            apply(&d.default_deformer(), Mat4::from_translation(Vec3::new(0.0, -100.0, 0.0)), &mut geo);
            assert!(geo.verts.iter().all(|v| (Vec3::from(v.normal).length() - 1.0).abs() < 1e-3));
        }
    }

    #[test]
    fn cage_has_twelve_bent_edges() {
        let g = cage(&bend(90.0, 0.0, true));
        assert_eq!(g.edges.len() / 2, 12 * 16);
        assert!(g.tris.is_empty());
        assert!(g.verts.iter().any(|v| v.pos[0] > 120.0), "cage follows the bend");
    }
}
