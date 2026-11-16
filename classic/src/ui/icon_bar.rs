//! The icon bar above the viewport: a row of command buttons the user arranges
//! by drag and drop.
//!
//! - Drag an icon along the bar to move it; an orange line shows where it lands.
//! - Drag an icon off the bar and let go to remove it.
//! - Drag icons in from the Icon Library (the "+" at the end of the bar), or
//!   click one there to add it at the end.
//! - Right-click for Remove, Add Separator, Icon Library and Reset.
//! - The Shapes button is a dropdown: click it for a list of the parametric
//!   shapes (it shows the last one made).
//!
//! The layout is saved next to the keyboard shortcuts as `icon_bar.json`.

use super::commands::{self, Cmd, Info, Shared};
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::generator::GeneratorKind;
use crate::pen::PenKind;
use crate::primitive::PrimitiveKind;
use crate::renderer::{DisplayMode, ViewFx};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, LayerId, Order, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::path::{Path, PathBuf};

/// Height of the bar.
pub const HEIGHT: f32 = 36.0;
const BUTTON: f32 = 30.0;
const SEPARATOR: f32 = 11.0;
const GAP: f32 = 1.0;

/// One place on the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Command(Cmd),
    Separator,
    /// A dropdown of the parametric shapes.
    Shapes,
}

impl Entry {
    fn width(self) -> f32 {
        match self {
            Entry::Command(_) | Entry::Shapes => BUTTON,
            Entry::Separator => SEPARATOR,
        }
    }
}

/// The shapes in the Shapes dropdown, in menu order.
pub const SHAPES: [PrimitiveKind; 11] = [
    PrimitiveKind::Sphere,
    PrimitiveKind::Cube,
    PrimitiveKind::Cone,
    PrimitiveKind::Pyramid,
    PrimitiveKind::Tube,
    PrimitiveKind::Cylinder,
    PrimitiveKind::Capsule,
    PrimitiveKind::Plane,
    PrimitiveKind::Torus,
    PrimitiveKind::Landscape,
    PrimitiveKind::Figure,
];
const SHAPES_ID: &str = "group.shapes";
/// Saved layouts older than this get the Shapes dropdown added in front.
const VERSION: u64 = 3;

/// Drawing and smoothing tools placed right after the Shapes dropdown.
fn modelling_entries() -> [Entry; 3] {
    [
        Entry::Command(Cmd::Pen(PenKind::Spline)),
        Entry::Command(Cmd::Pen(PenKind::Polygon)),
        Entry::Command(Cmd::Generator(GeneratorKind::Subdivision)),
    ]
}

/// The drag payload: what is being dragged and, for icons already on the
/// bar, where it came from.
#[derive(Clone, Copy, Debug)]
struct BarDrag {
    entry: Entry,
    from: Option<usize>,
}

pub struct IconBar {
    pub entries: Vec<Entry>,
    pub library_open: bool,
    search: String,
    path: Option<PathBuf>,
    dirty: bool,
}

/// The starting set: framing, display modes, render options, view toggles
/// and a few everyday edits.
pub fn default_entries() -> Vec<Entry> {
    use Entry::{Command as C, Separator as S};
    let mut v = vec![Entry::Shapes];
    v.extend(modelling_entries());
    v.extend([S, C(Cmd::FrameAll), C(Cmd::FrameSelected), C(Cmd::FrameDefault), S]);
    v.extend(DisplayMode::ALL.iter().map(|&m| C(Cmd::Display(m))));
    v.push(S);
    v.extend(ViewFx::ALL.iter().map(|&f| C(Cmd::ViewFx(f))));
    v.extend([S, C(Cmd::ToggleGrid), C(Cmd::ToggleSolo), C(Cmd::FourViews), S]);
    v.extend([C(Cmd::MakeEditable), C(Cmd::Duplicate), C(Cmd::Delete)]);
    v
}

/// `icon_bar.json` in the same folder as the keyboard shortcuts.
pub fn config_path() -> Option<PathBuf> {
    commands::keymap_path().map(|p| p.with_file_name("icon_bar.json"))
}

/// Encodes the bar as command ids, with "|" for separators.
pub fn to_json(entries: &[Entry], infos: &[Info]) -> String {
    let ids: Vec<String> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Separator => Some("|".to_owned()),
            Entry::Shapes => Some(SHAPES_ID.to_owned()),
            Entry::Command(c) => infos.iter().find(|i| i.cmd == *c).map(|i| i.id.clone()),
        })
        .collect();
    serde_json::json!({ "version": VERSION, "items": ids }).to_string()
}

/// Decodes a saved bar. Ids that no longer exist are skipped; anything
/// unreadable gives `None` (and the default bar is used).
pub fn from_json(text: &str, infos: &[Info]) -> Option<Vec<Entry>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let items = value.get("items")?.as_array()?;
    let mut entries: Vec<Entry> = items
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(|id| match id {
            "|" => Some(Entry::Separator),
            SHAPES_ID => Some(Entry::Shapes),
            _ => infos.iter().find(|i| i.id == id).map(|i| Entry::Command(i.cmd)),
        })
        .collect();
    // Bars saved before the Shapes dropdown existed get it at the front.
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    if version < 2 && !entries.contains(&Entry::Shapes) {
        entries.splice(0..0, [Entry::Shapes, Entry::Separator]);
        tidy(&mut entries);
    }
    // Bars saved before version 3 get the pens and Subdivision Surface after Shapes.
    if version < 3 {
        let at = entries.iter().position(|e| *e == Entry::Shapes).map_or(0, |i| i + 1);
        let missing: Vec<Entry> = modelling_entries().into_iter().filter(|e| !entries.contains(e)).collect();
        entries.splice(at..at, missing);
    }
    Some(entries)
}

/// Moves or inserts `drag` so that it lands before position `to` (counted on
/// the bar as it is before the move). Returns the new index.
fn place(entries: &mut Vec<Entry>, drag: BarDrag, to: usize) -> usize {
    let mut to = to.min(entries.len());
    if let Some(from) = drag.from.filter(|&f| f < entries.len()) {
        entries.remove(from);
        if from < to {
            to -= 1;
        }
    }
    entries.insert(to, drag.entry);
    to
}

/// Removes separators that end up doubled or at either end.
fn tidy(entries: &mut Vec<Entry>) {
    let mut out: Vec<Entry> = Vec::with_capacity(entries.len());
    for &e in entries.iter() {
        if e == Entry::Separator && matches!(out.last(), None | Some(Entry::Separator)) {
            continue;
        }
        out.push(e);
    }
    if out.last() == Some(&Entry::Separator) {
        out.pop();
    }
    *entries = out;
}

/// Short text for commands that have no icon of their own ("Select All" -> "SA").
fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [] => "?".to_owned(),
        [one] => one.chars().take(2).collect(),
        many => many.iter().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect(),
    }
}

/// Paints one command button (an icon, or initials when it has none).
fn paint_button(ui: &Ui, rect: Rect, info: Option<&Info>, hovered: bool, checked: bool, enabled: bool) {
    let painter = ui.painter();
    let r = rect.shrink(1.0);
    if checked {
        painter.rect_filled(r, 3.0, theme::PALETTE_ACTIVE);
    } else if hovered {
        painter.rect_filled(r, 3.0, theme::BG_HOVER);
    }
    let tint = if checked {
        Color32::WHITE
    } else if enabled {
        theme::TEXT
    } else {
        theme::TEXT_DIM.gamma_multiply(0.6)
    };
    paint_face(painter, r, info, tint);
}

fn paint_face(painter: &egui::Painter, r: Rect, info: Option<&Info>, tint: Color32) {
    match info.and_then(|i| i.icon) {
        Some(icon) => icons::paint(painter, r.shrink(6.0), icon, tint),
        None => {
            let text = info.map_or_else(|| "?".to_owned(), |i| initials(&i.name));
            painter.rect_stroke(r.shrink(4.0), 3.0, Stroke::new(1.0, tint.gamma_multiply(0.6)), StrokeKind::Inside);
            painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(11.0), tint);
        }
    }
}

fn paint_separator(ui: &Ui, rect: Rect) {
    let x = rect.center().x;
    ui.painter().vline(x, rect.shrink2(vec2(0.0, 6.0)).y_range(), Stroke::new(1.0, theme::BORDER_LIGHT));
}

impl IconBar {
    /// Loads the saved bar, or the default one.
    pub fn load(infos: &[Info]) -> Self {
        let path = config_path();
        Self::load_from(infos, path)
    }

    fn load_from(infos: &[Info], path: Option<PathBuf>) -> Self {
        let entries = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| from_json(&t, infos))
            .unwrap_or_else(default_entries);
        IconBar {
            entries,
            library_open: false,
            search: String::new(),
            path,
            dirty: false,
        }
    }

    pub fn reset(&mut self) {
        self.entries = default_entries();
        self.dirty = true;
    }

    /// Writes the layout if it changed. Returns an error message on failure.
    pub fn save_if_changed(&mut self, infos: &[Info]) -> Option<String> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let path = self.path.as_deref()?;
        save(path, &to_json(&self.entries, infos)).err()
    }

    /// The bar itself, across the top of `ui`'s available space.
    pub fn show(&mut self, ui: &mut Ui, ed: &mut Editor) {
        let Some(sh) = commands::shared(ui.ctx()) else { return };
        let width = ui.available_width();
        let (bar, bar_resp) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::click());
        let painter = ui.painter();
        painter.rect_filled(bar, 0.0, theme::BG_PANEL);
        painter.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));

        // Lay out the entries left to right.
        let mut x = bar.left() + 6.0;
        let mut slots: Vec<Rect> = Vec::with_capacity(self.entries.len());
        for e in &self.entries {
            let w = e.width();
            slots.push(Rect::from_min_size(pos2(x, bar.center().y - BUTTON * 0.5), vec2(w, BUTTON)));
            x += w + GAP;
        }
        let add_rect = Rect::from_min_size(pos2(x + 4.0, bar.center().y - BUTTON * 0.5), vec2(BUTTON, BUTTON));

        let dragging = egui::DragAndDrop::payload::<BarDrag>(ui.ctx()).map(|p| *p);
        let pointer = ui.ctx().pointer_latest_pos();
        let over_bar = pointer.is_some_and(|p| bar.expand2(vec2(0.0, 10.0)).contains(p));

        let mut remove: Option<usize> = None;
        let mut insert_separator: Option<usize> = None;
        let mut open_library = false;
        let mut reset = false;
        for (i, (&entry, &rect)) in self.entries.iter().zip(&slots).enumerate() {
            let id = ui.id().with(("icon-bar", i));
            let resp = ui.interact(rect, id, Sense::click_and_drag());
            let lifted = dragging.is_some_and(|d| d.from == Some(i));
            if resp.drag_started() {
                resp.dnd_set_drag_payload(BarDrag { entry, from: Some(i) });
            }
            match entry {
                Entry::Separator => {
                    if !lifted {
                        paint_separator(ui, rect);
                    }
                }
                Entry::Shapes => {
                    let open = shapes_open(ui.ctx());
                    if lifted {
                        ui.painter().rect_stroke(rect.shrink(2.0), 3.0, Stroke::new(1.0, theme::BORDER_LIGHT), StrokeKind::Inside);
                    } else {
                        let icon = icons::for_primitive(last_shape(ui.ctx()));
                        paint_dropdown_button(ui, rect, icon, resp.hovered() && dragging.is_none(), open);
                    }
                    if resp.clicked() {
                        set_shapes_open(ui.ctx(), !open);
                    }
                    if dragging.is_none() && !open {
                        resp.clone().on_hover_text("Shapes: click for the list of parametric shapes.\nDrag to move, drag off the bar to remove.");
                    }
                    if open && !lifted {
                        shapes_dropdown(ui, rect);
                    }
                }
                Entry::Command(cmd) => {
                    let info = sh.info(cmd);
                    let (enabled, checked) = commands::state(ed, cmd, sh.layout);
                    if lifted {
                        // The gap the icon was lifted from.
                        ui.painter().rect_stroke(rect.shrink(2.0), 3.0, Stroke::new(1.0, theme::BORDER_LIGHT), StrokeKind::Inside);
                    } else {
                        paint_button(ui, rect, info, resp.hovered() && dragging.is_none(), checked == Some(true), enabled);
                    }
                    if resp.clicked() && enabled {
                        commands::queue(ui.ctx(), cmd);
                    }
                    if dragging.is_none() {
                        let tip = format!("{}\nDrag to move, drag off the bar to remove.", sh.tooltip(cmd));
                        resp.clone().on_hover_text(tip);
                    }
                }
            }
            resp.context_menu(|ui| {
                if ui.button("Remove from Icon Bar").clicked() {
                    remove = Some(i);
                    ui.close();
                }
                if ui.button("Add Separator Here").clicked() {
                    insert_separator = Some(i);
                    ui.close();
                }
                ui.separator();
                if ui.button("Icon Library...").clicked() {
                    open_library = true;
                    ui.close();
                }
                if ui.button("Reset Icon Bar").clicked() {
                    reset = true;
                    ui.close();
                }
            });
        }

        // The "+" button opens the library.
        let add = ui.interact(add_rect, ui.id().with("icon-bar-add"), Sense::click());
        if add.hovered() {
            ui.painter().rect_filled(add_rect.shrink(1.0), 3.0, theme::BG_HOVER);
        }
        let c = add_rect.center();
        let tint = if self.library_open { theme::ACCENT } else { theme::TEXT_DIM };
        ui.painter().hline(c.x - 6.0..=c.x + 6.0, c.y, Stroke::new(1.5, tint));
        ui.painter().vline(c.x, c.y - 6.0..=c.y + 6.0, Stroke::new(1.5, tint));
        if add.clicked() {
            self.library_open = !self.library_open;
        }
        add.on_hover_text("Icon Library: drag icons onto the bar");

        bar_resp.context_menu(|ui| {
            if ui.button("Icon Library...").clicked() {
                open_library = true;
                ui.close();
            }
            if ui.button("Add Separator").clicked() {
                insert_separator = Some(usize::MAX);
                ui.close();
            }
            if ui.button("Reset Icon Bar").clicked() {
                reset = true;
                ui.close();
            }
            ui.separator();
            if ui.button("Hide Icon Bar").clicked() {
                commands::queue(ui.ctx(), Cmd::ToggleIconBar);
                ui.close();
            }
        });

        // Drag and drop: where the dragged icon would land, then the drop.
        if let Some(drag) = dragging {
            let target = pointer.filter(|_| over_bar).map(|p| {
                slots.iter().position(|r| p.x < r.center().x).unwrap_or(self.entries.len())
            });
            if let Some(t) = target {
                let x = if t < slots.len() { slots[t].left() - GAP * 0.5 } else { slots.last().map_or(bar.left() + 6.0, |r| r.right() + GAP * 0.5) };
                ui.painter().vline(x, bar.shrink2(vec2(0.0, 4.0)).y_range(), Stroke::new(2.0, theme::ACCENT));
            }
            if let Some(p) = pointer {
                paint_ghost(ui.ctx(), p, drag, &sh, target.is_none() && drag.from.is_some());
            }
            if ui.input(|i| i.pointer.any_released()) {
                let label = |e: Entry| match e {
                    Entry::Separator => "Separator".to_owned(),
                    Entry::Shapes => "Shapes".to_owned(),
                    Entry::Command(c) => sh.info(c).map_or_else(|| format!("{c:?}"), |i| i.name.clone()),
                };
                match (target, drag.from) {
                    (Some(t), _) => {
                        place(&mut self.entries, drag, t);
                        if drag.from.is_none() {
                            ed.status = format!("Added {} to the icon bar", label(drag.entry));
                        }
                        self.dirty = true;
                    }
                    (None, Some(from)) if from < self.entries.len() => {
                        self.entries.remove(from);
                        ed.status = format!("Removed {} from the icon bar", label(drag.entry));
                        self.dirty = true;
                    }
                    _ => {}
                }
                egui::DragAndDrop::clear_payload(ui.ctx());
            }
        }

        if let Some(i) = remove.filter(|&i| i < self.entries.len()) {
            self.entries.remove(i);
            self.dirty = true;
        }
        if let Some(i) = insert_separator {
            self.entries.insert(i.min(self.entries.len()), Entry::Separator);
            self.dirty = true;
        }
        if open_library {
            self.library_open = true;
        }
        if reset {
            self.reset();
        }
        if self.dirty {
            tidy(&mut self.entries);
        }
    }

    /// The Icon Library window: every command, grouped by menu, to drag onto
    /// the bar (or click to add at the end).
    pub fn library(&mut self, ctx: &egui::Context, ed: &mut Editor) {
        if !self.library_open {
            return;
        }
        let Some(sh) = commands::shared(ctx) else { return };
        let mut open = true;
        let mut add: Option<Entry> = None;
        egui::Window::new("Icon Library")
            .open(&mut open)
            .default_size(vec2(430.0, 470.0))
            .default_pos(pos2(320.0, 160.0))
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Drag an icon onto the bar above the viewport, or click it to add it at the end. Drag icons off the bar to remove them.")
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("e.g. shadow, cube, delete").desired_width(220.0));
                    if ui.button("Reset Bar").on_hover_text("Go back to the default icons").clicked() {
                        self.reset();
                    }
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let q = self.search.trim().to_owned();
                    let ql = q.to_lowercase();
                    let shapes_match = ql.is_empty()
                        || "shapes dropdown primitives".contains(&ql)
                        || SHAPES.iter().any(|k| k.name().to_lowercase().contains(&ql));
                    if ql.is_empty() || "separator divider".contains(&ql) || shapes_match {
                        group_heading(ui, "Layout");
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                            if shapes_match {
                                let on_bar = self.entries.contains(&Entry::Shapes);
                                if tile(ui, &sh, Entry::Shapes, on_bar, "Shapes dropdown: all the parametric shapes in one button") {
                                    add = Some(Entry::Shapes);
                                }
                            }
                            if (ql.is_empty() || "separator divider".contains(&ql))
                                && tile(ui, &sh, Entry::Separator, false, "Separator") {
                                    add = Some(Entry::Separator);
                                }
                        });
                    }
                    let mut categories: Vec<&str> = Vec::new();
                    for i in sh.infos.iter() {
                        if !categories.contains(&i.category) {
                            categories.push(i.category);
                        }
                    }
                    for cat in categories {
                        let hits: Vec<&Info> = sh
                            .infos
                            .iter()
                            .filter(|i| i.category == cat && commands::score(i, &q).is_some())
                            .collect();
                        if hits.is_empty() {
                            continue;
                        }
                        group_heading(ui, cat);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                            for info in hits {
                                let on_bar = self.entries.contains(&Entry::Command(info.cmd));
                                if tile(ui, &sh, Entry::Command(info.cmd), on_bar, &sh.tooltip(info.cmd)) {
                                    add = Some(Entry::Command(info.cmd));
                                }
                            }
                        });
                    }
                });
            });
        if let Some(e) = add {
            self.entries.push(e);
            self.dirty = true;
            tidy(&mut self.entries);
            if let Entry::Command(c) = e {
                ed.status = format!("Added {} to the icon bar", sh.info(c).map_or("icon", |i| i.name.as_str()));
            }
        }
        if !open {
            self.library_open = false;
        }
    }
}

fn group_heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().color(theme::TEXT));
}

/// A library tile. Returns true when clicked (add at the end).
fn tile(ui: &mut Ui, sh: &Shared, entry: Entry, on_bar: bool, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON + 4.0, BUTTON + 4.0), Sense::click_and_drag());
    if resp.drag_started() {
        resp.dnd_set_drag_payload(BarDrag { entry, from: None });
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, if resp.hovered() { theme::BG_HOVER } else { theme::BG_INSET });
    match entry {
        Entry::Separator => {
            painter.vline(rect.center().x, rect.shrink(7.0).y_range(), Stroke::new(1.0, theme::TEXT_DIM));
        }
        Entry::Shapes => paint_dropdown_face(painter, rect.shrink(2.0), Icon::Sphere, theme::TEXT),
        Entry::Command(cmd) => paint_face(painter, rect.shrink(2.0), sh.info(cmd), theme::TEXT),
    }
    if on_bar {
        // Already on the bar.
        painter.circle_filled(rect.right_top() + vec2(-4.0, 4.0), 2.5, theme::ACCENT);
    }
    let clicked = resp.clicked();
    resp.on_hover_text(format!("{tip}{}", if on_bar { "\n(already on the bar)" } else { "" }));
    clicked
}

/// The icon following the pointer while it is dragged.
fn paint_ghost(ctx: &egui::Context, at: egui::Pos2, drag: BarDrag, sh: &Shared, removing: bool) {
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("icon-bar-ghost")));
    let rect = Rect::from_center_size(at + vec2(10.0, 12.0), vec2(BUTTON, BUTTON));
    painter.rect_filled(rect, 4.0, theme::BG_HOVER.gamma_multiply(0.95));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, if removing { theme::AXIS_X } else { theme::ACCENT }), StrokeKind::Inside);
    match drag.entry {
        Entry::Separator => {
            painter.vline(rect.center().x, rect.shrink(7.0).y_range(), Stroke::new(1.0, theme::TEXT));
        }
        Entry::Shapes => paint_dropdown_face(&painter, rect.shrink(1.0), icons::for_primitive(last_shape(ctx)), theme::TEXT),
        Entry::Command(cmd) => paint_face(&painter, rect.shrink(1.0), sh.info(cmd), theme::TEXT),
    }
    if removing {
        let c = rect.right_top();
        painter.circle_filled(c, 7.0, theme::AXIS_X);
        icons::paint(&painter, Rect::from_center_size(c, vec2(9.0, 9.0)), Icon::Delete, Color32::WHITE);
        painter.text(rect.left_bottom() + vec2(0.0, 6.0), Align2::LEFT_TOP, "Release to remove", FontId::proportional(11.0), theme::TEXT);
    }
}

// --- the Shapes dropdown -------------------------------------------------------

fn shapes_open_id() -> Id {
    Id::new("icon-bar-shapes-open")
}

fn shapes_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(shapes_open_id())).unwrap_or(false)
}

fn set_shapes_open(ctx: &egui::Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(shapes_open_id(), open));
}

/// The shape made last from the dropdown (shown on its button).
fn last_shape(ctx: &egui::Context) -> PrimitiveKind {
    ctx.data(|d| d.get_temp::<usize>(Id::new("icon-bar-shapes-last")))
        .and_then(|i| SHAPES.get(i).copied())
        .unwrap_or(PrimitiveKind::Sphere)
}

/// An icon with a small "more" triangle in its corner.
fn paint_dropdown_face(painter: &egui::Painter, r: Rect, icon: Icon, tint: Color32) {
    icons::paint(painter, r.shrink(6.0), icon, tint);
    let c = r.right_bottom() - vec2(3.0, 3.0);
    painter.add(egui::Shape::convex_polygon(vec![c, c - vec2(6.0, 0.0), c - vec2(0.0, 6.0)], tint, Stroke::NONE));
}

fn paint_dropdown_button(ui: &Ui, rect: Rect, icon: Icon, hovered: bool, open: bool) {
    let r = rect.shrink(1.0);
    if open {
        ui.painter().rect_filled(r, 3.0, theme::PALETTE_ACTIVE);
    } else if hovered {
        ui.painter().rect_filled(r, 3.0, theme::BG_HOVER);
    }
    paint_dropdown_face(ui.painter(), r, icon, if open { Color32::WHITE } else { theme::TEXT });
}

/// The open dropdown: one row per shape, icon and name. Picking one makes it.
fn shapes_dropdown(ui: &Ui, button: Rect) {
    let ctx = ui.ctx().clone();
    let area = egui::Area::new(Id::new("icon-bar-shapes-popup"))
        .order(Order::Foreground)
        .fixed_pos(button.left_bottom() + vec2(0.0, 2.0))
        .show(&ctx, |ui| {
            egui::Frame::menu(ui.style()).fill(theme::BG_PANEL).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.set_min_width(150.0);
                for (i, kind) in SHAPES.iter().enumerate() {
                    if super::widgets::menu_item(ui, Some(icons::for_primitive(*kind)), kind.name(), "").clicked() {
                        commands::queue(ui.ctx(), Cmd::Primitive(*kind));
                        ui.ctx().data_mut(|d| d.insert_temp(Id::new("icon-bar-shapes-last"), i));
                        set_shapes_open(ui.ctx(), false);
                    }
                }
            });
        });
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !button.contains(p))
    });
    if pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        set_shapes_open(&ctx, false);
    }
}

fn save(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gizmo::Tool;

    fn c(cmd: Cmd) -> Entry {
        Entry::Command(cmd)
    }

    #[test]
    fn moving_right_and_left_lands_where_shown() {
        let a = c(Cmd::Undo);
        let b = c(Cmd::Redo);
        let d = c(Cmd::Delete);
        let mut v = vec![a, b, d];
        // Drag A to the gap after D (position 3).
        place(&mut v, BarDrag { entry: a, from: Some(0) }, 3);
        assert_eq!(v, vec![b, d, a]);
        // Drag A back before B (position 0).
        place(&mut v, BarDrag { entry: a, from: Some(2) }, 0);
        assert_eq!(v, vec![a, b, d]);
        // Dropping onto its own gap changes nothing.
        place(&mut v, BarDrag { entry: b, from: Some(1) }, 1);
        assert_eq!(v, vec![a, b, d]);
        // From the library: inserted, nothing removed.
        let t = c(Cmd::Tool(Tool::Move));
        place(&mut v, BarDrag { entry: t, from: None }, 1);
        assert_eq!(v, vec![a, t, b, d]);
    }

    #[test]
    fn tidy_drops_stray_separators() {
        let s = Entry::Separator;
        let mut v = vec![s, c(Cmd::Undo), s, s, c(Cmd::Redo), s];
        tidy(&mut v);
        assert_eq!(v, vec![c(Cmd::Undo), s, c(Cmd::Redo)]);
    }

    #[test]
    fn saved_layout_round_trips_and_skips_unknown_ids() {
        let infos = commands::all();
        let bar = default_entries();
        assert_eq!(from_json(&to_json(&bar, &infos), &infos), Some(bar));
        let text = r#"{"version":3,"items":["edit.undo","|","no.such.command","view.shadows"]}"#;
        assert_eq!(
            from_json(text, &infos),
            Some(vec![c(Cmd::Undo), Entry::Separator, c(Cmd::ViewFx(ViewFx::Shadows))])
        );
        // A bar saved before the Shapes dropdown existed gets it in front, once.
        let old = r#"{"version":1,"items":["edit.undo"]}"#;
        let mut expected = vec![Entry::Shapes];
        expected.extend(modelling_entries());
        expected.extend([Entry::Separator, c(Cmd::Undo)]);
        assert_eq!(from_json(old, &infos), Some(expected));
        // Version 2 had the Shapes dropdown: only the new tools are added after it.
        let v2 = r#"{"version":2,"items":["group.shapes","|","edit.undo","mesh.polygon_pen"]}"#;
        let mut expected = vec![Entry::Shapes, c(Cmd::Pen(PenKind::Spline)), c(Cmd::Generator(GeneratorKind::Subdivision))];
        expected.extend([Entry::Separator, c(Cmd::Undo), c(Cmd::Pen(PenKind::Polygon))]);
        assert_eq!(from_json(v2, &infos), Some(expected), "never a second copy");
        let removed = r#"{"version":3,"items":["edit.undo"]}"#;
        assert_eq!(from_json(removed, &infos), Some(vec![c(Cmd::Undo)]), "removing them sticks");
        assert_eq!(from_json("not json", &infos), None);
    }

    #[test]
    fn every_default_entry_is_a_registered_command_with_an_icon() {
        let infos = commands::all();
        for e in default_entries() {
            if let Entry::Command(cmd) = e {
                let info = infos.iter().find(|i| i.cmd == cmd).expect("registered");
                assert!(info.icon.is_some(), "{} has no icon", info.name);
            }
        }
    }

    #[test]
    fn shapes_dropdown_lists_every_requested_shape_once() {
        let names: Vec<&str> = SHAPES.iter().map(|k| k.name()).collect();
        assert_eq!(names, ["Sphere", "Cube", "Cone", "Pyramid", "Tube", "Cylinder", "Capsule", "Plane", "Torus", "Landscape", "Figure"]);
        for k in PrimitiveKind::ALL {
            assert!(SHAPES.contains(&k), "{k:?} missing from the dropdown");
        }
        assert_eq!(default_entries()[0], Entry::Shapes);
    }

    #[test]
    fn initials_for_iconless_commands() {
        assert_eq!(initials("Select All"), "SA");
        assert_eq!(initials("Weld"), "We");
        assert_eq!(initials("Open..."), "Op");
    }

    #[test]
    fn missing_file_gives_default_bar() {
        let infos = commands::all();
        let bar = IconBar::load_from(&infos, Some(PathBuf::from("/nonexistent/forma3d/icon_bar.json")));
        assert_eq!(bar.entries, default_entries());
    }
}
