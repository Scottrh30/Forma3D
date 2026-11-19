//! Forma3D's interface: a Leptos app that runs in the Tauri window (or any
//! browser with WebGPU or WebGL 2). The modelling engine is `forma3d-core`,
//! compiled into the same WebAssembly module.

mod assets;
mod attributes;
mod bottom;
mod chrome;
mod commands;
mod components;
mod customize;
mod fields;
mod host;
mod iconbar;
mod icons;
mod interact;
mod keymap;
mod managers;
mod objects;
mod pen;
mod prefs;
mod render;
mod settings;
mod state;
mod timeline;
mod viewcfg;
mod viewport;
mod vinput;
mod worker;
mod workspace;

use chrome::{Commander, Dialogs, StatusBar, TitleBar, Toolbar};
use keymap::{Key, Mods, Shortcut};
use leptos::prelude::*;
use state::App;

/// How long the first key of a sequence (M of M~S) waits for the second.
const SEQUENCE_MS: f64 = 3000.0;

/// Keyboard shortcuts from the keymap (exact modifiers), including two-key
/// sequences. Returns true when the key was used.
fn handle_key(app: App, ev: &web_sys::KeyboardEvent) -> bool {
    if customize::capture_key(app, ev) {
        return true;
    }
    let Some(key) = Key::from_event(ev) else {
        return false;
    };
    let m = Mods::of(ev);
    let now = window().performance().map(|p| p.now()).unwrap_or(0.0);
    if app
        .prefix
        .get_untracked()
        .is_some_and(|(_, t)| now - t > SEQUENCE_MS)
    {
        app.prefix.set(None);
    }
    // A menu or dialog open: Esc closes it, other keys do nothing.
    let modal = app.commander.get_untracked()
        || app.ask.with_untracked(|a| a.is_some())
        || app.about.get_untracked()
        || app.prefs_open.get_untracked()
        || app.customize_open.get_untracked()
        || app.render.with_untracked(|r| r.settings_open)
        || app.icon_library.get_untracked();
    if key.name() == "Esc" && (modal || app.menu.get_untracked().is_some()) {
        app.menu.set(None);
        app.commander.set(false);
        app.about.set(false);
        app.prefs_open.set(false);
        app.customize_open.set(false);
        app.render.update(|r| r.settings_open = false);
        app.ask.set(None);
        return true;
    }
    if modal {
        return false;
    }
    // Second key of a sequence.
    if let Some((first, _)) = app.prefix.get_untracked() {
        app.prefix.set(None);
        if matches!(key.name(), "Esc") {
            return true;
        }
        match app
            .keymap
            .with_untracked(|k| k.command_for_seq(first, key, m))
        {
            Some(cmd) => commands::run(app, cmd),
            None => app.status(format!(
                "{}~{} is not a shortcut",
                first.label(),
                Shortcut::from_event(key, m).label()
            )),
        }
        return true;
    }
    // The pens take Enter and Backspace.
    if m.none() && app.peek(|e| e.pen.is_some()) && matches!(key.name(), "Enter" | "Backspace") {
        viewport::pen_key(app, key.name() == "Enter");
        return true;
    }
    let sc = Shortcut::from_event(key, m);
    if app.keymap.with_untracked(|k| k.is_prefix(sc)) {
        app.prefix.set(Some((sc, now)));
        return true;
    }
    if let Some(cmd) = app.keymap.with_untracked(|k| k.command_for(key, m)) {
        commands::run(app, cmd);
        return true;
    }
    false
}

#[component]
fn Root() -> impl IntoView {
    let app = App::new();
    provide_context(app);
    app.log_line("Forma3D started", false);
    settings::load_all(app);
    settings::start_autosave(app);
    render::init(app);
    workspace::install_tab_dragging();
    // Scene files the desktop app was started with, or that a second launch
    // (double-clicking another .f3d) passes on.
    if host::in_tauri() {
        leptos::task::spawn_local(async move {
            let files = host::startup_files().await;
            if !files.is_empty() {
                commands::open_paths(app, files);
            }
        });
        host::listen("open-files", move |v| {
            if let Ok(files) = serde_wasm_bindgen::from_value::<Vec<String>>(v) {
                commands::open_paths(app, files);
            }
        });
    }
    iconbar::install(app);

    // The window's close button, Alt+F4 and the taskbar ask first when there
    // are unsaved changes.
    host::on_close_requested(move || {
        if app.unsaved_projects() == 0 {
            return false;
        }
        commands::run(app, commands::Cmd::Quit);
        true
    });

    // Interface Scale (Preferences).
    Effect::new(move |last: Option<f32>| {
        let z = app.prefs.with(|p| p.ui_scale);
        if last != Some(z) && (last.is_some() || z != 100.0) {
            host::set_zoom(z);
        }
        z
    });

    let _keys = window_event_listener(leptos::ev::keydown, move |ev| {
        // Not while typing in a field (unless a shortcut is being recorded).
        let typing = ev
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(t).ok())
            .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"));
        if typing && app.capture.get_untracked().is_none() {
            return;
        }
        if ev.repeat()
            && app.prefix.get_untracked().is_none()
            && app.capture.get_untracked().is_none()
        {
            // Held keys repeat only plain commands (not sequences or captures).
            if let Some(k) = Key::from_event(&ev)
                && app
                    .keymap
                    .with_untracked(|km| km.is_prefix(Shortcut::from_event(k, Mods::of(&ev))))
            {
                ev.prevent_default();
                return;
            }
        }
        if handle_key(app, &ev) {
            ev.prevent_default();
        }
    });
    // A click anywhere else closes an open menu.
    let _close = window_event_listener(leptos::ev::mousedown, move |_| {
        if app.menu.get_untracked().is_some() {
            app.menu.set(None);
        }
    });

    view! {
        <div class="app" on:contextmenu=|ev| ev.prevent_default()>
            <TitleBar />
            <Toolbar />
            <workspace::Body />
            <StatusBar />
            <Commander />
            <Dialogs />
            <render::RenderSettingsDialog />
            <viewcfg::ConfigureView />
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(Root);
}
