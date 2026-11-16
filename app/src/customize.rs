//! Customize Commands: rebind any command's keyboard shortcut. Click a
//! shortcut, then press the new key; a second key makes a sequence (M~S),
//! Enter keeps one key, Esc cancels and Backspace removes the shortcut.

use crate::commands::{self, Cmd};
use crate::icons::icon_s;
use crate::keymap::{Binding, Conflict, Key, Mods, Shortcut, bindable};
use crate::state::{App, app};
use leptos::prelude::*;

fn finish(app: App, cmd: Cmd, binding: Binding) {
    let name = |c: Cmd| commands::info(c).name;
    let result = app
        .keymap
        .try_update(|k| k.assign(cmd, binding))
        .expect("keymap is alive");
    let ok = result.is_ok();
    let msg = match result {
        Ok(None) => format!("{} is now {}", name(cmd), binding.label()),
        Ok(Some(other)) => format!(
            "{} is now {} (taken from {})",
            name(cmd),
            binding.label(),
            name(other)
        ),
        Err(Conflict::StartsSequences(cmds)) => {
            let key = binding.first.label();
            let list: Vec<String> = cmds
                .iter()
                .take(5)
                .filter_map(|c| {
                    let seq = app
                        .keymap
                        .with_untracked(|k| k.shortcuts(*c))
                        .into_iter()
                        .find(|b| b.then.is_some() && b.first == binding.first)?;
                    Some(format!("{} {}", seq.label(), name(*c)))
                })
                .collect();
            format!(
                "{key} starts other shortcuts ({}), so it can't be used alone. Add a second key instead.",
                list.join(", ")
            )
        }
        Err(Conflict::FirstKeyIsShortcut(other)) => format!(
            "{} is the shortcut for {}, so it can't start a sequence",
            binding.first.label(),
            name(other)
        ),
    };
    if ok {
        crate::settings::save_keymap(app);
    }
    app.capture_msg.set(msg);
    app.capture.set(None);
}

/// Handles a key press while a shortcut is being captured. Returns true when
/// it took the key.
pub fn capture_key(app: App, ev: &web_sys::KeyboardEvent) -> bool {
    let Some((cmd, first)) = app.capture.get_untracked() else {
        return false;
    };
    if matches!(ev.key().as_str(), "Shift" | "Control" | "Alt" | "Meta") {
        return true;
    }
    let Some(key) = Key::from_event(ev) else {
        app.capture_msg
            .set(format!("{} can't be used as a shortcut", ev.key()));
        return true;
    };
    let m = Mods::of(ev);
    match first {
        None if key.name() == "Esc" && m.none() => {
            app.capture_msg.set("Cancelled".to_owned());
            app.capture.set(None);
        }
        None if key.name() == "Backspace" && m.none() => {
            app.keymap.update(|k| k.clear(cmd));
            crate::settings::save_keymap(app);
            app.capture_msg.set("Shortcut removed".to_owned());
            app.capture.set(None);
        }
        None if bindable(key) => {
            let sc = Shortcut::from_event(key, m);
            app.capture.set(Some((cmd, Some(sc))));
            app.capture_msg.set(format!("{}~ ... press a second key to make a sequence (like M~S), or Enter to use {} on its own", sc.label(), sc.label()));
        }
        None => app
            .capture_msg
            .set(format!("{} can't be used as a shortcut", key.name())),
        Some(_) if key.name() == "Esc" && m.none() => {
            app.capture_msg.set("Cancelled".to_owned());
            app.capture.set(None);
        }
        Some(f) if key.name() == "Enter" => finish(app, cmd, Binding::single(f)),
        Some(f) if bindable(key) => finish(
            app,
            cmd,
            Binding {
                first: f,
                then: Some(Shortcut::from_event(key, m)),
            },
        ),
        Some(_) => app
            .capture_msg
            .set(format!("{} can't be used as a shortcut", key.name())),
    }
    true
}

#[component]
pub fn Customize() -> impl IntoView {
    let app = app();
    let filter = RwSignal::new(String::new());
    let close = move || {
        app.customize_open.set(false);
        app.capture.set(None);
        app.capture_msg.set(String::new());
    };
    let list = move || -> Vec<commands::Info> {
        commands::search(commands::infos(), &filter.get())
            .into_iter()
            .cloned()
            .collect()
    };
    view! {
        <Show when=move || app.customize_open.get()>
            <div class="modal-bg" on:mousedown=move |_| close()>
                <div class="dialog customize" on:mousedown=|ev| ev.stop_propagation()>
                    <h2>"Customize Commands"</h2>
                    <div class="crow">
                        <input
                            class="text-in"
                            placeholder="Search commands"
                            prop:value=move || filter.get()
                            on:input=move |ev| filter.set(event_target_value(&ev))
                            on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation()
                        />
                        <button class="btn" on:click=move |_| {
                            app.keymap.update(|k| k.reset_all());
                            crate::settings::save_keymap(app);
                            app.capture_msg.set("All shortcuts back to the defaults".to_owned());
                        }>"Reset All"</button>
                    </div>
                    <p class="hint">
                        "Click a shortcut, then press the new key. Press a second key for a sequence (M~S) or Enter to keep one key. Esc cancels, Backspace removes the shortcut."
                    </p>
                    <p class="msgline">{move || app.capture_msg.get()}</p>
                    <div class="klist">
                        {move || {
                            list()
                                .into_iter()
                                .map(|i| {
                                    let c = i.cmd;
                                    let capturing = move || app.capture.get().is_some_and(|(x, _)| x == c);
                                    let text = move || match app.capture.get() {
                                        Some((x, Some(f))) if x == c => format!("{}~ ...", f.label()),
                                        Some((x, None)) if x == c => "Press keys...".to_owned(),
                                        _ => {
                                            let all: Vec<String> = app.keymap.with(|k| k.shortcuts(c)).iter().map(Binding::label).collect();
                                            if all.is_empty() { "—".to_owned() } else { all.join(", ") }
                                        }
                                    };
                                    let custom = move || app.keymap.with(|k| k.is_custom(c));
                                    view! {
                                        <div class="krow">
                                            <span class="ico">{icon_s(i.icon)}</span>
                                            <span class="kname">{i.name.clone()}</span>
                                            <span class="cat">{i.category}</span>
                                            <button
                                                class="btn kbtn"
                                                class:primary=capturing
                                                title="Click, then press the new shortcut"
                                                on:click=move |_| {
                                                    if capturing() {
                                                        app.capture.set(None);
                                                    } else {
                                                        app.capture.set(Some((c, None)));
                                                        app.capture_msg.set(String::new());
                                                    }
                                                }
                                            >
                                                {text}
                                            </button>
                                            <button
                                                class="btn small"
                                                style:visibility=move || if custom() { "visible" } else { "hidden" }
                                                title="Back to the default shortcut"
                                                on:click=move |_| {
                                                    app.keymap.update(|k| k.reset(c));
                                                    crate::settings::save_keymap(app);
                                                }
                                            >
                                                "Reset"
                                            </button>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    <div class="btns">
                        <span class="hint" style="margin-right:auto">{crate::settings::location()}</span>
                        <button class="btn primary" on:click=move |_| close()>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
