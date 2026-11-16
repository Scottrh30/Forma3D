//! What the viewport tools see of the pointer, and how they draw.
//!
//! The pointer events of the view are turned into an `Input` per view (where
//! the mouse is, whether this event is a click, the start of a drag and so
//! on), like an immediate-mode UI's response. The tools then run on that
//! input, change the editor, and draw their overlay into a `Painter`, which
//! writes SVG.

use crate::keymap::Mods;
use forma3d_core::camera::{Camera, ViewKind};
use glam::{Vec2, Vec3};
use std::fmt::Write;

/// The pointer as one view sees it, in that view's pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub mouse: Option<Vec2>,
    /// The pointer is over this view (and no other view holds the pointer).
    pub hovered: bool,
    pub press_origin: Option<Vec2>,
    pub primary_down: bool,
    /// The primary button was released without dragging.
    pub clicked: bool,
    pub double_clicked: bool,
    /// The primary button moved far enough to start a drag (this event).
    pub drag_started: bool,
    /// The primary button is held and dragging.
    pub dragged: bool,
    pub mods: Mods,
    /// The camera is being moved (orbit, pan, zoom).
    pub navigating: bool,
}

/// One view: which camera, and its size on screen.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub kind: ViewKind,
    pub size: Vec2,
    pub cam: Camera,
}

impl View {
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let ndc = self.cam.project(p, self.size.x / self.size.y.max(1.0))?;
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * self.size.x,
            (1.0 - (ndc.y * 0.5 + 0.5)) * self.size.y,
        ))
    }

    pub fn ray(&self, m: Vec2) -> (Vec3, Vec3) {
        let ndc = Vec2::new(
            m.x / self.size.x.max(1.0) * 2.0 - 1.0,
            1.0 - m.y / self.size.y.max(1.0) * 2.0,
        );
        self.cam.ray(ndc, self.size.x / self.size.y.max(1.0))
    }

    pub fn world_per_pixel(&self, at: Vec3) -> f32 {
        self.cam.world_per_pixel(at, self.size.y)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    LeftTop,
    LeftBottom,
    RightTop,
    CenterCenter,
}

/// Draws the overlay as SVG elements.
#[derive(Default)]
pub struct Painter {
    pub svg: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Painter {
    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: &str) {
        let _ = write!(
            self.svg,
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="{width}" stroke-linecap="round"/>"#,
            a.x, a.y, b.x, b.y
        );
    }

    /// A polyline that breaks where a point is missing (behind the camera).
    pub fn path(&mut self, pts: &[Option<Vec2>], width: f32, color: &str) {
        let mut d = String::new();
        let mut down = false;
        for p in pts {
            match p {
                Some(p) => {
                    let _ = write!(d, "{}{:.1} {:.1}", if down { "L" } else { "M" }, p.x, p.y);
                    down = true;
                }
                None => down = false,
            }
        }
        if !d.is_empty() {
            let _ = write!(
                self.svg,
                r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linejoin="round"/>"#
            );
        }
    }

    /// Many separate segments in one element (cage wires, selected edges).
    pub fn segments(&mut self, segs: &[(Vec2, Vec2)], width: f32, color: &str) {
        if segs.is_empty() {
            return;
        }
        let mut d = String::with_capacity(segs.len() * 24);
        for (a, b) in segs {
            let _ = write!(d, "M{:.1} {:.1}L{:.1} {:.1}", a.x, a.y, b.x, b.y);
        }
        let _ = write!(
            self.svg,
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round"/>"#
        );
    }

    pub fn circle(&mut self, c: Vec2, r: f32, fill: &str) {
        let _ = write!(
            self.svg,
            r#"<circle cx="{:.1}" cy="{:.1}" r="{r}" fill="{fill}"/>"#,
            c.x, c.y
        );
    }

    /// Many dots of one size and colour in one element.
    pub fn dots(&mut self, pts: &[Vec2], r: f32, fill: &str) {
        if pts.is_empty() {
            return;
        }
        let mut d = String::with_capacity(pts.len() * 40);
        for p in pts {
            let _ = write!(
                d,
                "M{:.1} {:.1}m-{r} 0a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 -{} 0",
                p.x,
                p.y,
                r * 2.0,
                r * 2.0
            );
        }
        let _ = write!(self.svg, r#"<path d="{d}" fill="{fill}"/>"#);
    }

    pub fn ring(&mut self, c: Vec2, r: f32, width: f32, color: &str) {
        let _ = write!(
            self.svg,
            r#"<circle cx="{:.1}" cy="{:.1}" r="{r}" fill="none" stroke="{color}" stroke-width="{width}"/>"#,
            c.x, c.y
        );
    }

    pub fn square(&mut self, c: Vec2, half: f32, fill: &str) {
        let _ = write!(
            self.svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{}" height="{}" fill="{fill}"/>"#,
            c.x - half,
            c.y - half,
            half * 2.0,
            half * 2.0
        );
    }

    pub fn rect(&mut self, a: Vec2, b: Vec2, fill: &str, stroke: &str) {
        let (lo, hi) = (a.min(b), a.max(b));
        let _ = write!(
            self.svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1"/>"#,
            lo.x,
            lo.y,
            hi.x - lo.x,
            hi.y - lo.y
        );
    }

    pub fn poly(&mut self, pts: &[Vec2], fill: &str) {
        if pts.len() < 3 {
            return;
        }
        let mut d = String::new();
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, p.x, p.y);
        }
        let _ = write!(self.svg, r#"<path d="{d}Z" fill="{fill}"/>"#);
    }

    /// Filled triangles in one element.
    pub fn triangles(&mut self, tris: &[[Vec2; 3]], fill: &str) {
        if tris.is_empty() {
            return;
        }
        let mut d = String::with_capacity(tris.len() * 40);
        for [a, b, c] in tris {
            let _ = write!(
                d,
                "M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z",
                a.x, a.y, b.x, b.y, c.x, c.y
            );
        }
        let _ = write!(self.svg, r#"<path d="{d}" fill="{fill}"/>"#);
    }

    pub fn text(&mut self, p: Vec2, anchor: Anchor, text: &str, size: f32, color: &str) {
        let (ta, base, dy) = match anchor {
            Anchor::LeftTop => ("start", "hanging", 0.0),
            Anchor::LeftBottom => ("start", "alphabetic", -3.0),
            Anchor::RightTop => ("end", "hanging", 0.0),
            Anchor::CenterCenter => ("middle", "central", 0.0),
        };
        let _ = write!(
            self.svg,
            r#"<text x="{:.1}" y="{:.1}" fill="{color}" font-size="{size}" text-anchor="{ta}" dominant-baseline="{base}" paint-order="stroke" stroke="rgba(0,0,0,.45)" stroke-width="2.5">{}</text>"#,
            p.x,
            p.y + dy,
            esc(text)
        );
    }
}

pub const POINT: &str = "rgb(120,170,255)";
pub const SELECTED: &str = "rgb(255,150,40)";
pub const HOVER: &str = "rgb(255,225,110)";
pub const HANDLE: &str = "rgb(200,120,255)";
pub const HOT: &str = "#ffd34d";
pub const TEXT: &str = "#e6e8ec";
pub const DIM: &str = "#9aa0aa";
pub const ACCENT: &str = "#4c8dff";

pub fn axis_color(i: usize) -> &'static str {
    ["#e5534b", "#57ab5a", "#539bf5"][i % 3]
}
