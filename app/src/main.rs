//! Forma3D's interface: a Leptos app that runs in the Tauri window (or any
//! browser with WebGPU or WebGL 2). The modelling engine is `forma3d-core`,
//! compiled into the same WebAssembly module.

mod attributes;
mod bottom;
mod chrome;
mod commands;
mod fields;
mod host;
mod icons;
mod objects;
mod state;
mod timeline;
mod viewport;

use chrome::{Commander, Dialogs, Palette, StatusBar, TitleBar, Toolbar};
use leptos::prelude::*;
use state::App;

#[component]
fn Root() -> impl IntoView {
    let app = App::new();
    provide_context(app);
    app.log_line("Forma3D started", false);

    // Keyboard shortcuts (not while typing in a field).
    let _keys = window_event_listener(leptos::ev::keydown, move |ev| {
        let typing = ev
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(t).ok())
            .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"));
        if typing {
            return;
        }
        if ev.key() == "Escape" {
            app.menu.set(None);
            app.commander.set(false);
            app.about.set(false);
            return;
        }
        if app.commander.get_untracked() || app.ask.with_untracked(|a| a.is_some()) {
            return;
        }
        if let Some(c) = commands::for_key(&ev) {
            ev.prevent_default();
            commands::run(app, c);
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
            <div class="body">
                <Palette />
                <viewport::Viewport />
                <div class="panel right">
                    <div class="tabs"><div class="tab on">"Objects"</div></div>
                    <div style="flex:1;display:flex;flex-direction:column;min-height:0">
                        <objects::ObjectManager />
                    </div>
                    <div class="tabs" style="border-top:1px solid var(--line)"><div class="tab on">"Attributes"</div></div>
                    <div style="flex:1.5;display:flex;flex-direction:column;min-height:0">
                        <attributes::Attributes />
                    </div>
                </div>
                <bottom::Bottom />
            </div>
            <StatusBar />
            <Commander />
            <Dialogs />
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(Root);
}
