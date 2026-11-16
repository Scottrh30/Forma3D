//! The Layer Manager: layers with a colour and Solo / View / Render /
//! Manager / Locked switches. Drag objects from the Object Manager onto a
//! layer to put them on it.

use super::commands::{Cmd, menu_item as item};
use super::objects::layer_color;
use super::widgets::{ObjectDrag, header_bar, menu_item_enabled};
use crate::editor::Editor;
use crate::icons::Icon;
use crate::layers::{LayerFlag, LayerId};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

const ROW_H: f32 = 22.0;
const FLAG_W: f32 = 22.0;
const COUNT_W: f32 = 34.0;

fn selected_id() -> Id {
    Id::new("layer-manager-selected")
}

fn renaming_id() -> Id {
    Id::new("layer-manager-renaming")
}

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    let mut selected: Option<LayerId> = ui.data(|d| d.get_temp(selected_id())).filter(|l| ed.scene.layer(*l).is_some());
    let mut renaming: Option<LayerId> = ui.data(|d| d.get_temp(renaming_id()));

    header_bar(ui, |ui| {
        ui.menu_button("Layer", |ui| {
            if super::widgets::menu_item(ui, Some(Icon::Layers), "New Layer", "").clicked() {
                selected = Some(ed.scene.add_layer("Layer"));
                ed.label("New Layer");
            }
            item(ui, ed, Cmd::NewLayerFromSelection);
            ui.separator();
            if menu_item_enabled(ui, selected.is_some(), None, "Add Selected Objects to Layer", "").clicked()
                && let Some(l) = selected
            {
                let sel = ed.selection.clone();
                ed.scene.set_layer(&sel, Some(l));
                ed.label("Set Layer");
            }
            if menu_item_enabled(ui, selected.is_some(), None, "Select Objects on Layer", "").clicked()
                && let Some(l) = selected
            {
                ed.selection = ed.scene.layer_members(l);
            }
            ui.separator();
            if menu_item_enabled(ui, selected.is_some(), None, "Rename Layer", "").clicked() {
                renaming = selected;
            }
            if menu_item_enabled(ui, selected.is_some(), Some(Icon::Delete), "Delete Layer", "").clicked()
                && let Some(l) = selected.take()
            {
                ed.scene.remove_layer(l);
                ed.label("Delete Layer");
            }
        });
        ui.menu_button("Edit", |ui| {
            let any_solo = ed.scene.layers().iter().any(|l| l.solo);
            if menu_item_enabled(ui, any_solo, None, "Solo Off (all layers)", "").clicked() {
                let ids: Vec<LayerId> = ed.scene.layers().iter().map(|l| l.id).collect();
                for id in ids {
                    if let Some(l) = ed.scene.layer_mut(id) {
                        l.solo = false;
                    }
                }
                ed.label("Solo Off");
            }
            for flag in [LayerFlag::View, LayerFlag::Render, LayerFlag::Manager] {
                if super::widgets::menu_item(ui, None, &format!("{} On (all layers)", flag.label()), "").clicked() {
                    let ids: Vec<LayerId> = ed.scene.layers().iter().map(|l| l.id).collect();
                    for id in ids {
                        if let Some(l) = ed.scene.layer_mut(id) {
                            flag.set(l, true);
                        }
                    }
                    ed.label("Layer Switches");
                }
            }
            if super::widgets::menu_item(ui, None, "Unlock All", "").clicked() {
                let ids: Vec<LayerId> = ed.scene.layers().iter().map(|l| l.id).collect();
                for id in ids {
                    if let Some(l) = ed.scene.layer_mut(id) {
                        l.locked = false;
                    }
                }
                ed.label("Unlock Layers");
            }
        });
    });

    // Column headings.
    let width = ui.available_width();
    let (head, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let flags_x = head.right() - COUNT_W - FLAG_W * LayerFlag::ALL.len() as f32;
    ui.painter().text(pos2(head.left() + 34.0, head.center().y), Align2::LEFT_CENTER, "Name", FontId::proportional(11.5), theme::TEXT_DIM);
    for (i, f) in LayerFlag::ALL.iter().enumerate() {
        let r = Rect::from_min_size(pos2(flags_x + i as f32 * FLAG_W, head.top()), vec2(FLAG_W, head.height()));
        ui.painter().text(r.center(), Align2::CENTER_CENTER, f.letter(), FontId::proportional(11.5), theme::TEXT_DIM);
        ui.interact(r, Id::new(("layer-head", i)), Sense::hover()).on_hover_text(f.tip());
    }
    ui.painter().text(pos2(head.right() - COUNT_W * 0.5, head.center().y), Align2::CENTER_CENTER, "#", FontId::proportional(11.5), theme::TEXT_DIM);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));

    let layers: Vec<LayerId> = ed.scene.layers().iter().map(|l| l.id).collect();
    let mut dropped: Option<(LayerId, u64)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if layers.is_empty() {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.colored_label(theme::TEXT_DIM, "No layers yet.");
                ui.colored_label(theme::TEXT_DIM, "Layer > New Layer, or select objects and use Add to New Layer.");
            });
        }
        for lid in layers {
            row(ui, ed, lid, &mut selected, &mut renaming, &mut dropped);
        }
        let rest = ui.allocate_response(vec2(ui.available_width(), ui.available_height().max(20.0)), Sense::click());
        if rest.clicked() {
            selected = None;
        }
        rest.context_menu(|ui| {
            if super::widgets::menu_item(ui, Some(Icon::Layers), "New Layer", "").clicked() {
                selected = Some(ed.scene.add_layer("Layer"));
                ed.label("New Layer");
            }
            item(ui, ed, Cmd::NewLayerFromSelection);
        });
    });
    if let Some((lid, obj)) = dropped {
        let targets = if ed.is_selected(obj) { ed.selection.clone() } else { vec![obj] };
        ed.scene.set_layer(&targets, Some(lid));
        ed.label("Set Layer");
    }
    ui.data_mut(|d| {
        d.insert_temp(selected_id(), selected);
        d.insert_temp(renaming_id(), renaming);
    });
}

fn row(ui: &mut Ui, ed: &mut Editor, lid: LayerId, selected: &mut Option<LayerId>, renaming: &mut Option<LayerId>, dropped: &mut Option<(LayerId, u64)>) {
    let Some(layer) = ed.scene.layer(lid).cloned() else { return };
    let count = ed.scene.layer_members(lid).len();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let is_sel = *selected == Some(lid);
    if is_sel {
        ui.painter().rect_filled(rect, 0.0, theme::SELECT_ACTIVE_BG);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::BG_HOVER.gamma_multiply(0.6));
    }
    let cy = rect.center().y;
    let flags_x = rect.right() - COUNT_W - FLAG_W * LayerFlag::ALL.len() as f32;

    // Colour swatch: click to change.
    let swatch = Rect::from_center_size(pos2(rect.left() + 16.0, cy), vec2(22.0, 14.0));
    let mut rgb = layer.color;
    let sw = ui
        .scope_builder(egui::UiBuilder::new().max_rect(swatch), |ui| {
            ui.spacing_mut().interact_size = vec2(22.0, 14.0);
            ui.color_edit_button_rgb(&mut rgb)
        })
        .inner
        .on_hover_text("Layer colour (click to change)");
    if sw.changed() {
        if let Some(l) = ed.scene.layer_mut(lid) {
            l.color = rgb;
        }
        ed.label("Layer Colour");
    }

    // Name (double-click to rename).
    let name_rect = Rect::from_min_max(pos2(rect.left() + 34.0, rect.top() + 1.0), pos2(flags_x - 4.0, rect.bottom() - 1.0));
    if *renaming == Some(lid) {
        let mut text = layer.name.clone();
        let edit = ui.put(name_rect, egui::TextEdit::singleline(&mut text).desired_width(name_rect.width()));
        if !edit.has_focus() && !edit.lost_focus() {
            edit.request_focus();
        }
        if text != layer.name && !text.trim().is_empty() {
            if let Some(l) = ed.scene.layer_mut(lid) {
                l.name = text;
            }
            ed.label("Rename Layer");
        }
        if edit.lost_focus() {
            *renaming = None;
        }
    } else {
        let color = if is_sel { theme::ACCENT } else { theme::TEXT };
        ui.painter().with_clip_rect(name_rect).text(pos2(name_rect.left(), cy), Align2::LEFT_CENTER, &layer.name, FontId::proportional(13.0), color);
    }

    // Switches.
    for (i, f) in LayerFlag::ALL.iter().enumerate() {
        let r = Rect::from_min_size(pos2(flags_x + i as f32 * FLAG_W, rect.top()), vec2(FLAG_W, rect.height()));
        ui.painter().vline(r.left(), rect.y_range(), Stroke::new(1.0, theme::BORDER));
        let on = f.get(&layer);
        let hit = ui.interact(r, Id::new(("layer-flag", lid, i)), Sense::click());
        let box_r = Rect::from_center_size(r.center(), vec2(13.0, 13.0));
        let (fill, text) = match (f, on) {
            (LayerFlag::Locked, true) => (Color32::from_rgb(200, 90, 70), theme::TEXT),
            (LayerFlag::Solo, true) => (theme::ACCENT, Color32::BLACK),
            (_, true) => (layer_color(layer.color).gamma_multiply(0.85), Color32::BLACK),
            (_, false) => (theme::BG_INSET, theme::TEXT_DIM),
        };
        ui.painter().rect_filled(box_r, 2.0, if hit.hovered() { fill.gamma_multiply(1.2) } else { fill });
        ui.painter().text(box_r.center(), Align2::CENTER_CENTER, f.letter(), FontId::proportional(10.0), text);
        if hit.clicked() {
            if let Some(l) = ed.scene.layer_mut(lid) {
                f.set(l, !on);
            }
            ed.label(format!("Layer {}", f.label()));
        }
        hit.on_hover_text(f.tip());
    }
    ui.painter().text(pos2(rect.right() - COUNT_W * 0.5, cy), Align2::CENTER_CENTER, count.to_string(), FontId::proportional(11.5), theme::TEXT_DIM);

    if resp.clicked() {
        *selected = Some(lid);
    }
    if resp.double_clicked() {
        *renaming = Some(lid);
    }
    resp.context_menu(|ui| {
        *selected = Some(lid);
        if super::widgets::menu_item(ui, None, "Add Selected Objects", "").clicked() {
            let sel = ed.selection.clone();
            ed.scene.set_layer(&sel, Some(lid));
            ed.label("Set Layer");
        }
        if super::widgets::menu_item(ui, None, "Select Objects", "").clicked() {
            ed.selection = ed.scene.layer_members(lid);
        }
        if super::widgets::menu_item(ui, None, "Rename", "").clicked() {
            *renaming = Some(lid);
        }
        ui.separator();
        if super::widgets::menu_item(ui, None, "Move Up", "").clicked() {
            ed.scene.move_layer(lid, true);
            ed.label("Move Layer");
        }
        if super::widgets::menu_item(ui, None, "Move Down", "").clicked() {
            ed.scene.move_layer(lid, false);
            ed.label("Move Layer");
        }
        ui.separator();
        if super::widgets::menu_item(ui, Some(Icon::Delete), "Delete Layer", "").clicked() {
            ed.scene.remove_layer(lid);
            ed.label("Delete Layer");
        }
    });

    // Objects dragged from the Object Manager land on the layer.
    if let Some(p) = egui::DragAndDrop::payload::<ObjectDrag>(ui.ctx())
        && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
        && rect.contains(pos)
    {
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
        if ui.input(|i| i.pointer.any_released()) {
            *dropped = Some((lid, p.0));
        }
    }
}
