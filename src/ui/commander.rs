//! The Commander (Shift+C): type part of a command's name and press Enter.
//! And Customize Commands: rebind any command's keyboard shortcut.

use super::commands::{self, Binding, Cmd, Conflict, Info, Keymap, Shared, Shortcut};
use crate::editor::Editor;
use crate::icons;
use crate::theme;
use eframe::egui::{self, Align2, Color32, Event, FontId, Key, Modifiers, Rect, Sense, Stroke, pos2, vec2};

const ROW_H: f32 = 26.0;
const MAX_ROWS: usize = 12;

#[derive(Default)]
pub struct Commander {
    pub open: bool,
    query: String,
    selected: usize,
    focus: bool,
}

impl Commander {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.focus = true;
    }

    /// Draws the Commander; returns the command to run, if one was chosen.
    /// `extra` holds scene-dependent commands (one Material Tag per material).
    pub fn show(&mut self, ctx: &egui::Context, ed: &Editor, sh: &Shared, extra: &[Info], recent: &[Cmd]) -> Option<Cmd> {
        if !self.open {
            return None;
        }
        let all: Vec<Info> = sh.infos.iter().cloned().chain(extra.iter().cloned()).collect();
        // Results: recent commands first when nothing is typed.
        let results: Vec<&Info> = if self.query.trim().is_empty() {
            let mut list: Vec<&Info> = recent.iter().filter_map(|c| all.iter().find(|i| i.cmd == *c)).collect();
            list.extend(all.iter().filter(|i| !recent.contains(&i.cmd)));
            list
        } else {
            commands::search(&all, &self.query)
        };
        let count = results.len();
        if count > 0 {
            self.selected = self.selected.min(count - 1);
        }
        let (down, up, enter, escape) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        if down && count > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }

        let screen = ctx.content_rect();
        let width = 540.0f32.min(screen.width() - 40.0);
        let mut chosen = None;
        let mut close = escape;
        let area = egui::Area::new(egui::Id::new("commander"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.center().x - width * 0.5, screen.top() + 90.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(theme::BG_PANEL).stroke(Stroke::new(1.0, theme::BORDER_LIGHT)).show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Commander").strong());
                        ui.colored_label(theme::TEXT_DIM, "type to search · Up/Down · Enter runs · Esc closes");
                    });
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search commands...")
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(15.0)),
                    );
                    if self.focus {
                        edit.request_focus();
                        self.focus = false;
                    }
                    if edit.changed() {
                        self.selected = 0;
                    }
                    ui.add_space(4.0);
                    if results.is_empty() {
                        ui.colored_label(theme::TEXT_DIM, "  No matching command");
                        return;
                    }
                    let layout = sh.layout;
                    egui::ScrollArea::vertical().max_height(ROW_H * MAX_ROWS as f32).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, info) in results.iter().enumerate() {
                            let (enabled, checked) = commands::state(ed, info.cmd, layout);
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                                self.selected = i;
                            }
                            let sel = i == self.selected;
                            if sel {
                                ui.painter().rect_filled(rect, 3.0, theme::MENU_HOVER);
                                if down || up {
                                    resp.scroll_to_me(None);
                                }
                            }
                            paint_row(ui, rect, info, enabled, checked, &sh.keymap, sel);
                            if resp.clicked() && enabled {
                                chosen = Some(info.cmd);
                            }
                        }
                    });
                });
            });
        if enter
            && let Some(info) = results.get(self.selected)
                && commands::state(ed, info.cmd, sh.layout).0 {
                    chosen = Some(info.cmd);
                }
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p))
        });
        if chosen.is_some() || clicked_outside {
            close = true;
        }
        if close {
            self.open = false;
        }
        chosen
    }
}

fn paint_row(ui: &egui::Ui, rect: Rect, info: &Info, enabled: bool, checked: Option<bool>, keymap: &Keymap, sel: bool) {
    let painter = ui.painter();
    let color = if !enabled {
        theme::TEXT_DIM.gamma_multiply(0.7)
    } else if sel {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(16.0, 16.0));
    if let Some(icon) = info.icon {
        icons::paint(painter, icon_rect, icon, color);
    }
    let mut name = info.name.clone();
    if checked == Some(true) {
        name.push_str("  (on)");
    }
    let galley = painter.layout_no_wrap(name, FontId::proportional(13.5), color);
    let name_w = galley.size().x;
    painter.galley(pos2(rect.left() + 34.0, rect.center().y - galley.size().y * 0.5), galley, color);
    painter.text(
        pos2(rect.left() + 44.0 + name_w, rect.center().y),
        Align2::LEFT_CENTER,
        info.category,
        FontId::proportional(11.5),
        theme::TEXT_DIM,
    );
    let key = keymap.label(info.cmd);
    if !key.is_empty() {
        painter.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, key, FontId::proportional(12.5), theme::TEXT_DIM);
    }
}

// ---------------------------------------------------------------------------
// Customize Commands
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Customize {
    pub open: bool,
    filter: String,
    /// The command waiting for its new shortcut.
    pub capturing: Option<Cmd>,
    /// First key of the shortcut being typed (a second key makes a sequence).
    first: Option<Shortcut>,
    message: String,
}

impl Customize {
    pub fn open(&mut self) {
        self.open = true;
        self.capturing = None;
        self.first = None;
        self.message.clear();
    }

    fn finish(&mut self, keymap: &mut Keymap, infos: &[Info], cmd: Cmd, binding: Binding) -> bool {
        let name = |c: Cmd| infos.iter().find(|i| i.cmd == c).map_or("?".to_owned(), |i| i.name.clone());
        let key = binding.label();
        let (msg, changed) = match keymap.assign(cmd, binding) {
            Ok(Some(other)) => (format!("{key} now runs {} (taken from {})", name(cmd), name(other)), true),
            Ok(None) => (format!("{key} now runs {}", name(cmd)), true),
            Err(Conflict::StartsSequences(cmds)) => {
                // Show the sequences themselves (M~S Bevel), not the commands' other keys.
                let list: Vec<String> = cmds
                    .iter()
                    .take(5)
                    .filter_map(|c| {
                        let seq = keymap.shortcuts(*c).into_iter().find(|b| b.then.is_some() && b.first == binding.first)?;
                        Some(format!("{} {}", seq.label(), name(*c)))
                    })
                    .collect();
                (format!("{key} starts other shortcuts ({}), so it can't be used alone. Add a second key instead.", list.join(", ")), false)
            }
            Err(Conflict::FirstKeyIsShortcut(other)) => {
                (format!("{} is the shortcut for {}, so it can't start a sequence", binding.first.label(), name(other)), false)
            }
        };
        self.message = msg;
        self.capturing = None;
        self.first = None;
        changed
    }

    /// Draws the window; returns true when the keymap changed.
    pub fn show(&mut self, ctx: &egui::Context, infos: &[Info], keymap: &mut Keymap, saved_to: &str) -> bool {
        if !self.open {
            return false;
        }
        let mut changed = false;
        // Key capture comes first so the key never reaches anything else.
        if let Some(cmd) = self.capturing {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                let plain = m == Modifiers::NONE;
                match self.first {
                    None if key == Key::Escape && plain => {
                        self.message = "Cancelled".to_owned();
                        self.capturing = None;
                    }
                    None if key == Key::Backspace && plain => {
                        keymap.clear(cmd);
                        self.message = "Shortcut removed".to_owned();
                        self.capturing = None;
                        changed = true;
                    }
                    None if commands::bindable(key) => {
                        let sc = Shortcut::from_event(key, m);
                        self.first = Some(sc);
                        self.message = format!(
                            "{}~ ... press a second key to make a sequence (like M~S), or Enter to use {} on its own",
                            sc.label(),
                            sc.label()
                        );
                    }
                    None => self.message = format!("{} can't be used as a shortcut", key.name()),
                    Some(_) if key == Key::Escape && plain => {
                        self.message = "Cancelled".to_owned();
                        self.capturing = None;
                        self.first = None;
                    }
                    Some(first) if key == Key::Enter => {
                        changed |= self.finish(keymap, infos, cmd, Binding::single(first));
                    }
                    Some(first) if commands::bindable(key) => {
                        let binding = Binding { first, then: Some(Shortcut::from_event(key, m)) };
                        changed |= self.finish(keymap, infos, cmd, binding);
                    }
                    Some(_) => self.message = format!("{} can't be used as a shortcut", key.name()),
                }
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_))));
            }
        }

        let mut open = self.open;
        egui::Window::new("Customize Commands")
            .open(&mut open)
            .default_size([600.0, 520.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("command name").desired_width(220.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset All").clicked() {
                            keymap.reset_all();
                            self.message = "All shortcuts back to the defaults".to_owned();
                            changed = true;
                        }
                    });
                });
                ui.colored_label(
                    theme::TEXT_DIM,
                    "Click a shortcut, then press the new key. Press a second key for a sequence (M~S) or Enter \
                     to keep one key. Esc cancels, Backspace removes the shortcut.",
                );
                if !self.message.is_empty() {
                    ui.colored_label(theme::ACCENT, &self.message);
                }
                ui.separator();
                let list = commands::search(infos, &self.filter);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("keymap-grid").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                        for info in list {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                if let Some(icon) = info.icon {
                                    icons::paint(ui.painter(), r, icon, theme::TEXT);
                                }
                                ui.label(&info.name);
                            });
                            ui.colored_label(theme::TEXT_DIM, info.category);
                            let capturing = self.capturing == Some(info.cmd);
                            let text = match (capturing, self.first) {
                                (true, Some(f)) => format!("{}~ ...", f.label()),
                                (true, None) => "Press keys...".to_owned(),
                                _ => {
                                    let all: Vec<String> = keymap.shortcuts(info.cmd).iter().map(Binding::label).collect();
                                    if all.is_empty() { "—".to_owned() } else { all.join(", ") }
                                }
                            };
                            let button = egui::Button::new(text).min_size(vec2(130.0, 0.0)).selected(capturing);
                            if ui.add(button).on_hover_text("Click, then press the new shortcut").clicked() {
                                self.capturing = if capturing { None } else { Some(info.cmd) };
                                self.first = None;
                                self.message.clear();
                            }
                            if keymap.is_custom(info.cmd) {
                                if ui.small_button("Reset").on_hover_text("Back to the default shortcut").clicked() {
                                    keymap.reset(info.cmd);
                                    changed = true;
                                }
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.colored_label(theme::TEXT_DIM, format!("Saved to {saved_to}"));
            });
        if !open {
            self.capturing = None;
            self.first = None;
        }
        self.open = open;
        changed
    }
}
