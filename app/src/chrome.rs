//! The window's chrome: the title bar with its menus and command search, the
//! toolbar, the mode palette, the status bar, and the dialogs.

use crate::commands::{self, Cmd};
use crate::host;
use crate::icons::{icon, icon_s};
use crate::state::app;
use forma3d_core::camera::ViewKind;
use forma3d_core::component::EditMode;
use forma3d_core::deformer::DeformerKind;
use forma3d_core::generator::GeneratorKind;
use forma3d_core::gizmo::Tool;
use forma3d_core::light::LightKind;
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::renderer::DisplayMode;
use leptos::prelude::*;

#[derive(Clone)]
pub enum Entry {
    C(Cmd),
    Sep,
    Sub(&'static str, Vec<Entry>),
}

fn cmds(list: impl IntoIterator<Item = Cmd>) -> Vec<Entry> {
    list.into_iter().map(Entry::C).collect()
}

pub fn menu(name: &str) -> Vec<Entry> {
    use Entry::*;
    match name {
        "File" => vec![C(Cmd::New), C(Cmd::Open), Sep, C(Cmd::Save), C(Cmd::SaveAs), C(Cmd::Revert), Sep, C(Cmd::Merge), Sep, C(Cmd::Quit)],
        "Edit" => vec![
            C(Cmd::Undo),
            C(Cmd::Redo),
            Sep,
            C(Cmd::Cut),
            C(Cmd::Copy),
            C(Cmd::Paste),
            C(Cmd::Delete),
            Sep,
            C(Cmd::Duplicate),
            C(Cmd::Group),
        ],
        "Create" => {
            let mut v = cmds(PrimitiveKind::ALL.map(Cmd::Primitive));
            v.push(Sep);
            v.push(C(Cmd::Null));
            v.push(Sub("Generators", cmds(GeneratorKind::ALL.map(Cmd::Generator))));
            v.push(Sub("Deformers", cmds(DeformerKind::ALL.map(Cmd::Deformer))));
            v.push(Sep);
            v.push(C(Cmd::NewMaterial));
            v
        }
        "Light" => cmds(LightKind::ALL.map(Cmd::Light)),
        "Modes" => vec![C(Cmd::Mode(EditMode::Model)), C(Cmd::Mode(EditMode::Object)), Sep, C(Cmd::MakeEditable)],
        "Select" => vec![C(Cmd::SelectAll), C(Cmd::Deselect), C(Cmd::InvertSelection), C(Cmd::SelectChildren)],
        "Tools" => {
            let mut v = cmds([Tool::Select, Tool::Move, Tool::Scale, Tool::Rotate].map(Cmd::Tool));
            v.push(Sep);
            v.extend(cmds([0, 1, 2].map(Cmd::ToggleAxis)));
            v.push(C(Cmd::ToggleCoord));
            v.push(Sep);
            v.push(C(Cmd::ResetTransform));
            v.push(C(Cmd::ToggleEnabled));
            v
        }
        "View" => {
            let mut v = vec![C(Cmd::FrameAll), C(Cmd::FrameSelected), Sep];
            v.extend(cmds(ViewKind::ALL.map(Cmd::Camera)));
            v
        }
        "Display" => cmds(DisplayMode::ALL.map(Cmd::Shading)),
        "Options" => vec![C(Cmd::ToggleGrid)],
        "Help" => vec![C(Cmd::Commander), Sep, C(Cmd::About)],
        _ => vec![],
    }
}

#[component]
fn MenuEntries(entries: Vec<Entry>) -> impl IntoView {
    let app = app();
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Sep => view! { <div class="msep"></div> }.into_any(),
            Entry::Sub(name, items) => view! {
                <div class="sub">
                    <div class="mi"><span class="chk"></span><span class="ico"></span>{name}<span class="arrow">"›"</span></div>
                    <div class="dropdown"><MenuEntries entries=items /></div>
                </div>
            }
            .into_any(),
            Entry::C(c) => {
                let info = commands::info(c);
                let st = move || app.read(|ed| commands::state(c, ed, &app));
                view! {
                    <div
                        class="mi"
                        class:off=move || !st().0
                        on:click=move |ev| {
                            ev.stop_propagation();
                            commands::run(app, c);
                        }
                    >
                        <span class="chk">{move || if st().1 == Some(true) { "✓" } else { "" }}</span>
                        <span class="ico">{icon_s(info.icon)}</span>
                        {info.label}
                        <span class="key">{info.key}</span>
                    </div>
                }
                .into_any()
            }
        })
        .collect_view()
}

/// A menu button with its dropdown.
#[component]
pub fn MenuButton(name: &'static str) -> impl IntoView {
    let app = app();
    let open = move || app.menu.get() == Some(name);
    view! {
        <div style="position:relative">
            <button
                class="menu-btn"
                class:open=open
                on:mousedown=move |ev| {
                    ev.stop_propagation();
                    app.menu.update(|m| *m = if *m == Some(name) { None } else { Some(name) });
                }
                on:mouseenter=move |_| {
                    if app.menu.get_untracked().is_some() {
                        app.menu.set(Some(name));
                    }
                }
            >
                {name}
            </button>
            <Show when=open>
                <div class="dropdown" on:mousedown=|ev| ev.stop_propagation()>
                    <MenuEntries entries=menu(name) />
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn TitleBar() -> impl IntoView {
    let app = app();
    let title = move || app.read(|e| e.project_name());
    let dirty = move || app.read(|e| e.is_dirty());
    view! {
        <div class="title" data-tauri-drag-region>
            <div class="logo">
                <svg viewBox="0 0 24 24"><path d="M12 3 20 7.5v9L12 21 4 16.5v-9z" /><path d="M4 7.5 12 12l8-4.5M12 12v9" /></svg>
            </div>
            <div class="menus">
                {["File", "Edit", "Create", "Light", "Modes", "Select", "Tools", "View", "Help"]
                    .into_iter()
                    .map(|n| view! { <MenuButton name=n /> })
                    .collect_view()}
            </div>
            <div class="drag" data-tauri-drag-region>
                <div class="search" on:click=move |_| app.commander.set(true)>
                    {icon_s("search")}
                    "Search commands"
                    <kbd>"Shift C"</kbd>
                </div>
            </div>
            <div class="proj" data-tauri-drag-region>
                <b>{title}{move || if dirty() { " •" } else { "" }}</b>
                " · Forma3D"
            </div>
            <Show when=host::in_tauri>
                <button class="wbtn" title="Minimize" on:click=|_| host::window_call("minimize")>
                    <svg viewBox="0 0 12 12"><path d="M1 6h10" /></svg>
                </button>
                <button class="wbtn" title="Maximize" on:click=|_| host::window_call("toggleMaximize")>
                    <svg viewBox="0 0 12 12"><rect x="1.5" y="1.5" width="9" height="9" rx="1.5" /></svg>
                </button>
                <button class="wbtn x" title="Close" on:click=move |_| commands::run(app, Cmd::Quit)>
                    <svg viewBox="0 0 12 12"><path d="m1.5 1.5 9 9m0-9-9 9" /></svg>
                </button>
            </Show>
        </div>
    }
}

/// A toolbar button for a command, with a tooltip.
#[component]
pub fn CmdButton(cmd: Cmd) -> impl IntoView {
    let app = app();
    let info = commands::info(cmd);
    let tip = if info.key.is_empty() { info.label.clone() } else { format!("{}  ({})", info.label, info.key) };
    let st = move || app.read(|ed| commands::state(cmd, ed, &app));
    view! {
        <button
            class="tb"
            class:on=move || st().1 == Some(true)
            class:off=move || !st().0
            title=tip
            on:click=move |_| commands::run(app, cmd)
        >
            {icon(info.icon)}
        </button>
    }
}

/// A toolbar button that opens a list of commands.
#[component]
pub fn DropButton(id: &'static str, icon_name: &'static str, title: &'static str, items: Vec<Cmd>) -> impl IntoView {
    let app = app();
    let entries: Vec<Entry> = items.iter().copied().map(Entry::C).collect();
    let open = move || app.menu.get() == Some(id);
    view! {
        <div style="position:relative">
            <button
                class="tb"
                class:on=open
                title=title
                on:mousedown=move |ev| {
                    ev.stop_propagation();
                    app.menu.update(|m| *m = if *m == Some(id) { None } else { Some(id) });
                }
            >
                {icon(icon_name)}
                <span class="more"></span>
            </button>
            <Show when=open>
                <div class="dropdown" style="top:34px" on:mousedown=|ev| ev.stop_propagation()>
                    <MenuEntries entries=entries.clone() />
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn Toolbar() -> impl IntoView {
    let app = app();
    let axis = move |a: u8, color: &'static str, bg: &'static str| {
        let on = move || app.read(|e| e.axes.get(forma3d_core::gizmo::Axis::ALL[a as usize]));
        view! {
            <button
                class="axis"
                class:on=on
                style=format!("color:{color};background:{bg}")
                title=format!("Lock / unlock the {} axis ({})", ["X", "Y", "Z"][a as usize], ["X", "Y", "Z"][a as usize])
                on:click=move |_| commands::run(app, Cmd::ToggleAxis(a))
            >
                {["X", "Y", "Z"][a as usize]}
            </button>
        }
    };
    view! {
        <div class="toolbar">
            <div class="group">
                <CmdButton cmd=Cmd::Undo />
                <CmdButton cmd=Cmd::Redo />
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::Tool(Tool::Select) />
                <CmdButton cmd=Cmd::Tool(Tool::Move) />
                <CmdButton cmd=Cmd::Tool(Tool::Scale) />
                <CmdButton cmd=Cmd::Tool(Tool::Rotate) />
                <CmdButton cmd=Cmd::ToggleCoord />
            </div>
            <div class="group">
                {axis(0, "var(--x)", "rgba(229,83,75,.16)")}
                {axis(1, "var(--y)", "rgba(87,171,90,.16)")}
                {axis(2, "var(--z)", "rgba(83,155,245,.16)")}
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::Primitive(PrimitiveKind::Cube) />
                <DropButton id="tb-shapes" icon_name="sphere" title="Shapes" items=PrimitiveKind::ALL.map(Cmd::Primitive).to_vec() />
                <CmdButton cmd=Cmd::Null />
                <DropButton id="tb-gen" icon_name="generator" title="Generators" items=GeneratorKind::ALL.map(Cmd::Generator).to_vec() />
                <DropButton id="tb-def" icon_name="deformer" title="Deformers" items=DeformerKind::ALL.map(Cmd::Deformer).to_vec() />
                <DropButton id="tb-light" icon_name="light" title="Lights" items=LightKind::ALL.map(Cmd::Light).to_vec() />
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::Duplicate />
                <CmdButton cmd=Cmd::Group />
                <CmdButton cmd=Cmd::Delete />
            </div>
        </div>
    }
}

#[component]
pub fn Palette() -> impl IntoView {
    view! {
        <div class="panel palette">
            <CmdButton cmd=Cmd::MakeEditable />
            <div class="vsep"></div>
            <CmdButton cmd=Cmd::Mode(EditMode::Model) />
            <CmdButton cmd=Cmd::Mode(EditMode::Object) />
            <div class="vsep"></div>
            <CmdButton cmd=Cmd::FrameAll />
            <CmdButton cmd=Cmd::FrameSelected />
        </div>
    }
}

#[component]
pub fn StatusBar() -> impl IntoView {
    let app = app();
    let dirty = move || app.read(|e| e.is_dirty());
    let msg = move || app.read(|e| e.status.clone());
    let counts = move || {
        app.read(|e| {
            let mut n = 0;
            e.scene.walk(|_, _| n += 1);
            format!("{n} objects · {} selected", e.selection.len())
        })
    };
    view! {
        <div class="status">
            <span>
                <i class="dot" class:dirty=dirty></i>
                {move || if dirty() { "Unsaved changes" } else { "Saved" }}
            </span>
            <span class="msg">{msg}</span>
            <span class="r">{counts}</span>
        </div>
    }
}

/// Shift+C: find and run any command by name.
#[component]
pub fn Commander() -> impl IntoView {
    let app = app();
    let query = RwSignal::new(String::new());
    let hl = RwSignal::new(0usize);
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if app.commander.get() {
            query.set(String::new());
            hl.set(0);
            if let Some(i) = input.get() {
                let _ = i.focus();
            }
        }
    });
    let results = move || {
        let q = query.get().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        commands::all()
            .into_iter()
            .filter(|c| {
                let i = commands::info(*c);
                let hay = format!("{} {}", i.label, i.category).to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .take(40)
            .collect::<Vec<_>>()
    };
    let close = move || app.commander.set(false);
    let run = move |c: Cmd| {
        close();
        commands::run(app, c);
    };
    view! {
        <Show when=move || app.commander.get()>
            <div class="modal-bg" on:mousedown=move |_| close()>
                <div class="commander" on:mousedown=|ev| ev.stop_propagation()>
                    <input
                        node_ref=input
                        placeholder="Type a command (e.g. cube, frame all, undo)"
                        prop:value=move || query.get()
                        on:input=move |ev| {
                            query.set(event_target_value(&ev));
                            hl.set(0);
                        }
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            let n = results().len();
                            match ev.key().as_str() {
                                "ArrowDown" => {
                                    ev.prevent_default();
                                    hl.update(|h| *h = (*h + 1).min(n.saturating_sub(1)));
                                }
                                "ArrowUp" => {
                                    ev.prevent_default();
                                    hl.update(|h| *h = h.saturating_sub(1));
                                }
                                "Enter" => {
                                    if let Some(c) = results().get(hl.get_untracked()).copied() {
                                        run(c);
                                    }
                                }
                                "Escape" => close(),
                                _ => {}
                            }
                            ev.stop_propagation();
                        }
                    />
                    <div class="list">
                        {move || {
                            results()
                                .into_iter()
                                .enumerate()
                                .map(|(k, c)| {
                                    let i = commands::info(c);
                                    view! {
                                        <div class="mi" class:hl=move || hl.get() == k on:mouseenter=move |_| hl.set(k) on:click=move |_| run(c)>
                                            <span class="ico">{icon_s(i.icon)}</span>
                                            {i.label}
                                            <span class="cat">{i.category}{if i.key.is_empty() { String::new() } else { format!(" · {}", i.key) }}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
pub fn Dialogs() -> impl IntoView {
    let app = app();
    view! {
        {move || {
            app.ask
                .get()
                .map(|a| {
                    let then = a.then;
                    view! {
                        <div class="modal-bg">
                            <div class="dialog">
                                <h2>{a.title.clone()}</h2>
                                <p>{a.text.clone()}</p>
                                <div class="btns">
                                    <button class="btn" on:click=move |_| app.ask.set(None)>"Cancel"</button>
                                    <button class="btn primary" on:click=move |_| commands::run_confirmed(app, then)>{a.ok.clone()}</button>
                                </div>
                            </div>
                        </div>
                    }
                })
        }}
        <Show when=move || app.about.get()>
            <div class="modal-bg" on:mousedown=move |_| app.about.set(false)>
                <div class="dialog" on:mousedown=|ev| ev.stop_propagation()>
                    <h2>"Forma3D"</h2>
                    <p>
                        "A 3D modelling app written in Rust, with a Leptos interface in a Tauri window. Version "
                        {env!("CARGO_PKG_VERSION")} "."
                    </p>
                    <p class="hint">{move || format!("Graphics: {}", app.gpu_name.get())}</p>
                    <div class="btns">
                        <button class="btn primary" on:click=move |_| app.about.set(false)>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

