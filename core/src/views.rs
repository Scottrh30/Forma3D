//! The view panels: how they are laid out, each panel's own options and
//! background image, the view history behind Undo View, and the copy of it
//! all that is saved with the scene.

use serde::{Deserialize, Serialize};

use crate::camera::{Camera, ViewKind};
use crate::renderer::{DisplayMode, ViewEffects};

/// How the viewport is split into panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewLayout {
    /// One panel (the active one).
    Single,
    /// Two panels side by side.
    TwoSide,
    /// Two panels, one above the other.
    TwoStack,
    /// One large panel on the left, two on the right.
    ThreeLeft,
    /// One large panel at the top, two below.
    ThreeTop,
    /// Four panels.
    Quad,
}

impl ViewLayout {
    pub const ALL: [ViewLayout; 6] = [
        ViewLayout::Single,
        ViewLayout::TwoSide,
        ViewLayout::TwoStack,
        ViewLayout::ThreeLeft,
        ViewLayout::ThreeTop,
        ViewLayout::Quad,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewLayout::Single => "1 View",
            ViewLayout::TwoSide => "2 Views Side by Side",
            ViewLayout::TwoStack => "2 Views Stacked",
            ViewLayout::ThreeLeft => "3 Views, Large Left",
            ViewLayout::ThreeTop => "3 Views, Large Top",
            ViewLayout::Quad => "4 Views",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ViewLayout::Single => "single",
            ViewLayout::TwoSide => "two_side",
            ViewLayout::TwoStack => "two_stack",
            ViewLayout::ThreeLeft => "three_left",
            ViewLayout::ThreeTop => "three_top",
            ViewLayout::Quad => "quad",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ViewLayout::Single => "single",
            ViewLayout::TwoSide => "layout-2h",
            ViewLayout::TwoStack => "layout-2v",
            ViewLayout::ThreeLeft => "layout-3l",
            ViewLayout::ThreeTop => "layout-3t",
            ViewLayout::Quad => "quad",
        }
    }

    /// The panels shown, largest first for the three-view layouts.
    /// `single` is the panel a one-view layout shows.
    pub fn panels(self, single: ViewKind) -> Vec<ViewKind> {
        use ViewKind::*;
        match self {
            ViewLayout::Single => vec![single],
            ViewLayout::TwoSide | ViewLayout::TwoStack => vec![Perspective, Front],
            ViewLayout::ThreeLeft | ViewLayout::ThreeTop => vec![Perspective, Top, Front],
            ViewLayout::Quad => vec![Top, Perspective, Right, Front],
        }
    }

    /// Where each panel of [`ViewLayout::panels`] goes, as fractions of the
    /// viewport: (x, y, width, height).
    pub fn cells(self) -> Vec<[f32; 4]> {
        match self {
            ViewLayout::Single => vec![[0.0, 0.0, 1.0, 1.0]],
            ViewLayout::TwoSide => vec![[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 1.0]],
            ViewLayout::TwoStack => vec![[0.0, 0.0, 1.0, 0.5], [0.0, 0.5, 1.0, 0.5]],
            ViewLayout::ThreeLeft => vec![[0.0, 0.0, 0.62, 1.0], [0.62, 0.0, 0.38, 0.5], [0.62, 0.5, 0.38, 0.5]],
            ViewLayout::ThreeTop => vec![[0.0, 0.0, 1.0, 0.62], [0.0, 0.62, 0.5, 0.38], [0.5, 0.62, 0.5, 0.38]],
            ViewLayout::Quad => vec![
                [0.0, 0.0, 0.5, 0.5],
                [0.5, 0.0, 0.5, 0.5],
                [0.0, 0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5, 0.5],
            ],
        }
    }
}

/// How a background image sits in its panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackgroundFit {
    /// The whole image, centred (it may leave bars).
    #[default]
    Fit,
    /// Fills the panel (it may be cropped).
    Fill,
    /// Stretched to the panel.
    Stretch,
    /// Fixed in the scene: a reference image `size` wide at `offset`, that
    /// moves and zooms with the view (parallel views only).
    Scene,
}

impl BackgroundFit {
    pub const ALL: [BackgroundFit; 4] = [BackgroundFit::Fit, BackgroundFit::Fill, BackgroundFit::Stretch, BackgroundFit::Scene];

    pub fn label(self) -> &'static str {
        match self {
            BackgroundFit::Fit => "Fit",
            BackgroundFit::Fill => "Fill",
            BackgroundFit::Stretch => "Stretch",
            BackgroundFit::Scene => "In Scene",
        }
    }
}

/// A panel's background (reference) image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Background {
    /// The file it came from (for display).
    pub name: String,
    /// The PNG or JPEG file, base64.
    pub data: String,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub fit: BackgroundFit,
    /// In Scene: width in scene units (cm).
    #[serde(default = "default_size")]
    pub size: f32,
    /// In Scene: the image centre's offset along the view's right and up.
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default = "yes")]
    pub show: bool,
}

fn default_opacity() -> f32 {
    0.8
}
fn default_size() -> f32 {
    1000.0
}
fn yes() -> bool {
    true
}

impl Background {
    pub fn from_file(name: &str, bytes: &[u8]) -> Result<Background, String> {
        crate::texture::Image::decode(bytes)?;
        use base64::Engine as _;
        Ok(Background {
            name: name.to_owned(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            opacity: default_opacity(),
            fit: BackgroundFit::Fit,
            size: default_size(),
            offset: [0.0, 0.0],
            show: true,
        })
    }

    pub fn image(&self) -> Result<crate::texture::Image, String> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(self.data.as_bytes()).map_err(|e| e.to_string())?;
        crate::texture::Image::decode(&bytes)
    }

    /// Identifies the picture (not its placement), to cache the decoded image.
    pub fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.data.hash(&mut h);
        h.finish()
    }

    pub fn sanitized(&self) -> Background {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        Background {
            opacity: f(self.opacity, 0.0, 1.0, default_opacity()),
            size: f(self.size, 0.01, 1e7, default_size()),
            offset: [f(self.offset[0], -1e7, 1e7, 0.0), f(self.offset[1], -1e7, 1e7, 0.0)],
            ..self.clone()
        }
    }

    /// Where the image goes in a panel `aspect` wide (width / height), for
    /// an image `image_aspect` wide: (x0, y0, x1, y1) in clip space (y up).
    pub fn placement(&self, cam: &Camera, aspect: f32, image_aspect: f32) -> [f32; 4] {
        let image_aspect = image_aspect.max(1e-3);
        let aspect = aspect.max(1e-3);
        let (hw, hh) = match self.fit {
            BackgroundFit::Stretch => (1.0, 1.0),
            BackgroundFit::Fit | BackgroundFit::Fill => {
                let wider = image_aspect > aspect;
                let fit_width = if self.fit == BackgroundFit::Fit { wider } else { !wider };
                if fit_width {
                    (1.0, aspect / image_aspect)
                } else {
                    (image_aspect / aspect, 1.0)
                }
            }
            BackgroundFit::Scene if cam.kind.is_ortho() => {
                // Scene units per clip unit, then the image's half size and centre.
                let per_clip_y = cam.ortho_half_height;
                let per_clip_x = cam.ortho_half_height * aspect;
                let half_w = self.size * 0.5;
                let half_h = half_w / image_aspect;
                // The world origin's position on screen, plus the offset.
                let o = glam::Vec3::ZERO - cam.target;
                let cx = (o.dot(cam.right()) + self.offset[0]) / per_clip_x;
                let cy = (o.dot(cam.up()) + self.offset[1]) / per_clip_y;
                let (hx, hy) = (half_w / per_clip_x, half_h / per_clip_y);
                return [cx - hx, cy - hy, cx + hx, cy + hy];
            }
            // A perspective view can't hold a flat image in the scene: fit it.
            BackgroundFit::Scene => {
                return Background { fit: BackgroundFit::Fit, ..self.clone() }.placement(cam, aspect, image_aspect);
            }
        };
        [-hw, -hh, hw, hh]
    }
}

/// A panel's own settings: shading, render options, guides and background.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanelOptions {
    pub display: DisplayMode,
    pub fx: ViewEffects,
    #[serde(default = "yes")]
    pub grid: bool,
    #[serde(default = "yes")]
    pub compass: bool,
    /// The render frame and title / action safe areas.
    #[serde(default)]
    pub safe_frames: bool,
    #[serde(default)]
    pub background: Option<Background>,
}

impl Default for PanelOptions {
    fn default() -> Self {
        PanelOptions {
            display: DisplayMode::GouraudLines,
            fx: ViewEffects::default(),
            grid: true,
            compass: true,
            safe_frames: false,
            background: None,
        }
    }
}

/// How far back Undo View goes, per panel.
pub const VIEW_HISTORY: usize = 64;

/// A panel's earlier cameras (Undo View) and the ones undone (Redo View).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewHistory {
    undo: Vec<Camera>,
    redo: Vec<Camera>,
}

impl ViewHistory {
    /// Remembers `current` before the view changes. Repeats are skipped.
    pub fn record(&mut self, current: Camera) {
        if self.undo.last() == Some(&current) {
            return;
        }
        self.undo.push(current);
        if self.undo.len() > VIEW_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The camera before `current`, if any; `current` becomes redoable.
    pub fn undo(&mut self, current: Camera) -> Option<Camera> {
        while let Some(c) = self.undo.pop() {
            if c != current {
                self.redo.push(current);
                return Some(c);
            }
        }
        None
    }

    pub fn redo(&mut self, current: Camera) -> Option<Camera> {
        let c = self.redo.pop()?;
        self.undo.push(current);
        Some(c)
    }

    pub fn can_undo(&self, current: &Camera) -> bool {
        self.undo.iter().any(|c| c != current)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// The views as saved in a scene file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedViews {
    pub layout: ViewLayout,
    pub single_view: ViewKind,
    #[serde(default = "perspective")]
    pub active_view: ViewKind,
    /// One per panel, in [`ViewKind::ALL`] order.
    pub cameras: Vec<Camera>,
    pub panels: Vec<PanelOptions>,
}

fn perspective() -> ViewKind {
    ViewKind::Perspective
}

impl SavedViews {
    /// Cleans numbers and fills in missing panels, so a damaged or older
    /// file still opens.
    pub fn sanitized(mut self) -> SavedViews {
        let fresh = ViewKind::ALL.map(Camera::new);
        self.cameras.truncate(4);
        while self.cameras.len() < 4 {
            self.cameras.push(fresh[self.cameras.len()]);
        }
        for (c, d) in self.cameras.iter_mut().zip(fresh) {
            let ok = c.target.is_finite() && c.distance.is_finite() && c.yaw.is_finite() && c.pitch.is_finite() && c.fov_y.is_finite() && c.ortho_half_height.is_finite();
            if !ok {
                *c = Camera { kind: c.kind, ..d };
            }
            c.distance = c.distance.clamp(5.0, 500_000.0);
            c.ortho_half_height = c.ortho_half_height.clamp(1.0, 500_000.0);
            c.fov_y = c.fov_y.clamp(1f32.to_radians(), 170f32.to_radians());
            c.pitch = c.pitch.clamp(-89f32.to_radians(), 89f32.to_radians());
        }
        self.panels.truncate(4);
        while self.panels.len() < 4 {
            self.panels.push(PanelOptions::default());
        }
        for p in &mut self.panels {
            p.background = p.background.as_ref().map(Background::sanitized);
        }
        if !ViewKind::ALL.contains(&self.single_view) {
            self.single_view = ViewKind::Perspective;
        }
        if !ViewKind::ALL.contains(&self.active_view) {
            self.active_view = self.single_view;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_has_a_cell_per_panel_and_covers_the_viewport() {
        for l in ViewLayout::ALL {
            let panels = l.panels(ViewKind::Top);
            let cells = l.cells();
            assert_eq!(panels.len(), cells.len(), "{l:?}");
            let area: f32 = cells.iter().map(|c| c[2] * c[3]).sum();
            assert!((area - 1.0).abs() < 1e-4, "{l:?} covers {area}");
            let mut seen = std::collections::HashSet::new();
            assert!(panels.iter().all(|p| seen.insert(*p)), "{l:?} repeats a panel");
        }
        assert_eq!(ViewLayout::Single.panels(ViewKind::Right), vec![ViewKind::Right]);
    }

    #[test]
    fn view_history_undoes_and_redoes() {
        let mut h = ViewHistory::default();
        let a = Camera::new(ViewKind::Perspective);
        let mut b = a;
        b.yaw += 0.5;
        let mut c = b;
        c.distance *= 2.0;
        h.record(a);
        h.record(a); // repeats are skipped
        h.record(b);
        assert_eq!(h.undo(c), Some(b));
        assert_eq!(h.undo(b), Some(a));
        assert_eq!(h.undo(a), None);
        assert_eq!(h.redo(a), Some(b));
        assert_eq!(h.redo(b), Some(c));
        assert!(!h.can_redo());
        // A new change after undoing drops the redo steps.
        h.undo(c);
        h.record(b);
        assert!(!h.can_redo());
    }

    #[test]
    fn view_history_is_bounded() {
        let mut h = ViewHistory::default();
        let mut c = Camera::new(ViewKind::Perspective);
        for i in 0..(VIEW_HISTORY * 2) {
            c.yaw = i as f32 * 0.01;
            h.record(c);
        }
        assert_eq!(h.undo.len(), VIEW_HISTORY);
    }

    #[test]
    fn backgrounds_fit_fill_and_sit_in_the_scene() {
        let bg = Background { name: "a".into(), data: String::new(), opacity: 1.0, fit: BackgroundFit::Fit, size: 200.0, offset: [0.0, 0.0], show: true };
        let cam = Camera::new(ViewKind::Front);
        // A wide image in a square panel: full width, letterboxed.
        let r = bg.placement(&cam, 1.0, 2.0);
        assert_eq!(r, [-1.0, -0.5, 1.0, 0.5]);
        let fill = Background { fit: BackgroundFit::Fill, ..bg.clone() }.placement(&cam, 1.0, 2.0);
        assert_eq!(fill, [-2.0, -1.0, 2.0, 1.0]);
        // In Scene: 200 units wide in a view 500 units half-high.
        let mut cam = cam;
        cam.target = glam::Vec3::ZERO;
        let s = Background { fit: BackgroundFit::Scene, ..bg.clone() }.placement(&cam, 1.0, 2.0);
        assert!((s[2] - s[0] - 200.0 / 500.0).abs() < 1e-5, "{s:?}");
        // Panning the view the other way moves the image.
        cam.target.x = 100.0;
        let moved = Background { fit: BackgroundFit::Scene, ..bg }.placement(&cam, 1.0, 2.0);
        assert!(moved[0] < s[0]);
    }

    #[test]
    fn saved_views_round_trip_and_repair() {
        let v = SavedViews {
            layout: ViewLayout::ThreeLeft,
            single_view: ViewKind::Front,
            active_view: ViewKind::Top,
            cameras: vec![Camera::new(ViewKind::Isometric)],
            panels: vec![PanelOptions { safe_frames: true, ..Default::default() }],
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: SavedViews = serde_json::from_str(&json).unwrap();
        let fixed = back.sanitized();
        assert_eq!(fixed.cameras.len(), 4);
        assert_eq!(fixed.panels.len(), 4);
        assert_eq!(fixed.cameras[0].kind, ViewKind::Isometric);
        assert!(fixed.panels[0].safe_frames);
        let mut bad = fixed.clone();
        bad.cameras[1].distance = f32::NAN;
        assert!(bad.sanitized().cameras[1].distance.is_finite());
    }
}
