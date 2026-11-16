//! The viewport tools in Model and Object mode (and Workplane mode): picking,
//! the transform gizmo, Tweak, Axis mode and snapping; plus what every view
//! draws on top (its name, the axis compass, the snap marker).

use crate::vinput::{ACCENT, Anchor, HOT, Input, Painter, TEXT, View, axis_color};
use forma3d_core::camera::{Camera, ViewKind};
use forma3d_core::component::EditMode;
use forma3d_core::editor::{Editor, RenderObject};
use forma3d_core::gizmo::{
    self, Axis, AxisEnabled, CoordSystem, Drag, DragView, GizmoFrame, Handle, Tool,
};
use forma3d_core::picking::{self, PickItem};
use forma3d_core::renderer::ItemKind;
use glam::{Vec2, Vec3};

/// What the tools need for snapping.
pub struct SnapCtx<'a> {
    pub objects: &'a [RenderObject],
    pub grid_unit: f32,
}

/// The visible objects as snap targets. `keep` filters by id; `no_origin` is
/// an object whose own origin must not attract (it is the one moving).
pub fn snap_sources<'a>(
    objects: &'a [RenderObject],
    keep: impl Fn(u64) -> bool,
    no_origin: Option<u64>,
) -> Vec<forma3d_core::snap::Source<'a>> {
    objects
        .iter()
        .filter(|o| keep(o.id))
        .map(|o| forma3d_core::snap::Source {
            world: o.world,
            mesh: &o.mesh,
            surface: o.kind == ItemKind::Surface,
            origin: Some(o.id) != no_origin,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn snap_query<'a>(
    scene: &forma3d_core::scene::Scene,
    cam: &Camera,
    kind: ViewKind,
    project: &'a dyn Fn(Vec3) -> Option<Vec2>,
    mouse: Vec2,
    ray: (Vec3, Vec3),
    grid_unit: f32,
    start: Vec3,
    extra_points: &'a [Vec3],
) -> forma3d_core::snap::Query<'a> {
    forma3d_core::snap::Query {
        project,
        mouse,
        ray,
        ortho_forward: kind.is_ortho().then(|| cam.forward()),
        workplane: scene.workplane.matrix(),
        grid_unit,
        start,
        extra_points,
    }
}

/// The snap marker: a ring on the target with what it snapped to.
pub fn draw_snap(
    p: &mut Painter,
    hit: Option<forma3d_core::snap::SnapHit>,
    project: &dyn Fn(Vec3) -> Option<Vec2>,
) {
    let Some(hit) = hit else { return };
    let Some(at) = project(hit.pos) else { return };
    let c = "rgb(90,220,255)";
    p.ring(at, 7.0, 2.0, c);
    p.circle(at, 2.5, c);
    p.text(
        at + Vec2::new(10.0, -10.0),
        Anchor::LeftBottom,
        hit.kind.label(),
        11.5,
        c,
    );
}

pub fn draw_gizmo(
    p: &mut Painter,
    tool: Tool,
    gf: &GizmoFrame,
    project: &dyn Fn(Vec3) -> Option<Vec2>,
    enabled: AxisEnabled,
    hot: Option<Handle>,
) {
    let Some(o) = project(gf.origin) else { return };
    for a in Axis::ALL {
        if !enabled.get(a) {
            continue;
        }
        let is_hot = hot == Some(Handle::Axis(a));
        let col = if is_hot { HOT } else { axis_color(a.index()) };
        let w = if is_hot { 3.0 } else { 2.0 };
        match tool {
            Tool::Move | Tool::Scale => {
                let Some(tip) = project(gf.origin + gf.axis(a) * gf.axis_len()) else {
                    continue;
                };
                p.line(o, tip, w, col);
                let dir = (tip - o).normalize_or_zero();
                if tool == Tool::Move {
                    let n = Vec2::new(-dir.y, dir.x);
                    let base = tip - dir * 14.0;
                    p.poly(&[tip + dir * 4.0, base + n * 5.5, base - n * 5.5], col);
                } else {
                    p.square(tip, 4.5, col);
                }
            }
            Tool::Rotate => {
                let pts: Vec<Option<Vec2>> = gf.ring_points(a).into_iter().map(project).collect();
                p.path(&pts, w, col);
            }
            Tool::Select => {}
        }
    }
    p.circle(o, 3.5, TEXT);
}

/// The view's name and modes, the axis compass and the border.
pub fn view_chrome(p: &mut Painter, ed: &Editor, v: &View, active: bool, quad: bool) {
    if ed.view_opts.compass {
        let c = Vec2::new(40.0, v.size.y - 40.0);
        let cam = &v.cam;
        let mut axes: Vec<(f32, usize, Vec2)> = (0..3)
            .map(|i| {
                let dir = [Vec3::X, Vec3::Y, Vec3::Z][i];
                (
                    dir.dot(cam.forward()),
                    i,
                    Vec2::new(dir.dot(cam.right()), -dir.dot(cam.up())),
                )
            })
            .collect();
        axes.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, i, d) in axes {
            let tip = c + d * 22.0;
            p.line(c, tip, 2.0, axis_color(i));
            p.circle(tip, 3.0, axis_color(i));
            p.text(
                tip + d * 9.0,
                Anchor::CenterCenter,
                ["X", "Y", "Z"][i],
                10.0,
                "#d2d5db",
            );
        }
    }
    let mut label = if ed.mode == EditMode::Model {
        v.kind.label().to_owned()
    } else {
        format!("{}  ·  {} mode", v.kind.label(), ed.mode.label())
    };
    for (on, name) in [
        (ed.axis_mode, "Axis"),
        (ed.tweak, "Tweak"),
        (ed.snap.enabled, "Snap"),
        (ed.quantize.enabled, "Quantize"),
        (ed.view_opts.solo, "Solo"),
    ] {
        if on {
            label.push_str("  ·  ");
            label.push_str(name);
        }
    }
    p.text(
        Vec2::new(10.0, 9.0),
        Anchor::LeftTop,
        &label,
        12.5,
        "#e1e3e8",
    );
    if quad && active {
        p.rect(Vec2::splat(1.0), v.size - Vec2::splat(1.0), "none", ACCENT);
    }
}

/// Model, Object and Workplane modes: the gizmo, Tweak, picking and snapping.
pub fn object_mode(
    ed: &mut Editor,
    v: &View,
    inp: &Input,
    pick_items: &[PickItem],
    snap: &SnapCtx,
    p: &mut Painter,
) {
    let kind = v.kind;
    let cam = v.cam;
    let mods = inp.mods;
    let project = |q: Vec3| v.project(q);
    let wp = ed.scene.workplane;
    let workplane_mode = ed.mode == EditMode::Workplane;
    let active = ed.active().filter(|id| !ed.scene.is_locked(*id));
    let gizmo_frame = if workplane_mode {
        (ed.tool != Tool::Select).then(|| {
            GizmoFrame::workplane(
                &wp,
                ed.coord == CoordSystem::Object,
                v.world_per_pixel(wp.pos),
            )
        })
    } else {
        active.filter(|_| ed.tool != Tool::Select).map(|id| {
            let origin = ed.scene.world_position(id);
            GizmoFrame::new(
                &ed.scene,
                id,
                ed.coord == CoordSystem::Object,
                v.world_per_pixel(origin),
            )
        })
    };
    let mut hot: Option<Handle> = None;
    if let (Some(gf), Some(m)) = (&gizmo_frame, inp.mouse)
        && ed.drag.is_none()
        && !inp.navigating
        && !mods.alt
    {
        hot = gizmo::hit_test(ed.tool, gf, &project, m, ed.axes);
    }
    if let Some(d) = &ed.drag
        && ed.active_view == kind
    {
        hot = Some(d.handle);
    }

    // Begin a drag.
    let tweak_tool = if ed.tool == Tool::Select {
        Tool::Move
    } else {
        ed.tool
    };
    if inp.drag_started
        && !mods.alt
        && (ed.tool != Tool::Select || (ed.tweak && !workplane_mode))
        && let Some(start) = inp.press_origin.or(inp.mouse)
    {
        let ray = v.ray(start);
        let view = DragView {
            forward: cam.forward(),
            right: cam.right(),
            up: cam.up(),
        };
        let on_handle =
            gizmo_frame.and_then(|gf| gizmo::hit_test(ed.tool, &gf, &project, start, ed.axes));
        if workplane_mode {
            if let Some(gf) = gizmo_frame {
                ed.drag = Drag::begin_workplane(
                    &ed.scene,
                    ed.tool,
                    on_handle.unwrap_or(Handle::Free),
                    gf,
                    view,
                    ed.axes,
                    ray,
                    start,
                );
            }
        } else {
            // Tweak: the object under the cursor, unless a handle was grabbed.
            let (target, tool, frame) = if ed.tweak && on_handle.is_none() {
                match picking::pick(pick_items, ray.0, ray.1).map(|(id, _)| id) {
                    Some(id) => {
                        if !ed.is_selected(id) {
                            ed.select_only(id);
                        } else {
                            ed.selection.retain(|&s| s != id);
                            ed.selection.push(id);
                        }
                        let origin = ed.scene.world_position(id);
                        let gf = GizmoFrame::new(
                            &ed.scene,
                            id,
                            ed.coord == CoordSystem::Object,
                            v.world_per_pixel(origin),
                        );
                        (Some(id), tweak_tool, Some(gf))
                    }
                    None => (None, ed.tool, None),
                }
            } else {
                (
                    active.filter(|_| ed.tool != Tool::Select),
                    ed.tool,
                    gizmo_frame,
                )
            };
            if let (Some(a), Some(gf)) = (target, frame)
                && (!ed.axis_mode || ed.begin_axis_drag(a))
            {
                ed.drag = Drag::begin(
                    &ed.scene,
                    a,
                    tool,
                    on_handle.unwrap_or(Handle::Free),
                    gf,
                    view,
                    ed.axes,
                    ray,
                    start,
                );
                if ed.drag.is_none() {
                    ed.axis_base = None;
                }
            }
        }
        let q = ed.quantize;
        if let Some(d) = ed.drag.as_mut() {
            d.quantize = q;
        }
        if let Some(d) = ed.drag.clone() {
            ed.active_view = kind;
            let what = if d.workplane {
                " Workplane"
            } else if ed.axis_mode {
                " Axis"
            } else {
                ""
            };
            ed.label(match d.tool {
                Tool::Move => format!("Move{what}"),
                Tool::Rotate => format!("Rotate{what}"),
                Tool::Scale => format!("Scale{what}"),
                Tool::Select => "Edit".to_owned(),
            });
        }
    }
    // Continue or finish a drag.
    if ed.drag.is_some() && ed.active_view == kind {
        if inp.dragged {
            if let (Some(d), Some(m)) = (ed.drag.clone(), inp.mouse) {
                let ray = v.ray(m);
                d.update(&mut ed.scene, ray, m);
                ed.snap_hit = None;
                if d.tool == Tool::Move && ed.snap.enabled {
                    let skip: std::collections::HashSet<u64> = if d.workplane {
                        Default::default()
                    } else {
                        ed.scene.subtree(d.id).into_iter().collect()
                    };
                    // In Axis mode the object's geometry stays put, so its points are fair game.
                    let own = (ed.axis_mode && !d.workplane).then_some(d.id);
                    let sources = snap_sources(
                        snap.objects,
                        |id| !skip.contains(&id) || Some(id) == own,
                        own,
                    );
                    let q = snap_query(
                        &ed.scene,
                        &cam,
                        kind,
                        &project,
                        m,
                        ray,
                        snap.grid_unit,
                        d.start_world_pos(),
                        &[],
                    );
                    if let Some(hit) = forma3d_core::snap::find(&ed.snap, &sources, &q) {
                        d.move_to(&mut ed.scene, hit.pos);
                        ed.snap_hit = Some(hit);
                    }
                }
                ed.apply_axis_compensation();
            }
        } else if !inp.primary_down {
            ed.finish_drag();
        }
    }

    // Picking.
    if inp.clicked
        && !mods.alt
        && let Some(m) = inp.mouse
    {
        let (ro, rd) = v.ray(m);
        let hit = picking::pick(pick_items, ro, rd).map(|(id, _)| id);
        match (hit, mods.ctrl || mods.shift) {
            (Some(id), true) => ed.toggle_selection(id),
            (Some(id), false) => ed.select_only(id),
            (None, false) => ed.clear_selection(),
            (None, true) => {}
        }
        ed.active_view = kind;
    }

    // Overlay.
    if let Some(gf) = &gizmo_frame {
        draw_gizmo(p, ed.tool, gf, &project, ed.axes, hot);
        if workplane_mode {
            if let Some(o) = project(gf.origin) {
                p.text(
                    o + Vec2::new(10.0, 10.0),
                    Anchor::LeftTop,
                    "Workplane",
                    11.5,
                    HOT,
                );
            }
        } else if ed.axis_mode
            && let Some(o) = project(gf.origin)
        {
            p.ring(o, 9.0, 1.5, HOT);
        }
    }
    if ed.drag.is_some() && ed.active_view == kind {
        draw_snap(p, ed.snap_hit, &project);
    }
}
