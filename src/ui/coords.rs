//! The Coordinates Manager, laid out like Cinema 4D's: Position, Size and
//! Rotation columns, an Object (Rel) / World switch, a Size / Scale switch,
//! and changes that take effect on Apply (or Enter).

use super::widgets::number;
use crate::editor::Editor;
use crate::theme;
use eframe::egui::{self, Id, Key, Ui};
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Values {
    pos: Vec3,
    size: Vec3,
    rot: Vec3,
}

/// What the fields were loaded from, and what the user typed since.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Staged {
    key: (u64, u8, bool, bool),
    base: Values,
    edit: Values,
}

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| show_inner(ui, ed));
}

fn show_inner(ui: &mut Ui, ed: &mut Editor) {
    let world_id = Id::new("coords-world-mode");
    let scale_id = Id::new("coords-scale-mode");
    let stage_id = Id::new("coords-staged");
    let mut world = ui.data(|d| d.get_temp::<bool>(world_id)).unwrap_or(false);
    let mut scale_mode = ui.data(|d| d.get_temp::<bool>(scale_id)).unwrap_or(false);

    let component = ed.mode.is_component() && ed.edit_target().is_some();
    let target = if component { ed.edit_target() } else { ed.active() };
    let Some(id) = target else {
        ui.add_space(8.0);
        ui.colored_label(theme::TEXT_DIM, "   Select an object");
        return;
    };
    let current = if component {
        match ed.component_bounds() {
            Some((lo, hi)) => Values { pos: (lo + hi) * 0.5, size: hi - lo, rot: Vec3::ZERO },
            None => {
                ui.add_space(8.0);
                ui.colored_label(theme::TEXT_DIM, format!("   No {} selected", ed.mode.label().to_lowercase()));
                return;
            }
        }
    } else {
        let Some(o) = ed.scene.get(id) else { return };
        let (pos, rot) = if world {
            (ed.scene.world_position(id), ed.scene.world_rotation_hpb(id))
        } else {
            (o.pos, o.rot)
        };
        let scale = o.scale;
        let size = if scale_mode { scale } else { ed.object_size(id).unwrap_or(Vec3::ZERO) };
        Values { pos, size, rot }
    };

    // Reload the fields when the target changes, or when nothing is pending.
    let key = (id, ed.mode as u8, world, scale_mode);
    let mut st = ui.data(|d| d.get_temp::<Staged>(stage_id)).filter(|s| s.key == key).unwrap_or(Staged { key, base: current, edit: current });
    if st.edit == st.base {
        st = Staged { key, base: current, edit: current };
    }

    let size_unit = if scale_mode && !component { "" } else { " cm" };
    egui::Grid::new("coords-grid").num_columns(6).spacing([6.0, 3.0]).show(ui, |ui| {
        for title in ["Position", "", if scale_mode && !component { "Scale" } else { "Size" }, "", "Rotation", ""] {
            ui.label(egui::RichText::new(title).color(theme::TEXT_DIM));
        }
        ui.end_row();
        for (i, (a, r)) in [("X", "H"), ("Y", "P"), ("Z", "B")].into_iter().enumerate() {
            ui.colored_label(theme::axis_color(i), a);
            number(ui, &mut st.edit.pos[i], 1.0, " cm");
            ui.colored_label(theme::axis_color(i), a);
            number(ui, &mut st.edit.size[i], if size_unit.is_empty() { 0.01 } else { 1.0 }, size_unit);
            ui.colored_label(theme::TEXT_DIM, r);
            ui.add_enabled_ui(!component, |ui| number(ui, &mut st.edit.rot[i], 0.5, "°"));
            ui.end_row();
        }
    });
    ui.add_space(2.0);
    let pending = st.edit != st.base;
    let mut apply = pending && ui.input(|i| i.key_pressed(Key::Enter));
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!component, |ui| {
            egui::ComboBox::from_id_salt("coord-mode")
                .selected_text(if world || component { "World" } else { "Object (Rel)" })
                .width(104.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut world, false, "Object (Rel)");
                    ui.selectable_value(&mut world, true, "World");
                });
            egui::ComboBox::from_id_salt("size-mode")
                .selected_text(if scale_mode { "Scale" } else { "Size" })
                .width(70.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut scale_mode, false, "Size");
                    ui.selectable_value(&mut scale_mode, true, "Scale");
                });
        });
        let button = egui::Button::new(if pending { "Apply *" } else { "Apply" });
        if ui.add_enabled(pending, button).on_hover_text("Apply the values (or press Enter)").clicked() {
            apply = true;
        }
    });

    if apply {
        let v = st.edit;
        if component {
            ed.set_component_bounds(v.pos, v.size.max(Vec3::ZERO));
        } else {
            if world {
                ed.scene.set_world_position(id, v.pos);
                ed.scene.set_world_rotation_hpb(id, v.rot);
            } else if let Some(o) = ed.scene.get_mut(id) {
                o.pos = v.pos;
                o.rot = v.rot;
            }
            if scale_mode {
                if let Some(o) = ed.scene.get_mut(id) {
                    o.scale = v.size;
                }
            } else {
                ed.set_object_size(id, v.size);
            }
            ed.label("Coordinates");
        }
        // Nothing pending any more, so the fields reload from the scene next frame.
        st.base = st.edit;
    }
    ui.data_mut(|d| {
        d.insert_temp(world_id, world);
        d.insert_temp(scale_id, scale_mode);
        d.insert_temp(stage_id, st);
    });
}
