//! Turns the editor's scene into what the ray tracer needs: world-space
//! triangles in a BVH, object colours, lights (with their link sets), the
//! sky and the camera.

use super::bvh::{Bvh, Tri};
use super::{RenderSettings, WorkingSpace};
use crate::light::{Light, LightKind};
use crate::scene::ObjectKind;
use crate::scene_camera::CameraPose;
use crate::sky::Sky;
use glam::{Mat3, Vec3};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RLightKind {
    Point,
    Spot,
    Area,
    /// Light from one direction; `parallel` lights only what's in front of
    /// the light's plane.
    Directional { parallel: bool },
}

#[derive(Clone, Debug)]
pub struct RLight {
    pub kind: RLightKind,
    pub pos: Vec3,
    /// The direction the light travels (its +Z).
    pub dir: Vec3,
    /// Area light axes (half sizes included).
    pub right: Vec3,
    pub up: Vec3,
    /// Linear colour × intensity (working space).
    pub color: Vec3,
    pub inner_cos: f32,
    pub outer_cos: f32,
    /// Fades to nothing at this distance.
    pub radius: Option<f32>,
    pub shadows: bool,
    /// The light's size for soft shadows (cm; for a direction, the angle's tangent).
    pub softness: f32,
    pub density: f32,
    pub shadow_color: Vec3,
    /// Per render object: whether the light reaches it (light linking).
    pub lights: Vec<bool>,
}

impl RLight {
    pub fn reaches(&self, object: u32) -> bool {
        self.lights.get(object as usize).copied().unwrap_or(true)
    }
}

#[derive(Clone, Debug)]
pub struct RObject {
    pub id: u64,
    /// Linear colour (working space).
    pub color: Vec3,
    pub material: Option<u64>,
}

pub struct RenderScene {
    pub bvh: Bvh,
    pub objects: Vec<RObject>,
    pub lights: Vec<RLight>,
    /// Even light from all around (Dome lights; the sky in Standard).
    pub ambient: Vec3,
    /// The sky (seen behind everything, and lighting the Physical engine).
    pub sky: Option<Sky>,
    /// The sky's light in the Standard engine (already in `ambient`).
    pub sky_ambient: Vec3,
    pub camera: CameraPose,
    pub width: u32,
    pub height: u32,
    /// The scene's size (for ray offsets and AO).
    pub extent: f32,
    pub working: WorkingSpace,
}

/// Linear sRGB → ACEScg.
const TO_AP1: Mat3 = Mat3::from_cols_array(&[0.613_097, 0.070_194, 0.020_616, 0.339_523, 0.916_354, 0.109_570, 0.047_379, 0.013_452, 0.869_815]);
/// ACEScg → linear sRGB.
const FROM_AP1: Mat3 = Mat3::from_cols_array(&[1.704_859, -0.130_078, -0.023_964, -0.621_715, 1.140_734, -0.128_975, -0.083_299, -0.010_560, 1.153_013]);

pub fn to_working(c: Vec3, w: WorkingSpace) -> Vec3 {
    match w {
        WorkingSpace::LinearSrgb => c,
        WorkingSpace::AcesCg => TO_AP1 * c,
    }
}

pub fn from_working(c: Vec3, w: WorkingSpace) -> Vec3 {
    match w {
        WorkingSpace::LinearSrgb => c,
        WorkingSpace::AcesCg => (FROM_AP1 * c).max(Vec3::ZERO),
    }
}

/// sRGB-encoded colour (as the colour pickers store it) → linear.
pub fn srgb_to_linear(c: [f32; 3]) -> Vec3 {
    Vec3::from(c.map(|x| if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }))
}

/// Gathers the scene at `frame` for a render of `size` pixels.
pub fn build_scene(ed: &mut crate::editor::Editor, settings: &RenderSettings, frame: i32, size: (u32, u32), fallback: &CameraPose) -> RenderScene {
    let saved = ed.timeline.frame;
    ed.timeline.frame = frame;
    let time = ed.time();
    let w = settings.working_space;
    let objects = ed.final_objects();
    let scene = &ed.scene;

    let mut tris = Vec::new();
    let mut robjects = Vec::with_capacity(objects.len());
    for (index, o) in objects.iter().enumerate() {
        let normal_m = o.world.inverse().transpose();
        let m = &o.mesh;
        let pos: Vec<Vec3> = m.vertices.iter().map(|v| o.world.transform_point3(Vec3::from(v.pos))).collect();
        let nor: Vec<Vec3> = m.vertices.iter().map(|v| normal_m.transform_vector3(Vec3::from(v.normal)).normalize_or(Vec3::Y)).collect();
        for t in m.indices.chunks_exact(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            if a >= pos.len() || b >= pos.len() || c >= pos.len() {
                continue;
            }
            let p = [pos[a], pos[b], pos[c]];
            if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() < 1e-14 {
                continue;
            }
            tris.push(Tri::new(p, [nor[a], nor[b], nor[c]], index as u32));
        }
        let base = if settings.material_override { settings.override_color } else { o.color };
        robjects.push(RObject { id: o.id, color: to_working(srgb_to_linear(base), w), material: scene.get(o.id).and_then(|x| x.material) });
    }

    // Lights and the sky.
    let mut found: Vec<(Light, glam::Mat4)> = Vec::new();
    let mut sky: Option<Sky> = None;
    scene.walk(|o, _| {
        if !scene.render_visible(o.id) {
            return;
        }
        match &o.kind {
            ObjectKind::Light(l) => found.push((l.clone(), scene.effective_world(o.id))),
            ObjectKind::Sky(s) if sky.is_none() => sky = Some(s.sanitized()),
            _ => {}
        }
    });
    let ids: Vec<u64> = robjects.iter().map(|o| o.id).collect();
    let mut lights = Vec::new();
    let mut ambient = Vec3::ZERO;
    for (l, world) in &found {
        let l = l.sanitized();
        let covered: HashSet<u64> = l.link_objects.iter().flat_map(|&id| scene.subtree(id)).collect();
        let reach: Vec<bool> = ids.iter().map(|&id| l.lights_object(&covered, id)).collect();
        push_light(&mut lights, &mut ambient, &l, *world, reach, w);
    }
    let mut sky_ambient = Vec3::ZERO;
    if let Some(s) = &sky {
        for (l, world) in crate::editor::sky_lights(s) {
            if l.kind == LightKind::Dome {
                sky_ambient = to_working(Vec3::from(l.color) * l.intensity / 100.0, w);
            } else {
                push_light(&mut lights, &mut ambient, &l, world, Vec::new(), w);
            }
        }
    }

    let camera = scene
        .render_camera
        .filter(|id| matches!(scene.get(*id).map(|o| &o.kind), Some(ObjectKind::Camera(_))))
        .and_then(|id| crate::scene_camera::pose(scene, id, time))
        .unwrap_or(*fallback);

    // Without any light, light the scene like the viewport's default light:
    // from above the camera's left shoulder.
    if lights.is_empty() && ambient == Vec3::ZERO && sky.is_none() {
        let dir = (camera.forward + camera.up * -0.6 + camera.right() * 0.4).normalize_or(Vec3::NEG_Y);
        lights.push(RLight {
            kind: RLightKind::Directional { parallel: false },
            pos: camera.eye,
            dir,
            right: Vec3::X,
            up: Vec3::Y,
            color: Vec3::splat(0.9),
            inner_cos: 1.0,
            outer_cos: 0.0,
            radius: None,
            shadows: false,
            softness: 0.0,
            density: 1.0,
            shadow_color: Vec3::ZERO,
            lights: Vec::new(),
        });
        ambient = to_working(Vec3::splat(0.22), w);
    }

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for t in &tris {
        lo = lo.min(t.a).min(t.a + t.e1).min(t.a + t.e2);
        hi = hi.max(t.a).max(t.a + t.e1).max(t.a + t.e2);
    }
    let extent = if lo.x <= hi.x { (hi - lo).length().max(1.0) } else { 100.0 };
    ed.timeline.frame = saved;
    let bvh = Bvh::build(tris);
    RenderScene {
        bvh,
        objects: robjects,
        lights,
        ambient: ambient + sky_ambient,
        sky,
        sky_ambient,
        camera,
        width: size.0,
        height: size.1,
        extent,
        working: w,
    }
}

fn push_light(lights: &mut Vec<RLight>, ambient: &mut Vec3, l: &Light, world: glam::Mat4, reach: Vec<bool>, w: WorkingSpace) {
    let pos = world.transform_point3(Vec3::ZERO);
    let dir = world.transform_vector3(Vec3::Z).normalize_or(Vec3::NEG_Y);
    let mut color = srgb_to_linear(l.color) * (l.intensity / 100.0);
    let kind = match l.kind {
        LightKind::Dome => {
            *ambient += to_working(color, w);
            return;
        }
        LightKind::Sun => {
            color *= Vec3::from(Light::sun_color(dir));
            let up = (-dir.y).clamp(0.0, 1.0);
            *ambient += to_working(Vec3::new(0.30, 0.38, 0.52) * (0.25 + 0.75 * up) * (l.intensity / 100.0) * 0.6, w);
            RLightKind::Directional { parallel: false }
        }
        LightKind::Infinite => RLightKind::Directional { parallel: false },
        LightKind::Parallel => RLightKind::Directional { parallel: true },
        LightKind::Spot => RLightKind::Spot,
        LightKind::Area => RLightKind::Area,
        LightKind::Point | LightKind::Omni => RLightKind::Point,
    };
    let directional = matches!(kind, RLightKind::Directional { .. });
    let right = world.transform_vector3(Vec3::X).normalize_or(Vec3::X) * l.size[0] * 0.5;
    let up = world.transform_vector3(Vec3::Y).normalize_or(Vec3::Y) * l.size[1] * 0.5;
    // An area light's brightness doesn't depend on its size; others are points.
    let softness = if directional { (l.shadow_softness / 1000.0).min(0.5) } else { l.shadow_softness };
    lights.push(RLight {
        kind,
        pos,
        dir,
        right,
        up,
        color: to_working(color, w),
        inner_cos: l.inner_angle.to_radians().cos(),
        outer_cos: l.outer_angle.to_radians().cos(),
        radius: (l.falloff && !directional).then_some(l.radius),
        shadows: l.shadows,
        softness,
        density: l.shadow_density / 100.0,
        shadow_color: srgb_to_linear(l.shadow_color),
        lights: reach,
    });
}
