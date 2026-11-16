//! Edit > Preferences: settings for the whole app, saved as `prefs.json`
//! in the app's settings folder (the browser's storage outside Tauri).

use crate::state::app;
use forma3d_core::editor::Editor;
use forma3d_core::renderer::DisplayMode;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

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
    pub startup: Startup,
    /// Shading for new projects (a `DisplayMode` id).
    pub new_shading: String,
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
            undo_steps: forma3d_core::history::DEFAULT_STEPS,
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
            ui_scale: if self.ui_scale.is_finite() {
                self.ui_scale.clamp(75.0, 200.0)
            } else {
                100.0
            },
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
        DisplayMode::ALL
            .into_iter()
            .find(|m| m.id() == self.new_shading)
            .unwrap_or(DisplayMode::GouraudLines)
    }

    /// Settings every project follows.
    pub fn apply_to(&self, ed: &mut Editor) {
        ed.history.set_limit(self.undo_steps);
    }

    /// Settings for a project made with New Project.
    pub fn apply_to_new(&self, ed: &mut Editor) {
        self.apply_to(ed);
        ed.display = self.shading();
        ed.view_opts.grid = self.new_grid;
    }
}

fn row(label: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <div class="prow">
            <span class="plabel">{label}</span>
            <div class="pval">{body}</div>
        </div>
    }
}

/// The Preferences dialog.
#[component]
pub fn PrefsDialog() -> impl IntoView {
    let app = app();
    let p = app.prefs;
    let set = move |f: Box<dyn FnOnce(&mut Prefs)>| {
        let mut next = p.get_untracked();
        f(&mut next);
        let next = next.sanitized();
        if next != p.get_untracked() {
            p.set(next.clone());
            app.each_project(|e| next.apply_to(e));
            crate::settings::save_prefs(app);
        }
    };
    view! {
        <Show when=move || app.prefs_open.get()>
            <div class="modal-bg" on:mousedown=move |_| app.prefs_open.set(false)>
                <div class="dialog prefs" on:mousedown=|ev| ev.stop_propagation()>
                    <h2>"Preferences"</h2>
                    <h3>"Interface"</h3>
                    {row("Interface Scale", view! {
                        <input
                            type="range"
                            min="75"
                            max="200"
                            step="5"
                            prop:value=move || p.with(|p| p.ui_scale.to_string())
                            on:input=move |ev| {
                                let v: f32 = event_target_value(&ev).parse().unwrap_or(100.0);
                                set(Box::new(move |p| p.ui_scale = v));
                            }
                        />
                        <span class="hint">{move || format!("{:.0} %", p.with(|p| p.ui_scale))}</span>
                    })}
                    {row("Keyboard Shortcuts", view! {
                        <button class="btn" on:click=move |_| {
                            app.prefs_open.set(false);
                            app.customize_open.set(true);
                        }>"Customize Commands..."</button>
                    })}
                    <h3>"Files"</h3>
                    {row("On Startup", view! {
                        <select class="sel" on:change=move |ev| {
                            let demo = event_target_value(&ev) == "demo";
                            set(Box::new(move |p| p.startup = if demo { Startup::Demo } else { Startup::Empty }));
                        }>
                            <option value="demo" selected=move || p.with(|p| p.startup == Startup::Demo)>"Starter scene"</option>
                            <option value="empty" selected=move || p.with(|p| p.startup == Startup::Empty)>"Empty scene"</option>
                        </select>
                    })}
                    {row("Autosave", view! {
                        <label class="check">
                            <input
                                type="checkbox"
                                prop:checked=move || p.with(|p| p.autosave_minutes > 0)
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    set(Box::new(move |p| p.autosave_minutes = if on { 5 } else { 0 }));
                                }
                            />
                            "every"
                        </label>
                        <input
                            class="text-in small"
                            type="number"
                            min="1"
                            max="120"
                            disabled=move || p.with(|p| p.autosave_minutes == 0)
                            prop:value=move || p.with(|p| if p.autosave_minutes == 0 { 5 } else { p.autosave_minutes }.to_string())
                            on:change=move |ev| {
                                let m: u32 = event_target_value(&ev).parse().unwrap_or(5);
                                set(Box::new(move |p| if p.autosave_minutes > 0 { p.autosave_minutes = m.max(1) }));
                            }
                        />
                        <span class="hint">"min"</span>
                    })}
                    <p class="hint">"Changed projects are copied to the autosave folder; your files are not touched."</p>
                    {row("Unsaved Changes", view! {
                        <label class="check">
                            <input
                                type="checkbox"
                                prop:checked=move || p.with(|p| p.confirm_discard)
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    set(Box::new(move |p| p.confirm_discard = on));
                                }
                            />
                            "Ask before closing or reverting"
                        </label>
                    })}
                    <h3>"New Projects"</h3>
                    {row("Shading", view! {
                        <select class="sel" on:change=move |ev| {
                            let id = event_target_value(&ev);
                            set(Box::new(move |p| p.new_shading = id));
                        }>
                            {DisplayMode::ALL
                                .map(|m| view! { <option value=m.id() selected=move || p.with(|p| p.new_shading == m.id())>{m.label()}</option> })
                                .collect_view()}
                        </select>
                    })}
                    {row("Grid", view! {
                        <label class="check">
                            <input
                                type="checkbox"
                                prop:checked=move || p.with(|p| p.new_grid)
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    set(Box::new(move |p| p.new_grid = on));
                                }
                            />
                            "Show the floor grid"
                        </label>
                    })}
                    <h3>"Memory"</h3>
                    {row("Undo Steps", view! {
                        <input
                            class="text-in small"
                            type="number"
                            min="10"
                            max="2000"
                            prop:value=move || p.with(|p| p.undo_steps.to_string())
                            on:change=move |ev| {
                                let n: usize = event_target_value(&ev).parse().unwrap_or(200);
                                set(Box::new(move |p| p.undo_steps = n));
                            }
                        />
                    })}
                    <div class="btns">
                        <span class="hint" style="margin-right:auto">{crate::settings::location()}</span>
                        <button class="btn" on:click=move |_| set(Box::new(|p| *p = Prefs::default()))>"Reset to Defaults"</button>
                        <button class="btn primary" on:click=move |_| app.prefs_open.set(false)>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forma3d_core::scene::Scene;

    #[test]
    fn round_trip_and_repair() {
        let p = Prefs::default();
        assert_eq!(
            serde_json::from_str::<Prefs>(&serde_json::to_string(&p).unwrap()).unwrap(),
            p
        );
        let partial: Prefs = serde_json::from_str(r#"{"ui_scale": 125}"#).unwrap();
        assert_eq!(partial.ui_scale, 125.0);
        let bad = Prefs {
            ui_scale: 9000.0,
            undo_steps: 1,
            new_shading: "nope".into(),
            autosave_minutes: 999,
            ..Prefs::default()
        };
        let fixed = bad.sanitized();
        assert_eq!(
            (fixed.ui_scale, fixed.undo_steps, fixed.autosave_minutes),
            (200.0, 10, 120)
        );
        let p = Prefs {
            new_shading: DisplayMode::Quick.id().into(),
            new_grid: false,
            undo_steps: 15,
            ..Prefs::default()
        };
        let mut ed = Editor::new(Scene::new());
        p.apply_to_new(&mut ed);
        assert_eq!(ed.display, DisplayMode::Quick);
        assert!(!ed.view_opts.grid);
        assert_eq!(ed.history.limit(), 15);
    }
}
