//! Viewport cameras: a perspective orbit camera and the three orthographic
//! views (Top, Right, Front). Left-handed, Y-up, depth range 0..1 (wgpu).

use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewKind {
    Perspective,
    Top,
    Right,
    Front,
}

impl ViewKind {
    pub const ALL: [ViewKind; 4] = [ViewKind::Perspective, ViewKind::Top, ViewKind::Right, ViewKind::Front];

    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Perspective => "Perspective",
            ViewKind::Top => "Top",
            ViewKind::Right => "Right",
            ViewKind::Front => "Front",
        }
    }

    pub fn is_ortho(self) -> bool {
        self != ViewKind::Perspective
    }

    /// Position in [`ViewKind::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Forward and up vectors of an orthographic view.
    fn ortho_basis(self) -> (Vec3, Vec3) {
        match self {
            ViewKind::Top => (Vec3::NEG_Y, Vec3::Z),
            // Looks along +X, so +Z runs right-to-left on screen, as in Cinema 4D.
            ViewKind::Right => (Vec3::X, Vec3::Y),
            ViewKind::Front | ViewKind::Perspective => (Vec3::Z, Vec3::Y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub kind: ViewKind,
    pub target: Vec3,
    /// Perspective: distance from `target` to the eye.
    pub distance: f32,
    /// Perspective: heading around Y, radians. 0 puts the eye on the -Z side.
    pub yaw: f32,
    /// Perspective: elevation, radians.
    pub pitch: f32,
    pub fov_y: f32,
    /// Orthographic: half the visible world height.
    pub ortho_half_height: f32,
}

const ORTHO_STANDOFF: f32 = 20_000.0;
const MAX_PITCH: f32 = 89.0 * std::f32::consts::PI / 180.0;

impl Camera {
    pub fn new(kind: ViewKind) -> Self {
        Camera {
            kind,
            target: Vec3::new(0.0, 60.0, 0.0),
            distance: 1100.0,
            yaw: -35f32.to_radians(),
            pitch: 22f32.to_radians(),
            fov_y: 45f32.to_radians(),
            ortho_half_height: 500.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        match self.kind {
            ViewKind::Perspective => -self.eye_direction(),
            k => k.ortho_basis().0,
        }
    }

    fn eye_direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    fn up_hint(&self) -> Vec3 {
        match self.kind {
            ViewKind::Perspective => Vec3::Y,
            k => k.ortho_basis().1,
        }
    }

    pub fn eye(&self) -> Vec3 {
        match self.kind {
            ViewKind::Perspective => self.target + self.eye_direction() * self.distance,
            _ => self.target - self.forward() * ORTHO_STANDOFF,
        }
    }

    /// Camera right vector (left-handed: `up x forward`).
    pub fn right(&self) -> Vec3 {
        self.up_hint().cross(self.forward()).normalize_or_zero()
    }

    /// Camera up vector, orthogonal to forward and right.
    pub fn up(&self) -> Vec3 {
        self.forward().cross(self.right()).normalize_or_zero()
    }

    pub fn view(&self) -> Mat4 {
        view::look_at_mat4(self.eye(), self.eye() + self.forward(), self.up_hint())
    }

    fn clip_planes(&self) -> (f32, f32) {
        match self.kind {
            ViewKind::Perspective => ((self.distance * 0.01).clamp(0.5, 200.0), (self.distance * 200.0).max(20_000.0)),
            _ => (1.0, ORTHO_STANDOFF * 2.0),
        }
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        let aspect = aspect.max(1e-3);
        let (near, far) = self.clip_planes();
        match self.kind {
            ViewKind::Perspective => directx::perspective(self.fov_y, aspect, near, far),
            _ => {
                let h = self.ortho_half_height;
                directx::orthographic(-h * aspect, h * aspect, -h, h, near, far)
            }
        }
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj(aspect) * self.view()
    }

    /// World-space size of one screen pixel at `point`.
    pub fn world_per_pixel(&self, point: Vec3, viewport_height: f32) -> f32 {
        let h = viewport_height.max(1.0);
        match self.kind {
            ViewKind::Perspective => {
                let depth = (point - self.eye()).dot(self.forward()).max(1e-3);
                2.0 * depth * (self.fov_y * 0.5).tan() / h
            }
            _ => 2.0 * self.ortho_half_height / h,
        }
    }

    /// Orbit by a screen-space drag (perspective only).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if self.kind != ViewKind::Perspective {
            return;
        }
        let sensitivity = 0.35f32.to_radians();
        self.yaw -= dx * sensitivity;
        self.pitch = (self.pitch + dy * sensitivity).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Pan so the scene follows the cursor.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let wpp = self.world_per_pixel(self.target, viewport_height);
        self.target += (-self.right() * dx + self.up() * dy) * wpp;
    }

    /// Multiplicative zoom: factors below 1 zoom in.
    pub fn zoom(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        match self.kind {
            ViewKind::Perspective => self.distance = (self.distance * factor).clamp(5.0, 500_000.0),
            _ => self.ortho_half_height = (self.ortho_half_height * factor).clamp(1.0, 500_000.0),
        }
    }

    /// Re-centre on a bounding sphere so it fills the view.
    pub fn frame(&mut self, center: Vec3, radius: f32) {
        let radius = radius.max(10.0);
        self.target = center;
        match self.kind {
            ViewKind::Perspective => self.distance = (radius / (self.fov_y * 0.5).sin() * 1.15).clamp(5.0, 500_000.0),
            _ => self.ortho_half_height = (radius * 1.2).clamp(1.0, 500_000.0),
        }
    }

    /// World point -> normalised device coordinates (x, y in -1..1 with y up,
    /// z in 0..1). `None` when the point is behind the camera.
    pub fn project(&self, p: Vec3, aspect: f32) -> Option<Vec3> {
        let clip = self.view_proj(aspect) * Vec4::new(p.x, p.y, p.z, 1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        Some(Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w))
    }

    /// World ray through an NDC point: `(origin, unit direction)`.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let unproject = |z: f32| {
            let p = inv * Vec4::new(ndc.x, ndc.y, z, 1.0);
            p.truncate() / p.w
        };
        let near = unproject(0.0);
        let far = unproject(1.0);
        (near, (far - near).normalize_or_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).length() < eps
    }

    #[test]
    fn target_projects_to_screen_centre() {
        for kind in ViewKind::ALL {
            let cam = Camera::new(kind);
            let ndc = cam.project(cam.target, 1.5).unwrap();
            assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4, "{kind:?}: {ndc:?}");
            assert!((0.0..=1.0).contains(&ndc.z), "{kind:?}: depth {}", ndc.z);
        }
    }

    #[test]
    fn world_x_runs_left_to_right_in_front_and_top_views() {
        for kind in [ViewKind::Front, ViewKind::Top] {
            let cam = Camera::new(kind);
            let left = cam.project(cam.target - Vec3::X * 100.0, 1.0).unwrap();
            let right = cam.project(cam.target + Vec3::X * 100.0, 1.0).unwrap();
            assert!(right.x > left.x, "{kind:?}");
        }
    }

    #[test]
    fn up_is_up_on_screen() {
        let cam = Camera::new(ViewKind::Front);
        let low = cam.project(cam.target, 1.0).unwrap();
        let high = cam.project(cam.target + Vec3::Y * 100.0, 1.0).unwrap();
        assert!(high.y > low.y);
        // Top view: +Z is screen-up.
        let top = Camera::new(ViewKind::Top);
        let a = top.project(top.target, 1.0).unwrap();
        let b = top.project(top.target + Vec3::Z * 100.0, 1.0).unwrap();
        assert!(b.y > a.y);
    }

    #[test]
    fn right_view_puts_positive_z_on_the_left() {
        let cam = Camera::new(ViewKind::Right);
        let a = cam.project(cam.target, 1.0).unwrap();
        let b = cam.project(cam.target + Vec3::Z * 100.0, 1.0).unwrap();
        assert!(b.x < a.x);
    }

    #[test]
    fn ray_through_centre_hits_target_in_every_view() {
        for kind in ViewKind::ALL {
            let cam = Camera::new(kind);
            let (o, d) = cam.ray(Vec2::ZERO, 1.7);
            let to_target = cam.target - o;
            let along = to_target.dot(d);
            assert!(close(o + d * along, cam.target, 0.5), "{kind:?}");
        }
    }

    #[test]
    fn ray_round_trips_through_projection() {
        let cam = Camera::new(ViewKind::Perspective);
        let p = Vec3::new(120.0, 30.0, -80.0);
        let ndc = cam.project(p, 1.6).unwrap();
        let (o, d) = cam.ray(Vec2::new(ndc.x, ndc.y), 1.6);
        let t = (p - o).dot(d);
        assert!(close(o + d * t, p, 0.05));
    }

    #[test]
    fn orbit_keeps_distance_and_clamps_pitch() {
        let mut cam = Camera::new(ViewKind::Perspective);
        let d0 = (cam.eye() - cam.target).length();
        cam.orbit(200.0, -90.0);
        assert!(((cam.eye() - cam.target).length() - d0).abs() < 0.1);
        cam.orbit(0.0, 100_000.0);
        assert!(cam.pitch <= MAX_PITCH + 1e-6);
        cam.orbit(0.0, -1_000_000.0);
        assert!(cam.pitch >= -MAX_PITCH - 1e-6);
    }

    #[test]
    fn orbit_drag_right_carries_the_foreground_right() {
        // Dragging right swings the camera to the left around the target, so
        // geometry between the camera and the target slides right (and the
        // target itself stays put), like grabbing the scene with the cursor.
        let mut cam = Camera::new(ViewKind::Perspective);
        let near = cam.target - cam.forward() * 100.0;
        let near_before = cam.project(near, 1.0).unwrap();
        let target_before = cam.project(cam.target, 1.0).unwrap();
        cam.orbit(50.0, 0.0);
        let near_after = cam.project(near, 1.0).unwrap();
        let target_after = cam.project(cam.target, 1.0).unwrap();
        assert!(near_after.x > near_before.x, "foreground should move right");
        assert!((target_after - target_before).length() < 1e-4, "target must not move");
    }

    #[test]
    fn orbit_drag_down_carries_the_foreground_down() {
        let mut cam = Camera::new(ViewKind::Perspective);
        let near = cam.target - cam.forward() * 100.0;
        let before = cam.project(near, 1.0).unwrap();
        cam.orbit(0.0, 40.0);
        let after = cam.project(near, 1.0).unwrap();
        assert!(after.y < before.y, "foreground should move down");
    }

    #[test]
    fn pan_moves_scene_exactly_with_cursor() {
        let mut cam = Camera::new(ViewKind::Front);
        let p = cam.target;
        let before = cam.project(p, 1.0).unwrap();
        cam.pan(100.0, 0.0, 800.0); // drag 100 px to the right in an 800 px tall view
        let after = cam.project(p, 1.0).unwrap();
        // NDC spans 2 units over 800 px; aspect is 1 so x scales the same way.
        let expected = 100.0 / 800.0 * 2.0;
        assert!((after.x - before.x - expected).abs() < 1e-3, "{}", after.x - before.x);
        // Dragging down moves content down.
        let mut cam = Camera::new(ViewKind::Front);
        let before = cam.project(p, 1.0).unwrap();
        cam.pan(0.0, 100.0, 800.0);
        let after = cam.project(p, 1.0).unwrap();
        assert!(after.y < before.y);
    }

    #[test]
    fn world_per_pixel_matches_projection() {
        let cam = Camera::new(ViewKind::Perspective);
        let p = cam.target;
        let wpp = cam.world_per_pixel(p, 600.0);
        // A world offset along camera-right of 100*wpp should be 100 px = 100/600*2 NDC in y-units.
        let a = cam.project(p, 1.0).unwrap();
        let b = cam.project(p + cam.right() * (100.0 * wpp), 1.0).unwrap();
        assert!(((b.x - a.x) - 100.0 / 600.0 * 2.0).abs() < 5e-3, "{}", b.x - a.x);
    }

    #[test]
    fn frame_fits_sphere() {
        let mut cam = Camera::new(ViewKind::Perspective);
        cam.frame(Vec3::new(10.0, 20.0, 30.0), 200.0);
        assert!(close(cam.target, Vec3::new(10.0, 20.0, 30.0), 1e-4));
        // The sphere's silhouette must fit within the vertical field of view.
        let half_angle = (200.0f32 / cam.distance).asin();
        assert!(half_angle < cam.fov_y * 0.5);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut cam = Camera::new(ViewKind::Perspective);
        cam.zoom(1e-9);
        assert!(cam.distance >= 5.0);
        cam.zoom(1e12);
        assert!(cam.distance <= 500_000.0);
        let d = cam.distance;
        cam.zoom(f32::NAN);
        cam.zoom(-1.0);
        assert_eq!(cam.distance, d);
    }

    #[test]
    fn behind_camera_does_not_project() {
        let cam = Camera::new(ViewKind::Perspective);
        let behind = cam.eye() - cam.forward() * 50.0;
        assert!(cam.project(behind, 1.0).is_none());
    }
}
