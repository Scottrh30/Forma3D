//! The Take Manager: variations of the scene. Click a take to make it
//! active; while a take other than Main is active, every change to an
//! object's position, rotation, scale, visibility, material, layer or on/off
//! switch is recorded as that take's override (Auto Take).

use super::commands::{Cmd, menu_item as item};
use super::widgets::{header_bar, heading, menu_item, menu_item_enabled};
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, Align2, FontId, Id, Sense, Stroke, Ui, pos2, vec2};

const ROW_H: f32 = 22.0;

fn renaming_id() -> Id {
    Id::new("take-manager-renaming")
}

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    let mut renaming: Option<usize> = ui.data(|d| d.get_temp(renaming_id()));
    let active = ed.scene.active_take();

    header_bar(ui, |ui| {
        ui.menu_button("Take", |ui| {
            item(ui, ed, Cmd::NewTake);
            if menu_item_enabled(ui, active > 0, None, "Clone Take", "").clicked() {
                ed.scene.clone_take(active);
                ed.label("Clone Take");
            }
            if menu_item_enabled(ui, active > 0, None, "Rename Take", "").clicked() {
                renaming = Some(active);
            }
            ui.separator();
            if menu_item_enabled(ui, active > 0, Some(Icon::Delete), "Delete Take", "").clicked() {
                ed.scene.remove_take(active);
                ed.label("Delete Take");
            }
        });
        ui.menu_button("Overrides", |ui| {
            let can = active > 0 && !ed.selection.is_empty();
            if menu_item_enabled(ui, can, None, "Remove Overrides from Selected Objects", "").clicked() {
                let sel = ed.selection.clone();
                ed.scene.clear_take_overrides(&sel);
                ed.label("Remove Overrides");
            }
            if menu_item_enabled(ui, active > 0, None, "Remove All Overrides of this Take", "").clicked() {
                let ids: Vec<u64> = ed.scene.take_overrides(active).into_iter().map(|(id, _)| id).collect();
                ed.scene.clear_take_overrides(&ids);
                ed.label("Remove Overrides");
            }
        });
    });

    let names: Vec<String> = ed.scene.takes().takes.iter().map(|t| t.name.clone()).collect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, name) in names.iter().enumerate() {
            let count = ed.scene.take_override_count(i);
            let width = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
            let is_active = i == active;
            if is_active {
                ui.painter().rect_filled(rect, 0.0, theme::SELECT_ACTIVE_BG);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, theme::BG_HOVER.gamma_multiply(0.6));
            }
            let indent = if i == 0 { 6.0 } else { 22.0 };
            let cy = rect.center().y;
            // The active take's marker.
            let mark = pos2(rect.left() + indent + 6.0, cy);
            if is_active {
                ui.painter().circle_filled(mark, 4.5, theme::ACCENT);
            } else {
                ui.painter().circle_stroke(mark, 4.0, Stroke::new(1.0, theme::TEXT_DIM));
            }
            let icon_r = egui::Rect::from_center_size(pos2(rect.left() + indent + 22.0, cy), vec2(14.0, 14.0));
            icons::paint(ui.painter(), icon_r, Icon::Takes, if is_active { theme::ACCENT } else { theme::TEXT_DIM });
            let name_x = rect.left() + indent + 34.0;
            if renaming == Some(i) && i > 0 {
                let r = egui::Rect::from_min_max(pos2(name_x, rect.top() + 1.0), pos2(rect.right() - 70.0, rect.bottom() - 1.0));
                let mut text = name.clone();
                let edit = ui.put(r, egui::TextEdit::singleline(&mut text).desired_width(r.width()));
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if text != *name {
                    ed.scene.rename_take(i, &text);
                    ed.label("Rename Take");
                }
                if edit.lost_focus() {
                    renaming = None;
                }
            } else {
                let color = if is_active { theme::ACCENT } else { theme::TEXT };
                ui.painter().text(pos2(name_x, cy), Align2::LEFT_CENTER, name, FontId::proportional(13.0), color);
            }
            if i > 0 {
                let label = match count {
                    0 => "no overrides".to_owned(),
                    1 => "1 override".to_owned(),
                    n => format!("{n} overrides"),
                };
                ui.painter().text(pos2(rect.right() - 8.0, cy), Align2::RIGHT_CENTER, label, FontId::proportional(11.0), theme::TEXT_DIM);
            }
            if resp.clicked() && !is_active {
                ed.scene.set_active_take(i);
                ed.label("Switch Take");
                ed.status = format!("Take \"{name}\" active");
            }
            if resp.double_clicked() && i > 0 {
                renaming = Some(i);
            }
            resp.context_menu(|ui| {
                if menu_item(ui, None, "Make Active", "").clicked() {
                    ed.scene.set_active_take(i);
                    ed.label("Switch Take");
                }
                if i > 0 {
                    if menu_item(ui, None, "Rename", "").clicked() {
                        renaming = Some(i);
                    }
                    if menu_item(ui, None, "Clone", "").clicked() {
                        ed.scene.clone_take(i);
                        ed.label("Clone Take");
                    }
                    ui.separator();
                    if menu_item(ui, Some(Icon::Delete), "Delete", "").clicked() {
                        ed.scene.remove_take(i);
                        ed.label("Delete Take");
                    }
                }
            });
        }

        ui.add_space(8.0);
        if active == 0 {
            note(ui, "Main is the scene itself: changes here apply to every take that doesn't override them. Add a take, then change objects to record its overrides.");
        } else {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                heading(ui, "Overrides of the active take");
            });
            let overrides = ed.scene.take_overrides(active);
            if overrides.is_empty() {
                note(ui, "None yet: move, hide or recolour objects to record overrides for this take.");
            }
            for (id, ov) in overrides {
                let Some(name) = ed.scene.get(id).map(|o| o.name.clone()) else { continue };
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    if ui.selectable_label(ed.is_selected(id), name).clicked() {
                        ed.select_only(id);
                    }
                    ui.colored_label(theme::TEXT_DIM, ov.fields().join(", "));
                });
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(renaming_id(), renaming));
}

fn note(ui: &mut Ui, text: &str) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
        ui.colored_label(theme::TEXT_DIM, text);
    });
}
