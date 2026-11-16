//! The Object Manager: hierarchy tree with selection, rename, visibility and drag-and-drop.

use super::widgets::{MaterialDrag, ObjectDrag};
use crate::editor::Editor;
use crate::deformer::DeformerKind;
use crate::generator::GeneratorKind;
use crate::icons::{self, Icon};
use super::commands::{Cmd, key_label, menu_item as item};
use super::widgets::{header_bar, menu_item, menu_item_enabled};
use crate::primitive::PrimitiveKind;
use crate::scene::{DropTarget, ObjectId, Visibility};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, PointerButton, Rect, Sense, Stroke, Ui, pos2, vec2};

const ROW_H: f32 = 20.0;
const INDENT: f32 = 16.0;
const ENABLE_W: f32 = 22.0;
const DOTS_W: f32 = 16.0;
const TAGS_W: f32 = 44.0;

struct Row {
    id: ObjectId,
    depth: usize,
}

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    toolbar(ui, ed);

    let mut rows = Vec::new();
    collect(ed, None, 0, &mut rows);

    let mut drop: Option<(ObjectId, DropTarget)> = None;
    let mut material_drop: Option<(ObjectId, u64)> = None;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in &rows {
            row_ui(ui, ed, row, &mut drop, &mut material_drop);
        }
        // Empty area below the rows: drop here to move to the top level, click to deselect.
        let rest = ui.allocate_response(vec2(ui.available_width(), ui.available_height().max(30.0)), Sense::click());
        if rest.clicked() {
            ed.clear_selection();
        }
        if let Some(p) = rest.dnd_release_payload::<ObjectDrag>() {
            drop = Some((p.0, DropTarget::Root));
        }
        rest.context_menu(|ui| create_menu(ui, ed));
    });

    if let Some((id, target)) = drop {
        // Dragging a selected row moves the whole selection.
        let moving: Vec<ObjectId> = if ed.is_selected(id) {
            ed.scene.top_level(&ed.selection)
        } else {
            vec![id]
        };
        let mut moved = false;
        for m in moving {
            moved |= ed.scene.move_object(m, target);
        }
        if moved {
            if let DropTarget::Into(parent) = target {
                ed.expanded.insert(parent);
            }
            ed.label("Move in Hierarchy");
        }
    }
    if let Some((obj, mat)) = material_drop {
        let targets = if ed.is_selected(obj) { ed.selection.clone() } else { vec![obj] };
        ed.assign_material(&targets, Some(mat));
    }
}

fn collect(ed: &Editor, parent: Option<ObjectId>, depth: usize, out: &mut Vec<Row>) {
    for &id in ed.scene.children_of(parent) {
        // Objects on a layer with Manager switched off stay out of the list.
        if !ed.scene.in_manager(id) {
            continue;
        }
        out.push(Row { id, depth });
        if ed.expanded.contains(&id) {
            collect(ed, Some(id), depth + 1, out);
        }
    }
}

/// The Object Manager's own menu strip.
fn toolbar(ui: &mut Ui, ed: &mut Editor) {
    header_bar(ui, |ui| {
        ui.menu_button("Create", |ui| create_menu(ui, ed));
        ui.menu_button("Edit", |ui| {
            if menu_item(ui, None, "Select All", &key_label(ui.ctx(), Cmd::SelectAll)).clicked() {
                ed.select_all();
            }
            if menu_item(ui, None, "Deselect All", &key_label(ui.ctx(), Cmd::Cancel)).clicked() {
                ed.clear_selection();
            }
            item(ui, ed, Cmd::InvertSelection);
            item(ui, ed, Cmd::SelectChildren);
            ui.separator();
            if menu_item(ui, Some(Icon::Duplicate), "Duplicate", &key_label(ui.ctx(), Cmd::Duplicate)).clicked() {
                ed.duplicate_selection();
            }
            if menu_item(ui, Some(Icon::Delete), "Delete", &key_label(ui.ctx(), Cmd::Delete)).clicked() {
                ed.delete_selection();
            }
        });
        ui.menu_button("View", |ui| {
            item(ui, ed, Cmd::ExpandAll);
            item(ui, ed, Cmd::CollapseAll);
        });
        ui.menu_button("Object", |ui| object_commands(ui, ed));
        ui.menu_button("Tags", |ui| tags_menu(ui, ed));
    });
}

/// Commands on the selected objects (Object menu and row context menu).
fn object_commands(ui: &mut Ui, ed: &mut Editor) {
    for c in [Cmd::Rename, Cmd::Group, Cmd::MakeEditable, Cmd::Duplicate, Cmd::ToggleEnabled, Cmd::ResetTransform] {
        item(ui, ed, c);
    }
    ui.separator();
    layer_menu(ui, ed);
    ui.separator();
    let has = !ed.selection.is_empty();
    // In the Object Manager, Delete always removes objects (even in component modes).
    if menu_item_enabled(ui, has, Some(Icon::Delete), "Delete", &key_label(ui.ctx(), Cmd::Delete)).clicked() {
        ed.delete_selection();
    }
}

/// Layer > the selection's layer (Layer Manager colours).
pub fn layer_menu(ui: &mut Ui, ed: &mut Editor) {
    let has = !ed.selection.is_empty();
    ui.add_enabled_ui(has, |ui| {
        ui.menu_button("Layer", |ui| {
            let current = ed.active().and_then(|a| ed.scene.get(a)).and_then(|o| o.layer);
            let layers: Vec<(u64, String, [f32; 3])> = ed.scene.layers().iter().map(|l| (l.id, l.name.clone(), l.color)).collect();
            if super::widgets::menu_check(ui, current.is_none(), "No Layer", "").clicked() {
                let sel = ed.selection.clone();
                ed.scene.set_layer(&sel, None);
                ed.label("Set Layer");
            }
            for (lid, name, color) in layers {
                let resp = super::widgets::menu_check(ui, current == Some(lid), &name, "");
                let r = resp.rect;
                ui.painter().rect_filled(Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(9.0, 9.0)), 1.0, layer_color(color));
                if resp.clicked() {
                    let sel = ed.selection.clone();
                    ed.scene.set_layer(&sel, Some(lid));
                    ed.label("Set Layer");
                }
            }
            ui.separator();
            item(ui, ed, Cmd::NewLayerFromSelection);
        });
    });
}

pub fn layer_color(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// Tags: materials are the only tag type so far.
fn tags_menu(ui: &mut Ui, ed: &mut Editor) {
    let has = !ed.selection.is_empty();
    let mats: Vec<(u64, String)> = ed.scene.materials().iter().map(|m| (m.id, m.name.clone())).collect();
    ui.add_enabled_ui(has, |ui| {
        ui.menu_button("Material Tag", |ui| {
            for (mid, name) in mats {
                if menu_item(ui, Some(Icon::Material), &name, "").clicked() {
                    super::commands::queue(ui.ctx(), Cmd::AssignMaterial(mid));
                }
            }
        });
    });
    item(ui, ed, Cmd::RemoveMaterial);
}

/// The light commands, shared by the Light menu and Create > Light.
pub fn light_items(ui: &mut Ui, ed: &Editor) {
    for k in crate::light::LightKind::ALL {
        if k == crate::light::LightKind::Infinite {
            ui.separator();
        }
        item(ui, ed, Cmd::Light(k));
    }
}

pub fn create_menu(ui: &mut Ui, ed: &Editor) {
    ui.menu_button("Object", |ui| {
        for k in PrimitiveKind::ALL {
            item(ui, ed, Cmd::Primitive(k));
        }
        ui.separator();
        item(ui, ed, Cmd::Null);
    });
    ui.menu_button("Light", |ui| light_items(ui, ed));
    ui.menu_button("Spline", |ui| {
        item(ui, ed, Cmd::Pen(crate::pen::PenKind::Spline));
    });
    ui.menu_button("Generator", |ui| {
        for k in GeneratorKind::ALL {
            item(ui, ed, Cmd::Generator(k));
        }
    });
    ui.menu_button("Deformer", |ui| {
        for k in DeformerKind::ALL {
            item(ui, ed, Cmd::Deformer(k));
        }
    });
}

fn row_ui(
    ui: &mut Ui,
    ed: &mut Editor,
    row: &Row,
    drop: &mut Option<(ObjectId, DropTarget)>,
    material_drop: &mut Option<(ObjectId, u64)>,
) {
    let Some(obj) = ed.scene.get(row.id) else { return };
    let id = row.id;
    let has_children = !obj.children.is_empty();
    let name = obj.name.clone();
    let vis_editor = obj.vis_editor;
    let vis_render = obj.vis_render;
    let enabled = obj.enabled;
    let material = obj.material.and_then(|m| ed.scene.material(m)).map(|m| (m.id, m.color));
    let icon = icons::for_kind(&obj.kind);
    let hidden_by_parent = !ed.scene.editor_visible(id) && vis_editor != Visibility::Off;
    let layer = ed.scene.object_layer(id).map(|l| (l.color, l.name.clone()));

    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click_and_drag());
    let selected = ed.is_selected(id);
    let active = ed.active() == Some(id);
    let painter = ui.painter();

    if selected {
        painter.rect_filled(rect, 0.0, if active { theme::SELECT_ACTIVE_BG } else { theme::SELECT_BG });
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::BG_HOVER.gamma_multiply(0.6));
    }

    let x0 = rect.left() + 4.0 + row.depth as f32 * INDENT;
    let cy = rect.center().y;
    // The layer's colour, as a strip on the left edge.
    if let Some((c, lname)) = &layer {
        let strip = Rect::from_min_max(rect.left_top(), pos2(rect.left() + 3.0, rect.bottom()));
        painter.rect_filled(strip, 0.0, layer_color(*c));
        ui.interact(strip.expand2(vec2(2.0, 0.0)), Id::new(("layer-strip", id)), Sense::hover()).on_hover_text(format!("Layer: {lname}"));
    }
    let painter = ui.painter();

    // expand arrow
    let arrow_rect = Rect::from_center_size(pos2(x0 + 6.0, cy), vec2(14.0, 14.0));
    if has_children {
        let open = ed.expanded.contains(&id);
        let c = theme::TEXT_DIM;
        let pts = if open {
            vec![pos2(x0 + 1.0, cy - 2.5), pos2(x0 + 11.0, cy - 2.5), pos2(x0 + 6.0, cy + 3.5)]
        } else {
            vec![pos2(x0 + 3.5, cy - 5.0), pos2(x0 + 3.5, cy + 5.0), pos2(x0 + 9.5, cy)]
        };
        painter.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
    }
    // icon + name
    let icon_rect = Rect::from_center_size(pos2(x0 + 24.0, cy), vec2(15.0, 15.0));
    icons::paint(painter, icon_rect, icon, theme::TEXT);
    let name_x = x0 + 36.0;

    if ed.renaming == Some(id) {
        let edit_rect = Rect::from_min_max(pos2(name_x - 2.0, rect.top() + 1.0), pos2(rect.right() - TAGS_W - DOTS_W - ENABLE_W - 4.0, rect.bottom() - 1.0));
        let mut text = name.clone();
        let edit = ui.put(edit_rect, egui::TextEdit::singleline(&mut text).desired_width(edit_rect.width()));
        if !edit.has_focus() && !edit.lost_focus() {
            edit.request_focus();
        }
        if text != name && !text.trim().is_empty() {
            if let Some(o) = ed.scene.get_mut(id) {
                o.name = text;
            }
            ed.label("Rename");
        }
        if edit.lost_focus() {
            ed.renaming = None;
        }
    } else {
        let color = if selected {
            theme::ACCENT
        } else if hidden_by_parent || vis_editor == Visibility::Off {
            theme::TEXT_DIM
        } else {
            theme::TEXT
        };
        let max_x = rect.right() - TAGS_W - DOTS_W - ENABLE_W - 4.0;
        let clip = Rect::from_min_max(pos2(name_x, rect.top()), pos2(max_x, rect.bottom()));
        painter.with_clip_rect(clip).text(pos2(name_x, cy), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), color);
    }

    // Right-hand columns, as in Cinema 4D: on/off switch (generators and
    // deformers), the two stacked visibility dots, then tags.
    let tags_x = rect.right() - TAGS_W;
    let dots_x = tags_x - DOTS_W;
    let enable_x = dots_x - ENABLE_W;
    let painter = ui.painter();
    for x in [enable_x, dots_x, tags_x] {
        painter.vline(x, rect.y_range(), Stroke::new(1.0, theme::BORDER));
    }
    if ed.scene.can_disable(id) {
        let r = Rect::from_min_max(pos2(enable_x, rect.top()), pos2(dots_x, rect.bottom()));
        let hit = ui.interact(r, Id::new(("enable", id)), Sense::click());
        let c = r.center();
        if enabled {
            ui.painter().add(egui::Shape::line(
                vec![c + vec2(-5.0, 0.0), c + vec2(-1.5, 4.0), c + vec2(5.0, -4.5)],
                Stroke::new(2.0, Color32::from_rgb(100, 200, 90)),
            ));
        } else {
            let st = Stroke::new(2.0, Color32::from_rgb(225, 70, 60));
            ui.painter().line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], st);
            ui.painter().line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], st);
        }
        if hit.clicked() {
            if let Some(o) = ed.scene.get_mut(id) {
                o.enabled = !o.enabled;
            }
            ed.label(if enabled { "Disable" } else { "Enable" });
        }
        hit.on_hover_text(if enabled { "Enabled (click to switch off)" } else { "Switched off (click to enable)" });
    }
    let dot_col = (dots_x + DOTS_W * 0.5).round();
    for (state, is_editor, dy) in [(vis_editor, true, -3.5), (vis_render, false, 3.5)] {
        let dot = pos2(dot_col, cy + dy);
        let color = match state {
            Visibility::Default => Color32::from_gray(128),
            Visibility::On => Color32::from_rgb(110, 205, 90),
            Visibility::Off => Color32::from_rgb(225, 70, 60),
        };
        let r = Rect::from_center_size(dot, vec2(DOTS_W, 9.0));
        let hit = ui.interact(r, Id::new(("vis", id, is_editor)), Sense::click());
        ui.painter().circle_filled(dot, if hit.hovered() { 3.6 } else { 3.0 }, color);
        if hit.clicked() {
            if let Some(o) = ed.scene.get_mut(id) {
                let slot = if is_editor { &mut o.vis_editor } else { &mut o.vis_render };
                *slot = slot.cycled();
            }
            ed.label("Change Visibility");
        }
        hit.on_hover_text(if is_editor {
            "Visible in editor: grey default, green on, red off (click to cycle)"
        } else {
            "Visible in renderer: grey default, green on, red off (click to cycle)"
        });
    }
    if let Some((_, c)) = material {
        // Material tag: a small shaded ball.
        let centre = pos2(tags_x + 12.0, cy);
        let col = Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
        ui.painter().circle_filled(centre, 6.5, col.gamma_multiply(0.75));
        ui.painter().circle_filled(centre + vec2(-1.5, -1.5), 4.5, col);
        ui.painter().circle_filled(centre + vec2(-2.5, -2.5), 1.6, Color32::from_white_alpha(160));
    }

    // ---- interaction ----
    if resp.clicked_by(PointerButton::Primary) {
        let pos = resp.interact_pointer_pos().unwrap_or(rect.center());
        if has_children && arrow_rect.expand(2.0).contains(pos) {
            if !ed.expanded.remove(&id) {
                ed.expanded.insert(id);
            }
        } else {
            let m = ui.input(|i| i.modifiers);
            if m.command || m.shift {
                ed.toggle_selection(id);
            } else {
                ed.select_only(id);
            }
        }
    }
    if resp.double_clicked() {
        ed.renaming = Some(id);
    }
    resp.context_menu(|ui| {
        if !ed.is_selected(id) {
            ed.select_only(id);
        }
        object_commands(ui, ed);
        ui.separator();
        tags_menu(ui, ed);
    });

    if resp.drag_started_by(PointerButton::Primary) && ed.renaming != Some(id) {
        egui::DragAndDrop::set_payload(ui.ctx(), ObjectDrag(id));
    }

    // ---- drop targets ----
    if let Some(p) = egui::DragAndDrop::payload::<ObjectDrag>(ui.ctx())
        && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && rect.contains(pos) {
                let t = (pos.y - rect.top()) / rect.height();
                let target = if t < 0.25 {
                    DropTarget::Before(id)
                } else if t > 0.75 {
                    DropTarget::After(id)
                } else {
                    DropTarget::Into(id)
                };
                let valid = p.0 != id && !ed.scene.is_descendant(id, p.0);
                if valid {
                    let painter = ui.painter();
                    match target {
                        DropTarget::Into(_) => {
                            painter.rect_stroke(rect, 2.0, Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
                        }
                        DropTarget::Before(_) => {
                            painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(2.0, theme::ACCENT));
                        }
                        _ => {
                            painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(2.0, theme::ACCENT));
                        }
                    }
                    if ui.input(|i| i.pointer.any_released()) {
                        *drop = Some((p.0, target));
                    }
                }
            }
    if let Some(p) = egui::DragAndDrop::payload::<MaterialDrag>(ui.ctx())
        && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && rect.contains(pos) {
                ui.painter().rect_stroke(rect, 2.0, Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
                if ui.input(|i| i.pointer.any_released()) {
                    *material_drop = Some((id, p.0));
                }
            }
}
