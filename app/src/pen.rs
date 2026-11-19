//! Viewport input and overlay for the Spline Pen and Polygon Pen.
//!
//! Clicks land on the drawing plane: the workplane in the Perspective view,
//! the XY plane in Front, ZY in Right and XZ in Top, through the origin. The
//! Polygon Pen also snaps to the target mesh's points and lands on its
//! surface when the click hits it.

use crate::interact::{SnapCtx, draw_snap, snap_sources};
use crate::vinput::{Anchor, DIM, HANDLE, HOVER, Input, POINT, Painter, SELECTED, View};
use forma3d_core::camera::ViewKind;
use forma3d_core::editor::Editor;
use forma3d_core::gizmo::ray_plane;
use forma3d_core::pen::{PenDrag, PenKind, PenVertex};
use forma3d_core::picking::ray_triangle;
use glam::{Mat4, Vec2, Vec3};

const ACTIVE: &str = SELECTED;
const SNAP_PX: f32 = 10.0;

fn drawing_plane(kind: ViewKind, wp: &forma3d_core::scene::Workplane) -> Vec3 {
    if kind == ViewKind::Perspective || kind.is_axonometric() {
        wp.normal()
    } else {
        kind.plane_normal()
    }
}

/// Where the mouse ray meets the plane through `through`; a plane facing the
/// camera when the ray runs along it.
fn on_plane(v: &View, normal: Vec3, m: Vec2, through: Vec3) -> Vec3 {
    let (ro, rd) = v.ray(m);
    if rd.normalize_or_zero().dot(normal).abs() > 0.05
        && let Some(t) = ray_plane(ro, rd, through, normal)
        && t > 0.0
    {
        return ro + rd * t;
    }
    ray_plane(ro, rd, through, -v.cam.forward()).map_or(through, |t| ro + rd * t)
}

fn hint(p: &mut Painter, m: Vec2, text: &str) {
    p.text(
        m + Vec2::new(14.0, -14.0),
        Anchor::LeftBottom,
        text,
        12.0,
        DIM,
    );
}

pub fn pen_mode(ed: &mut Editor, v: &View, inp: &Input, snap: &SnapCtx, p: &mut Painter) {
    let Some(pen_kind) = ed.pen.as_ref().map(|q| q.kind) else {
        return;
    };
    let mouse = inp.mouse;
    let busy = inp.navigating || inp.mods.alt;
    let target = match pen_kind {
        PenKind::Spline => ed.pen_spline().map(|(id, _, _)| id),
        PenKind::Polygon => ed.pen_mesh().map(|(id, _, _)| id),
    };
    let sources = snap_sources(snap.objects, |id| Some(id) != target, None);
    let settings = ed.snap;
    let scene_wp = ed.scene.workplane;
    let project = |q: Vec3| v.project(q);
    let snapper = |m: Vec2| -> Option<forma3d_core::snap::SnapHit> {
        if !settings.enabled {
            return None;
        }
        let q = forma3d_core::snap::Query {
            project: &project,
            mouse: m,
            ray: v.ray(m),
            ortho_forward: v.kind.is_ortho().then(|| v.cam.forward()),
            workplane: scene_wp.matrix(),
            grid_unit: snap.grid_unit,
            start: scene_wp.pos,
            extra_points: &[],
        };
        forma3d_core::snap::find(&settings, &sources, &q)
    };
    match pen_kind {
        PenKind::Spline => spline_pen(ed, v, inp, mouse, busy, &snapper, p),
        PenKind::Polygon => polygon_pen(ed, v, inp, mouse, busy, &snapper, p),
    }
    if !busy
        && inp.hovered
        && let Some(m) = mouse
    {
        draw_snap(p, snapper(m), &project);
    }
    p.text(
        Vec2::new(v.size.x - 10.0, 30.0),
        Anchor::RightTop,
        &format!("{}  (Enter or Esc to finish)", pen_kind.label()),
        12.0,
        HOVER,
    );
}

#[derive(Clone, Copy)]
enum SplineHover {
    Handle { index: usize, out: bool },
    Point(usize),
    Segment { segment: usize, t: f32 },
}

fn spline_pen(
    ed: &mut Editor,
    v: &View,
    inp: &Input,
    mouse: Option<Vec2>,
    busy: bool,
    snapper: &dyn Fn(Vec2) -> Option<forma3d_core::snap::SnapHit>,
    p: &mut Painter,
) {
    let kind = v.kind;
    let project = |q: Vec3| v.project(q);
    let normal = drawing_plane(kind, &ed.scene.workplane);
    let place = |m: Vec2, through: Vec3| {
        snapper(m).map_or_else(|| on_plane(v, normal, m, through), |h| h.pos)
    };

    let hover = match (ed.pen_spline(), mouse) {
        (Some((_, s, world)), Some(m))
            if !busy && inp.hovered && ed.pen.as_ref().is_some_and(|q| q.drag.is_none()) =>
        {
            let near = |q: Vec3| {
                project(world.transform_point3(q))
                    .map(|r| r.distance(m))
                    .filter(|d| *d < SNAP_PX)
            };
            let mut found = None;
            for (i, pt) in s.points.iter().enumerate() {
                for (out, tan) in [(true, pt.tan_out), (false, pt.tan_in)] {
                    if tan != Vec3::ZERO && near(pt.pos + tan).is_some() {
                        found = Some(SplineHover::Handle { index: i, out });
                    }
                }
            }
            if found.is_none() {
                found = s
                    .points
                    .iter()
                    .position(|pt| near(pt.pos).is_some())
                    .map(SplineHover::Point);
            }
            if found.is_none()
                && let Some((segment, t, d)) =
                    s.nearest(|q| project(world.transform_point3(q)).map(|r| r.distance(m)))
                && d < 6.0
            {
                found = Some(SplineHover::Segment { segment, t });
            }
            found
        }
        _ => None,
    };
    let anchor = ed
        .pen_spline()
        .map(|(_, _, w)| w.w_axis.truncate())
        .unwrap_or(if kind == ViewKind::Perspective || kind.is_axonometric() {
            ed.scene.workplane.pos
        } else {
            Vec3::ZERO
        });

    if !busy {
        if inp.clicked
            && let Some(m) = mouse
        {
            match hover {
                Some(SplineHover::Point(0))
                    if ed
                        .pen_spline()
                        .is_some_and(|(_, s, _)| s.points.len() >= 3 && !s.closed) =>
                {
                    ed.pen_close_spline();
                }
                Some(SplineHover::Point(_)) | Some(SplineHover::Handle { .. }) => {}
                Some(SplineHover::Segment { segment, t }) => {
                    ed.pen_insert_spline_point(segment, t);
                }
                None => {
                    let q = place(m, anchor);
                    ed.pen_add_spline_point(q);
                }
            }
            ed.active_view = v.slot;
        }
        if inp.drag_started
            && let Some(start) = inp.press_origin
        {
            let drag = match hover {
                Some(SplineHover::Handle { index, out }) => Some(PenDrag::Tangent { index, out }),
                Some(SplineHover::Point(i)) => Some(PenDrag::MovePoint(i)),
                Some(SplineHover::Segment { segment, t }) => ed
                    .pen_insert_spline_point(segment, t)
                    .map(PenDrag::MovePoint),
                None => {
                    let q = place(start, anchor);
                    Some(PenDrag::NewPoint(ed.pen_add_spline_point(q)))
                }
            };
            if let Some(pen) = ed.pen.as_mut() {
                pen.drag = drag;
            }
            ed.active_view = v.slot;
        }
        let drag = ed.pen.as_ref().and_then(|q| q.drag);
        if let (Some(drag), Some(m), Some((_, s, world))) = (drag, mouse, ed.pen_spline())
            && inp.dragged
        {
            let point_of = |i: usize| s.points.get(i).map(|q| world.transform_point3(q.pos));
            match drag {
                PenDrag::NewPoint(i) => {
                    if let Some(at) = point_of(i) {
                        let q = on_plane(v, normal, m, at);
                        ed.pen_pull_tangents(i, q);
                    }
                }
                PenDrag::MovePoint(i) => {
                    if let Some(at) = point_of(i) {
                        let q = place(m, at);
                        ed.pen_move_spline_point(i, q);
                    }
                }
                PenDrag::Tangent { index, out } => {
                    if let Some(at) = point_of(index) {
                        let q = on_plane(v, normal, m, at);
                        ed.pen_move_tangent(index, out, q, inp.mods.shift);
                    }
                }
            }
        }
        if !inp.primary_down
            && let Some(pen) = ed.pen.as_mut()
        {
            pen.drag = None;
        }
    }

    // Overlay.
    if let Some((_, s, world)) = ed.pen_spline() {
        let w = |q: Vec3| project(world.transform_point3(q));
        for (i, pt) in s.points.iter().enumerate() {
            let Some(at) = w(pt.pos) else { continue };
            for (out, tan) in [(false, pt.tan_in), (true, pt.tan_out)] {
                if tan != Vec3::ZERO
                    && let Some(h) = w(pt.pos + tan)
                {
                    p.line(at, h, 1.0, HANDLE);
                    let hot = matches!(hover, Some(SplineHover::Handle { index, out: o }) if index == i && o == out);
                    p.circle(
                        h,
                        if hot { 5.0 } else { 3.5 },
                        if hot { HOVER } else { HANDLE },
                    );
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
            p.square(at, if hot { 4.5 } else { 3.5 }, color);
        }
        if let (Some(last), Some(m), None) = (s.points.last(), mouse, hover)
            && !s.closed
            && !busy
            && inp.hovered
            && ed.pen.as_ref().is_some_and(|q| q.drag.is_none())
            && let Some(a) = w(last.pos)
        {
            p.line(a, m, 1.0, "rgba(255,225,110,.55)");
        }
        if let (Some(SplineHover::Segment { segment, t }), Some(m)) = (hover, mouse) {
            if let Some(q) = w(s.eval(segment, t)) {
                p.ring(q, 5.0, 1.5, HOVER);
            }
            hint(p, m, "click to insert a point");
        } else if let (Some(SplineHover::Point(0)), Some(m)) = (hover, mouse) {
            if s.points.len() >= 3 && !s.closed {
                hint(p, m, "click to close the spline");
            }
        } else if let (Some(m), 0) = (mouse.filter(|_| inp.hovered), s.points.len()) {
            hint(p, m, "click to start a spline");
        }
    } else if let Some(m) = mouse.filter(|_| inp.hovered) {
        hint(p, m, "click to start a spline (drag for a curve)");
    }
}

fn mesh_points(ed: &Editor) -> Option<(Vec<Vec3>, Mat4)> {
    let (_, mesh, world) = ed.pen_mesh()?;
    Some((
        mesh.points
            .iter()
            .map(|q| world.transform_point3(*q))
            .collect(),
        world,
    ))
}

/// Where the polygon faces: towards the viewer of `v`.
fn toward_viewer(v: &View, at: Vec3) -> Vec3 {
    if v.kind.is_ortho() {
        -v.cam.forward()
    } else {
        v.cam.eye() - at
    }
}

fn pending_centre(ed: &Editor) -> Vec3 {
    let pts = mesh_points(ed);
    let w = pts.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
    let pending = ed
        .pen
        .as_ref()
        .map(|q| q.pending.clone())
        .unwrap_or_default();
    pending
        .iter()
        .filter_map(|c| match *c {
            PenVertex::Existing(i) => pts.as_ref().and_then(|(q, _)| q.get(i as usize).copied()),
            PenVertex::New(l) => Some(w.transform_point3(l)),
        })
        .fold(Vec3::ZERO, |a, b| a + b)
        / pending.len().max(1) as f32
}

/// Enter: finishes the polygon being drawn, or leaves the pen.
pub fn enter(ed: &mut Editor, v: &View) {
    match ed.pen.as_ref().map(|q| (q.kind, q.pending.is_empty())) {
        Some((PenKind::Polygon, false)) => {
            let c = pending_centre(ed);
            ed.pen_commit_polygon(toward_viewer(v, c));
        }
        Some((k, _)) => {
            ed.stop_pen();
            ed.status = format!("{} finished", k.label());
        }
        None => {}
    }
}

/// Backspace: takes back the last point or corner.
pub fn backspace(ed: &mut Editor) {
    match ed.pen.as_ref().map(|q| q.kind) {
        Some(PenKind::Spline) => ed.pen_remove_last_spline_point(),
        Some(PenKind::Polygon) => {
            if let Some(q) = ed.pen.as_mut() {
                q.pending.pop();
            }
        }
        None => {}
    }
}

fn polygon_pen(
    ed: &mut Editor,
    v: &View,
    inp: &Input,
    mouse: Option<Vec2>,
    busy: bool,
    snapper: &dyn Fn(Vec2) -> Option<forma3d_core::snap::SnapHit>,
    p: &mut Painter,
) {
    let kind = v.kind;
    let project = |q: Vec3| v.project(q);
    let points = mesh_points(ed);
    let world = points.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
    let pending = ed
        .pen
        .as_ref()
        .map(|q| q.pending.clone())
        .unwrap_or_default();
    let corner_world = |c: &PenVertex| match *c {
        PenVertex::Existing(i) => points
            .as_ref()
            .and_then(|(q, _)| q.get(i as usize).copied()),
        PenVertex::New(local) => Some(world.transform_point3(local)),
    };
    let closes = mouse.is_some_and(|m| {
        pending.len() >= 3
            && pending
                .first()
                .and_then(corner_world)
                .and_then(project)
                .is_some_and(|q| q.distance(m) < SNAP_PX)
    });
    let snap = mouse.filter(|_| !closes).and_then(|m| {
        let (pts, _) = points.as_ref()?;
        pts.iter()
            .enumerate()
            .filter_map(|(i, q)| Some((i, project(*q)?.distance(m))))
            .filter(|(_, d)| *d < SNAP_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i as u32)
    });

    let normal = drawing_plane(kind, &ed.scene.workplane);
    let plane_point = if kind == ViewKind::Perspective || kind.is_axonometric() {
        ed.scene.workplane.pos
    } else {
        Vec3::ZERO
    };
    let landing = |m: Vec2| -> Vec3 {
        if let Some(hit) = snapper(m) {
            return hit.pos;
        }
        let (ro, rd) = v.ray(m);
        if let Some((_, mesh, w)) = ed.pen_mesh() {
            let mut best: Option<f32> = None;
            for i in 0..mesh.polys.len() {
                for [a, b, c] in mesh.triangulate(i) {
                    let q = |k: usize| w.transform_point3(mesh.points[mesh.polys[i][k] as usize]);
                    if let Some(t) = ray_triangle(ro, rd, q(a), q(b), q(c))
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
        on_plane(v, normal, m, plane_point)
    };

    let clicked = !busy && inp.clicked;
    let landed = if clicked && !closes && snap.is_none() {
        mouse.map(landing)
    } else {
        None
    };
    if clicked && mouse.is_some() {
        ed.active_view = v.slot;
        if closes {
            let c = pending_centre(ed);
            ed.pen_commit_polygon(toward_viewer(v, c));
        } else if let Some(i) = snap {
            ed.pen_add_corner(PenVertex::Existing(i));
        } else if let Some(q) = landed {
            ed.pen_add_corner(PenVertex::New(q));
        }
    }

    // Overlay: the target's points (what you can snap to), when there aren't too many.
    let points = mesh_points(ed);
    if let Some((pts, _)) = &points
        && pts.len() <= 20_000
    {
        let dots: Vec<Vec2> = pts.iter().filter_map(|q| project(*q)).collect();
        p.dots(&dots, 2.0, "rgba(120,170,255,.8)");
    }
    let world = points.as_ref().map_or(Mat4::IDENTITY, |(_, w)| *w);
    let pending = ed
        .pen
        .as_ref()
        .map(|q| q.pending.clone())
        .unwrap_or_default();
    let screen: Vec<Vec2> = pending
        .iter()
        .filter_map(|c| match *c {
            PenVertex::Existing(i) => points
                .as_ref()
                .and_then(|(q, _)| q.get(i as usize).copied()),
            PenVertex::New(l) => Some(world.transform_point3(l)),
        })
        .filter_map(project)
        .collect();
    if screen.len() >= 3 {
        p.poly(&screen, "rgba(255,150,40,.16)");
    }
    for w in screen.windows(2) {
        p.line(w[0], w[1], 2.0, ACTIVE);
    }
    if let (Some(last), Some(m)) = (screen.last(), mouse)
        && !busy
        && inp.hovered
    {
        p.line(*last, m, 1.0, "rgba(255,225,110,.63)");
        if screen.len() >= 2 {
            p.line(m, screen[0], 1.0, "rgba(255,225,110,.27)");
        }
    }
    for (i, q) in screen.iter().enumerate() {
        let first_hot = i == 0 && closes;
        p.square(
            *q,
            if first_hot { 5.0 } else { 3.5 },
            if first_hot { HOVER } else { ACTIVE },
        );
    }
    if let (Some(i), Some((pts, _))) = (snap, &points)
        && let Some(q) = pts.get(i as usize).and_then(|q| project(*q))
    {
        p.ring(q, 6.0, 1.5, HOVER);
    }
    if let Some(m) = mouse.filter(|_| inp.hovered) {
        let text = if closes {
            "click to make the polygon"
        } else if pending.is_empty() {
            "click the first corner"
        } else if pending.len() < 3 {
            "click the next corner"
        } else {
            "next corner, or click the first one (Enter) to finish"
        };
        hint(p, m, text);
    }
}
