//! The Structure Manager: the active polygon object's points and polygons as
//! tables. Values can be typed or dragged; clicking a row's number selects
//! that point or polygon.

use super::widgets::header_bar;
use crate::component::{EditMode, SelectOp, mesh_of, mesh_of_mut};
use crate::editor::Editor;
use crate::theme;
use eframe::egui::{self, Align, Id, Layout, Sense, Ui, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Points,
    Polygons,
}

fn table_id() -> Id {
    Id::new("structure-manager-table")
}

const ROW_H: f32 = 20.0;
const INDEX_W: f32 = 54.0;

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    let default = if ed.mode == EditMode::Polygons { Table::Polygons } else { Table::Points };
    let mut table: Table = ui.data(|d| d.get_temp::<u8>(table_id())).map_or(default, |t| if t == 1 { Table::Polygons } else { Table::Points });

    let target = ed.edit_target();
    let counts = target.and_then(|id| mesh_of(&ed.scene, id)).map(|m| (m.points.len(), m.polys.len()));
    header_bar(ui, |ui| {
        ui.menu_button("Mode", |ui| {
            if super::widgets::menu_check(ui, table == Table::Points, "Points", "").clicked() {
                table = Table::Points;
            }
            if super::widgets::menu_check(ui, table == Table::Polygons, "Polygons", "").clicked() {
                table = Table::Polygons;
            }
        });
        ui.add_space(8.0);
        ui.selectable_value(&mut table, Table::Points, "Points");
        ui.selectable_value(&mut table, Table::Polygons, "Polygons");
        if let (Some(id), Some((np, npoly))) = (target, counts) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let name = ed.scene.get(id).map(|o| o.name.clone()).unwrap_or_default();
                ui.colored_label(theme::TEXT_DIM, format!("{name}: {np} points, {npoly} polygons"));
            });
        }
    });
    ui.data_mut(|d| d.insert_temp(table_id(), if table == Table::Polygons { 1u8 } else { 0u8 }));

    let Some(id) = target else {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.colored_label(theme::TEXT_DIM, "Select a polygon object to see its points and polygons.");
            ui.colored_label(theme::TEXT_DIM, "Press C to make a primitive editable.");
        });
        return;
    };
    let Some((np, npoly)) = counts else { return };

    // Column headings.
    let col_w = ((ui.available_width() - INDEX_W - 16.0) / if table == Table::Points { 3.0 } else { 4.0 }).max(40.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_sized(vec2(INDEX_W, 18.0), egui::Label::new(egui::RichText::new("Index").color(theme::TEXT_DIM)));
        let heads: &[&str] = if table == Table::Points { &["X", "Y", "Z"] } else { &["A", "B", "C", "D"] };
        for h in heads {
            ui.add_sized(vec2(col_w, 18.0), egui::Label::new(egui::RichText::new(*h).color(theme::TEXT_DIM)));
        }
    });

    let rows = if table == Table::Points { np } else { npoly };
    let mut edited = false;
    let mut select: Option<(u32, SelectOp)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW_H, rows, |ui, range| {
        for i in range {
            let selected = match table {
                Table::Points => ed.comp.points.contains(&(i as u32)),
                Table::Polygons => ed.comp.polys.contains(&(i as u32)),
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.set_height(ROW_H);
                let idx = ui.add_sized(vec2(INDEX_W, ROW_H), egui::Button::selectable(selected, i.to_string()).sense(Sense::click()));
                if idx.clicked() {
                    let m = ui.input(|i| i.modifiers);
                    let op = if m.command { SelectOp::Remove } else if m.shift { SelectOp::Add } else { SelectOp::Replace };
                    select = Some((i as u32, op));
                }
                let Some(mesh) = mesh_of_mut(&mut ed.scene, id) else { return };
                match table {
                    Table::Points => {
                        let Some(p) = mesh.points.get_mut(i) else { return };
                        for k in 0..3 {
                            let r = ui.add_sized(vec2(col_w, ROW_H), egui::DragValue::new(&mut p[k]).speed(0.5).max_decimals(3).suffix(" cm"));
                            edited |= r.changed();
                        }
                    }
                    Table::Polygons => {
                        let n = mesh.polys.get(i).map_or(0, |p| p.len());
                        let max = (np as u32).saturating_sub(1);
                        let before = mesh.polys.get(i).cloned().unwrap_or_default();
                        let mut row_changed = false;
                        for k in 0..4 {
                            if k < n {
                                let mut v = mesh.polys[i][k];
                                let r = ui.add_sized(vec2(col_w, ROW_H), egui::DragValue::new(&mut v).range(0..=max).speed(0.2));
                                if r.changed() {
                                    mesh.polys[i][k] = v;
                                    row_changed = true;
                                }
                            } else {
                                ui.add_sized(vec2(col_w, ROW_H), egui::Label::new(egui::RichText::new("–").color(theme::TEXT_DIM)));
                            }
                        }
                        if n > 4 {
                            ui.colored_label(theme::TEXT_DIM, format!(" +{}", n - 4)).on_hover_text(format!("{:?}", mesh.polys[i]));
                        }
                        // A polygon can't use the same point twice: undo such an edit.
                        if row_changed {
                            let p = &mesh.polys[i];
                            let unique = p.iter().enumerate().all(|(a, x)| !p[..a].contains(x));
                            if unique {
                                edited = true;
                            } else {
                                mesh.polys[i] = before;
                            }
                        }
                    }
                }
            });
        }
    });
    if edited {
        ed.label(if table == Table::Points { "Edit Point" } else { "Edit Polygon" });
    }
    if let Some((i, op)) = select {
        match table {
            Table::Points => {
                if ed.mode != EditMode::Points {
                    ed.set_mode(EditMode::Points);
                }
                ed.comp.apply_points([i], op);
            }
            Table::Polygons => {
                if ed.mode != EditMode::Polygons {
                    ed.set_mode(EditMode::Polygons);
                }
                ed.comp.apply_polys([i], op);
            }
        }
    }
}
