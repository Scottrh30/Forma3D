//! Reading and writing the app's settings files (Preferences, shortcuts,
//! workspaces) and autosaves. In the Tauri app they live in the app's
//! settings folder; in a plain browser, in the page's local storage.

use crate::host;
use crate::state::App;
use leptos::prelude::*;

pub const PREFS: &str = "prefs.json";
pub const KEYMAP: &str = "keymap.json";
pub const WORKSPACES: &str = "workspaces.json";

/// Where settings are kept, for the dialogs.
pub fn location() -> &'static str {
    if host::in_tauri() {
        "Saved in the app's settings folder"
    } else {
        "Saved in this browser"
    }
}

fn write(app: App, name: &'static str, text: String) {
    if !app.loaded.get_untracked() {
        return;
    }
    leptos::task::spawn_local(async move {
        if let Err(e) = host::write_setting(name, &text).await {
            app.error(format!("Could not save {name}: {e}"));
        }
    });
}

pub fn save_prefs(app: App) {
    let text = serde_json::to_string_pretty(&app.prefs.get_untracked()).unwrap_or_default();
    write(app, PREFS, text);
}

pub fn save_keymap(app: App) {
    let text = app
        .keymap
        .with_untracked(|k| k.to_json(crate::commands::infos()));
    write(app, KEYMAP, text);
}

pub fn save_workspaces(app: App) {
    let text = serde_json::to_string(&app.saved_workspaces()).unwrap_or_default();
    write(app, WORKSPACES, text);
}

/// Reads every settings file, then applies them. Runs once at start-up.
pub fn load_all(app: App) {
    leptos::task::spawn_local(async move {
        if let Some(text) = host::read_setting(PREFS).await
            && let Ok(p) = serde_json::from_str::<crate::prefs::Prefs>(&text)
        {
            let p = p.sanitized();
            app.each_project(|e| p.apply_to(e));
            if p.startup == crate::prefs::Startup::Empty && app.peek(|e| e.is_pristine()) {
                let mut ed = forma3d_core::editor::Editor::new(forma3d_core::scene::Scene::new());
                p.apply_to_new(&mut ed);
                ed.status = "Ready".to_owned();
                app.replace_project(ed);
            }
            app.prefs.set(p);
        }
        if let Some(text) = host::read_setting(KEYMAP).await {
            app.keymap.set(crate::keymap::Keymap::from_json(
                &text,
                crate::commands::infos(),
            ));
        }
        if let Some(text) = host::read_setting(WORKSPACES).await
            && let Ok(saved) = serde_json::from_str::<crate::workspace::Saved>(&text)
        {
            app.load_workspaces(saved);
        }
        app.loaded.set(true);
        app.view_rev.update(|r| *r += 1);
    });
}

/// Copies changed projects to the autosave folder every few minutes (as set
/// in Preferences); a project's own file is never touched.
pub fn start_autosave(app: App) {
    let last = StoredValue::new(0.0f64);
    let tick = move || {
        let minutes = app.prefs.with_untracked(|p| p.autosave_minutes);
        let now = window()
            .performance()
            .map(|p| p.now() / 1000.0)
            .unwrap_or(0.0);
        if minutes == 0 {
            last.set_value(now);
            return;
        }
        if now - last.get_value() < minutes as f64 * 60.0 {
            return;
        }
        last.set_value(now);
        let mut files = Vec::new();
        let mut index = 0;
        let mut collect = |e: &forma3d_core::editor::Editor| {
            index += 1;
            if !e.is_dirty() {
                return;
            }
            let stem = e.project_name().replace(['/', '\\', ':'], "_");
            let name = format!("{}-{}.forma3d.json", stem.trim_end_matches(".json"), index);
            if let Ok(text) = e.scene.to_json() {
                files.push((name, text));
            }
        };
        app.others.with_value(|o| o.iter().for_each(&mut collect));
        app.ed.with_value(&mut collect);
        if files.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            let n = files.len();
            let mut where_ = String::new();
            for (name, text) in files {
                match host::write_autosave(&name, &text).await {
                    Ok(p) => where_ = p,
                    Err(e) => {
                        app.error(format!("Autosave failed: {e}"));
                        return;
                    }
                }
            }
            app.status(format!("Autosaved {n} project(s) to {where_}"));
        });
    };
    let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(tick);
    if let Some(w) = web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(
            wasm_bindgen::JsCast::unchecked_ref(cb.as_ref()),
            20_000,
        );
    }
    cb.forget();
}
