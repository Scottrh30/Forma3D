//! Edit > Preferences: settings that apply to the whole app, saved next to
//! the keyboard shortcuts as `prefs.json`.

use super::commands;
use crate::editor::Editor;
use crate::renderer::DisplayMode;
use crate::theme;
use eframe::egui::{self, Ui};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Startup {
    /// The cube, sphere and torus starter scene.
    Demo,
    /// An empty scene.
    Empty,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Size of the whole interface, in percent.
    pub ui_scale: f32,
    /// Undo steps kept per project.
    pub undo_steps: usize,
    /// What the app opens with.
    pub startup: Startup,
    /// Shading for new projects (a `DisplayMode` id).
    pub new_shading: String,
    /// New projects show the floor grid.
    pub new_grid: bool,
    /// Minutes between autosaves of changed projects (0 = off).
    pub autosave_minutes: u32,
    /// Ask before closing or reverting a project with unsaved changes.
    pub confirm_discard: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            ui_scale: 100.0,
            undo_steps: crate::history::DEFAULT_STEPS,
            startup: Startup::Demo,
            new_shading: DisplayMode::GouraudLines.id().to_owned(),
            new_grid: true,
            autosave_minutes: 0,
            confirm_discard: true,
        }
    }
}

impl Prefs {
    pub fn sanitized(&self) -> Prefs {
        Prefs {
            ui_scale: if self.ui_scale.is_finite() { self.ui_scale.clamp(75.0, 200.0) } else { 100.0 },
            undo_steps: self.undo_steps.clamp(10, 2000),
            startup: self.startup,
            new_shading: if DisplayMode::ALL.iter().any(|m| m.id() == self.new_shading) {
                self.new_shading.clone()
            } else {
                DisplayMode::GouraudLines.id().to_owned()
            },
            new_grid: self.new_grid,
            autosave_minutes: self.autosave_minutes.min(120),
            confirm_discard: self.confirm_discard,
        }
    }

    pub fn shading(&self) -> DisplayMode {
        DisplayMode::ALL.into_iter().find(|m| m.id() == self.new_shading).unwrap_or(DisplayMode::GouraudLines)
    }

    /// Settings every project follows (undo steps).
    pub fn apply_to(&self, ed: &mut Editor) {
        ed.history.set_limit(self.undo_steps);
    }

    /// Settings for a project made with New Project.
    pub fn apply_to_new(&self, ed: &mut Editor) {
        self.apply_to(ed);
        ed.display = self.shading();
        ed.view_opts.grid = self.new_grid;
    }

    pub fn path() -> Option<PathBuf> {
        commands::keymap_path().map(|p| p.with_file_name("prefs.json"))
    }

    pub fn load(path: Option<&Path>) -> Prefs {
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Prefs>(&t).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    /// Where autosaves go: a folder of their own, never the project's file.
    pub fn autosave_dir() -> Option<PathBuf> {
        commands::keymap_path().map(|p| p.with_file_name("autosave"))
    }
}

/// What the window asks the app to do besides changing settings.
#[derive(Default)]
pub struct WindowResult {
    pub changed: bool,
    pub open_shortcuts: bool,
}

/// The Preferences window. `prefs` is edited in place.
pub fn window(ctx: &egui::Context, open: &mut bool, prefs: &mut Prefs, saved_to: &str) -> WindowResult {
    let mut result = WindowResult::default();
    if !*open {
        return result;
    }
    let before = prefs.clone();
    egui::Window::new("Preferences")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_pos(egui::pos2(360.0, 140.0))
        .show(ctx, |ui| {
            section(ui, "Interface");
            egui::Grid::new("prefs-ui").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                ui.label("Interface Scale");
                ui.add(egui::Slider::new(&mut prefs.ui_scale, 75.0..=200.0).step_by(5.0).suffix(" %"));
                ui.end_row();
                ui.label("Keyboard Shortcuts");
                if ui.button("Customize Commands...").clicked() {
                    result.open_shortcuts = true;
                }
                ui.end_row();
            });

            section(ui, "Files");
            egui::Grid::new("prefs-files").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                ui.label("On Startup");
                egui::ComboBox::from_id_salt("prefs-startup")
                    .selected_text(match prefs.startup {
                        Startup::Demo => "Starter scene",
                        Startup::Empty => "Empty scene",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut prefs.startup, Startup::Demo, "Starter scene");
                        ui.selectable_value(&mut prefs.startup, Startup::Empty, "Empty scene");
                    });
                ui.end_row();
                ui.label("Autosave");
                ui.horizontal(|ui| {
                    let mut on = prefs.autosave_minutes > 0;
                    if ui.checkbox(&mut on, "every").changed() {
                        prefs.autosave_minutes = if on { 5 } else { 0 };
                    }
                    // Edit a copy while off: the box's 1-minute minimum must not
                    // quietly switch autosave on.
                    let mut minutes = if on { prefs.autosave_minutes } else { 5 };
                    ui.add_enabled(on, egui::DragValue::new(&mut minutes).range(1..=120).suffix(" min"));
                    if on {
                        prefs.autosave_minutes = minutes;
                    }
                });
                ui.end_row();
                ui.label("");
                ui.colored_label(theme::TEXT_DIM, "Changed projects are copied to the autosave folder; your files are not touched.");
                ui.end_row();
                ui.label("Unsaved Changes");
                ui.checkbox(&mut prefs.confirm_discard, "Ask before closing or reverting");
                ui.end_row();
            });

            section(ui, "New Projects");
            egui::Grid::new("prefs-new").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                ui.label("Shading");
                egui::ComboBox::from_id_salt("prefs-shading").selected_text(prefs.shading().label()).show_ui(ui, |ui| {
                    for m in DisplayMode::ALL {
                        ui.selectable_value(&mut prefs.new_shading, m.id().to_owned(), m.label());
                    }
                });
                ui.end_row();
                ui.label("Grid");
                ui.checkbox(&mut prefs.new_grid, "Show the floor grid");
                ui.end_row();
            });

            section(ui, "Memory");
            egui::Grid::new("prefs-mem").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                ui.label("Undo Steps");
                ui.add(egui::DragValue::new(&mut prefs.undo_steps).range(10..=2000).speed(1.0));
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Reset to Defaults").clicked() {
                    *prefs = Prefs::default();
                }
                ui.colored_label(theme::TEXT_DIM, format!("Saved to {saved_to}"));
            });
        });
    *prefs = prefs.sanitized();
    result.changed = *prefs != before;
    result
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    super::widgets::heading(ui, title);
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    #[test]
    fn defaults_round_trip_and_bad_values_are_fixed() {
        let p = Prefs::default();
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Prefs>(&text).unwrap(), p);
        // Older or partial files fill in the rest.
        let partial: Prefs = serde_json::from_str(r#"{"ui_scale": 125}"#).unwrap();
        assert_eq!(partial.ui_scale, 125.0);
        assert_eq!(partial.undo_steps, Prefs::default().undo_steps);
        let bad = Prefs { ui_scale: 9000.0, undo_steps: 1, new_shading: "nope".into(), autosave_minutes: 999, ..Prefs::default() };
        let fixed = bad.sanitized();
        assert_eq!((fixed.ui_scale, fixed.undo_steps, fixed.autosave_minutes), (200.0, 10, 120));
        assert_eq!(fixed.shading(), DisplayMode::GouraudLines);
        assert_eq!(Prefs::load(Some(Path::new("/nonexistent/prefs.json"))), Prefs::default());
    }

    #[test]
    fn new_projects_take_the_settings() {
        let p = Prefs { new_shading: DisplayMode::Quick.id().into(), new_grid: false, undo_steps: 15, ..Prefs::default() };
        let mut ed = Editor::new(Scene::new());
        p.apply_to_new(&mut ed);
        assert_eq!(ed.display, DisplayMode::Quick);
        assert!(!ed.view_opts.grid);
        assert_eq!(ed.history.limit(), 15);
    }
}
