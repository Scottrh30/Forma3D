//! Light objects (the Light menu): Area, Point, Spot, Omni, Infinite, Dome
//! and Sun.
//!
//! As in Cinema 4D a light shines along its object's +Z axis, and as soon as
//! the scene has a visible light the viewport uses the scene's lights instead
//! of the default light attached to the camera.

use crate::mesh::{Mesh, Vertex};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LightKind {
    Area,
    Point,
    Spot,
    Omni,
    Infinite,
    Dome,
    Sun,
}

impl LightKind {
    /// In Light menu order.
    pub const ALL: [LightKind; 7] =
        [LightKind::Area, LightKind::Point, LightKind::Spot, LightKind::Omni, LightKind::Infinite, LightKind::Dome, LightKind::Sun];

    pub fn name(self) -> &'static str {
        match self {
            LightKind::Area => "Area Light",
            LightKind::Point => "Point Light",
            LightKind::Spot => "Spot Light",
            LightKind::Omni => "Omni Light",
            LightKind::Infinite => "Infinite Light",
            LightKind::Dome => "Dome Light",
            LightKind::Sun => "Sun",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            LightKind::Area => "area",
            LightKind::Point => "point",
            LightKind::Spot => "spot",
            LightKind::Omni => "omni",
            LightKind::Infinite => "infinite",
            LightKind::Dome => "dome",
            LightKind::Sun => "sun",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            LightKind::Area => "light softbox rectangle panel",
            LightKind::Point => "light bulb inverse square",
            LightKind::Spot => "light cone spotlight",
            LightKind::Omni => "light standard all directions",
            LightKind::Infinite => "light directional parallel distant",
            LightKind::Dome => "light sky environment ambient hdri",
            LightKind::Sun => "light daylight sky physical",
        }
    }

    pub fn default_light(self) -> Light {
        let mut l = Light { kind: self, ..Light::default() };
        match self {
            LightKind::Area | LightKind::Point | LightKind::Spot => {
                l.falloff = true;
                l.radius = 2500.0;
                if self == LightKind::Spot {
                    l.inner_angle = 25.0;
                    l.outer_angle = 40.0;
                }
            }
            LightKind::Dome => {
                l.color = [0.78, 0.85, 1.0];
                l.shadows = false;
            }
            LightKind::Sun => l.intensity = 110.0,
            LightKind::Omni | LightKind::Infinite => {}
        }
        l
    }

    /// Where a new light goes (position, HPB), so it lights what you see:
    /// a key light above, to the left of and in front of the camera's view,
    /// aimed at what the camera looks at. `toward_camera` points from the
    /// camera's target towards the camera.
    pub fn default_placement(self, target: Vec3, toward_camera: Vec3) -> (Vec3, Vec3) {
        if self == LightKind::Dome {
            return (target, Vec3::ZERO);
        }
        let mut back = Vec3::new(toward_camera.x, 0.0, toward_camera.z).normalize_or(Vec3::NEG_Z);
        // Swing 30° to the camera's left.
        back = glam::Quat::from_rotation_y(30f32.to_radians()) * back;
        // Scaled to the view: about as far out as the camera, raised 35°.
        let reach = toward_camera.length().clamp(300.0, 20_000.0) * 0.8;
        let pos = target + back * reach + Vec3::Y * reach * 0.7;
        (pos, aim_hpb(target - pos))
    }

    fn directional(self) -> bool {
        matches!(self, LightKind::Infinite | LightKind::Sun)
    }
}

/// HPB angles (degrees) that point an object's +Z axis along `dir`.
pub fn aim_hpb(dir: Vec3) -> Vec3 {
    let d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        return Vec3::ZERO;
    }
    let h = d.x.atan2(d.z);
    let p = (-d.y).clamp(-1.0, 1.0).asin();
    Vec3::new(h.to_degrees(), p.to_degrees(), 0.0)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    pub color: [f32; 3],
    /// Brightness in percent (100 = normal).
    pub intensity: f32,
    /// Fades with distance, reaching zero at `radius`.
    pub falloff: bool,
    pub radius: f32,
    /// Spot cone: full brightness inside `inner`, none outside `outer` (degrees).
    pub inner_angle: f32,
    pub outer_angle: f32,
    /// Area light rectangle (cm).
    pub size: [f32; 2],
    /// Casts shadows when the viewport's Shadows option is on.
    pub shadows: bool,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            kind: LightKind::Omni,
            color: [1.0, 1.0, 1.0],
            intensity: 100.0,
            falloff: false,
            radius: 1000.0,
            inner_angle: 20.0,
            outer_angle: 30.0,
            size: [200.0, 200.0],
            shadows: true,
        }
    }
}

impl Light {
    pub fn sanitized(&self) -> Light {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let outer = f(self.outer_angle, 1.0, 89.0, 30.0);
        Light {
            kind: self.kind,
            color: self.color.map(|c| f(c, 0.0, 1.0, 1.0)),
            intensity: f(self.intensity, 0.0, 10_000.0, 100.0),
            falloff: self.falloff,
            radius: f(self.radius, 1.0, 1e7, 1000.0),
            inner_angle: f(self.inner_angle, 0.0, outer, 20.0),
            outer_angle: outer,
            size: self.size.map(|s| f(s, 0.1, 1e6, 200.0)),
            shadows: self.shadows,
        }
    }

    /// Whether this light uses a direction (rather than a position).
    pub fn directional(&self) -> bool {
        matches!(self.kind, LightKind::Infinite | LightKind::Sun)
    }

    /// The Sun's colour follows its height: white at noon, orange towards the
    /// horizon, deep red at sunset. `dir` is the direction the light travels.
    pub fn sun_color(dir: Vec3) -> [f32; 3] {
        let elevation = (-dir.normalize_or_zero().y).clamp(0.0, 1.0);
        let warm = 1.0 - elevation.powf(0.45);
        let c = Vec3::ONE.lerp(Vec3::new(1.0, 0.55, 0.28), warm);
        // Below the horizon there's no sun.
        let up = (-dir.normalize_or_zero().y * 8.0 + 0.2).clamp(0.0, 1.0);
        (c * up).to_array()
    }

    /// The light's outline for the viewport, in its own space (+Z = direction).
    pub fn gizmo(&self) -> Mesh {
        let mut lines: Vec<(Vec3, Vec3)> = Vec::new();
        let circle = |lines: &mut Vec<(Vec3, Vec3)>, centre: Vec3, r: f32, axis: usize| {
            let n = 24;
            for k in 0..n {
                let a = TAU * k as f32 / n as f32;
                let b = TAU * (k + 1) as f32 / n as f32;
                let at = |t: f32| {
                    let (s, c) = t.sin_cos();
                    centre + match axis {
                        0 => Vec3::new(0.0, c * r, s * r),
                        1 => Vec3::new(c * r, 0.0, s * r),
                        _ => Vec3::new(c * r, s * r, 0.0),
                    }
                };
                lines.push((at(a), at(b)));
            }
        };
        let arrow = |lines: &mut Vec<(Vec3, Vec3)>, from: Vec3, len: f32| {
            let tip = from + Vec3::Z * len;
            lines.push((from, tip));
            for side in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y] {
                lines.push((tip, tip - Vec3::Z * len * 0.18 + side * len * 0.08));
            }
        };
        match self.kind {
            LightKind::Omni | LightKind::Point => {
                let r = 18.0;
                for axis in 0..3 {
                    circle(&mut lines, Vec3::ZERO, r, axis);
                }
                // Rays: a point light's are short and dashed, an omni's long.
                let ray = if self.kind == LightKind::Omni { 34.0 } else { 28.0 };
                for d in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                    lines.push((d * (r + 4.0), d * ray));
                }
            }
            LightKind::Spot => {
                let len = 160.0;
                let r_out = len * self.outer_angle.to_radians().tan();
                let r_in = len * self.inner_angle.to_radians().tan();
                circle(&mut lines, Vec3::Z * len, r_out, 2);
                if r_in > 0.5 {
                    circle(&mut lines, Vec3::Z * len, r_in, 2);
                }
                for k in 0..4 {
                    let a = TAU * k as f32 / 4.0;
                    lines.push((Vec3::ZERO, Vec3::new(a.cos() * r_out, a.sin() * r_out, len)));
                }
                circle(&mut lines, Vec3::ZERO, 8.0, 2);
            }
            LightKind::Area => {
                let (w, h) = (self.size[0] * 0.5, self.size[1] * 0.5);
                let c = [Vec3::new(-w, -h, 0.0), Vec3::new(w, -h, 0.0), Vec3::new(w, h, 0.0), Vec3::new(-w, h, 0.0)];
                for k in 0..4 {
                    lines.push((c[k], c[(k + 1) % 4]));
                }
                lines.push((c[0], c[2]));
                lines.push((c[1], c[3]));
                arrow(&mut lines, Vec3::ZERO, (w.min(h) * 0.8).max(40.0));
            }
            LightKind::Infinite => {
                for off in [Vec3::new(-20.0, 0.0, 0.0), Vec3::ZERO, Vec3::new(20.0, 0.0, 0.0)] {
                    arrow(&mut lines, off - Vec3::Z * 40.0, 100.0);
                }
            }
            LightKind::Sun => {
                circle(&mut lines, Vec3::ZERO, 22.0, 2);
                for k in 0..8 {
                    let a = TAU * k as f32 / 8.0;
                    let d = Vec3::new(a.cos(), a.sin(), 0.0);
                    lines.push((d * 28.0, d * 40.0));
                }
                arrow(&mut lines, Vec3::ZERO, 120.0);
            }
            LightKind::Dome => {
                // A wire hemisphere: the sky around the scene.
                let r = 60.0;
                circle(&mut lines, Vec3::ZERO, r, 1);
                for k in 0..4 {
                    let a = TAU * k as f32 / 8.0;
                    let (s, c) = a.sin_cos();
                    for j in 0..8 {
                        let t0 = std::f32::consts::FRAC_PI_2 * j as f32 / 8.0;
                        let t1 = std::f32::consts::FRAC_PI_2 * (j + 1) as f32 / 8.0;
                        let p = |t: f32| Vec3::new(c * t.cos() * r, t.sin() * r, s * t.cos() * r);
                        let q = |t: f32| Vec3::new(-c * t.cos() * r, t.sin() * r, -s * t.cos() * r);
                        lines.push((p(t0), p(t1)));
                        lines.push((q(t0), q(t1)));
                    }
                }
                circle(&mut lines, Vec3::Y * r * 0.7, r * 0.714, 1);
            }
        }
        let mut vertices = Vec::with_capacity(lines.len() * 2);
        let mut edges = Vec::with_capacity(lines.len() * 2);
        for (a, b) in lines {
            for p in [a, b] {
                edges.push(vertices.len() as u32);
                vertices.push(Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0] });
            }
        }
        Mesh::from_parts(vertices, Vec::new(), edges)
    }
}

/// Up to this many lights reach the shader (the first casts shadows).
pub const MAX_LIGHTS: usize = 8;

/// One light as the shader sees it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightRaw {
    /// xyz: world position; w: kind (0 point, 1 spot, 2 area, 3 directional).
    pub pos: [f32; 4],
    /// xyz: direction the light travels; w: cosine of the outer cone angle.
    pub dir: [f32; 4],
    /// rgb: colour times intensity; w: cosine of the inner cone angle.
    pub color: [f32; 4],
    /// x: falloff radius (0 = none); y: area half-size (softening);
    /// z: 1 when it casts shadows; w unused.
    pub params: [f32; 4],
}

/// The scene's lights for the viewport: direct lights (shadow caster first)
/// and the summed ambient of Dome lights (and the Sun's sky).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneLights {
    pub lights: Vec<LightRaw>,
    pub ambient: [f32; 3],
    /// Whether the scene has any light at all (then the camera light is off).
    pub any: bool,
}

impl SceneLights {
    /// Builds the shader data from (light, world matrix) pairs.
    pub fn gather<'a>(lights: impl IntoIterator<Item = (&'a Light, Mat4)>) -> SceneLights {
        let mut out = SceneLights::default();
        let mut direct: Vec<(bool, f32, LightRaw)> = Vec::new();
        let mut ambient = Vec3::ZERO;
        for (l, world) in lights {
            let l = l.sanitized();
            out.any = true;
            let pos = world.transform_point3(Vec3::ZERO);
            let dir = world.transform_vector3(Vec3::Z).normalize_or(Vec3::NEG_Y);
            let strength = l.intensity / 100.0;
            let mut color = Vec3::from(l.color) * strength;
            match l.kind {
                LightKind::Dome => {
                    ambient += color;
                    continue;
                }
                LightKind::Sun => {
                    let sun = Vec3::from(Light::sun_color(dir));
                    color *= sun;
                    // A sun comes with a soft blue sky.
                    let up = (-dir.y).clamp(0.0, 1.0);
                    ambient += Vec3::new(0.30, 0.38, 0.52) * (0.25 + 0.75 * up) * strength * 0.6;
                }
                _ => {}
            }
            let kind = match l.kind {
                LightKind::Spot => 1.0,
                LightKind::Area => 2.0,
                LightKind::Infinite | LightKind::Sun => 3.0,
                _ => 0.0,
            };
            let raw = LightRaw {
                pos: [pos.x, pos.y, pos.z, kind],
                dir: [dir.x, dir.y, dir.z, l.outer_angle.to_radians().cos()],
                color: [color.x, color.y, color.z, l.inner_angle.to_radians().cos()],
                params: [
                    if l.falloff && !l.directional() { l.radius } else { 0.0 },
                    if l.kind == LightKind::Area { (l.size[0] + l.size[1]) * 0.25 } else { 0.0 },
                    if l.shadows { 1.0 } else { 0.0 },
                    0.0,
                ],
            };
            let brightness = color.x + color.y + color.z;
            direct.push((l.shadows, brightness, raw));
        }
        // The brightest shadow-casting light goes first: it casts the shadows.
        direct.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
        out.lights = direct.into_iter().take(MAX_LIGHTS).map(|d| d.2).collect();
        out.ambient = ambient.to_array();
        out
    }

    /// Direction towards the shadow-casting light from the middle of the
    /// scene, if one casts shadows.
    pub fn shadow_direction(&self, scene_centre: Vec3) -> Option<Vec3> {
        let l = self.lights.first().filter(|l| l.params[2] > 0.5)?;
        let to_light = if l.pos[3] == 3.0 {
            -Vec3::new(l.dir[0], l.dir[1], l.dir[2])
        } else {
            Vec3::new(l.pos[0], l.pos[1], l.pos[2]) - scene_centre
        };
        Some(to_light.normalize_or_zero()).filter(|d| *d != Vec3::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aiming_points_plus_z_at_the_target() {
        for dir in [Vec3::new(1.0, -1.0, 0.5), Vec3::NEG_Y + Vec3::X * 0.01, Vec3::Z, Vec3::new(-3.0, 2.0, -1.0)] {
            let q = crate::scene::hpb_to_quat(aim_hpb(dir));
            let z = q * Vec3::Z;
            assert!(z.dot(dir.normalize()) > 0.9999, "{dir:?} -> {z:?}");
        }
    }

    #[test]
    fn new_lights_face_what_the_camera_sees() {
        let target = Vec3::new(0.0, 50.0, 0.0);
        let toward_camera = Vec3::new(0.0, 300.0, -1000.0);
        for k in LightKind::ALL {
            let (pos, rot) = k.default_placement(target, toward_camera);
            if k == LightKind::Dome {
                assert_eq!(pos, target);
                continue;
            }
            assert!(pos.y > target.y + 300.0, "{k:?} sits above");
            assert!(pos.z < -500.0, "{k:?} is well out on the camera's side");
            let z = crate::scene::hpb_to_quat(rot) * Vec3::Z;
            assert!(z.dot((target - pos).normalize()) > 0.999, "{k:?} aims at the target");
        }
    }

    #[test]
    fn every_kind_has_a_gizmo_and_sensible_defaults() {
        for k in LightKind::ALL {
            let l = k.default_light();
            let g = l.gizmo();
            assert!(!g.edges.is_empty() && g.indices.is_empty(), "{k:?}");
            assert_eq!(l.sanitized(), l, "{k:?} defaults are already clean");
        }
    }

    #[test]
    fn gather_sorts_out_kinds_and_shadow_caster() {
        let omni = LightKind::Omni.default_light();
        let mut dim = LightKind::Spot.default_light();
        dim.intensity = 20.0;
        let mut no_shadow = LightKind::Point.default_light();
        no_shadow.intensity = 500.0;
        no_shadow.shadows = false;
        let dome = LightKind::Dome.default_light();
        let at = Mat4::from_translation(Vec3::new(0.0, 300.0, 0.0));
        let lit = SceneLights::gather([(&dim, at), (&no_shadow, at), (&omni, at), (&dome, Mat4::IDENTITY)]);
        assert!(lit.any);
        assert_eq!(lit.lights.len(), 3, "the dome is ambient, not a direct light");
        assert!(lit.ambient.iter().all(|c| *c > 0.3));
        // The brightest light that casts shadows comes first, not the brightest overall.
        assert_eq!(lit.lights[0].color[0], 1.0, "the omni at 100 %");
        assert_eq!(lit.lights[0].pos[3], 0.0);
        assert_eq!(lit.shadow_direction(Vec3::ZERO), Some(Vec3::Y));
        assert!(SceneLights::gather([]).lights.is_empty());
        // With no light casting shadows there is no shadow direction.
        assert_eq!(SceneLights::gather([(&no_shadow, at)]).shadow_direction(Vec3::ZERO), None);
        assert!(!SceneLights::gather([]).any);
    }

    #[test]
    fn the_sun_warms_towards_the_horizon_and_sets_below_it() {
        let noon = Light::sun_color(Vec3::NEG_Y);
        let evening = Light::sun_color(Vec3::new(1.0, -0.12, 0.0));
        let night = Light::sun_color(Vec3::new(1.0, 0.3, 0.0));
        assert!(noon.iter().all(|c| *c > 0.95));
        assert!(evening[2] < evening[0] * 0.75, "orange: {evening:?}");
        assert!(night.iter().all(|c| *c < 1e-3));
    }

    #[test]
    fn bad_numbers_are_cleaned() {
        let l = Light { intensity: f32::NAN, outer_angle: 500.0, inner_angle: 400.0, radius: -3.0, ..Light::default() };
        let s = l.sanitized();
        assert_eq!(s.intensity, 100.0);
        assert!(s.outer_angle <= 89.0 && s.inner_angle <= s.outer_angle && s.radius >= 1.0);
    }
}
