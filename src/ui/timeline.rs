//! The timeline under the viewport: a frame ruler with the current-frame
//! marker, and Cinema 4D-style transport controls. There are no keyframes yet,
//! so it only moves the time cursor.

use super::widgets::palette_button;
use crate::editor::Editor;
use crate::icons::Icon;
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

pub const HEIGHT: f32 = 58.0;

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    let dt = ui.input(|i| i.stable_dt).min(0.1);
    ed.timeline.tick(dt);
    if ed.timeline.playing {
        ui.ctx().request_repaint();
    }
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, theme::BG_PANEL);
    ui.painter().hline(full.x_range(), full.top(), Stroke::new(1.0, theme::BORDER));
    ruler(ui, ed, Rect::from_min_size(full.min + vec2(8.0, 4.0), vec2(full.width() - 16.0, 22.0)));
    let row = Rect::from_min_max(pos2(full.left(), full.top() + 28.0), full.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| transport(ui, ed));
}

fn ruler(ui: &mut Ui, ed: &mut Editor, rect: Rect) {
    let t = &mut ed.timeline;
    let resp = ui.interact(rect, ui.id().with("ruler"), Sense::click_and_drag());
    let span = (t.max - t.min).max(1) as f32;
    let x_of = |f: i32| rect.left() + (f - t.min) as f32 / span * rect.width();
    if let (true, Some(p)) = (resp.is_pointer_button_down_on(), resp.interact_pointer_pos()) {
        let f = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        t.frame = t.min + (f * span).round() as i32;
    }
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_filled(rect, 2.0, theme::BG_INSET);
    // Ticks: a label every 10 frames when there is room, minor ticks in between.
    let px_per_frame = rect.width() / span;
    let major = [1, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|&m| m as f32 * px_per_frame >= 36.0).unwrap_or(1000);
    let minor = if px_per_frame >= 6.0 { 1 } else { (major / 5).max(1) };
    let first = t.min - t.min.rem_euclid(minor);
    let mut f = first;
    while f <= t.max {
        if f >= t.min {
            let x = x_of(f);
            let is_major = f.rem_euclid(major) == 0;
            let h = if is_major { 9.0 } else { 4.0 };
            painter.line_segment([pos2(x, rect.bottom() - h), pos2(x, rect.bottom())], Stroke::new(1.0, theme::TEXT_DIM));
            if is_major {
                painter.text(pos2(x + 2.0, rect.top() + 1.0), Align2::LEFT_TOP, f.to_string(), FontId::proportional(10.0), theme::TEXT_DIM);
            }
        }
        f += minor;
    }
    // Current frame: a coloured box with the number on a vertical line.
    let x = x_of(t.frame);
    let marker = Color32::from_rgb(78, 140, 220);
    painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(2.0, marker));
    let label = t.frame.to_string();
    let w = 8.0 + 6.5 * label.len() as f32;
    let tag = Rect::from_center_size(pos2(x, rect.top() + 6.0), vec2(w, 13.0));
    painter.rect_filled(tag, 2.0, marker);
    painter.text(tag.center(), Align2::CENTER_CENTER, label, FontId::proportional(10.5), Color32::WHITE);
}

fn frame_field(ui: &mut Ui, value: &mut i32, tip: &str) -> bool {
    ui.add(egui::DragValue::new(value).suffix(" F").speed(0.3)).on_hover_text(tip).changed()
}

fn transport(ui: &mut Ui, ed: &mut Editor) {
    let t = &mut ed.timeline;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(8.0);
        let (mut lo, mut hi) = (t.min, t.max);
        let changed_lo = frame_field(ui, &mut lo, "First frame");
        ui.add_space(6.0);
        let mut cur = t.frame;
        if frame_field(ui, &mut cur, "Current frame") {
            t.frame = cur.clamp(t.min, t.max);
        }
        // Centre the transport buttons.
        let buttons_w = 7.0 * 31.0;
        let gap = ((ui.available_width() - buttons_w - 120.0) * 0.5).max(6.0);
        ui.add_space(gap);
        if palette_button(ui, Icon::ToStart, false, "Go to start").clicked() {
            t.frame = t.min;
        }
        if palette_button(ui, Icon::PrevFrame, false, "Previous frame").clicked() {
            t.frame = (t.frame - 1).max(t.min);
        }
        if palette_button(ui, Icon::PlayBackward, t.playing && t.reverse, "Play backwards").clicked() {
            t.playing = !(t.playing && t.reverse);
            t.reverse = true;
        }
        let fwd = t.playing && !t.reverse;
        if palette_button(ui, if fwd { Icon::Pause } else { Icon::Play }, fwd, "Play forwards / pause").clicked() {
            t.playing = !fwd;
            t.reverse = false;
        }
        if palette_button(ui, Icon::NextFrame, false, "Next frame").clicked() {
            t.frame = (t.frame + 1).min(t.max);
        }
        if palette_button(ui, Icon::ToEnd, false, "Go to end").clicked() {
            t.frame = t.max;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            let changed_hi = frame_field(ui, &mut hi, "Last frame");
            ui.colored_label(theme::TEXT_DIM, format!("{} fps", t.fps as i32));
            if changed_lo || changed_hi {
                t.set_range(lo, hi);
            }
        });
    });
}
