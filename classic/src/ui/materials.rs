//! The Material Manager: material balls that can be dragged onto objects, and
//! a small Material Editor window (double-click a ball).

use super::widgets::{MaterialDrag, header_bar};
use crate::editor::Editor;
use crate::theme;
use eframe::egui::{self, Color32, Id, Sense, Stroke, Ui, vec2};

fn to_color(c: [f32; 3]) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

/// A shaded preview ball: base colour, darker rim, soft highlight.
fn paint_ball(painter: &egui::Painter, centre: egui::Pos2, r: f32, c: [f32; 3]) {
    let base = to_color(c);
    let steps = 10;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let shade = 0.45 + 0.65 * t;
        let col = Color32::from_rgb(
            (base.r() as f32 * shade).min(255.0) as u8,
            (base.g() as f32 * shade).min(255.0) as u8,
            (base.b() as f32 * shade).min(255.0) as u8,
        );
        let off = vec2(-r * 0.28, -r * 0.30) * t;
        painter.circle_filled(centre + off, r * (1.0 - 0.75 * t), col);
    }
    painter.circle_filled(centre + vec2(-r * 0.36, -r * 0.40), r * 0.16, Color32::from_white_alpha(170));
}

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    let editor_id = Id::new("material-editor");
    header_bar(ui, |ui| {
        use super::commands::{Cmd, menu_item as item};
        ui.menu_button("Create", |ui| {
            item(ui, ed, Cmd::NewMaterial);
        });
        ui.menu_button("Edit", |ui| {
            item(ui, ed, Cmd::OpenMaterialEditor);
            item(ui, ed, Cmd::ApplyMaterial);
            ui.separator();
            item(ui, ed, Cmd::DeleteMaterial);
        });
    });

    let mats: Vec<(u64, String, [f32; 3])> = ed.scene.materials().iter().map(|m| (m.id, m.name.clone(), m.color)).collect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for (id, name, color) in &mats {
                let (rect, resp) = ui.allocate_exact_size(vec2(62.0, 66.0), Sense::click_and_drag());
                let painter = ui.painter();
                let sel = ed.selected_material == Some(*id);
                if sel {
                    painter.rect_filled(rect, 3.0, theme::SELECT_BG);
                } else if resp.hovered() {
                    painter.rect_filled(rect, 3.0, theme::BG_HOVER.gamma_multiply(0.6));
                }
                paint_ball(painter, rect.center_top() + vec2(0.0, 25.0), 21.0, *color);
                let label_rect = egui::Rect::from_min_max(rect.left_bottom() - vec2(0.0, 15.0), rect.right_bottom());
                painter.with_clip_rect(label_rect).text(
                    label_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    name,
                    egui::FontId::proportional(11.0),
                    if sel { theme::ACCENT } else { theme::TEXT },
                );
                if resp.clicked() {
                    ed.selected_material = Some(*id);
                }
                if resp.double_clicked() {
                    ui.data_mut(|d| d.insert_temp(editor_id, Some(*id)));
                }
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), MaterialDrag(*id));
                }
                resp.on_hover_text("Drag onto an object in the viewport or Object Manager; double-click to edit");
            }
        });
    });

    // The Material Editor window.
    let open: Option<u64> = ui.data(|d| d.get_temp::<Option<u64>>(editor_id)).flatten();
    if let Some(mid) = open {
        let mut keep = true;
        let mut changed = false;
        if let Some(m) = ed.scene.material_mut(mid) {
            egui::Window::new(format!("Material Editor [{}]", m.name))
                .id(Id::new("material-editor-window"))
                .open(&mut keep)
                .collapsible(false)
                .resizable(false)
                .default_width(300.0)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                        ui.painter().rect_filled(r, 3.0, theme::BG_DEEP);
                        paint_ball(ui.painter(), r.center(), 40.0, m.color);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Color").strong());
                            let mut name = m.name.clone();
                            if ui.text_edit_singleline(&mut name).changed() && !name.trim().is_empty() {
                                m.name = name;
                                changed = true;
                            }
                            changed |= ui.color_edit_button_rgb(&mut m.color).changed();
                            ui.add_space(4.0);
                            ui.colored_label(theme::TEXT_DIM, "Only the colour channel exists so far.");
                        });
                    });
                    ui.painter().hline(ui.min_rect().x_range(), ui.min_rect().bottom(), Stroke::new(1.0, theme::BORDER));
                });
        } else {
            keep = false;
        }
        if changed {
            ed.label("Edit Material");
        }
        if !keep {
            ui.data_mut(|d| d.insert_temp::<Option<u64>>(editor_id, None));
        }
    }
}
