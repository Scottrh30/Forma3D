//! The dark, dense look: neutral grey panels, slightly lighter headers, and a
//! single orange accent for selection, in the spirit of Cinema 4D's default UI.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

// Neutral dark greys close to the default Cinema 4D theme: managers sit on a
// mid grey, their menu strips are darker, fields are inset darker still.
pub const BG_DEEP: Color32 = Color32::from_rgb(30, 30, 31);
pub const BG_INSET: Color32 = Color32::from_rgb(36, 36, 37);
pub const BG_PANEL: Color32 = Color32::from_rgb(49, 49, 50);
pub const BG_HEADER: Color32 = Color32::from_rgb(40, 40, 41);
pub const BG_HOVER: Color32 = Color32::from_rgb(68, 68, 70);
pub const BORDER: Color32 = Color32::from_rgb(22, 22, 23);
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(70, 70, 72);
pub const TEXT: Color32 = Color32::from_rgb(212, 212, 214);
pub const TEXT_DIM: Color32 = Color32::from_rgb(142, 142, 146);
pub const ACCENT: Color32 = Color32::from_rgb(240, 150, 48);
/// Object Manager selection: a soft lighter row with the name in orange.
pub const SELECT_BG: Color32 = Color32::from_rgb(62, 66, 74);
pub const SELECT_ACTIVE_BG: Color32 = Color32::from_rgb(70, 76, 88);
pub const MENU_HOVER: Color32 = Color32::from_rgb(64, 96, 140);
pub const PALETTE_ACTIVE: Color32 = Color32::from_rgb(70, 108, 160);
/// Viewport background gradient (top, bottom).
pub const VIEW_TOP: Color32 = Color32::from_rgb(64, 64, 66);
pub const VIEW_BOTTOM: Color32 = Color32::from_rgb(38, 38, 40);
pub const AXIS_X: Color32 = Color32::from_rgb(232, 84, 72);
pub const AXIS_Y: Color32 = Color32::from_rgb(124, 206, 84);
pub const AXIS_Z: Color32 = Color32::from_rgb(82, 134, 240);
pub const GIZMO_HOT: Color32 = Color32::from_rgb(255, 224, 96);

pub fn axis_color(index: usize) -> Color32 {
    match index {
        0 => AXIS_X,
        1 => AXIS_Y,
        _ => AXIS_Z,
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.extreme_bg_color = BG_DEEP; // text edits, scroll area backgrounds
    v.faint_bg_color = BG_INSET;
    v.code_bg_color = BG_INSET;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(3);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = SELECT_BG;
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(200, 214, 236));

    let radius = CornerRadius::same(3);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.weak_bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = Color32::from_rgb(70, 70, 73);
    w.inactive.weak_bg_fill = Color32::from_rgb(64, 64, 67);
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = BG_HOVER;
    w.hovered.weak_bg_fill = BG_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(110, 110, 116));
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.hovered.corner_radius = radius;

    w.active.bg_fill = Color32::from_rgb(98, 98, 104);
    w.active.weak_bg_fill = Color32::from_rgb(98, 98, 104);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.corner_radius = radius;

    w.open.bg_fill = BG_HOVER;
    w.open.weak_bg_fill = BG_HOVER;
    w.open.corner_radius = radius;

    ctx.set_visuals(v);

    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);
        style.spacing.interact_size.y = 20.0;
        style.spacing.menu_margin = egui::Margin::same(3);
        style.spacing.menu_spacing = 0.0;
        // Menus size to their rows; rows keep the shortcut beside the name.
        style.spacing.menu_width = 240.0;
        style.spacing.window_margin = egui::Margin::same(8);
        style.text_styles = [
            (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(12.5, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

/// The `egui_dock` style that matches the rest of the UI.
pub fn dock_style(ui_style: &egui::Style) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(ui_style);
    s.main_surface_border_stroke = Stroke::NONE;
    s.separator.color_idle = BORDER;
    s.separator.color_hovered = ACCENT;
    s.separator.color_dragged = ACCENT;
    s.separator.width = 2.0;
    s.tab_bar.bg_fill = BG_HEADER;
    s.tab_bar.height = 21.0;
    s.tab_bar.hline_color = BORDER;
    s.tab.tab_body.bg_fill = BG_PANEL;
    s.tab.tab_body.inner_margin = egui::Margin::same(0);
    s.tab.tab_body.stroke = Stroke::NONE;
    for tab in [&mut s.tab.active, &mut s.tab.focused, &mut s.tab.inactive] {
        tab.corner_radius = CornerRadius::ZERO;
        tab.outline_color = BORDER;
    }
    s.tab.active.bg_fill = BG_PANEL;
    s.tab.active.text_color = TEXT;
    s.tab.focused.bg_fill = BG_PANEL;
    s.tab.focused.text_color = Color32::WHITE;
    s.tab.inactive.bg_fill = BG_HEADER;
    s.tab.inactive.text_color = TEXT_DIM;
    s
}
