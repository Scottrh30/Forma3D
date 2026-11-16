//! The 3D view(s): drawing, camera navigation, picking and the transform gizmo.

use super::widgets::MaterialDrag;
use crate::camera::{Camera, ViewKind};
use crate::component::EditMode;
use crate::editor::{Editor, NavMode, ViewLayout};
use crate::gizmo::{self, Axis, CoordSystem, Drag, DragView, GizmoFrame, Handle, Tool};
use crate::picking;
use crate::primitive::PrimitiveKind;
use crate::renderer::{self, CameraUniform, DrawItem, FrameScene, PostUniform, ViewCallback};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use glam::{Vec2, Vec3};
use std::sync::Arc;

pub const HEADER_H: f32 = 22.0;

thread_local! {
    /// Tessellated copies of the scene's meshes (Options > Tessellation).
    static TESSELLATION: std::cell::RefCell<crate::tessellate::Cache> = Default::default();
}

pub fn show(ui: &mut Ui, ed: &mut Editor, target_is_srgb: bool) {
    let whole = ui.available_rect_before_wrap();
    ui.allocate_rect(whole, Sense::hover());
    ui.painter().rect_filled(whole, 0.0, theme::BORDER);
    let (full, timeline_rect) = if ed.show_timeline && whole.height() > 200.0 {
        let split = whole.bottom() - super::timeline::HEIGHT;
        (Rect::from_min_max(whole.min, pos2(whole.right(), split)), Some(Rect::from_min_max(pos2(whole.left(), split), whole.max)))
    } else {
        (whole, None)
    };

    let mut objects = ed.render_objects();
    objects.retain(|o| ed.passes_filter(o.id));
    let fx = ed.view_opts.fx;
    let items: Vec<DrawItem> = TESSELLATION.with_borrow_mut(|cache| {
        let items = objects
            .iter()
            .map(|o| DrawItem {
                mesh: if fx.tessellation && o.kind == renderer::ItemKind::Surface {
                    cache.get(&o.mesh)
                } else {
                    o.mesh.clone()
                },
            model: o.world,
            color: o.color,
                selection: o.selection,
                kind: o.kind,
            })
            .collect();
        cache.end_frame();
        items
    });
    // Objects on locked layers can't be picked.
    let pick_items: Vec<_> = objects.iter().filter(|o| !ed.scene.is_locked(o.id)).map(|o| o.pick_item()).collect();
    let mut scene_frame = FrameScene::new(items, ed.display, fx);
    scene_frame.lights = ed.scene_lights();
    let frame = Arc::new(scene_frame);

    let views: Vec<(ViewKind, Rect)> = match ed.layout {
        ViewLayout::Single => vec![(ed.single_view, full)],
        ViewLayout::Quad => {
            let gap = 2.0;
            let w = (full.width() - gap) * 0.5;
            let h = (full.height() - gap) * 0.5;
            let cell = |col: f32, row: f32| {
                Rect::from_min_size(pos2(full.left() + col * (w + gap), full.top() + row * (h + gap)), vec2(w, h))
            };
            vec![
                (ViewKind::Top, cell(0.0, 0.0)),
                (ViewKind::Perspective, cell(1.0, 0.0)),
                (ViewKind::Right, cell(0.0, 1.0)),
                (ViewKind::Front, cell(1.0, 1.0)),
            ]
        }
    };

    for (kind, rect) in views {
        if rect.width() < 40.0 || rect.height() < HEADER_H + 20.0 {
            continue;
        }
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
        paint_gradient(ui.painter(), body);
        view(ui, ed, kind, body, &frame, &pick_items, &objects, target_is_srgb);
        // Drawn after the 3D view so its menus are on top.
        view_header(ui, ed, kind, header, body);
    }

    if let Some(r) = timeline_rect {
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| super::timeline::show(ui, ed));
    }
}

/// The viewport background: a soft vertical gradient.
fn paint_gradient(painter: &egui::Painter, r: Rect) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), theme::VIEW_TOP);
    mesh.colored_vertex(r.right_top(), theme::VIEW_TOP);
    mesh.colored_vertex(r.right_bottom(), theme::VIEW_BOTTOM);
    mesh.colored_vertex(r.left_bottom(), theme::VIEW_BOTTOM);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Each view's own menu strip (View, Cameras, Display, Options, Filter, Panel)
/// and the drag-to-navigate icons on the right, as in Cinema 4D.
fn view_header(ui: &mut Ui, ed: &mut Editor, kind: ViewKind, header: Rect, body: Rect) {
    use super::commands::{Cmd, menu_item as item};
    use super::widgets::menu_check;
    let ctx = ui.ctx().clone();
    let key = |c: Cmd| super::commands::key_label(&ctx, c);
    ui.painter().rect_filled(header, 0.0, theme::BG_HEADER);
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let nav_w = 4.0 * 20.0 + 6.0;
    let menus = Rect::from_min_max(header.min, pos2(header.right() - nav_w, header.bottom()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(menus).id_salt(("view-menus", kind.index())), |ui| {
        super::widgets::flat_menus(ui);
        ui.horizontal_centered(|ui| {
            // Room for the dock's corner grip.
            ui.add_space(10.0);
            // Registry commands act on the active view, so a click here makes
            // this view active first.
            let mut clicked = false;
            ui.menu_button("View", |ui| {
                for c in [Cmd::FrameSelected, Cmd::FrameAll, Cmd::FrameDefault] {
                    clicked |= item(ui, ed, c);
                }
                ui.separator();
                clicked |= item(ui, ed, Cmd::ToggleSolo);
            });
            ui.menu_button("Cameras", |ui| {
                for k in ViewKind::ALL {
                    clicked |= item(ui, ed, Cmd::Camera(k));
                }
            });
            // Display: what the view shows besides the objects.
            ui.menu_button("Display", |ui| {
                for c in [Cmd::ToggleGrid, Cmd::ToggleCompass, Cmd::ToggleSolo] {
                    clicked |= item(ui, ed, c);
                }
                ui.separator();
                for c in [Cmd::ToggleTimeline, Cmd::ToggleIconBar] {
                    clicked |= item(ui, ed, c);
                }
            });
            // Shading: how surfaces are drawn.
            ui.menu_button("Shading", |ui| {
                for m in renderer::DisplayMode::ALL {
                    if m == renderer::DisplayMode::Quick {
                        ui.separator();
                    }
                    clicked |= item(ui, ed, Cmd::Display(m));
                }
            });
            ui.menu_button("Options", |ui| {
                for fx in renderer::ViewFx::ALL {
                    clicked |= item(ui, ed, Cmd::ViewFx(fx));
                }
            });
            ui.menu_button("Filter", |ui| {
                for f in super::commands::ViewFilter::ALL {
                    clicked |= item(ui, ed, Cmd::Filter(f));
                }
                ui.separator();
                clicked |= item(ui, ed, Cmd::ToggleGrid);
            });
            if clicked {
                ed.active_view = kind;
            }
            ui.menu_button("Panel", |ui| {
                if menu_check(ui, ed.layout == ViewLayout::Single, "Single View", "").clicked() {
                    ed.layout = ViewLayout::Single;
                    ed.single_view = kind;
                }
                if menu_check(ui, ed.layout == ViewLayout::Quad, "4 Views", &key(Cmd::FourViews)).clicked() {
                    ed.layout = ViewLayout::Quad;
                }
            });
        });
    });

    // Navigation icons: drag on them to pan / zoom / orbit; the last one
    // toggles between this view alone and four views.
    use crate::icons::{self, Icon};
    let icons_list = [
        (Icon::NavPan, "Drag to move the view"),
        (Icon::NavZoom, "Drag to zoom the view"),
        (Icon::NavOrbit, "Drag to rotate the view"),
        (Icon::NavMaximize, "Toggle this view / four views"),
    ];
    for (i, (icon, tip)) in icons_list.iter().enumerate() {
        let r = Rect::from_min_size(pos2(header.right() - nav_w + 4.0 + i as f32 * 20.0, header.top() + 2.0), vec2(18.0, 18.0));
        let sense = if *icon == Icon::NavMaximize { Sense::click() } else { Sense::drag() };
        let resp = ui.interact(r, ui.id().with(("nav", kind.index(), i)), sense).on_hover_text(*tip);
        let hot = resp.hovered() || resp.dragged();
        if hot {
            ui.painter().rect_filled(r, 2.0, theme::BG_HOVER);
        }
        icons::paint(ui.painter(), r.shrink(2.0), *icon, if hot { Color32::WHITE } else { theme::TEXT_DIM });
        let d = resp.drag_delta();
        let cam = &mut ed.cameras[kind.index()];
        match icon {
            Icon::NavPan if resp.dragged() => cam.pan(d.x, d.y, body.height()),
            Icon::NavZoom if resp.dragged() => cam.zoom((1.0 + (d.y - d.x) * 0.006).clamp(0.5, 2.0)),
            Icon::NavOrbit if resp.dragged() && !kind.is_ortho() => cam.orbit(d.x, d.y),
            Icon::NavMaximize if resp.clicked() => {
                if ed.layout == ViewLayout::Quad {
                    ed.layout = ViewLayout::Single;
                    ed.single_view = kind;
                } else {
                    ed.layout = ViewLayout::Quad;
                }
                ed.active_view = kind;
            }
            _ => {}
        }
        if resp.dragged() {
            ed.active_view = kind;
        }
    }
}

pub(super) fn to_screen(cam: &Camera, rect: Rect, p: Vec3) -> Option<Vec2> {
    let aspect = rect.width() / rect.height();
    let ndc = cam.project(p, aspect)?;
    Some(Vec2::new(
        rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
        rect.top() + (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height(),
    ))
}

pub(super) fn ray_at(cam: &Camera, rect: Rect, px: Vec2) -> (Vec3, Vec3) {
    let aspect = rect.width() / rect.height();
    let ndc = Vec2::new(
        (px.x - rect.left()) / rect.width() * 2.0 - 1.0,
        1.0 - (px.y - rect.top()) / rect.height() * 2.0,
    );
    cam.ray(ndc, aspect)
}

pub(super) fn p2(v: Vec2) -> Pos2 {
    pos2(v.x, v.y)
}

#[allow(clippy::too_many_arguments)]
fn view(
    ui: &mut Ui,
    ed: &mut Editor,
    kind: ViewKind,
    rect: Rect,
    frame: &Arc<FrameScene>,
    pick_items: &[picking::PickItem],
    objects: &[crate::editor::RenderObject],
    target_is_srgb: bool,
) {
    let slot = kind.index();
    let id = ui.id().with(("view", slot));
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let aspect = rect.width() / rect.height();
    let mods = ui.input(|i| i.modifiers);

    // ---- navigation ------------------------------------------------------
    let delta = resp.drag_delta();
    let nav = if resp.dragged_by(PointerButton::Primary) && mods.alt && !mods.command {
        Some(if kind.is_ortho() { NavMode::Pan } else { NavMode::Orbit })
    } else if resp.dragged_by(PointerButton::Middle) {
        Some(NavMode::Pan)
    } else if resp.dragged_by(PointerButton::Secondary) {
        Some(if mods.alt {
            NavMode::Zoom
        } else if kind.is_ortho() {
            NavMode::Pan
        } else {
            NavMode::Orbit
        })
    } else {
        None
    };
    if let Some(mode) = nav {
        ed.active_view = kind;
        ed.nav = Some(mode);
        let cam = &mut ed.cameras[slot];
        match mode {
            NavMode::Orbit => cam.orbit(delta.x, delta.y),
            NavMode::Pan => cam.pan(delta.x, delta.y, rect.height()),
            NavMode::Zoom => cam.zoom((1.0 - (delta.x - delta.y) * 0.005).clamp(0.5, 2.0)),
        }
    } else if resp.drag_stopped() {
        ed.nav = None;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            ed.cameras[slot].zoom((1.0 - scroll * 0.0025).clamp(0.5, 2.0));
        }
    }
    if resp.is_pointer_button_down_on() || resp.hovered() && ui.input(|i| i.pointer.any_pressed()) {
        ed.active_view = kind;
    }

    let cam = ed.cameras[slot];
    let mouse = resp.hover_pos().or(resp.interact_pointer_pos()).map(|p| Vec2::new(p.x, p.y));

    // ---- draw the scene --------------------------------------------------
    // The Perspective view shows the workplane's grid once it has been moved.
    let wp = ed.scene.workplane;
    let on_workplane = kind == ViewKind::Perspective && !wp.is_default();
    let mut grid = if on_workplane { renderer::build_grid_on(&cam, wp.matrix()) } else { renderer::build_grid(&cam) };
    let grid_unit = grid.unit;
    if !ed.view_opts.grid {
        grid.vertices.clear();
    }
    let mut uniform = CameraUniform::new(&cam, aspect, target_is_srgb, grid.center, grid.radius);
    // The floor catches shadows where it lies flat in view (and the grid shows it).
    let floor = ed.view_opts.grid && !on_workplane && matches!(kind, ViewKind::Perspective | ViewKind::Top);
    let fx = frame.fx;
    uniform.set_lights(&frame.lights, frame.bounds);
    uniform.set_effects(fx, frame.bounds, floor);
    uniform.set_shading(frame.mode);
    let ppp = ui.ctx().pixels_per_point();
    let size_px = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
    ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewCallback {
            slot,
            camera: uniform,
            post: PostUniform::new(&cam, aspect, size_px, fx),
            size_px,
            grid: grid.vertices,
            scene: frame.clone(),
        },
    ));

    let snap_ctx = SnapCtx { objects, grid_unit };
    if ed.pen.is_some() {
        super::pen::interact(ui, ed, kind, rect, &resp, &cam, mods, nav.is_some(), &snap_ctx);
        view_chrome(&ui.painter_at(rect), ed, kind, rect, &cam);
        return;
    }
    if ed.mode.is_component() {
        super::components::interact(ui, ed, kind, rect, &resp, &cam, mods, nav.is_some(), &snap_ctx);
        view_chrome(&ui.painter_at(rect), ed, kind, rect, &cam);
        return;
    }

    // ---- gizmo -----------------------------------------------------------
    let workplane_mode = ed.mode == EditMode::Workplane;
    let active = ed.active().filter(|id| !ed.scene.is_locked(*id));
    let gizmo_frame = if workplane_mode {
        (ed.tool != Tool::Select).then(|| GizmoFrame::workplane(&wp, ed.coord == CoordSystem::Object, cam.world_per_pixel(wp.pos, rect.height())))
    } else {
        active.filter(|_| ed.tool != Tool::Select).map(|id| {
            let origin = ed.scene.world_position(id);
            let wpp = cam.world_per_pixel(origin, rect.height());
            GizmoFrame::new(&ed.scene, id, ed.coord == CoordSystem::Object, wpp)
        })
    };
    let project = |p: Vec3| to_screen(&cam, rect, p);
    let mut hot: Option<Handle> = None;
    if let (Some(gf), Some(m)) = (&gizmo_frame, mouse)
        && ed.drag.is_none() && nav.is_none() && !mods.alt {
            hot = gizmo::hit_test(ed.tool, gf, &project, m, ed.axes);
        }
    if let Some(d) = &ed.drag
        && ed.active_view == kind {
            hot = Some(d.handle);
        }

    // begin a drag
    let tweak_tool = if ed.tool == Tool::Select { Tool::Move } else { ed.tool };
    if resp.drag_started_by(PointerButton::Primary) && !mods.alt && (ed.tool != Tool::Select || (ed.tweak && !workplane_mode))
        && let Some(start) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
            let start = Vec2::new(start.x, start.y);
            let ray = ray_at(&cam, rect, start);
            let view = DragView { forward: cam.forward(), right: cam.right(), up: cam.up() };
            let on_handle = gizmo_frame.and_then(|gf| gizmo::hit_test(ed.tool, &gf, &project, start, ed.axes));
            if workplane_mode {
                if let Some(gf) = gizmo_frame {
                    ed.drag = Drag::begin_workplane(&ed.scene, ed.tool, on_handle.unwrap_or(Handle::Free), gf, view, ed.axes, ray, start);
                }
            } else {
                // Tweak: the object under the cursor, unless a handle was grabbed.
                let (target, tool, frame) = if ed.tweak && on_handle.is_none() {
                    match picking::pick(pick_items, ray.0, ray.1).map(|(id, _)| id) {
                        Some(id) => {
                            if !ed.is_selected(id) {
                                ed.select_only(id);
                            } else {
                                // Make it the active object, keeping the rest selected.
                                ed.selection.retain(|&s| s != id);
                                ed.selection.push(id);
                            }
                            let origin = ed.scene.world_position(id);
                            let gf = GizmoFrame::new(&ed.scene, id, ed.coord == CoordSystem::Object, cam.world_per_pixel(origin, rect.height()));
                            (Some(id), tweak_tool, Some(gf))
                        }
                        None => (None, ed.tool, None),
                    }
                } else {
                    (active.filter(|_| ed.tool != Tool::Select), ed.tool, gizmo_frame)
                };
                if let (Some(a), Some(gf)) = (target, frame)
                    && (!ed.axis_mode || ed.begin_axis_drag(a)) {
                        ed.drag = Drag::begin(&ed.scene, a, tool, on_handle.unwrap_or(Handle::Free), gf, view, ed.axes, ray, start);
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
                let what = if d.workplane { " Workplane" } else if ed.axis_mode { " Axis" } else { "" };
                ed.label(match d.tool {
                    Tool::Move => format!("Move{what}"),
                    Tool::Rotate => format!("Rotate{what}"),
                    Tool::Scale => format!("Scale{what}"),
                    Tool::Select => "Edit".to_owned(),
                });
            }
        }
    // continue / finish a drag
    if ed.drag.is_some() && ed.active_view == kind {
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(d), Some(p)) = (ed.drag.clone(), ui.input(|i| i.pointer.interact_pos())) {
                let m = Vec2::new(p.x, p.y);
                let ray = ray_at(&cam, rect, m);
                d.update(&mut ed.scene, ray, m);
                // Snapping: a moved object's axis lands on what is near the cursor.
                ed.snap_hit = None;
                if d.tool == Tool::Move && ed.snap.enabled {
                    let skip: std::collections::HashSet<u64> =
                        if d.workplane { Default::default() } else { ed.scene.subtree(d.id).into_iter().collect() };
                    // In Axis mode the object's geometry stays put, so its points are fair game.
                    let own = (ed.axis_mode && !d.workplane).then_some(d.id);
                    let sources = snap_sources(objects, |id| !skip.contains(&id) || Some(id) == own, own);
                    let q = snap_query(&ed.scene, &cam, kind, &project, m, ray, grid_unit, d.start_world_pos(), &[]);
                    if let Some(hit) = crate::snap::find(&ed.snap, &sources, &q) {
                        d.move_to(&mut ed.scene, hit.pos);
                        ed.snap_hit = Some(hit);
                    }
                }
                ed.apply_axis_compensation();
            }
        } else if !ui.input(|i| i.pointer.primary_down()) {
            ed.finish_drag();
        }
    }

    // ---- picking ---------------------------------------------------------
    if resp.clicked_by(PointerButton::Primary) && !mods.alt
        && let Some(p) = resp.interact_pointer_pos() {
            let (ro, rd) = ray_at(&cam, rect, Vec2::new(p.x, p.y));
            let hit = picking::pick(pick_items, ro, rd).map(|(id, _)| id);
            match (hit, mods.command || mods.shift) {
                (Some(id), true) => ed.toggle_selection(id),
                (Some(id), false) => ed.select_only(id),
                (None, false) => ed.clear_selection(),
                (None, true) => {}
            }
        }

    // material dropped onto an object
    if let Some(payload) = resp.dnd_release_payload::<MaterialDrag>()
        && let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            let (ro, rd) = ray_at(&cam, rect, Vec2::new(p.x, p.y));
            if let Some((id, _)) = picking::pick(pick_items, ro, rd) {
                ed.assign_material(&[id], Some(payload.0));
                ed.status = "Material assigned".to_owned();
            }
        }

    resp.context_menu(|ui| {
        ui.label(egui::RichText::new("Create").color(theme::TEXT_DIM));
        for k in PrimitiveKind::ALL {
            if ui.button(k.name()).clicked() {
                ed.create_primitive(k);
                ui.close();
            }
        }
        if ui.button("Null").clicked() {
            ed.create_null();
            ui.close();
        }
        ui.separator();
        ui.add_enabled_ui(!ed.selection.is_empty(), |ui| {
            if ui.button("Delete").clicked() {
                ed.delete_selection();
                ui.close();
            }
            if ui.button("Duplicate").clicked() {
                ed.duplicate_selection();
                ui.close();
            }
        });
    });

    // ---- overlays --------------------------------------------------------
    let painter = ui.painter_at(rect);
    if let Some(gf) = &gizmo_frame {
        draw_gizmo(&painter, ed.tool, gf, &project, ed.axes, hot);
        if workplane_mode {
            painter.text(
                p2(project(gf.origin).unwrap_or_default()) + vec2(10.0, 10.0),
                Align2::LEFT_TOP,
                "Workplane",
                FontId::proportional(11.5),
                theme::GIZMO_HOT,
            );
        } else if ed.axis_mode
            && let Some(o) = project(gf.origin) {
                painter.circle_stroke(p2(o), 9.0, Stroke::new(1.5, theme::GIZMO_HOT));
            }
    }
    if ed.drag.is_some() && ed.active_view == kind {
        draw_snap(&painter, ed.snap_hit, &project);
    }
    view_chrome(&painter, ed, kind, rect, &cam);
}

/// What the pens and component tools need for snapping.
pub(super) struct SnapCtx<'a> {
    pub objects: &'a [crate::editor::RenderObject],
    pub grid_unit: f32,
}

/// The visible objects as snap targets. `keep` filters by id; `no_origin`
/// is an object whose own origin must not attract (it is the one moving).
pub(super) fn snap_sources<'a>(
    objects: &'a [crate::editor::RenderObject],
    keep: impl Fn(u64) -> bool,
    no_origin: Option<u64>,
) -> Vec<crate::snap::Source<'a>> {
    objects
        .iter()
        .filter(|o| keep(o.id))
        .map(|o| crate::snap::Source {
            world: o.world,
            mesh: &o.mesh,
            surface: o.kind == renderer::ItemKind::Surface,
            origin: Some(o.id) != no_origin,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn snap_query<'a>(
    scene: &crate::scene::Scene,
    cam: &Camera,
    kind: ViewKind,
    project: &'a dyn Fn(Vec3) -> Option<Vec2>,
    mouse: Vec2,
    ray: (Vec3, Vec3),
    grid_unit: f32,
    start: Vec3,
    extra_points: &'a [Vec3],
) -> crate::snap::Query<'a> {
    crate::snap::Query {
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
pub(super) fn draw_snap(painter: &egui::Painter, hit: Option<crate::snap::SnapHit>, project: &dyn Fn(Vec3) -> Option<Vec2>) {
    let Some(hit) = hit else { return };
    let Some(at) = project(hit.pos) else { return };
    let c = Color32::from_rgb(90, 220, 255);
    painter.circle_stroke(p2(at), 7.0, Stroke::new(2.0, c));
    painter.circle_filled(p2(at), 2.5, c);
    painter.text(p2(at) + vec2(10.0, -10.0), Align2::LEFT_BOTTOM, hit.kind.label(), FontId::proportional(11.5), c);
}

/// View name, axis compass and border, drawn on top of everything.
fn view_chrome(painter: &egui::Painter, ed: &Editor, kind: ViewKind, rect: Rect, cam: &Camera) {
    if ed.view_opts.compass {
        draw_axis_indicator(painter, cam, rect);
    }
    let mut label = if ed.mode == crate::component::EditMode::Model {
        kind.label().to_owned()
    } else {
        format!("{}  ·  {} mode", kind.label(), ed.mode.label())
    };
    for (on, name) in [(ed.axis_mode, "Axis"), (ed.tweak, "Tweak"), (ed.snap.enabled, "Snap"), (ed.quantize.enabled, "Quantize")] {
        if on {
            label.push_str("  ·  ");
            label.push_str(name);
        }
    }
    painter.text(rect.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, label, FontId::proportional(12.5), Color32::from_gray(225));
    let border = if ed.layout == ViewLayout::Quad && ed.active_view == kind {
        Stroke::new(2.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    painter.rect_stroke(rect, 0.0, border, egui::StrokeKind::Inside);
}

pub(super) fn draw_gizmo(
    painter: &egui::Painter,
    tool: Tool,
    gf: &GizmoFrame,
    project: &dyn Fn(Vec3) -> Option<Vec2>,
    enabled: gizmo::AxisEnabled,
    hot: Option<Handle>,
) {
    let Some(origin) = project(gf.origin) else { return };
    for a in Axis::ALL {
        if !enabled.get(a) {
            continue;
        }
        let is_hot = hot == Some(Handle::Axis(a));
        let color = if is_hot { theme::GIZMO_HOT } else { theme::axis_color(a.index()) };
        let width = if is_hot { 3.0 } else { 2.0 };
        match tool {
            Tool::Move | Tool::Scale => {
                let Some(tip) = project(gf.origin + gf.axis(a) * gf.axis_len()) else { continue };
                painter.line_segment([p2(origin), p2(tip)], Stroke::new(width, color));
                let dir = (tip - origin).normalize_or_zero();
                if tool == Tool::Move {
                    let n = Vec2::new(-dir.y, dir.x);
                    let base = tip - dir * 14.0;
                    painter.add(Shape::convex_polygon(
                        vec![p2(tip + dir * 4.0), p2(base + n * 5.0), p2(base - n * 5.0)],
                        color,
                        Stroke::NONE,
                    ));
                } else {
                    painter.rect_filled(Rect::from_center_size(p2(tip), vec2(9.0, 9.0)), 1.0, color);
                }
            }
            Tool::Rotate => {
                let pts: Vec<Option<Vec2>> = gf.ring_points(a).into_iter().map(project).collect();
                for w in pts.windows(2) {
                    if let [Some(p), Some(q)] = w {
                        painter.line_segment([p2(*p), p2(*q)], Stroke::new(width, color));
                    }
                }
            }
            Tool::Select => {}
        }
    }
    painter.circle_filled(p2(origin), 3.5, theme::TEXT);
}

/// The little XYZ compass in the corner of each view.
fn draw_axis_indicator(painter: &egui::Painter, cam: &Camera, rect: Rect) {
    let center = rect.left_bottom() + vec2(40.0, -40.0);
    let (right, up) = (cam.right(), cam.up());
    let mut axes: Vec<(f32, usize, Vec2)> = (0..3)
        .map(|i| {
            let dir = [Vec3::X, Vec3::Y, Vec3::Z][i];
            (dir.dot(cam.forward()), i, Vec2::new(dir.dot(right), -dir.dot(up)))
        })
        .collect();
    // Far axes first so near ones draw on top.
    axes.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, i, d) in axes {
        let tip = center + vec2(d.x, d.y) * 22.0;
        let color = theme::axis_color(i);
        painter.line_segment([center, tip], Stroke::new(2.0, color));
        painter.circle_filled(tip, 3.0, color);
        painter.text(
            tip + vec2(d.x, d.y) * 8.0,
            Align2::CENTER_CENTER,
            ["X", "Y", "Z"][i],
            FontId::proportional(10.0),
            Color32::from_gray(210),
        );
    }
}
