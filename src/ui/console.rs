//! The Console: a log of what the app did (every status message, with the
//! project and time) and a command line that runs any command by name.

use super::commands::{self, Cmd, Info};
use super::widgets::header_bar;
use crate::theme;
use eframe::egui::{self, Color32, FontId, Key, RichText, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Info,
    Command,
    Error,
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// Seconds since the app started.
    pub time: f64,
    pub project: String,
    pub text: String,
    pub kind: EntryKind,
}

const MAX_ENTRIES: usize = 2000;

#[derive(Default)]
pub struct Console {
    pub entries: Vec<Entry>,
    input: String,
    filter: String,
    /// Previous command lines, newest last.
    history: Vec<String>,
    history_pos: Option<usize>,
    /// The status line last logged, per project (by its name).
    last_status: String,
}

impl Console {
    pub fn log(&mut self, time: f64, project: &str, text: impl Into<String>, kind: EntryKind) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        self.entries.push(Entry { time, project: project.to_owned(), text, kind });
        if self.entries.len() > MAX_ENTRIES {
            let extra = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..extra);
        }
    }

    /// Logs the editor's status line when it changes.
    pub fn watch_status(&mut self, time: f64, project: &str, status: &str) {
        if status != self.last_status {
            self.last_status = status.to_owned();
            let lower = status.to_lowercase();
            let kind = if lower.contains("could not") || lower.contains("error") || lower.contains("failed") { EntryKind::Error } else { EntryKind::Info };
            self.log(time, project, status, kind);
        }
    }
}

/// Finds the command a typed line names: its id, its exact name, or the one
/// command whose name contains every word.
pub fn resolve<'a>(infos: &'a [Info], line: &str) -> Result<&'a Info, String> {
    let q = line.trim().to_lowercase();
    if q.is_empty() {
        return Err("type a command name".into());
    }
    if let Some(i) = infos.iter().find(|i| i.id == q || i.name.to_lowercase() == q) {
        return Ok(i);
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let hits: Vec<&Info> = infos
        .iter()
        .filter(|i| {
            let hay = format!("{} {}", i.name, i.id).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect();
    match hits.len() {
        0 => Err(format!("no command matches \"{}\"", line.trim())),
        1 => Ok(hits[0]),
        n => Err(format!(
            "{n} commands match \"{}\": {}{}",
            line.trim(),
            hits.iter().take(6).map(|i| i.name.as_str()).collect::<Vec<_>>().join(", "),
            if n > 6 { ", ..." } else { "" }
        )),
    }
}

pub fn show(ui: &mut Ui, console: &mut Console, time: f64, project: &str) {
    header_bar(ui, |ui| {
        if ui.button("Clear").clicked() {
            console.entries.clear();
        }
        ui.add(egui::TextEdit::singleline(&mut console.filter).hint_text("Filter").desired_width(140.0));
        ui.colored_label(theme::TEXT_DIM, format!("{} messages", console.entries.len()));
    });

    let input_h = 40.0;
    let list_h = (ui.available_height() - input_h).max(40.0);
    let filter = console.filter.to_lowercase();
    egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for e in console.entries.iter().filter(|e| filter.is_empty() || e.text.to_lowercase().contains(&filter) || e.project.to_lowercase().contains(&filter)) {
            let secs = e.time as u64;
            let stamp = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
            let color = match e.kind {
                EntryKind::Info => theme::TEXT,
                EntryKind::Command => Color32::from_rgb(140, 190, 255),
                EntryKind::Error => Color32::from_rgb(235, 120, 100),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(stamp).font(FontId::monospace(11.5)).color(theme::TEXT_DIM));
                ui.label(RichText::new(&e.project).font(FontId::monospace(11.5)).color(theme::TEXT_DIM));
                ui.label(RichText::new(&e.text).font(FontId::monospace(12.0)).color(color));
            });
        }
    });

    // The command line.
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new(">").font(FontId::monospace(13.0)).color(theme::ACCENT));
        let edit = ui.add(
            egui::TextEdit::singleline(&mut console.input)
                .font(FontId::monospace(12.5))
                .hint_text("Type a command (e.g. \"cube\", \"frame all\", \"help\") and press Enter")
                .desired_width(f32::INFINITY),
        );
        if edit.has_focus() {
            let (up, down) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
            if (up || down) && !console.history.is_empty() {
                let n = console.history.len();
                let pos = match (console.history_pos, up) {
                    (None, true) => Some(n - 1),
                    (Some(p), true) => Some(p.saturating_sub(1)),
                    (Some(p), false) if p + 1 < n => Some(p + 1),
                    _ => None,
                };
                console.history_pos = pos;
                console.input = pos.map(|p| console.history[p].clone()).unwrap_or_default();
            }
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let line = std::mem::take(&mut console.input);
            console.history_pos = None;
            if !line.trim().is_empty() {
                console.history.retain(|h| *h != line);
                console.history.push(line.clone());
            }
            run_line(ui, console, &line, time, project);
            edit.request_focus();
        }
    });
}

fn run_line(ui: &Ui, console: &mut Console, line: &str, time: f64, project: &str) {
    let trimmed = line.trim();
    console.log(time, project, format!("> {trimmed}"), EntryKind::Command);
    match trimmed.to_lowercase().as_str() {
        "" => {}
        "clear" | "cls" => console.entries.clear(),
        "help" | "?" => {
            for text in [
                "Type any command's name (or part of it) to run it: \"cube\", \"frame all\", \"enable snap\", \"workspace: uv edit\".",
                "Command ids work too, e.g. \"view.frame_all\". \"list <words>\" shows matching commands; \"clear\" empties the log.",
                "Up / Down recall earlier lines.",
            ] {
                console.log(time, project, text, EntryKind::Info);
            }
        }
        _ if trimmed.to_lowercase().starts_with("list") => {
            let Some(sh) = commands::shared(ui.ctx()) else { return };
            let words: Vec<String> = trimmed[4..].split_whitespace().map(str::to_lowercase).collect();
            let hits: Vec<String> = sh
                .infos
                .iter()
                .filter(|i| {
                    let hay = format!("{} {} {}", i.name, i.id, i.keywords).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .map(|i| format!("{}  ({})", i.name, i.id))
                .collect();
            console.log(time, project, format!("{} command(s)", hits.len()), EntryKind::Info);
            for h in hits.into_iter().take(60) {
                console.log(time, project, h, EntryKind::Info);
            }
        }
        _ => {
            let Some(sh) = commands::shared(ui.ctx()) else { return };
            match resolve(&sh.infos, trimmed) {
                Ok(info) => {
                    let cmd: Cmd = info.cmd;
                    console.log(time, project, format!("running {}", info.name), EntryKind::Info);
                    commands::queue(ui.ctx(), cmd);
                }
                Err(e) => console.log(time, project, e, EntryKind::Error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_lines_find_commands() {
        let infos = commands::all();
        assert_eq!(resolve(&infos, "view.frame_all").unwrap().cmd, Cmd::FrameAll);
        assert_eq!(resolve(&infos, "Frame All").unwrap().cmd, Cmd::FrameAll);
        assert_eq!(resolve(&infos, "enable snap").unwrap().cmd, Cmd::ToggleSnap);
        assert!(resolve(&infos, "frame").unwrap_err().contains("commands match"));
        assert!(resolve(&infos, "no such thing").is_err());
    }

    #[test]
    fn status_changes_are_logged_once() {
        let mut c = Console::default();
        c.watch_status(1.0, "Untitled", "Saved");
        c.watch_status(2.0, "Untitled", "Saved");
        c.watch_status(3.0, "Untitled", "Could not save: disk full");
        assert_eq!(c.entries.len(), 2);
        assert_eq!(c.entries[1].kind, EntryKind::Error);
    }
}
