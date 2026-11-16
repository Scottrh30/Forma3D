//! Panels for the Track, UV Edit and Sculpt workspaces.
//!
//! The tools behind them (camera tracking, UV unwrapping, sculpt brushes) are
//! not built yet. Each panel shows the space its tool will use and says so
//! plainly, rather than offering controls that do nothing.

use crate::editor::Editor;
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

/// A centred note: a title line and a dimmer explanation.
fn note(ui: &Ui, rect: Rect, title: &str, body: &str) {
    let painter = ui.painter();
    let width = (rect.width() - 40.0).clamp(120.0, 360.0);
    let galley = painter.layout(body.to_owned(), FontId::proportional(12.0), theme::TEXT_DIM, width);
    let h = 24.0 + galley.size().y;
    let top = rect.center().y - h * 0.5;
    let card = Rect::from_center_size(pos2(rect.center().x, rect.center().y), vec2(width + 28.0, h + 22.0));
    painter.rect_filled(card, 4.0, theme::BG_PANEL.gamma_multiply(0.92));
    painter.rect_stroke(card, 4.0, Stroke::new(1.0, theme::BORDER_LIGHT), StrokeKind::Inside);
    painter.text(pos2(rect.center().x, top + 8.0), Align2::CENTER_CENTER, title, FontId::proportional(14.0), theme::TEXT);
    painter.galley(pos2(rect.center().x - galley.size().x * 0.5, top + 22.0), galley, theme::TEXT_DIM);
}

/// The UV Editor: the 0-1 UV square with a grid, ready for UV islands.
pub fn uv_editor(ui: &mut Ui, ed: &Editor) {
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::VIEW_BOTTOM);

    // The UV square, as large as fits with a margin for labels.
    let side = (rect.width().min(rect.height()) - 56.0).max(40.0);
    let square = Rect::from_center_size(rect.center(), vec2(side, side));
    painter.rect_filled(square, 0.0, theme::VIEW_TOP);
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let strong = i == 0 || i == 10 || i == 5;
        let stroke = Stroke::new(1.0, if strong { theme::BORDER_LIGHT } else { theme::BORDER });
        painter.vline(square.left() + side * t, square.y_range(), stroke);
        painter.hline(square.x_range(), square.top() + side * t, stroke);
    }
    painter.rect_stroke(square, 0.0, Stroke::new(1.0, theme::TEXT_DIM), StrokeKind::Outside);
    let label = |p, a, t: &str, c: Color32| {
        painter.text(p, a, t, FontId::proportional(11.0), c);
    };
    label(square.left_bottom() + vec2(-6.0, 4.0), Align2::RIGHT_TOP, "0", theme::TEXT_DIM);
    label(square.right_bottom() + vec2(0.0, 4.0), Align2::CENTER_TOP, "1", theme::TEXT_DIM);
    label(square.left_top() + vec2(-6.0, 0.0), Align2::RIGHT_CENTER, "1", theme::TEXT_DIM);
    label(pos2(square.center().x, square.bottom() + 4.0), Align2::CENTER_TOP, "U", theme::AXIS_X);
    label(pos2(square.left() - 6.0, square.center().y), Align2::RIGHT_CENTER, "V", theme::AXIS_Y);

    let selected = ed.selection.first().and_then(|&id| ed.scene.get(id)).map(|o| o.name.clone());
    let body = match selected {
        Some(name) => format!(
            "\"{name}\" has no UV map: Forma3D meshes don't carry UV coordinates yet. When UV unwrapping is added, its UV islands will appear on this square to edit."
        ),
        None => "Forma3D meshes don't carry UV coordinates yet. When UV unwrapping is added, the selected object's UV islands will appear on this square to edit.".to_owned(),
    };
    note(ui, square, "UV Editor", &body);
    resp.on_hover_text("UV space: U runs left to right, V bottom to top, 0 to 1");
}

/// The Footage panel for camera tracking: a clip viewer with transport.
pub fn footage(ui: &mut Ui, _ed: &Editor) {
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    let painter = ui.painter_at(full);
    painter.rect_filled(full, 0.0, Color32::from_gray(24));

    // A film frame in 16:9 with sprocket holes.
    let avail = full.shrink2(vec2(16.0, 34.0));
    let w = avail.width().min(avail.height() * 16.0 / 9.0);
    let frame = Rect::from_center_size(pos2(avail.center().x, full.top() + 16.0 + avail.height() * 0.5), vec2(w, w * 9.0 / 16.0));
    painter.rect_filled(frame, 2.0, Color32::from_gray(34));
    painter.rect_stroke(frame, 2.0, Stroke::new(1.0, theme::BORDER_LIGHT), StrokeKind::Inside);
    let holes = ((frame.width() / 26.0) as i32).max(4);
    for i in 0..holes {
        let x = frame.left() + (i as f32 + 0.5) * frame.width() / holes as f32;
        for y in [frame.top() + 7.0, frame.bottom() - 7.0] {
            painter.rect_filled(Rect::from_center_size(pos2(x, y), vec2(9.0, 5.0)), 1.0, Color32::from_gray(20));
        }
    }
    note(
        ui,
        frame,
        "No footage",
        "Camera tracking isn't built yet. This panel will hold the video clip, its tracking points and the camera solve; the solved camera then drives the viewport.",
    );

    // Transport row (disabled until there is a clip to play).
    let row = Rect::from_min_max(pos2(full.left() + 8.0, full.bottom() - 26.0), pos2(full.right() - 8.0, full.bottom() - 4.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add_enabled_ui(false, |ui| {
        let _ = ui.button("Load Footage...");
        let _ = ui.button("Add Track");
        let _ = ui.button("Solve Camera");
    });
}

/// Sculpt brushes and their settings.
pub fn brushes(ui: &mut Ui, _ed: &Editor) {
    const BRUSHES: [&str; 8] = ["Draw", "Smooth", "Grab", "Inflate", "Pinch", "Flatten", "Crease", "Mask"];
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(6.0);
            ui.colored_label(
                theme::TEXT_DIM,
                "Sculpting isn't built yet. These are the brushes it will have; they turn on when sculpting is added.",
            );
        });
        ui.add_space(8.0);
        ui.add_enabled_ui(false, |ui| {
            egui::Grid::new("brushes").num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
                for (i, name) in BRUSHES.iter().enumerate() {
                    ui.add_sized(vec2(84.0, 26.0), egui::Button::new(*name));
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(10.0);
            let mut radius = 50.0f32;
            let mut strength = 50.0f32;
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::Slider::new(&mut radius, 1.0..=500.0).suffix(" px"));
            });
            ui.horizontal(|ui| {
                ui.label("Strength");
                ui.add(egui::Slider::new(&mut strength, 0.0..=100.0).suffix(" %"));
            });
            let mut symmetry = true;
            ui.checkbox(&mut symmetry, "X Symmetry");
        });
    });
}
