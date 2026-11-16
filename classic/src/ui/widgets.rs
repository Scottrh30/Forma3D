//! Small reusable widgets and drag-and-drop payloads.

use crate::icons::{self, Icon};
use crate::scene::{MaterialId, ObjectId};
use crate::theme;
use eframe::egui::{self, Color32, Response, Sense, Ui, vec2};

/// Payload while an object row is being dragged in the Object Manager.
#[derive(Clone, Copy, Debug)]
pub struct ObjectDrag(pub ObjectId);

/// Payload while a material swatch is being dragged.
#[derive(Clone, Copy, Debug)]
pub struct MaterialDrag(pub MaterialId);

/// A square icon button that can show a pressed/selected state.
pub fn icon_button(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    let size = vec2(28.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            theme::ACCENT.gamma_multiply(0.35)
        } else if resp.hovered() {
            theme::BG_HOVER
        } else {
            theme::BG_HEADER
        };
        let painter = ui.painter();
        painter.rect_filled(rect.shrink(1.0), 3.0, bg);
        if selected {
            painter.rect_stroke(rect.shrink(1.0), 3.0, egui::Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
        }
        let tint = if selected { Color32::WHITE } else { theme::TEXT };
        icons::paint(painter, rect.shrink(6.0), icon, tint);
    }
    resp.on_hover_text(tooltip)
}

/// The X / Y / Z lock toggles of the tool palette.
pub fn axis_toggle(ui: &mut Ui, label: &str, color: Color32, on: &mut bool) -> Response {
    let size = vec2(28.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let bg = if *on { color.gamma_multiply(0.45) } else { theme::BG_INSET };
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(1.0), 3.0, bg);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        if *on { Color32::WHITE } else { theme::TEXT_DIM },
    );
    resp.on_hover_text(format!("Enable / disable the {label} axis for the tools"))
}

/// A compact drag-able number box.
pub fn number(ui: &mut Ui, value: &mut f32, speed: f64, suffix: &str) -> Response {
    ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .max_decimals(3)
            .suffix(suffix),
    )
}

/// Section bar used inside the attribute panels (a darker strip with a title).
pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::BG_HEADER);
    ui.painter().text(
        rect.left_center() + vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.5),
        Color32::WHITE,
    );
    ui.add_space(2.0);
}

/// A parameter label padded with a dotted leader, as in Cinema 4D's
/// Attribute Manager ("Size . . . . .").
pub fn attr_label(ui: &mut Ui, text: &str) {
    const W: f32 = 132.0;
    let (rect, _) = ui.allocate_exact_size(vec2(W, 20.0), Sense::hover());
    let font = egui::FontId::proportional(12.5);
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT));
    let text_w = galley.size().x;
    let painter = ui.painter();
    painter.text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, text, font, theme::TEXT);
    let mut x = rect.left() + 8.0 + text_w + 5.0;
    while x < rect.right() - 6.0 {
        painter.circle_filled(egui::pos2(x, rect.center().y + 4.0), 0.8, theme::TEXT_DIM);
        x += 4.0;
    }
}

// ---------------------------------------------------------------------------
// Cinema 4D-style menus, palettes and manager headers
// ---------------------------------------------------------------------------

const MENU_ROW_H: f32 = 22.0;

/// A menu row: icon column, label, right-aligned shortcut, hover highlight.
pub fn menu_item(ui: &mut Ui, icon: Option<Icon>, label: &str, shortcut: &str) -> Response {
    menu_row(ui, icon, None, label, shortcut, true)
}

/// A menu row that can be greyed out.
pub fn menu_item_enabled(ui: &mut Ui, enabled: bool, icon: Option<Icon>, label: &str, shortcut: &str) -> Response {
    menu_row(ui, icon, None, label, shortcut, enabled)
}

/// A menu row with a tick mark when `checked`.
pub fn menu_check(ui: &mut Ui, checked: bool, label: &str, shortcut: &str) -> Response {
    menu_row(ui, None, Some(checked), label, shortcut, true)
}

pub fn menu_row(ui: &mut Ui, icon: Option<Icon>, check: Option<bool>, label: &str, shortcut: &str, enabled: bool) -> Response {
    let font = egui::FontId::proportional(12.5);
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT).size().x);
    let short_w = if shortcut.is_empty() {
        0.0
    } else {
        ui.fonts_mut(|f| f.layout_no_wrap(shortcut.to_owned(), font.clone(), theme::TEXT).size().x) + 24.0
    };
    let want = 30.0 + label_w + short_w + 12.0;
    // While egui measures a menu the available width is unbounded: ask only for
    // what this row needs, then fill the menu's width on later frames.
    let avail = if ui.is_sizing_pass() { 0.0 } else { ui.available_width() };
    let width = avail.max(want).max(170.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, MENU_ROW_H), sense);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if enabled && resp.hovered() {
            painter.rect_filled(rect, 2.0, theme::MENU_HOVER);
        }
        let text_color = if !enabled {
            theme::TEXT_DIM.gamma_multiply(0.7)
        } else if resp.hovered() {
            Color32::WHITE
        } else {
            theme::TEXT
        };
        let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 14.0, rect.center().y), vec2(15.0, 15.0));
        // A ticked toggle shows the tick in place of its icon.
        if let (Some(icon), false) = (icon, check == Some(true)) {
            icons::paint(painter, icon_rect, icon, text_color);
        }
        if check == Some(true) {
            let c = icon_rect.center();
            painter.add(egui::Shape::line(
                vec![c + vec2(-5.0, 0.0), c + vec2(-1.5, 4.0), c + vec2(5.5, -4.5)],
                egui::Stroke::new(1.8, theme::ACCENT),
            ));
        }
        painter.text(egui::pos2(rect.left() + 30.0, rect.center().y), egui::Align2::LEFT_CENTER, label, font.clone(), text_color);
        if !shortcut.is_empty() {
            painter.text(
                egui::pos2(rect.right() - 8.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                shortcut,
                font,
                theme::TEXT_DIM,
            );
        }
    }
    if resp.clicked() {
        ui.close();
    }
    resp
}

/// The dark strip with text menus at the top of a manager or viewport.
pub fn header_bar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(theme::BG_HEADER).inner_margin(egui::Margin::symmetric(4, 1));
    let r = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            flat_menus(ui);
            ui.horizontal(|ui| {
                ui.set_height(20.0);
                add(ui)
            })
            .inner
        })
        .inner;
    let line = ui.min_rect();
    ui.painter().hline(line.x_range(), ui.cursor().top(), egui::Stroke::new(1.0, theme::BORDER));
    r
}

/// Command-palette icon button (slightly larger than panel buttons).
pub fn palette_button(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    palette_button_enabled(ui, icon, selected, true, tooltip)
}

pub fn palette_button_enabled(ui: &mut Ui, icon: Icon, selected: bool, enabled: bool, tooltip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), sense);
    paint_palette_button(ui, rect, &resp, icon, selected, false, enabled);
    resp.on_hover_text(tooltip)
}

fn paint_palette_button(ui: &Ui, rect: egui::Rect, resp: &Response, icon: Icon, selected: bool, flyout: bool, enabled: bool) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let r = rect.shrink(1.0);
    if selected {
        painter.rect_filled(r, 3.0, theme::PALETTE_ACTIVE);
    } else if resp.hovered() {
        painter.rect_filled(r, 3.0, theme::BG_HOVER);
    }
    let tint = if selected {
        Color32::WHITE
    } else if enabled {
        theme::TEXT
    } else {
        theme::TEXT_DIM.gamma_multiply(0.6)
    };
    icons::paint(painter, r.shrink(6.0), icon, tint);
    if flyout {
        // The little corner triangle that marks a group of commands.
        let br = r.right_bottom() + vec2(-2.0, -2.0);
        painter.add(egui::Shape::convex_polygon(
            vec![br, br + vec2(-6.0, 0.0), br + vec2(0.0, -6.0)],
            theme::TEXT_DIM,
            egui::Stroke::NONE,
        ));
    }
}

/// A Cinema 4D-style flyout: the button runs the last-used command of its
/// group; holding it, right-clicking it or clicking its corner triangle opens
/// the whole group. Returns the index of the command to run.
pub fn flyout(ui: &mut Ui, id_salt: &str, items: &[(Icon, &str)], tooltip: &str) -> Option<usize> {
    let id = ui.id().with(("flyout", id_salt));
    let current = ui.data(|d| d.get_temp::<usize>(id)).unwrap_or(0).min(items.len().saturating_sub(1));
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let open_id = id.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    paint_palette_button(ui, rect, &resp, items[current].0, open, true, true);

    let mut chosen = None;
    let corner = egui::Rect::from_min_max(rect.right_bottom() - vec2(11.0, 11.0), rect.right_bottom());
    let held_long = resp.is_pointer_button_down_on()
        && ui.input(|i| i.pointer.press_start_time().is_some_and(|t| i.time - t > 0.35));
    if held_long || resp.secondary_clicked() {
        open = true;
    } else if resp.clicked() {
        let on_corner = resp.interact_pointer_pos().is_some_and(|p| corner.contains(p));
        if on_corner {
            open = !open;
        } else if !open {
            chosen = Some(current);
        }
    }
    if resp.is_pointer_button_down_on() {
        ui.ctx().request_repaint();
    }
    if !open {
        resp.clone().on_hover_text(format!("{tooltip} (hold for more)"));
    }

    if open {
        let area = egui::Area::new(id.with("popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).fill(theme::BG_PANEL).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, (icon, label)) in items.iter().enumerate() {
                        if menu_item(ui, Some(*icon), label, "").clicked() {
                            chosen = Some(i);
                        }
                    }
                });
            });
        let pressed_outside = ui.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !rect.contains(p))
        });
        if chosen.is_some() || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    if let Some(i) = chosen {
        ui.data_mut(|d| d.insert_temp(id, i));
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    chosen
}

/// Thin vertical divider between command groups.
pub fn palette_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 26.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0, theme::BORDER_LIGHT));
}

/// Text-only menu buttons, as in Cinema 4D's manager and viewport menu strips.
pub fn flat_menus(ui: &mut Ui) {
    ui.spacing_mut().button_padding = vec2(6.0, 1.0);
    ui.spacing_mut().item_spacing.x = 0.0;
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = egui::Stroke::NONE;
    w.hovered.bg_stroke = egui::Stroke::NONE;
    w.open.bg_stroke = egui::Stroke::NONE;
    w.inactive.fg_stroke.color = theme::TEXT;
}
