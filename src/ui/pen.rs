//! Viewport input and overlay for the Spline Pen and Polygon Pen.
//!
//! Clicks land on the drawing plane: the floor (XZ) in the Perspective and
//! Top views, the XY plane in Front and the ZY plane in Right, through the
//! origin, as in Cinema 4D. The Polygon Pen also snaps to the target mesh's
//! points and lands on its surface when the click hits it.

use super::viewport::{p2, ray_at, to_screen};
use crate::camera::{Camera, ViewKind};
use crate::editor::Editor;
use crate::gizmo::ray_plane;
use crate::pen::{PenDrag, PenKind, PenVertex};
use crate::picking::ray_triangle;
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, PointerButton, Rect, Response, Shape, Stroke, Ui, vec2};
use glam::{Mat4, Vec2, Vec3};

const POINT: Color32 = Color32::from_rgb(120, 170, 255);
const ACTIVE: Color32 = Color32::from_rgb(255, 150, 40);
const HOVER: Color32 = Color32::from_rgb(255, 225, 110);
const HANDLE: Color32 = Color32::from_rgb(200, 120, 255);
const SNAP_PX: f32 = 10.0;

/// The normal of the plane new points are drawn on in this view: the
/// workplane in the Perspective view, the view's own plane otherwise.
fn drawing_plane(kind: ViewKind, wp: &crate::scene::Workplane) -> Vec3 {
    match kind {
        ViewKind::Front => Vec3::Z,
        ViewKind::Right => Vec3::X,
        ViewKind::Top => Vec3::Y,
        ViewKind::Perspective => wp.normal(),
    }
}

/// Where the mouse ray meets the plane through `through` with the given
/// normal. Falls back to a plane facing the camera when the ray runs along it.
fn on_plane(cam: &Camera, rect: Rect, normal: Vec3, m: Vec2, through: Vec3) -> Vec3 {
    let (ro, rd) = ray_at(cam, rect, m);
    if rd.normalize_or_zero().dot(normal).abs() > 0.05
        && let Some(t) = ray_plane(ro, rd, through, normal)
        && t > 0.0
    {
        return ro + rd * t;
    }
    let facing = -cam.forward();
    ray_plane(ro, rd, through, facing).map_or(through, |t| ro + rd * t)
}

fn square(painter: &egui::Painter, at: egui::Pos2, half: f32, color: Color32) {
    painter.rect_filled(Rect::from_center_size(at, vec2(half * 2.0, half * 2.0)), 0.0, color);
}

fn hint(painter: &egui::Painter, m: Vec2, text: &str) {
    painter.text(p2(m) + vec2(14.0, -14.0), Align2::LEFT_BOTTOM, text, FontId::proportional(12.0), theme::TEXT_DIM);
}

#[allow(clippy::too_many_arguments)]
pub fn interact(
    ui: &mut Ui,
    ed: &mut Editor,
    kind: ViewKind,
    rect: Rect,
    resp: &Response,
    cam: &Camera,
    mods: Modifiers,
    navigating: bool,
    snap: &super::viewport::SnapCtx,
) {
    let Some(pen_kind) = ed.pen.as_ref().map(|p| p.kind) else { return };
    let mouse = resp.hover_pos().or(resp.interact_pointer_pos()).map(|p| Vec2::new(p.x, p.y));
    let busy = navigating || mods.alt;
    // Snapping: clicks land on what is near the cursor (but never on the
    // object being drawn, whose own points the pen handles itself).
    let target = match pen_kind {
        PenKind::Spline => ed.pen_spline().map(|(id, _, _)| id),
        PenKind::Polygon => ed.pen_mesh().map(|(id, _, _)| id),
    };
    let sources = super::viewport::snap_sources(snap.objects, |id| Some(id) != target, None);
    let settings = ed.snap;
    let scene_wp = ed.scene.workplane;
    let project = |p: Vec3| to_screen(cam, rect, p);
    let snapper = |m: Vec2| -> Option<crate::snap::SnapHit> {
        if !settings.enabled {
            return None;
        }
        let ray = ray_at(cam, rect, m);
        let q = crate::snap::Query {
            project: &project,
            mouse: m,
            ray,
            ortho_forward: kind.is_ortho().then(|| cam.forward()),
            workplane: scene_wp.matrix(),
            grid_unit: snap.grid_unit,
            start: scene_wp.pos,
            extra_points: &[],
        };
        crate::snap::find(&settings, &sources, &q)
    };
    match pen_kind {
        PenKind::Spline => spline_pen(ui, ed, kind, rect, resp, cam, mouse, busy, mods, &snapper),
        PenKind::Polygon => polygon_pen(ui, ed, kind, rect, resp, cam, mouse, busy, &snapper),
    }
    if !busy && let Some(m) = mouse {
        super::viewport::draw_snap(&ui.painter_at(rect), snapper(m), &project);
    }
    // The tool's name in the corner, so it's clear clicks draw instead of select.
    ui.painter_at(rect).text(
        rect.right_top() + vec2(-10.0, 30.0),
        Align2::RIGHT_TOP,
        format!("{}  (Esc to finish)", pen_kind.label()),
        FontId::proportional(12.0),
        HOVER,
    );
}

/// What is under the mouse for the Spline Pen.
#[derive(Clone, Copy)]
enum SplineHover {
    Handle { index: usize, out: bool },
    Point(usize),
    Segment { segment: usize, t: f32 },
}

#[allow(clippy::too_many_arguments)]
fn spline_pen(
    ui: &mut Ui,
    ed: &mut Editor,
    kind: ViewKind,
    rect: Rect,
    resp: &Response,
    cam: &Camera,
    mouse: Option<Vec2>,
    busy: bool,
    mods: Modifiers,
    snapper: &dyn Fn(Vec2) -> Option<crate::snap::SnapHit>,
) {
    let painter = ui.painter_at(rect);
    let project = |p: Vec3| to_screen(cam, rect, p);
    let normal = drawing_plane(kind, &ed.scene.workplane);
    let place = |m: Vec2, through: Vec3| snapper(m).map_or_else(|| on_plane(cam, rect, normal, m, through), |h| h.pos);

    // What's under the mouse (handles first, then points, then the curve).
    let hover = match (ed.pen_spline(), mouse) {
        (Some((_, s, world)), Some(m)) if !busy && ed.pen.as_ref().is_some_and(|p| p.drag.is_none()) => {
            let near = |p: Vec3| project(world.transform_point3(p)).map(|q| q.distance(m)).filter(|d| *d < SNAP_PX);
            let mut found = None;
            for (i, p) in s.points.iter().enumerate() {
                for (out, tan) in [(true, p.tan_out), (false, p.tan_in)] {
                    if tan != glam::Vec3::ZERO && near(p.pos + tan).is_some() {
                        found = Some(SplineHover::Handle { index: i, out });
                    }
                }
            }
            if found.is_none() {
                found = s.points.iter().position(|p| near(p.pos).is_some()).map(SplineHover::Point);
            }
            if found.is_none()
                && let Some((segment, t, d)) = s.nearest(|p| project(world.transform_point3(p)).map(|q| q.distance(m)))
                && d < 6.0
            {
                found = Some(SplineHover::Segment { segment, t });
            }
            found
        }
        _ => None,
    };
    let anchor = ed.pen_spline().map(|(_, _, w)| w.w_axis.truncate()).unwrap_or(if kind == ViewKind::Perspective { ed.scene.workplane.pos } else { Vec3::ZERO });

    // ---- clicks and drags --------------------------------------------------
    if !busy {
        if resp.clicked_by(PointerButton::Primary)
            && let Some(m) = mouse
        {
            match hover {
                Some(SplineHover::Point(0)) if ed.pen_spline().is_some_and(|(_, s, _)| s.points.len() >= 3 && !s.closed) => {
                    ed.pen_close_spline();
                }
                Some(SplineHover::Point(_)) | Some(SplineHover::Handle { .. }) => {}
                Some(SplineHover::Segment { segment, t }) => {
                    ed.pen_insert_spline_point(segment, t);
                }
                None => {
                    let p = place(m, anchor);
                    ed.pen_add_spline_point(p);
                }
            }
            ed.active_view = kind;
        }
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(start) = ui.input(|i| i.pointer.press_origin()).map(|p| Vec2::new(p.x, p.y))
        {
            let drag = match hover {
                Some(SplineHover::Handle { index, out }) => Some(PenDrag::Tangent { index, out }),
                Some(SplineHover::Point(i)) => Some(PenDrag::MovePoint(i)),
                Some(SplineHover::Segment { segment, t }) => ed.pen_insert_spline_point(segment, t).map(PenDrag::MovePoint),
                None => {
                    let p = place(start, anchor);
                    Some(PenDrag::NewPoint(ed.pen_add_spline_point(p)))
                }
            };
            if let Some(pen) = ed.pen.as_mut() {
                pen.drag = drag;
            }
            ed.active_view = kind;
        }
        let drag = ed.pen.as_ref().and_then(|p| p.drag);
        if let (Some(drag), Some(m), Some((_, s, world))) = (drag, mouse, ed.pen_spline()) {
            let point_of = |i: usize| s.points.get(i).map(|p| world.transform_point3(p.pos));
            if resp.dragged_by(PointerButton::Primary) {
                match drag {
                    PenDrag::NewPoint(i) => {
                        if let Some(at) = point_of(i) {
                            let p = on_plane(cam, rect, normal, m, at);
                            ed.pen_pull_tangents(i, p);
                        }
                    }
                    PenDrag::MovePoint(i) => {
                        if let Some(at) = point_of(i) {
                            let p = place(m, at);
                            ed.pen_move_spline_point(i, p);
                        }
                    }
                    PenDrag::Tangent { index, out } => {
                        if let Some(at) = point_of(index) {
                            let p = on_plane(cam, rect, normal, m, at);
                            ed.pen_move_tangent(index, out, p, mods.shift);
                        }
                    }
                }
            }
        }
        if !ui.input(|i| i.pointer.primary_down())
            && let Some(pen) = ed.pen.as_mut()
        {
            pen.drag = None;
        }
    }

    // Keys: Backspace removes the last point, Enter finishes.
    if ed.active_view == kind && !ui.ctx().egui_wants_keyboard_input() {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace)) {
            ed.pen_remove_last_spline_point();
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            ed.stop_pen();
            ed.status = "Spline Pen finished".to_owned();
            return;
        }
    }

    // ---- overlay ------------------------------------------------------------
    if let Some((_, s, world)) = ed.pen_spline() {
        let w = |p: Vec3| project(world.transform_point3(p)).map(p2);
        for (i, p) in s.points.iter().enumerate() {
            let Some(at) = w(p.pos) else { continue };
            for (out, tan) in [(false, p.tan_in), (true, p.tan_out)] {
                if tan != Vec3::ZERO
                    && let Some(h) = w(p.pos + tan)
                {
                    painter.line_segment([at, h], Stroke::new(1.0, HANDLE));
                    let hot = matches!(hover, Some(SplineHover::Handle { index, out: o }) if index == i && o == out);
                    painter.circle_filled(h, if hot { 5.0 } else { 3.5 }, if hot { HOVER } else { HANDLE });
                }
            }
            let hot = matches!(hover, Some(SplineHover::Point(j)) if j == i);
            let color = if hot {
                HOVER
            } else if i + 1 == s.points.len() {
                ACTIVE
            } else {
                POINT
            };
            square(&painter, at, if hot { 4.5 } else { 3.5 }, color);
        }
        // Rubber band from the last point to the mouse.
        if let (Some(last), Some(m), None) = (s.points.last(), mouse, hover)
            && !s.closed
            && !busy
            && ed.pen.as_ref().is_some_and(|p| p.drag.is_none())
            && let Some(a) = w(last.pos)
        {
            painter.line_segment([a, p2(m)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 225, 110, 140)));
        }
        if let (Some(SplineHover::Segment { segment, t }), Some(m)) = (hover, mouse) {
            if let Some(q) = w(s.eval(segment, t)) {
                painter.circle_stroke(q, 5.0, Stroke::new(1.5, HOVER));
            }
            hint(&painter, m, "click to insert a point");
        } else if let (Some(SplineHover::Point(0)), Some(m)) = (hover, mouse) {
            if s.points.len() >= 3 && !s.closed {
                hint(&painter, m, "click to close the spline");
            }
        } else if let (Some(m), 0) = (mouse, s.points.len()) {
            hint(&painter, m, "click to start a spline");
        }
    } else if let Some(m) = mouse {
        hint(&painter, m, "click to start a spline (drag for a curve)");
    }
}

/// The target mesh's points in world space, with their indices.
fn mesh_points(ed: &Editor) -> Option<(Vec<Vec3>, Mat4)> {
    let (_, mesh, world) = ed.pen_mesh()?;
    Some((mesh.points.iter().map(|p| world.transform_point3(*p)).collect(), world))
}

#[allow(clippy::too_many_arguments)]
fn polygon_pen(
    ui: &mut Ui,
    ed: &mut Editor,
    kind: ViewKind,
    rect: Rect,
    resp: &Response,
    cam: &Camera,
    mouse: Option<Vec2>,
    busy: bool,
    snapper: &dyn Fn(Vec2) -> Option<crate::snap::SnapHit>,
) {
    let painter = ui.painter_at(rect);
    let project = |p: Vec3| to_screen(cam, rect, p);
    let points = mesh_points(ed);
    let world = points.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
    let pending = ed.pen.as_ref().map(|p| p.pending.clone()).unwrap_or_default();
    let corner_world = |v: &PenVertex| match *v {
        PenVertex::Existing(i) => points.as_ref().and_then(|(p, _)| p.get(i as usize).copied()),
        PenVertex::New(local) => Some(world.transform_point3(local)),
    };

    // Snap: the first pending corner (to close), then the mesh's points.
    let closes = mouse.is_some_and(|m| {
        pending.len() >= 3
            && pending.first().and_then(corner_world).and_then(project).is_some_and(|q| q.distance(m) < SNAP_PX)
    });
    let snap = mouse.filter(|_| !closes).and_then(|m| {
        let (pts, _) = points.as_ref()?;
        pts.iter()
            .enumerate()
            .filter_map(|(i, p)| Some((i, project(*p)?.distance(m))))
            .filter(|(_, d)| *d < SNAP_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i as u32)
    });

    // Where a free click lands: on the target's surface if it hits, else the plane.
    let normal = drawing_plane(kind, &ed.scene.workplane);
    let plane_point = if kind == ViewKind::Perspective { ed.scene.workplane.pos } else { Vec3::ZERO };
    let landing = |m: Vec2| -> Vec3 {
        if let Some(hit) = snapper(m) {
            return hit.pos;
        }
        let (ro, rd) = ray_at(cam, rect, m);
        if let Some((_, mesh, w)) = ed.pen_mesh() {
            let mut best: Option<f32> = None;
            for i in 0..mesh.polys.len() {
                for [a, b, c] in mesh.triangulate(i) {
                    let p = |k: usize| w.transform_point3(mesh.points[mesh.polys[i][k] as usize]);
                    if let Some(t) = ray_triangle(ro, rd, p(a), p(b), p(c))
                        && best.is_none_or(|bt| t < bt)
                    {
                        best = Some(t);
                    }
                }
            }
            if let Some(t) = best {
                return ro + rd * t;
            }
        }
        on_plane(cam, rect, normal, m, plane_point)
    };

    let toward_viewer = |at: Vec3| if kind.is_ortho() { -cam.forward() } else { cam.eye() - at };
    let clicked = !busy && resp.clicked_by(PointerButton::Primary);
    // Work out the landing point (a ray cast) only for a free click.
    let landed = if clicked && !closes && snap.is_none() { mouse.map(landing) } else { None };
    let centre = pending.iter().filter_map(corner_world).fold(Vec3::ZERO, |a, b| a + b) / pending.len().max(1) as f32;
    if clicked && mouse.is_some() {
        ed.active_view = kind;
        if closes {
            ed.pen_commit_polygon(toward_viewer(centre));
        } else if let Some(i) = snap {
            ed.pen_add_corner(PenVertex::Existing(i));
        } else if let Some(p) = landed {
            ed.pen_add_corner(PenVertex::New(p));
        }
    }
    if ed.active_view == kind && !ui.ctx().egui_wants_keyboard_input() {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            let pending = ed.pen.as_ref().map(|p| p.pending.clone()).unwrap_or_default();
            if pending.is_empty() {
                ed.stop_pen();
                ed.status = "Polygon Pen finished".to_owned();
                return;
            }
            let pts = mesh_points(ed);
            let w = pts.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
            let centre = pending
                .iter()
                .filter_map(|v| match *v {
                    PenVertex::Existing(i) => pts.as_ref().and_then(|(p, _)| p.get(i as usize).copied()),
                    PenVertex::New(l) => Some(w.transform_point3(l)),
                })
                .fold(Vec3::ZERO, |a, b| a + b)
                / pending.len().max(1) as f32;
            ed.pen_commit_polygon(toward_viewer(centre));
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace))
            && let Some(p) = ed.pen.as_mut()
        {
            p.pending.pop();
        }
    }

    // ---- overlay ------------------------------------------------------------
    // The target's points (what you can snap to), when there aren't too many.
    let points = mesh_points(ed);
    if let Some((pts, _)) = &points
        && pts.len() <= 20_000
    {
        for p in pts {
            if let Some(q) = project(*p) {
                painter.circle_filled(p2(q), 2.0, POINT.gamma_multiply(0.8));
            }
        }
    }
    let world = points.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
    let pending = ed.pen.as_ref().map(|p| p.pending.clone()).unwrap_or_default();
    let screen: Vec<egui::Pos2> = pending
        .iter()
        .filter_map(|v| match *v {
            PenVertex::Existing(i) => points.as_ref().and_then(|(p, _)| p.get(i as usize).copied()),
            PenVertex::New(l) => Some(world.transform_point3(l)),
        })
        .filter_map(|p| project(p).map(p2))
        .collect();
    if screen.len() >= 3 {
        painter.add(Shape::convex_polygon(screen.clone(), Color32::from_rgba_unmultiplied(255, 150, 40, 40), Stroke::NONE));
    }
    for w in screen.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(2.0, ACTIVE));
    }
    if let (Some(last), Some(m)) = (screen.last(), mouse)
        && !busy
    {
        painter.line_segment([*last, p2(m)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 225, 110, 160)));
        if screen.len() >= 2 {
            painter.line_segment([p2(m), screen[0]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 225, 110, 70)));
        }
    }
    for (i, q) in screen.iter().enumerate() {
        let first_hot = i == 0 && closes;
        square(&painter, *q, if first_hot { 5.0 } else { 3.5 }, if first_hot { HOVER } else { ACTIVE });
    }
    if let (Some(i), Some((pts, _))) = (snap, &points)
        && let Some(q) = pts.get(i as usize).and_then(|p| project(*p))
    {
        painter.circle_stroke(p2(q), 6.0, Stroke::new(1.5, HOVER));
    }
    if let Some(m) = mouse {
        let text = if closes {
            "click to make the polygon"
        } else if pending.is_empty() {
            "click the first corner"
        } else if pending.len() < 3 {
            "click the next corner"
        } else {
            "next corner, or click the first one (Enter) to finish"
        };
        hint(&painter, m, text);
    }
}
