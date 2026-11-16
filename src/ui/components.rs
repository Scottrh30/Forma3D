//! Viewport interaction and overlays for Points / Edges / Polygons modes.

use super::viewport::{draw_gizmo, p2, ray_at, to_screen};
use crate::camera::{Camera, ViewKind};
use crate::component::{ComponentDrag, EditMode, PickView, SelectOp, mesh_of, selection_center};
use crate::editor::{Editor, MeshTool, ToolDrag};
use crate::gizmo::{self, CoordSystem, DragView, GizmoFrame, Handle, Tool};
use crate::scene::matrix_rotation;
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Modifiers, PointerButton, Rect, Response, Shape, Stroke, Ui, pos2, vec2};
use glam::{Vec2, Vec3};

const POINT: Color32 = Color32::from_rgb(120, 170, 255);
const SELECTED: Color32 = Color32::from_rgb(255, 150, 40);
const HOVER: Color32 = Color32::from_rgb(255, 225, 110);

fn select_op(m: Modifiers) -> SelectOp {
    if m.command {
        SelectOp::Remove
    } else if m.shift {
        SelectOp::Add
    } else {
        SelectOp::Replace
    }
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
    let painter = ui.painter_at(rect);
    let Some(id) = ed.edit_target() else {
        painter.text(
            rect.center_bottom() - vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            "Component mode: select a polygon object, or press C to make the selected object editable",
            FontId::proportional(13.0),
            theme::TEXT,
        );
        return;
    };
    let mouse = resp.hover_pos().or(resp.interact_pointer_pos()).map(|p| Vec2::new(p.x, p.y));
    let project = |p: Vec3| to_screen(cam, rect, p);
    let world = ed.scene.world_matrix(id);
    let mode = ed.mode;
    let alt = mods.alt;

    // Picks made against the current mesh (immutable borrow ends with this block).
    let (hover_point, hover_edge, hover_poly, affected, centre) = {
        let Some(mesh) = mesh_of(&ed.scene, id) else { return };
        let pv = PickView { mesh, world, eye: cam.eye(), forward: cam.forward(), ortho: kind.is_ortho(), project: &project };
        let busy = navigating || alt || ed.comp_drag.is_some() || ed.tool_drag.is_some() || ed.box_start.is_some();
        let (mut hp, mut he, mut hpoly) = (None, None, None);
        if let (Some(m), false) = (mouse, busy)
            && resp.hovered() {
                match (mode, ed.mesh_tool) {
                    (_, Some(MeshTool::LoopCut)) => he = pv.pick_edge(m, 10.0),
                    (EditMode::Points, _) => hp = pv.pick_point(m, 10.0),
                    (EditMode::Edges, _) => he = pv.pick_edge(m, 7.0),
                    (EditMode::Polygons, _) => hpoly = pv.pick_poly(ray_at(cam, rect, m)),
                    _ => {}
                }
            }
        let affected = ed.comp.affected_points(mesh, mode);
        let centre = selection_center(mesh, world, &affected);
        (hp, he, hpoly, affected, centre)
    };

    // ---- mesh tools ------------------------------------------------------
    if let Some(tool) = ed.mesh_tool {
        match tool {
            MeshTool::Extrude | MeshTool::Inset | MeshTool::Bevel => {
                if resp.drag_started_by(PointerButton::Primary) && !alt {
                    let needs_edges = tool == MeshTool::Bevel;
                    if needs_edges && ed.comp.edges.is_empty() {
                        ed.status = "Bevel: select edges first".to_owned();
                    } else if !needs_edges && ed.comp.polys.is_empty() {
                        ed.status = format!("{}: select polygons first", tool.label());
                    } else if let (Some(base), Some(c)) = (mesh_of(&ed.scene, id).cloned(), centre) {
                        let start_x = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).map_or(0.0, |p| p.x);
                        ed.tool_drag = Some(ToolDrag {
                            id,
                            base,
                            polys: ed.comp.polys.clone(),
                            edges: ed.comp.edges.clone(),
                            start_x,
                            world_per_px: cam.world_per_pixel(c, rect.height()),
                            amount: 0.0,
                        });
                        ed.active_view = kind;
                    }
                }
                if let Some(td) = ed.tool_drag.clone()
                    && ed.active_view == kind {
                        if resp.dragged_by(PointerButton::Primary) {
                            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                                ed.apply_tool_drag((p.x - td.start_x) * td.world_per_px);
                            }
                        } else if !ui.input(|i| i.pointer.primary_down()) {
                            ed.status = format!("{} {:.1} cm", tool.label(), td.amount);
                            ed.tool_drag = None;
                            if tool == MeshTool::Bevel {
                                // The old edge indices no longer describe the new mesh.
                                ed.comp.edges.clear();
                            }
                        }
                    }
                if let (Some(td), Some(m)) = (&ed.tool_drag, mouse) {
                    painter.text(p2(m) + vec2(14.0, -14.0), Align2::LEFT_BOTTOM, format!("{:.1} cm", td.amount), FontId::proportional(13.0), theme::GIZMO_HOT);
                }
            }
            MeshTool::Knife => {
                if resp.clicked_by(PointerButton::Primary) && !alt
                    && let Some(p) = resp.interact_pointer_pos() {
                        match ed.knife_start {
                            Some((v, [x, y])) if v == kind => {
                                let (a, b) = (Vec2::new(x, y), Vec2::new(p.x, p.y));
                                if (b - a).length() > 3.0 {
                                    // The cut plane contains both view rays (and the eye in perspective).
                                    let (o0, d0) = ray_at(cam, rect, a);
                                    let (o1, d1) = ray_at(cam, rect, b);
                                    let depth = (cam.target - cam.eye()).length().max(1.0);
                                    let (p0, p1) = (o0 + d0 * depth, o1 + d1 * depth);
                                    let normal = (p1 - p0).cross(d0);
                                    ed.knife_cut(p0, normal);
                                }
                                ed.knife_start = None;
                            }
                            _ => ed.knife_start = Some((kind, [p.x, p.y])),
                        }
                    }
                if let Some((v, [x, y])) = ed.knife_start {
                    if v == kind
                        && let Some(m) = mouse {
                            let a = pos2(x, y);
                            let b = p2(m);
                            // Faint infinite line plus the drawn segment.
                            let dir = (b - a).normalized();
                            if dir.is_finite() {
                                let far = rect.width() + rect.height();
                                painter.line_segment([a - dir * far, b + dir * far], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 225, 110, 90)));
                            }
                            painter.line_segment([a, b], Stroke::new(2.0, HOVER));
                            painter.circle_filled(a, 3.5, HOVER);
                        }
                } else if let Some(m) = mouse {
                    painter.text(p2(m) + vec2(14.0, -14.0), Align2::LEFT_BOTTOM, "click the start of the cut", FontId::proportional(12.0), theme::TEXT_DIM);
                }
            }
            MeshTool::LoopCut => {
                if let Some((edge, t)) = hover_edge {
                    // Preview: cut a copy and draw the new edges.
                    if let Some(mesh) = mesh_of(&ed.scene, id) {
                        let mut preview = mesh.clone();
                        for (a, b) in preview.loop_cut(edge, t) {
                            let (pa, pb) = (preview.points[a as usize], preview.points[b as usize]);
                            if let (Some(sa), Some(sb)) = (project(world.transform_point3(pa)), project(world.transform_point3(pb))) {
                                painter.line_segment([p2(sa), p2(sb)], Stroke::new(2.0, HOVER));
                            }
                        }
                    }
                    if resp.clicked_by(PointerButton::Primary) && !alt {
                        ed.loop_cut(edge, t);
                    }
                }
            }
        }
        draw_components(&painter, ed, id, cam, kind, &project, None, None, None);
        return;
    }

    // ---- gizmo for the selection -------------------------------------------
    let gizmo_frame = centre.filter(|_| ed.tool != Tool::Select).map(|origin| {
        let axes = if ed.coord == CoordSystem::Object {
            let r = matrix_rotation(world);
            [r * Vec3::X, r * Vec3::Y, r * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        GizmoFrame { origin, axes, world_per_px: cam.world_per_pixel(origin, rect.height()) }
    });
    let mut hot = None;
    if let (Some(gf), Some(m)) = (&gizmo_frame, mouse)
        && ed.comp_drag.is_none() && !navigating && !alt {
            hot = gizmo::hit_test(ed.tool, gf, &project, m, ed.axes);
        }
    if let Some(d) = &ed.comp_drag {
        hot = Some(d.handle);
    }

    if resp.drag_started_by(PointerButton::Primary) && !alt {
        // Where the button went down, not where the drag was first recognised.
        let start = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).map(|p| Vec2::new(p.x, p.y)).unwrap_or_default();
        ed.active_view = kind;
        let on_handle = gizmo_frame.as_ref().and_then(|gf| gizmo::hit_test(ed.tool, gf, &project, start, ed.axes));
        let mut tool = ed.tool;
        let mut frame = gizmo_frame;
        let mut moving = affected.clone();
        // Tweak: grab the point, edge or polygon under the cursor and move it.
        if ed.tweak && on_handle.is_none()
            && let Some(mesh) = mesh_of(&ed.scene, id) {
                let pv = PickView { mesh, world, eye: cam.eye(), forward: cam.forward(), ortho: kind.is_ortho(), project: &project };
                let picked = match mode {
                    EditMode::Points => pv.pick_point(start, 10.0).map(|p| (vec![p], vec![], vec![])),
                    EditMode::Edges => pv.pick_edge(start, 7.0).map(|(e, _)| (vec![], vec![e], vec![])),
                    EditMode::Polygons => pv.pick_poly(ray_at(cam, rect, start)).map(|p| (vec![], vec![], vec![p])),
                    _ => None,
                };
                if let Some((pts, edges, polys)) = picked {
                    let already = pts.iter().all(|p| ed.comp.points.contains(p))
                        && edges.iter().all(|e| ed.comp.edges.contains(&crate::generator::edge_key(e.0, e.1)))
                        && polys.iter().all(|p| ed.comp.polys.contains(p));
                    if !already {
                        ed.comp.apply_points(pts, SelectOp::Replace);
                        ed.comp.apply_edges(edges, SelectOp::Replace);
                        ed.comp.apply_polys(polys, SelectOp::Replace);
                    }
                    if let Some(mesh) = mesh_of(&ed.scene, id) {
                        moving = ed.comp.affected_points(mesh, mode);
                        if let Some(origin) = selection_center(mesh, world, &moving) {
                            let axes = frame.map_or([Vec3::X, Vec3::Y, Vec3::Z], |f| f.axes);
                            frame = Some(GizmoFrame { origin, axes, world_per_px: cam.world_per_pixel(origin, rect.height()) });
                        }
                    }
                    if tool == Tool::Select {
                        tool = Tool::Move;
                    }
                }
            }
        match frame {
            Some(gf) if !moving.is_empty() && tool != Tool::Select => {
                let handle = on_handle.unwrap_or(Handle::Free);
                let view = DragView { forward: cam.forward(), right: cam.right(), up: cam.up() };
                let ray = ray_at(cam, rect, start);
                ed.comp_drag = ComponentDrag::begin(&ed.scene, id, &moving, tool, handle, gf, view, ed.axes, ray, start);
                if let Some(d) = ed.comp_drag.as_mut() {
                    d.quantize = ed.quantize;
                }
                let what = mode.label();
                ed.label(match tool {
                    Tool::Move => format!("Move {what}"),
                    Tool::Rotate => format!("Rotate {what}"),
                    _ => format!("Scale {what}"),
                });
            }
            _ => ed.box_start = Some((kind, [start.x, start.y])),
        }
    }
    if ed.active_view == kind
        && let Some(d) = ed.comp_drag.clone() {
            if resp.dragged_by(PointerButton::Primary) {
                if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                    let m = Vec2::new(p.x, p.y);
                    let ray = ray_at(cam, rect, m);
                    d.update(&mut ed.scene, ray, m);
                    ed.snap_hit = None;
                    if d.tool == Tool::Move && ed.snap.enabled {
                        // The object's other points (the moving ones would chase the cursor).
                        let others: Vec<Vec3> = mesh_of(&ed.scene, id)
                            .map(|mesh| {
                                let moving = ed.comp.affected_points(mesh, mode);
                                (0..mesh.points.len() as u32)
                                    .filter(|p| !moving.contains(p))
                                    .map(|p| world.transform_point3(mesh.points[p as usize]))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let sources = super::viewport::snap_sources(snap.objects, |o| o != id, None);
                        let q = super::viewport::snap_query(&ed.scene, cam, kind, &project, m, ray, snap.grid_unit, d.origin(), &others);
                        if let Some(hit) = crate::snap::find(&ed.snap, &sources, &q) {
                            d.move_to(&mut ed.scene, hit.pos);
                            ed.snap_hit = Some(hit);
                        }
                    }
                }
            } else if !ui.input(|i| i.pointer.primary_down()) {
                ed.comp_drag = None;
                ed.snap_hit = None;
            }
        }

    // ---- box selection -----------------------------------------------------
    let mut box_rect = None;
    if let Some((v, [x, y])) = ed.box_start
        && v == kind {
            let now = ui.input(|i| i.pointer.interact_pos()).unwrap_or(pos2(x, y));
            let r = Rect::from_two_pos(pos2(x, y), now);
            box_rect = Some(r);
            if !ui.input(|i| i.pointer.primary_down()) {
                ed.box_start = None;
                if let Some(mesh) = mesh_of(&ed.scene, id) {
                    let pv = PickView { mesh, world, eye: cam.eye(), forward: cam.forward(), ortho: kind.is_ortho(), project: &project };
                    let (pts, edges, polys) = pv.in_rect(mode, Vec2::new(r.min.x, r.min.y), Vec2::new(r.max.x, r.max.y));
                    let op = select_op(mods);
                    match mode {
                        EditMode::Points => ed.comp.apply_points(pts, op),
                        EditMode::Edges => ed.comp.apply_edges(edges, op),
                        EditMode::Polygons => ed.comp.apply_polys(polys, op),
                        EditMode::Model | EditMode::Object | EditMode::Workplane => {}
                    }
                }
            }
        }

    // ---- click selection ---------------------------------------------------
    if resp.clicked_by(PointerButton::Primary) && !alt {
        let op = select_op(mods);
        match mode {
            EditMode::Points => match hover_point {
                Some(p) => ed.comp.apply_points([p], op),
                None if op == SelectOp::Replace => ed.comp.points.clear(),
                None => {}
            },
            EditMode::Edges => match hover_edge {
                Some((e, _)) => ed.comp.apply_edges([e], op),
                None if op == SelectOp::Replace => ed.comp.edges.clear(),
                None => {}
            },
            EditMode::Polygons => match hover_poly {
                Some(p) => ed.comp.apply_polys([p], op),
                None if op == SelectOp::Replace => ed.comp.polys.clear(),
                None => {}
            },
            EditMode::Model | EditMode::Object | EditMode::Workplane => {}
        }
    }
    // Double-click an edge: select its loop.
    if resp.double_clicked() && mode == EditMode::Edges
        && let (Some((e, _)), Some(mesh)) = (hover_edge, mesh_of(&ed.scene, id)) {
            let lp = mesh.edge_loop(e);
            let op = if mods.shift { SelectOp::Add } else { SelectOp::Replace };
            ed.comp.apply_edges(lp, op);
        }

    resp.context_menu(|ui| super::mesh_menu(ui, ed));

    // ---- overlays ------------------------------------------------------------
    draw_components(&painter, ed, id, cam, kind, &project, hover_point, hover_edge.map(|(e, _)| e), hover_poly);
    if let Some(gf) = &gizmo_frame {
        draw_gizmo(&painter, ed.tool, gf, &project, ed.axes, hot);
    }
    if ed.comp_drag.is_some() {
        super::viewport::draw_snap(&painter, ed.snap_hit, &project);
    }
    if let Some(r) = box_rect {
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(120, 170, 255, 30));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, POINT), egui::StrokeKind::Inside);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_components(
    painter: &egui::Painter,
    ed: &Editor,
    id: u64,
    cam: &Camera,
    kind: ViewKind,
    project: &dyn Fn(Vec3) -> Option<Vec2>,
    hover_point: Option<u32>,
    hover_edge: Option<(u32, u32)>,
    hover_poly: Option<u32>,
) {
    let Some(mesh) = mesh_of(&ed.scene, id) else { return };
    let world = ed.scene.world_matrix(id);
    let pv = PickView { mesh, world, eye: cam.eye(), forward: cam.forward(), ortho: kind.is_ortho(), project };
    let screen: Vec<Option<Vec2>> = (0..mesh.points.len() as u32).map(|p| pv.screen(p)).collect();
    let seg = |a: u32, b: u32| Some([p2(screen[a as usize]?), p2(screen[b as usize]?)]);
    let facing = pv.facing();

    // Cage: inside a generator (e.g. Subdivision Surface) the object itself isn't drawn.
    if ed.scene.consumer_ancestor(id).is_some() {
        for (a, b) in mesh.edges() {
            if let Some(s) = seg(a, b) {
                painter.line_segment(s, Stroke::new(1.0, Color32::from_rgba_unmultiplied(160, 190, 255, 120)));
            }
        }
    }

    let fill_poly = |i: usize, color: Color32| {
        let poly = &mesh.polys[i];
        for t in mesh.triangulate(i) {
            if let (Some(a), Some(b), Some(c)) = (screen[poly[t[0]] as usize], screen[poly[t[1]] as usize], screen[poly[t[2]] as usize]) {
                painter.add(Shape::convex_polygon(vec![p2(a), p2(b), p2(c)], color, Stroke::NONE));
            }
        }
    };

    match ed.mode {
        EditMode::Points => {
            let vis = pv.visible_points(&facing);
            for (i, s) in screen.iter().enumerate() {
                let Some(s) = s else { continue };
                let selected = ed.comp.points.contains(&(i as u32));
                if !vis[i] && !selected {
                    continue;
                }
                let (r, c) = if Some(i as u32) == hover_point {
                    (4.5, HOVER)
                } else if selected {
                    (3.5, SELECTED)
                } else {
                    (2.5, POINT)
                };
                painter.circle_filled(p2(*s), r, c);
            }
        }
        EditMode::Edges => {
            for &(a, b) in &ed.comp.edges {
                if let Some(s) = seg(a, b) {
                    painter.line_segment(s, Stroke::new(2.5, SELECTED));
                }
            }
            if let Some((a, b)) = hover_edge
                && let Some(s) = seg(a, b) {
                    painter.line_segment(s, Stroke::new(3.0, HOVER));
                }
        }
        EditMode::Polygons => {
            for &i in &ed.comp.polys {
                if (i as usize) < mesh.polys.len() {
                    fill_poly(i as usize, Color32::from_rgba_unmultiplied(255, 150, 40, if facing[i as usize] { 120 } else { 40 }));
                    let poly = &mesh.polys[i as usize];
                    for k in 0..poly.len() {
                        if let Some(s) = seg(poly[k], poly[(k + 1) % poly.len()]) {
                            painter.line_segment(s, Stroke::new(1.5, SELECTED));
                        }
                    }
                }
            }
            if let Some(i) = hover_poly {
                fill_poly(i as usize, Color32::from_rgba_unmultiplied(255, 225, 110, 60));
            }
        }
        EditMode::Model | EditMode::Object | EditMode::Workplane => {}
    }
}
