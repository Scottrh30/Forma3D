//! Camera objects (Create > Camera): the cameras a scene is rendered
//! through. A camera looks along its object's +Z axis with +Y up, like
//! Cinema 4D's. The rigs (Target, Stereo, Motion, Morph, Crane) are kinds of
//! the same object; `pose` works out where a camera is at a given time.

use crate::mesh::{Mesh, Vertex};
use crate::scene::{ObjectId, ObjectKind, Scene};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraKind {
    Standard,
    /// Always aims at its Look At object (a Null is made with it).
    Target,
    /// Renders a left and a right eye.
    Stereo,
    /// Handheld shake over time.
    Motion,
    /// Blends between two other cameras.
    Morph,
    /// A camera on a crane: column, boom and head.
    Crane,
}

impl CameraKind {
    pub const ALL: [CameraKind; 6] = [CameraKind::Standard, CameraKind::Target, CameraKind::Stereo, CameraKind::Motion, CameraKind::Morph, CameraKind::Crane];

    pub fn name(self) -> &'static str {
        match self {
            CameraKind::Standard => "Camera",
            CameraKind::Target => "Target Camera",
            CameraKind::Stereo => "Stereo Camera",
            CameraKind::Motion => "Motion Camera",
            CameraKind::Morph => "Camera Morph",
            CameraKind::Crane => "Camera Crane",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            CameraKind::Standard => "camera",
            CameraKind::Target => "target_camera",
            CameraKind::Stereo => "stereo_camera",
            CameraKind::Motion => "motion_camera",
            CameraKind::Morph => "camera_morph",
            CameraKind::Crane => "camera_crane",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            CameraKind::Standard => "camera shot render view lens",
            CameraKind::Target => "camera aim look at follow",
            CameraKind::Stereo => "camera 3d anaglyph eyes vr",
            CameraKind::Motion => "camera handheld shake wobble",
            CameraKind::Morph => "camera blend transition between",
            CameraKind::Crane => "camera rig boom jib arm",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Projection {
    #[default]
    Perspective,
    /// Orthographic: no perspective; `parallel_width` is what it sees.
    Parallel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StereoMode {
    /// Red / cyan glasses.
    #[default]
    Anaglyph,
    /// Left and right images next to each other.
    SideBySide,
    LeftEye,
    RightEye,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneCamera {
    pub kind: CameraKind,
    pub projection: Projection,
    /// Lens focal length (mm) and sensor width (mm): together the field of view.
    pub focal_length: f32,
    pub sensor_width: f32,
    /// Parallel projection: the width it sees (cm).
    pub parallel_width: f32,
    /// Nothing closer than this is drawn (cm).
    pub near_clip: f32,
    /// Aims the camera at this object (any kind; a Target camera needs one).
    pub look_at: Option<ObjectId>,
    /// Depth of field: sharp at `focus_distance` (or at `focus_object`).
    pub depth_of_field: bool,
    pub focus_distance: f32,
    pub focus_object: Option<ObjectId>,
    /// Lens aperture (f-number): smaller is blurrier.
    pub f_stop: f32,
    /// Stereo: distance between the eyes (cm) and how both are shown.
    pub eye_separation: f32,
    pub stereo_mode: StereoMode,
    /// Motion: shake (cm and degrees), how fast (per second), and a seed.
    pub shake_amount: f32,
    pub shake_rotation: f32,
    pub shake_speed: f32,
    pub shake_seed: u32,
    /// Morph: from camera A to camera B (0 = A, 100 = B).
    pub morph_from: Option<ObjectId>,
    pub morph_to: Option<ObjectId>,
    pub morph_blend: f32,
    /// Crane: column height, boom length and angle, heading of the boom,
    /// and the head's tilt (cm, degrees).
    pub crane_height: f32,
    pub boom_length: f32,
    pub boom_angle: f32,
    pub boom_heading: f32,
    pub head_tilt: f32,
}

impl Default for SceneCamera {
    fn default() -> Self {
        SceneCamera {
            kind: CameraKind::Standard,
            projection: Projection::Perspective,
            focal_length: 36.0,
            sensor_width: 36.0,
            parallel_width: 1000.0,
            near_clip: 1.0,
            look_at: None,
            depth_of_field: false,
            focus_distance: 1000.0,
            focus_object: None,
            f_stop: 2.8,
            eye_separation: 6.5,
            stereo_mode: StereoMode::Anaglyph,
            shake_amount: 4.0,
            shake_rotation: 0.6,
            shake_speed: 1.2,
            shake_seed: 1,
            morph_from: None,
            morph_to: None,
            morph_blend: 50.0,
            crane_height: 150.0,
            boom_length: 400.0,
            boom_angle: 15.0,
            boom_heading: 0.0,
            head_tilt: -10.0,
        }
    }
}

const STEREO: &[&str] = &["eye_separation", "stereo_mode"];
const MOTION: &[&str] = &["shake_amount", "shake_rotation", "shake_speed", "shake_seed"];
const MORPH: &[&str] = &["morph_from", "morph_to", "morph_blend"];
const CRANE: &[&str] = &["crane_height", "boom_length", "boom_angle", "boom_heading", "head_tilt"];

impl SceneCamera {
    pub fn new(kind: CameraKind) -> Self {
        SceneCamera { kind, ..SceneCamera::default() }
    }

    pub fn sanitized(&self) -> SceneCamera {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        SceneCamera {
            focal_length: f(self.focal_length, 1.0, 5000.0, 36.0),
            sensor_width: f(self.sensor_width, 1.0, 500.0, 36.0),
            parallel_width: f(self.parallel_width, 0.01, 1e7, 1000.0),
            near_clip: f(self.near_clip, 0.001, 1e5, 1.0),
            focus_distance: f(self.focus_distance, 0.1, 1e7, 1000.0),
            f_stop: f(self.f_stop, 0.5, 64.0, 2.8),
            eye_separation: f(self.eye_separation, 0.0, 1e4, 6.5),
            shake_amount: f(self.shake_amount, 0.0, 1e4, 4.0),
            shake_rotation: f(self.shake_rotation, 0.0, 45.0, 0.6),
            shake_speed: f(self.shake_speed, 0.0, 50.0, 1.2),
            morph_blend: f(self.morph_blend, 0.0, 100.0, 50.0),
            crane_height: f(self.crane_height, 0.0, 1e5, 150.0),
            boom_length: f(self.boom_length, 0.0, 1e5, 400.0),
            boom_angle: f(self.boom_angle, -89.0, 89.0, 15.0),
            boom_heading: f(self.boom_heading, -360.0, 360.0, 0.0),
            head_tilt: f(self.head_tilt, -89.0, 89.0, -10.0),
            ..self.clone()
        }
    }

    /// Settings that mean nothing for this kind (the Attribute Manager hides them).
    pub fn hidden_fields(&self) -> Vec<&'static str> {
        let mut hidden: Vec<&'static str> = Vec::new();
        for (kind, fields) in [(CameraKind::Stereo, STEREO), (CameraKind::Motion, MOTION), (CameraKind::Morph, MORPH), (CameraKind::Crane, CRANE)] {
            if self.kind != kind {
                hidden.extend_from_slice(fields);
            }
        }
        if self.projection == Projection::Parallel {
            hidden.extend(["focal_length", "sensor_width", "depth_of_field", "focus_distance", "focus_object", "f_stop"]);
        } else {
            hidden.push("parallel_width");
        }
        if self.kind == CameraKind::Morph {
            hidden.extend(["look_at", "projection", "focal_length", "sensor_width", "parallel_width"]);
        }
        if !self.depth_of_field {
            hidden.extend(["focus_object", "f_stop"]);
        }
        hidden
    }

    /// Horizontal field of view (radians).
    pub fn fov_x(&self) -> f32 {
        2.0 * (self.sensor_width * 0.5 / self.focal_length.max(0.01)).atan()
    }

    /// The camera's outline for the viewport, in its own space (+Z = view).
    pub fn gizmo(&self) -> Mesh {
        let mut lines: Vec<(Vec3, Vec3)> = Vec::new();
        // The body: a box behind the lens.
        let (bw, bh, bl) = (16.0, 20.0, 34.0);
        let b = |x: f32, y: f32, z: f32| Vec3::new(x * bw, y * bh, z);
        let corners = [b(-1.0, -1.0, -bl), b(1.0, -1.0, -bl), b(1.0, 1.0, -bl), b(-1.0, 1.0, -bl), b(-1.0, -1.0, 0.0), b(1.0, -1.0, 0.0), b(1.0, 1.0, 0.0), b(-1.0, 1.0, 0.0)];
        for k in 0..4 {
            lines.push((corners[k], corners[(k + 1) % 4]));
            lines.push((corners[k + 4], corners[(k + 1) % 4 + 4]));
            lines.push((corners[k], corners[k + 4]));
        }
        // The view: a frustum out to 120 cm, 16:9.
        let len = 120.0;
        let (hx, hy) = match self.projection {
            Projection::Perspective => {
                let hx = len * (self.fov_x() * 0.5).tan();
                (hx, hx * 9.0 / 16.0)
            }
            Projection::Parallel => (40.0, 22.5),
        };
        let near = match self.projection {
            Projection::Perspective => [Vec3::ZERO; 4],
            Projection::Parallel => [Vec3::new(-hx, -hy, 0.0), Vec3::new(hx, -hy, 0.0), Vec3::new(hx, hy, 0.0), Vec3::new(-hx, hy, 0.0)],
        };
        let far = [Vec3::new(-hx, -hy, len), Vec3::new(hx, -hy, len), Vec3::new(hx, hy, len), Vec3::new(-hx, hy, len)];
        for k in 0..4 {
            lines.push((near[k], far[k]));
            lines.push((far[k], far[(k + 1) % 4]));
        }
        // Which way is up.
        lines.push((Vec3::new(-hx * 0.3, hy * 1.08, len), Vec3::new(0.0, hy * 1.35, len)));
        lines.push((Vec3::new(0.0, hy * 1.35, len), Vec3::new(hx * 0.3, hy * 1.08, len)));
        if self.kind == CameraKind::Stereo {
            let s = self.eye_separation.max(4.0) * 0.5;
            for x in [-s, s] {
                lines.push((Vec3::new(x, 0.0, 0.0), Vec3::new(x, 0.0, len * 0.5)));
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

/// Where a camera is and how it sees, at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraPose {
    pub eye: Vec3,
    /// Unit vectors: where it looks, and its up (orthogonal to forward).
    pub forward: Vec3,
    pub up: Vec3,
    /// Horizontal field of view (radians), for a perspective camera.
    pub fov_x: f32,
    /// `Some(width in cm)` for a parallel camera.
    pub parallel_width: Option<f32>,
    pub near: f32,
    /// Depth of field: `Some((focus distance, lens radius))`.
    pub dof: Option<(f32, f32)>,
    /// Stereo: eye separation (cm) and mode.
    pub stereo: Option<(f32, StereoMode)>,
}

impl CameraPose {
    pub fn right(&self) -> Vec3 {
        // Left-handed like the editor camera: right = up x forward.
        self.up.cross(self.forward).normalize_or_zero()
    }

    /// The camera's world matrix (+Z forward, +Y up), without scale.
    pub fn matrix(&self) -> Mat4 {
        let basis = Mat3::from_cols(self.right(), self.up, self.forward);
        Mat4::from_rotation_translation(Quat::from_mat3(&basis).normalize(), self.eye)
    }

    /// Vertical field of view for an image of this aspect (width / height).
    pub fn fov_y(&self, aspect: f32) -> f32 {
        2.0 * ((self.fov_x * 0.5).tan() / aspect.max(1e-3)).atan()
    }

    /// The pose of one eye of a stereo camera (`side` -1 left, +1 right).
    pub fn eye_offset(&self, side: f32) -> CameraPose {
        let Some((sep, _)) = self.stereo else { return *self };
        CameraPose { eye: self.eye + self.right() * sep * 0.5 * side, ..*self }
    }

    /// From an editor camera (what a view shows when no camera is used).
    pub fn from_editor(cam: &crate::camera::Camera, aspect: f32) -> CameraPose {
        let fov_x = 2.0 * ((cam.fov_y * 0.5).tan() * aspect).atan();
        let parallel_width = cam.kind.is_ortho().then_some(cam.ortho_half_height * 2.0 * aspect);
        CameraPose {
            eye: cam.eye(),
            forward: cam.forward(),
            up: cam.up(),
            fov_x,
            parallel_width,
            near: 0.5,
            dof: None,
            stereo: None,
        }
    }

    /// An editor camera that shows what this pose sees (for looking through
    /// a camera in the viewport). The editor camera has no roll.
    pub fn to_editor(&self, aspect: f32, distance: f32) -> crate::camera::Camera {
        let mut cam = crate::camera::Camera::new(crate::camera::ViewKind::Perspective);
        let back = -self.forward.normalize_or(Vec3::Z);
        cam.pitch = back.y.clamp(-1.0, 1.0).asin().clamp(-89f32.to_radians(), 89f32.to_radians());
        cam.yaw = back.x.atan2(-back.z);
        cam.distance = distance.clamp(5.0, 500_000.0);
        cam.target = self.eye + self.forward * cam.distance;
        cam.fov_y = self.fov_y(aspect).clamp(1f32.to_radians(), 170f32.to_radians());
        cam
    }
}

/// Smooth pseudo-random wobble in -1..1 (a sum of sines with seeded phases).
fn wobble(t: f32, seed: u32, channel: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ channel.wrapping_mul(0x85EB_CA6B);
    let mut v = 0.0;
    let mut norm = 0.0;
    for (k, freq) in [1.0f32, 2.31, 4.77].into_iter().enumerate() {
        h = h.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let phase = (h >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
        let amp = 1.0 / (k as f32 + 1.0);
        v += (t * freq * std::f32::consts::TAU + phase).sin() * amp;
        norm += amp;
    }
    v / norm
}

/// Aims +Z of a matrix at `target`, keeping its position (world up = +Y).
pub fn aimed(world: Mat4, target: Vec3) -> Mat4 {
    let pos = world.w_axis.truncate();
    let dir = target - pos;
    if dir.length_squared() < 1e-8 {
        return world;
    }
    let rot = crate::scene::hpb_to_quat(crate::light::aim_hpb(dir));
    let scale = Vec3::new(world.x_axis.truncate().length(), world.y_axis.truncate().length(), world.z_axis.truncate().length());
    Mat4::from_scale_rotation_translation(scale, rot, pos)
}

fn depth(scene: &Scene, id: ObjectId, time: f32, depth: u32) -> Option<CameraPose> {
    let o = scene.get(id)?;
    let ObjectKind::Camera(c) = &o.kind else { return None };
    let c = c.sanitized();
    let world = scene.world_matrix(id);
    let lens = |fwd: Vec3, eye: Vec3| -> Option<(f32, f32)> {
        if !c.depth_of_field || c.projection == Projection::Parallel {
            return None;
        }
        let focus = c.focus_object.filter(|f| scene.get(*f).is_some()).map_or(c.focus_distance, |f| (scene.world_position(f) - eye).dot(fwd).max(0.1));
        // Aperture diameter = focal length / f-number (mm -> cm).
        let radius = c.focal_length / c.f_stop * 0.05;
        Some((focus, radius))
    };
    if c.kind == CameraKind::Morph {
        if depth > 8 {
            return None;
        }
        let a = c.morph_from.filter(|a| *a != id).and_then(|a| self::depth(scene, a, time, depth + 1));
        let b = c.morph_to.filter(|b| *b != id).and_then(|b| self::depth(scene, b, time, depth + 1));
        return match (a, b) {
            (Some(a), Some(b)) => {
                let t = c.morph_blend / 100.0;
                let forward = a.forward.lerp(b.forward, t).normalize_or(a.forward);
                let up0 = a.up.lerp(b.up, t);
                let right = up0.cross(forward).normalize_or(a.right());
                let up = forward.cross(right).normalize_or(Vec3::Y);
                Some(CameraPose {
                    eye: a.eye.lerp(b.eye, t),
                    forward,
                    up,
                    fov_x: a.fov_x + (b.fov_x - a.fov_x) * t,
                    parallel_width: match (a.parallel_width, b.parallel_width) {
                        (Some(x), Some(y)) => Some(x + (y - x) * t),
                        _ => None,
                    },
                    near: a.near.min(b.near),
                    dof: if t < 0.5 { a.dof } else { b.dof },
                    stereo: None,
                })
            }
            (Some(p), None) | (None, Some(p)) => Some(p),
            // Without both cameras it stays where it is.
            (None, None) => Some(plain(&c, world, lens)),
        };
    }
    let mut m = world;
    if c.kind == CameraKind::Crane {
        // The object is the crane's base; the camera sits on the boom's end.
        let base = world;
        let heading = Quat::from_rotation_y(c.boom_heading.to_radians());
        let lift = Quat::from_rotation_x(-c.boom_angle.to_radians());
        let boom = heading * lift * Vec3::Z * c.boom_length;
        let local_eye = Vec3::Y * c.crane_height + boom;
        let head = heading * Quat::from_rotation_x(-c.head_tilt.to_radians());
        let local = Mat4::from_rotation_translation(head, local_eye);
        m = base * local;
    }
    if let Some(t) = c.look_at.filter(|t| *t != id && scene.get(*t).is_some()) {
        m = aimed(m, scene.world_position(t));
    }
    let mut pose = plain(&c, m, lens);
    if c.kind == CameraKind::Motion {
        let s = time * c.shake_speed;
        let offset = (pose.right() * wobble(s, c.shake_seed, 0) + pose.up * wobble(s, c.shake_seed, 1) + pose.forward * wobble(s, c.shake_seed, 2) * 0.5) * c.shake_amount;
        let rot = Quat::from_axis_angle(pose.up, wobble(s, c.shake_seed, 3) * c.shake_rotation.to_radians())
            * Quat::from_axis_angle(pose.right(), wobble(s, c.shake_seed, 4) * c.shake_rotation.to_radians())
            * Quat::from_axis_angle(pose.forward, wobble(s, c.shake_seed, 5) * c.shake_rotation.to_radians() * 0.5);
        pose.eye += offset;
        pose.forward = (rot * pose.forward).normalize();
        pose.up = (rot * pose.up).normalize();
    }
    Some(pose)
}

fn plain(c: &SceneCamera, m: Mat4, lens: impl Fn(Vec3, Vec3) -> Option<(f32, f32)>) -> CameraPose {
    let eye = m.w_axis.truncate();
    let forward = m.transform_vector3(Vec3::Z).normalize_or(Vec3::Z);
    let up0 = m.transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
    let right = up0.cross(forward).normalize_or(Vec3::X);
    let up = forward.cross(right).normalize_or(Vec3::Y);
    CameraPose {
        eye,
        forward,
        up,
        fov_x: c.fov_x(),
        parallel_width: (c.projection == Projection::Parallel).then_some(c.parallel_width),
        near: c.near_clip,
        dof: lens(forward, eye),
        stereo: (c.kind == CameraKind::Stereo).then_some((c.eye_separation, c.stereo_mode)),
    }
}

/// The pose of camera object `id` at `time` seconds.
pub fn pose(scene: &Scene, id: ObjectId, time: f32) -> Option<CameraPose> {
    depth(scene, id, time, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam_scene(c: SceneCamera) -> (Scene, ObjectId) {
        let mut s = Scene::new();
        let id = s.add_object("Camera", ObjectKind::Camera(c), None);
        (s, id)
    }

    #[test]
    fn standard_camera_looks_along_plus_z() {
        let (mut s, id) = cam_scene(SceneCamera::default());
        s.get_mut(id).unwrap().pos = Vec3::new(0.0, 100.0, -500.0);
        let p = pose(&s, id, 0.0).unwrap();
        assert!((p.eye - Vec3::new(0.0, 100.0, -500.0)).length() < 1e-3);
        assert!(p.forward.dot(Vec3::Z) > 0.999);
        assert!(p.up.dot(Vec3::Y) > 0.999);
        assert!((p.fov_x - 2.0 * (0.5f32).atan()).abs() < 1e-4, "36 mm on 36 mm sensor");
        let ed = p.to_editor(1.5, 500.0);
        assert!(ed.forward().dot(p.forward) > 0.999);
        assert!((ed.eye() - p.eye).length() < 0.1);
    }

    #[test]
    fn target_camera_aims_at_its_target_and_morph_blends() {
        let mut s = Scene::new();
        let t = s.add_null(None);
        s.get_mut(t).unwrap().pos = Vec3::new(300.0, 0.0, 0.0);
        let a = s.add_object("A", ObjectKind::Camera(SceneCamera { look_at: Some(t), ..SceneCamera::new(CameraKind::Target) }), None);
        let p = pose(&s, a, 0.0).unwrap();
        assert!(p.forward.dot(Vec3::X) > 0.999, "{:?}", p.forward);
        let b = s.add_object("B", ObjectKind::Camera(SceneCamera::default()), None);
        s.get_mut(b).unwrap().pos = Vec3::new(0.0, 200.0, 0.0);
        let m = s.add_object("M", ObjectKind::Camera(SceneCamera { morph_from: Some(a), morph_to: Some(b), morph_blend: 50.0, ..SceneCamera::new(CameraKind::Morph) }), None);
        let pm = pose(&s, m, 0.0).unwrap();
        assert!((pm.eye.y - 100.0).abs() < 1e-3);
        // A morph of itself does not loop forever.
        if let Some(o) = s.get_mut(m)
            && let ObjectKind::Camera(c) = &mut o.kind
        {
            c.morph_from = Some(m);
            c.morph_to = Some(m);
        }
        assert!(pose(&s, m, 0.0).is_some());
    }

    #[test]
    fn crane_motion_stereo() {
        let (s, id) = cam_scene(SceneCamera { boom_angle: 0.0, boom_heading: 0.0, head_tilt: 0.0, ..SceneCamera::new(CameraKind::Crane) });
        let p = pose(&s, id, 0.0).unwrap();
        assert!((p.eye - Vec3::new(0.0, 150.0, 400.0)).length() < 1e-2, "{:?}", p.eye);
        let (s, id) = cam_scene(SceneCamera::new(CameraKind::Motion));
        let (p0, p1) = (pose(&s, id, 0.0).unwrap(), pose(&s, id, 0.37).unwrap());
        assert!(p0.eye != p1.eye, "motion cameras shake over time");
        assert!(p0.eye.length() < 20.0);
        let (s, id) = cam_scene(SceneCamera::new(CameraKind::Stereo));
        let p = pose(&s, id, 0.0).unwrap();
        let (l, r) = (p.eye_offset(-1.0), p.eye_offset(1.0));
        assert!(((r.eye - l.eye).length() - 6.5).abs() < 1e-3);
        assert!((r.eye - l.eye).dot(p.right()) > 0.0);
    }

    #[test]
    fn hidden_fields_and_gizmo() {
        for k in CameraKind::ALL {
            let c = SceneCamera::new(k);
            assert!(!c.gizmo().vertices.is_empty());
            let hidden = c.hidden_fields();
            assert!(!hidden.contains(&"focus_distance"));
            if k != CameraKind::Crane {
                assert!(hidden.contains(&"boom_length"));
            }
        }
        let json = serde_json::to_string(&SceneCamera::default()).unwrap();
        let back: SceneCamera = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SceneCamera::default());
        let partial: SceneCamera = serde_json::from_str(r#"{"kind":"Target"}"#).unwrap();
        assert_eq!(partial.kind, CameraKind::Target);
    }
}
