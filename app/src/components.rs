//! The viewport tools in Points, Edges and Polygons mode: picking and box
//! selection, the gizmo on the selected components, Tweak, and the mesh
//! tools (Extrude, Extrude Inner, Bevel, Knife, Loop Cut).

use crate::interact::{SnapCtx, draw_gizmo, draw_snap, snap_query, snap_sources};
use crate::keymap::Mods;
use crate::vinput::{Anchor, HOT, HOVER, Input, POINT, Painter, SELECTED, TEXT, View};
use forma3d_core::component::{
    ComponentDrag, EditMode, PickView, SelectOp, mesh_of, selection_center,
};
use forma3d_core::editor::{Editor, MeshTool, ToolDrag};
use forma3d_core::gizmo::{self, CoordSystem, DragView, GizmoFrame, Handle, Tool};
use forma3d_core::scene::matrix_rotation;
use glam::{Vec2, Vec3};

fn select_op(m: Mods) -> SelectOp {
    if m.ctrl {
        SelectOp::Remove
    } else if m.shift {
        SelectOp::Add
    } else {
        SelectOp::Replace
    }
}

pub fn component_mode(ed: &mut Editor, v: &View, inp: &Input, snap: &SnapCtx, p: &mut Painter) {
    let kind = v.kind;
    let cam = v.cam;
    let Some(id) = ed.edit_target() else {
        p.text(
            Vec2::new(v.size.x * 0.5, v.size.y - 30.0),
            Anchor::CenterCenter,
            "Component mode: select a polygon object, or press C to make the selected object editable",
            13.0,
            TEXT,
        );
        return;
    };
    let mouse = inp.mouse;
    let project = |q: Vec3| v.project(q);
    let world = ed.scene.world_matrix(id);
    let mode = ed.mode;
    let alt = inp.mods.alt;

    let (hover_point, hover_edge, hover_poly, affected, centre) = {
        let Some(mesh) = mesh_of(&ed.scene, id) else {
            return;
        };
        let pv = PickView {
            mesh,
            world,
            eye: cam.eye(),
            forward: cam.forward(),
            ortho: kind.is_ortho(),
            project: &project,
        };
        let busy = inp.navigating
            || alt
            || ed.comp_drag.is_some()
            || ed.tool_drag.is_some()
            || ed.box_start.is_some();
        let (mut hp, mut he, mut hpoly) = (None, None, None);
        if let (Some(m), false, true) = (mouse, busy, inp.hovered) {
            match (mode, ed.mesh_tool) {
                (_, Some(MeshTool::LoopCut)) => he = pv.pick_edge(m, 10.0),
                (EditMode::Points, _) => hp = pv.pick_point(m, 10.0),
                (EditMode::Edges, _) => he = pv.pick_edge(m, 7.0),
                (EditMode::Polygons, _) => hpoly = pv.pick_poly(v.ray(m)),
                _ => {}
            }
        }
        let affected = ed.comp.affected_points(mesh, mode);
        let centre = selection_center(mesh, world, &affected);
        (hp, he, hpoly, affected, centre)
    };

    // ---- mesh tools ----------------------------------------------------------
    if let Some(tool) = ed.mesh_tool {
        match tool {
            MeshTool::Extrude | MeshTool::Inset | MeshTool::Bevel => {
                if inp.drag_started && !alt {
                    let needs_edges = tool == MeshTool::Bevel;
                    if needs_edges && ed.comp.edges.is_empty() {
                        ed.status = "Bevel: select edges first".to_owned();
                    } else if !needs_edges && ed.comp.polys.is_empty() {
                        ed.status = format!("{}: select polygons first", tool.label());
                    } else if let (Some(base), Some(c)) = (mesh_of(&ed.scene, id).cloned(), centre)
                    {
                        let start_x = inp.press_origin.or(mouse).map_or(0.0, |q| q.x);
                        ed.tool_drag = Some(ToolDrag {
                            id,
                            base,
                            polys: ed.comp.polys.clone(),
                            edges: ed.comp.edges.clone(),
                            start_x,
                            world_per_px: v.world_per_pixel(c),
                            amount: 0.0,
                        });
                        ed.active_view = kind;
                        ed.label(tool.label());
                    }
                }
                if let Some(td) = ed.tool_drag.clone()
                    && ed.active_view == kind
                {
                    if inp.dragged {
                        if let Some(m) = mouse {
                            ed.apply_tool_drag((m.x - td.start_x) * td.world_per_px);
                        }
                    } else if !inp.primary_down {
                        ed.status = format!("{} {:.1} cm", tool.label(), td.amount);
                        ed.tool_drag = None;
                        if tool == MeshTool::Bevel {
                            // The old edge indices no longer describe the new mesh.
                            ed.comp.edges.clear();
                        }
                    }
                }
                if let (Some(td), Some(m)) = (&ed.tool_drag, mouse) {
                    p.text(
                        m + Vec2::new(14.0, -14.0),
                        Anchor::LeftBottom,
                        &format!("{:.1} cm", td.amount),
                        13.0,
                        HOT,
                    );
                } else if let Some(m) = mouse.filter(|_| inp.hovered) {
                    p.text(
                        m + Vec2::new(14.0, -14.0),
                        Anchor::LeftBottom,
                        &format!("{}: drag left or right", tool.label()),
                        12.0,
                        crate::vinput::DIM,
                    );
                }
            }
            MeshTool::Knife => {
                if inp.clicked
                    && !alt
                    && let Some(q) = mouse
                {
                    match ed.knife_start {
                        Some((vk, [x, y])) if vk == kind => {
                            let (a, b) = (Vec2::new(x, y), q);
                            if (b - a).length() > 3.0 {
                                // The cut plane contains both view rays (and the eye in perspective).
                                let (o0, d0) = v.ray(a);
                                let (o1, d1) = v.ray(b);
                                let depth = (cam.target - cam.eye()).length().max(1.0);
                                let (p0, p1) = (o0 + d0 * depth, o1 + d1 * depth);
                                let normal = (p1 - p0).cross(d0);
                                ed.knife_cut(p0, normal);
                            }
                            ed.knife_start = None;
                        }
                        _ => ed.knife_start = Some((kind, [q.x, q.y])),
                    }
                }
                if let Some((vk, [x, y])) = ed.knife_start {
                    if vk == kind
                        && let Some(m) = mouse
                    {
                        let a = Vec2::new(x, y);
                        let dir = (m - a).normalize_or_zero();
                        if dir != Vec2::ZERO {
                            let far = v.size.x + v.size.y;
                            p.line(a - dir * far, m + dir * far, 1.0, "rgba(255,225,110,.35)");
                        }
                        p.line(a, m, 2.0, HOVER);
                        p.circle(a, 3.5, HOVER);
                    }
                } else if let Some(m) = mouse.filter(|_| inp.hovered) {
                    p.text(
                        m + Vec2::new(14.0, -14.0),
                        Anchor::LeftBottom,
                        "click the start of the cut",
                        12.0,
                        crate::vinput::DIM,
                    );
                }
            }
            MeshTool::LoopCut => {
                if let Some((edge, t)) = hover_edge {
                    if let Some(mesh) = mesh_of(&ed.scene, id) {
                        let mut preview = mesh.clone();
                        let segs: Vec<(Vec2, Vec2)> = preview
                            .loop_cut(edge, t)
                            .into_iter()
                            .filter_map(|(a, b)| {
                                let (pa, pb) =
                                    (preview.points[a as usize], preview.points[b as usize]);
                                Some((
                                    project(world.transform_point3(pa))?,
                                    project(world.transform_point3(pb))?,
                                ))
                            })
                            .collect();
                        p.segments(&segs, 2.0, HOVER);
                    }
                    if inp.clicked && !alt {
                        ed.loop_cut(edge, t);
                    }
                }
            }
        }
        draw_components(p, ed, id, v, None, None, None);
        return;
    }

    // ---- gizmo for the selection ---------------------------------------------
    let gizmo_frame = centre.filter(|_| ed.tool != Tool::Select).map(|origin| {
        let axes = if ed.coord == CoordSystem::Object {
            let r = matrix_rotation(world);
            [r * Vec3::X, r * Vec3::Y, r * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        GizmoFrame {
            origin,
            axes,
            world_per_px: v.world_per_pixel(origin),
        }
    });
    let mut hot = None;
    if let (Some(gf), Some(m)) = (&gizmo_frame, mouse)
        && ed.comp_drag.is_none()
        && !inp.navigating
        && !alt
    {
        hot = gizmo::hit_test(ed.tool, gf, &project, m, ed.axes);
    }
    if let Some(d) = &ed.comp_drag {
        hot = Some(d.handle);
    }

    if inp.drag_started && !alt {
        let start = inp.press_origin.or(mouse).unwrap_or_default();
        ed.active_view = kind;
        let on_handle = gizmo_frame
            .as_ref()
            .and_then(|gf| gizmo::hit_test(ed.tool, gf, &project, start, ed.axes));
        let mut tool = ed.tool;
        let mut frame = gizmo_frame;
        let mut moving = affected.clone();
        // Tweak: grab the point, edge or polygon under the cursor and move it.
        if ed.tweak
            && on_handle.is_none()
            && let Some(mesh) = mesh_of(&ed.scene, id)
        {
            let pv = PickView {
                mesh,
                world,
                eye: cam.eye(),
                forward: cam.forward(),
                ortho: kind.is_ortho(),
                project: &project,
            };
            let picked = match mode {
                EditMode::Points => pv
                    .pick_point(start, 10.0)
                    .map(|q| (vec![q], vec![], vec![])),
                EditMode::Edges => pv
                    .pick_edge(start, 7.0)
                    .map(|(e, _)| (vec![], vec![e], vec![])),
                EditMode::Polygons => pv
                    .pick_poly(v.ray(start))
                    .map(|q| (vec![], vec![], vec![q])),
                _ => None,
            };
            if let Some((pts, edges, polys)) = picked {
                let already = pts.iter().all(|q| ed.comp.points.contains(q))
                    && edges.iter().all(|e| {
                        ed.comp
                            .edges
                            .contains(&forma3d_core::generator::edge_key(e.0, e.1))
                    })
                    && polys.iter().all(|q| ed.comp.polys.contains(q));
                if !already {
                    ed.comp.apply_points(pts, SelectOp::Replace);
                    ed.comp.apply_edges(edges, SelectOp::Replace);
                    ed.comp.apply_polys(polys, SelectOp::Replace);
                }
                if let Some(mesh) = mesh_of(&ed.scene, id) {
                    moving = ed.comp.affected_points(mesh, mode);
                    if let Some(origin) = selection_center(mesh, world, &moving) {
                        let axes = frame.map_or([Vec3::X, Vec3::Y, Vec3::Z], |f| f.axes);
                        frame = Some(GizmoFrame {
                            origin,
                            axes,
                            world_per_px: v.world_per_pixel(origin),
                        });
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
                let view = DragView {
                    forward: cam.forward(),
                    right: cam.right(),
                    up: cam.up(),
                };
                ed.comp_drag = ComponentDrag::begin(
                    &ed.scene,
                    id,
                    &moving,
                    tool,
                    handle,
                    gf,
                    view,
                    ed.axes,
                    v.ray(start),
                    start,
                );
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
        && let Some(d) = ed.comp_drag.clone()
    {
        if inp.dragged {
            if let Some(m) = mouse {
                let ray = v.ray(m);
                d.update(&mut ed.scene, ray, m);
                ed.snap_hit = None;
                if d.tool == Tool::Move && ed.snap.enabled {
                    // The object's other points (the moving ones would chase the cursor).
                    let others: Vec<Vec3> = mesh_of(&ed.scene, id)
                        .map(|mesh| {
                            let moving = ed.comp.affected_points(mesh, mode);
                            (0..mesh.points.len() as u32)
                                .filter(|q| !moving.contains(q))
                                .map(|q| world.transform_point3(mesh.points[q as usize]))
                                .collect()
                        })
                        .unwrap_or_default();
                    let sources = snap_sources(snap.objects, |o| o != id, None);
                    let q = snap_query(
                        &ed.scene,
                        &cam,
                        kind,
                        &project,
                        m,
                        ray,
                        snap.grid_unit,
                        d.origin(),
                        &others,
                    );
                    if let Some(hit) = forma3d_core::snap::find(&ed.snap, &sources, &q) {
                        d.move_to(&mut ed.scene, hit.pos);
                        ed.snap_hit = Some(hit);
                    }
                }
            }
        } else if !inp.primary_down {
            ed.comp_drag = None;
            ed.snap_hit = None;
        }
    }

    // ---- box selection -------------------------------------------------------
    let mut box_rect = None;
    if let Some((vk, [x, y])) = ed.box_start
        && vk == kind
    {
        let a = Vec2::new(x, y);
        let now = mouse.unwrap_or(a);
        box_rect = Some((a, now));
        if !inp.primary_down {
            ed.box_start = None;
            if let Some(mesh) = mesh_of(&ed.scene, id) {
                let pv = PickView {
                    mesh,
                    world,
                    eye: cam.eye(),
                    forward: cam.forward(),
                    ortho: kind.is_ortho(),
                    project: &project,
                };
                let (pts, edges, polys) = pv.in_rect(mode, a.min(now), a.max(now));
                let op = select_op(inp.mods);
                match mode {
                    EditMode::Points => ed.comp.apply_points(pts, op),
                    EditMode::Edges => ed.comp.apply_edges(edges, op),
                    EditMode::Polygons => ed.comp.apply_polys(polys, op),
                    EditMode::Model | EditMode::Object | EditMode::Workplane => {}
                }
            }
        }
    }

    // ---- click selection -----------------------------------------------------
    if inp.clicked && !alt {
        let op = select_op(inp.mods);
        match mode {
            EditMode::Points => match hover_point {
                Some(q) => ed.comp.apply_points([q], op),
                None if op == SelectOp::Replace => ed.comp.points.clear(),
                None => {}
            },
            EditMode::Edges => match hover_edge {
                Some((e, _)) => ed.comp.apply_edges([e], op),
                None if op == SelectOp::Replace => ed.comp.edges.clear(),
                None => {}
            },
            EditMode::Polygons => match hover_poly {
                Some(q) => ed.comp.apply_polys([q], op),
                None if op == SelectOp::Replace => ed.comp.polys.clear(),
                None => {}
            },
            EditMode::Model | EditMode::Object | EditMode::Workplane => {}
        }
    }
    // Double-click an edge: select its loop.
    if inp.double_clicked
        && mode == EditMode::Edges
        && let (Some((e, _)), Some(mesh)) = (hover_edge, mesh_of(&ed.scene, id))
    {
        let lp = mesh.edge_loop(e);
        let op = if inp.mods.shift {
            SelectOp::Add
        } else {
            SelectOp::Replace
        };
        ed.comp.apply_edges(lp, op);
    }

    // ---- overlays --------------------------------------------------------------
    draw_components(
        p,
        ed,
        id,
        v,
        hover_point,
        hover_edge.map(|(e, _)| e),
        hover_poly,
    );
    if let Some(gf) = &gizmo_frame {
        draw_gizmo(p, ed.tool, gf, &project, ed.axes, hot);
    }
    if ed.comp_drag.is_some() {
        draw_snap(p, ed.snap_hit, &project);
    }
    if let Some((a, b)) = box_rect {
        p.rect(a, b, "rgba(120,170,255,.12)", POINT);
    }
}

fn draw_components(
    p: &mut Painter,
    ed: &Editor,
    id: u64,
    v: &View,
    hover_point: Option<u32>,
    hover_edge: Option<(u32, u32)>,
    hover_poly: Option<u32>,
) {
    let Some(mesh) = mesh_of(&ed.scene, id) else {
        return;
    };
    let world = ed.scene.world_matrix(id);
    let project = |q: Vec3| v.project(q);
    let pv = PickView {
        mesh,
        world,
        eye: v.cam.eye(),
        forward: v.cam.forward(),
        ortho: v.kind.is_ortho(),
        project: &project,
    };
    let screen: Vec<Option<Vec2>> = (0..mesh.points.len() as u32)
        .map(|q| pv.screen(q))
        .collect();
    let seg = |a: u32, b: u32| {
        Some((
            screen.get(a as usize).copied().flatten()?,
            screen.get(b as usize).copied().flatten()?,
        ))
    };
    let facing = pv.facing();

    // Cage: inside a generator (e.g. Subdivision Surface) the object itself isn't drawn.
    if ed.scene.consumer_ancestor(id).is_some() {
        let segs: Vec<(Vec2, Vec2)> = mesh
            .edges()
            .into_iter()
            .filter_map(|(a, b)| seg(a, b))
            .collect();
        p.segments(&segs, 1.0, "rgba(160,190,255,.47)");
    }

    let tris_of = |i: usize| -> Vec<[Vec2; 3]> {
        let poly = &mesh.polys[i];
        mesh.triangulate(i)
            .into_iter()
            .filter_map(|t| {
                Some([
                    screen[poly[t[0]] as usize]?,
                    screen[poly[t[1]] as usize]?,
                    screen[poly[t[2]] as usize]?,
                ])
            })
            .collect()
    };

    match ed.mode {
        EditMode::Points => {
            let vis = pv.visible_points(&facing);
            let (mut plain, mut sel) = (Vec::new(), Vec::new());
            for (i, s) in screen.iter().enumerate() {
                let Some(s) = s else { continue };
                let selected = ed.comp.points.contains(&(i as u32));
                if !vis[i] && !selected {
                    continue;
                }
                if Some(i as u32) == hover_point {
                    continue;
                }
                if selected {
                    sel.push(*s)
                } else {
                    plain.push(*s)
                }
            }
            p.dots(&plain, 2.5, POINT);
            p.dots(&sel, 3.5, SELECTED);
            if let Some(s) = hover_point.and_then(|h| screen.get(h as usize).copied().flatten()) {
                p.circle(s, 4.5, HOVER);
            }
        }
        EditMode::Edges => {
            let segs: Vec<(Vec2, Vec2)> = ed
                .comp
                .edges
                .iter()
                .filter_map(|&(a, b)| seg(a, b))
                .collect();
            p.segments(&segs, 2.5, SELECTED);
            if let Some((a, b)) = hover_edge
                && let Some(s) = seg(a, b)
            {
                p.line(s.0, s.1, 3.0, HOVER);
            }
        }
        EditMode::Polygons => {
            let (mut front, mut back, mut outline) = (Vec::new(), Vec::new(), Vec::new());
            for &i in &ed.comp.polys {
                let i = i as usize;
                if i >= mesh.polys.len() {
                    continue;
                }
                if facing[i] {
                    front.extend(tris_of(i))
                } else {
                    back.extend(tris_of(i))
                }
                let poly = &mesh.polys[i];
                for k in 0..poly.len() {
                    if let Some(s) = seg(poly[k], poly[(k + 1) % poly.len()]) {
                        outline.push(s);
                    }
                }
            }
            p.triangles(&back, "rgba(255,150,40,.16)");
            p.triangles(&front, "rgba(255,150,40,.47)");
            p.segments(&outline, 1.5, SELECTED);
            if let Some(i) = hover_poly.filter(|&i| (i as usize) < mesh.polys.len()) {
                p.triangles(&tris_of(i as usize), "rgba(255,225,110,.24)");
            }
        }
        EditMode::Model | EditMode::Object | EditMode::Workplane => {}
    }
}
