//! The window's chrome: the title bar with its menus, workspace tabs and
//! command search, the toolbar, the mode palette, the status bar, the
//! Commander and the dialogs.

use crate::commands::{self, Cmd, MESH_TOOLS, ViewFilter};
use crate::host;
use crate::icons::{icon, icon_s};
use crate::state::app;
use crate::workspace::{Panel, Workspace};
use forma3d_core::camera::ViewKind;
use forma3d_core::component::EditMode;
use forma3d_core::deformer::DeformerKind;
use forma3d_core::editor::ViewLayout;
use forma3d_core::generator::GeneratorKind;
use forma3d_core::gizmo::{CoordSystem, Tool};
use forma3d_core::light::LightKind;
use forma3d_core::pen::PenKind;
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::scene_camera::CameraKind;
use forma3d_core::sky::SkyKind;
use forma3d_core::renderer::{DisplayMode, ViewFx};
use forma3d_core::snap::{SnapKind, SnapMode};
use leptos::prelude::*;

#[derive(Clone)]
pub enum Entry {
    C(Cmd),
    Sep,
    Sub(&'static str, Vec<Entry>),
    /// A control inside a menu: "snap-radius", "quantize", "projects".
    Widget(&'static str),
}

fn cmds(list: impl IntoIterator<Item = Cmd>) -> Vec<Entry> {
    list.into_iter().map(Entry::C).collect()
}

fn snap_menu() -> Vec<Entry> {
    use Entry::*;
    let mut v = vec![C(Cmd::ToggleSnap), Sep];
    v.extend(cmds(SnapMode::ALL.map(Cmd::SnapMode)));
    v.push(Sep);
    v.extend(cmds(SnapKind::ALL.map(Cmd::SnapTo)));
    v.extend([Sep, Widget("snap-radius")]);
    v
}

fn workplane_menu() -> Vec<Entry> {
    use Entry::*;
    let mut v = vec![C(Cmd::Mode(EditMode::Workplane)), Sep];
    v.extend(cmds([0, 1, 2].map(Cmd::AlignWorkplane)));
    v.extend([
        C(Cmd::AlignWorkplaneToSelection),
        C(Cmd::AlignSelectionToWorkplane),
    ]);
    v
}

pub fn mesh_menu() -> Vec<Entry> {
    use Entry::*;
    let mut v = vec![C(Cmd::MakeEditable), Sep, C(Cmd::Pen(PenKind::Polygon))];
    v.extend(cmds(MESH_TOOLS.map(Cmd::MeshTool)));
    v.push(Sep);
    v.extend(cmds([
        Cmd::Bridge,
        Cmd::Weld,
        Cmd::Optimize,
        Cmd::SubdivideSmooth,
        Cmd::SubdivideLinear,
    ]));
    v.push(Sep);
    v.extend(cmds([Cmd::Delete, Cmd::SelectAll, Cmd::ReverseNormals]));
    v
}

pub fn menu(name: &str) -> Vec<Entry> {
    use Entry::*;
    match name {
        "File" => vec![
            C(Cmd::New),
            C(Cmd::Open),
            C(Cmd::Close),
            C(Cmd::Revert),
            Sep,
            C(Cmd::Save),
            C(Cmd::SaveAs),
            Sep,
            C(Cmd::Merge),
            Sep,
            C(Cmd::Quit),
        ],
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
            Sep,
            C(Cmd::SaveAsset),
            C(Cmd::SaveMaterialAsset),
            Sep,
            C(Cmd::Preferences),
            C(Cmd::CustomizeCommands),
        ],
        "Create" => {
            let mut v = cmds(PrimitiveKind::ALL.map(Cmd::Primitive));
            v.push(Sep);
            v.push(C(Cmd::Null));
            v.push(C(Cmd::Pen(PenKind::Spline)));
            v.push(Sub(
                "Generators",
                cmds(GeneratorKind::ALL.map(Cmd::Generator)),
            ));
            v.push(Sub("Deformers", cmds(DeformerKind::ALL.map(Cmd::Deformer))));
            v.push(Sub("Camera", cmds(CameraKind::ALL.map(Cmd::SceneCamera))));
            v.push(Sep);
            v.push(C(Cmd::NewMaterial));
            v
        }
        "Light" => {
            let mut v = cmds(LightKind::ALL.map(Cmd::Light));
            v.extend([Sep, C(Cmd::TargetLight), Sep]);
            v.extend(cmds(SkyKind::ALL.map(Cmd::Sky)));
            v.extend([Sep, C(Cmd::ShowPanel(Panel::LightLister))]);
            v
        }
        "Render" => vec![
            C(Cmd::RenderView),
            C(Cmd::RenderRegion),
            C(Cmd::RenderToViewer),
            C(Cmd::StopRender),
            Sep,
            C(Cmd::AddToQueue),
            C(Cmd::ShowPanel(Panel::RenderQueue)),
            C(Cmd::ShowPanel(Panel::PictureViewer)),
            Sep,
            C(Cmd::RenderSettings),
        ],
        "Modes" => {
            let mut v = cmds(EditMode::ALL.map(Cmd::Mode));
            v.extend([
                Sep,
                C(Cmd::ToggleAxisMode),
                C(Cmd::CenterAxis),
                C(Cmd::ToggleTweak),
                Sep,
            ]);
            v.push(Sub("Snapping", snap_menu()));
            v.push(Sub(
                "Quantizing",
                vec![C(Cmd::ToggleQuantize), Sep, Widget("quantize")],
            ));
            v.push(Sub("Workplane", workplane_menu()));
            v.extend([Sep, C(Cmd::MakeEditable), Sep, C(Cmd::ToggleCoordSystem)]);
            v.extend(cmds([0, 1, 2].map(Cmd::ToggleAxis)));
            v
        }
        "Select" => vec![
            C(Cmd::SelectAll),
            C(Cmd::DeselectAll),
            C(Cmd::InvertSelection),
            Sep,
            C(Cmd::SelectChildren),
            C(Cmd::LoopSelect),
        ],
        "Tools" => {
            let mut v = cmds([Tool::Select, Tool::Move, Tool::Scale, Tool::Rotate].map(Cmd::Tool));
            v.extend([
                Sep,
                C(Cmd::Pen(PenKind::Spline)),
                C(Cmd::Pen(PenKind::Polygon)),
                Sep,
            ]);
            v.extend(cmds(MESH_TOOLS.map(Cmd::MeshTool)));
            v.extend([Sep, C(Cmd::ResetTransform), C(Cmd::ToggleEnabled)]);
            v
        }
        "Mesh" => mesh_menu(),
        "View" | "v-View" => {
            let mut v = vec![
                C(Cmd::UndoView),
                C(Cmd::RedoView),
                Sep,
                C(Cmd::FrameSelected),
                C(Cmd::FrameAll),
                C(Cmd::FrameDefault),
                Sep,
            ];
            v.extend(cmds(ViewKind::ALL.map(Cmd::Camera)));
            v.push(Sub("Layout", cmds(ViewLayout::ALL.map(Cmd::Layout))));
            if name == "View" {
                v.push(Sep);
                v.extend(cmds(DisplayMode::ALL.map(Cmd::Display)));
            }
            v.extend([
                Sep,
                C(Cmd::ToggleGrid),
                C(Cmd::ToggleCompass),
                C(Cmd::ToggleSafeFrames),
                C(Cmd::ToggleSolo),
                Sep,
                C(Cmd::ConfigureView),
            ]);
            v
        }
        "v-Cameras" => {
            let mut v = vec![C(Cmd::LookThrough), C(Cmd::EditorCamera), Sep, C(Cmd::UseRenderCamera), C(Cmd::FocusOnSelection), Sep];
            // What this panel shows: C4D's Cameras menu projections.
            v.extend(cmds(ViewKind::PROJECTIONS.map(Cmd::Projection)));
            v
        }
        "v-Display" => vec![
            C(Cmd::ToggleGrid),
            C(Cmd::ToggleCompass),
            C(Cmd::ToggleSafeFrames),
            C(Cmd::ToggleSolo),
            Sep,
            C(Cmd::LoadBackground),
            C(Cmd::ClearBackground),
            C(Cmd::ConfigureView),
            Sep,
            C(Cmd::ToggleTimeline),
        ],
        "v-Shading" => cmds(DisplayMode::ALL.map(Cmd::Display)),
        "v-Options" => cmds(ViewFx::ALL.map(Cmd::ViewFx)),
        "v-Filter" => {
            let mut v = cmds(ViewFilter::ALL.map(Cmd::Filter));
            v.extend([Sep, C(Cmd::ToggleGrid)]);
            v
        }
        "v-Panel" => {
            let mut v = cmds(ViewLayout::ALL.map(Cmd::Layout));
            v.extend([Sep, C(Cmd::ConfigureView)]);
            v
        }
        "Window" => {
            let mut v = vec![C(Cmd::Commander), C(Cmd::CustomizeCommands), Sep];
            let mut ws = cmds(Workspace::ALL.map(Cmd::Workspace));
            ws.extend([Sep, C(Cmd::NextWorkspace), C(Cmd::PrevWorkspace)]);
            v.push(Sub("Workspace", ws));
            v.push(C(Cmd::ResetLayout));
            v.push(Sub("Panels", cmds(Panel::OPENABLE.map(Cmd::ShowPanel))));
            v.extend([Sep, C(Cmd::ToggleTimeline), C(Cmd::ToggleIconBar), C(Cmd::IconLibrary), Sep, C(Cmd::Minimize), C(Cmd::ToggleMaximize), C(Cmd::ToggleFullscreen), Sep, Widget("projects")]);
            v
        }
        "Help" => vec![
            C(Cmd::Commander),
            C(Cmd::CustomizeCommands),
            Sep,
            C(Cmd::About),
        ],
        _ => vec![],
    }
}

/// Controls that live inside menus.
fn widget(name: &'static str) -> AnyView {
    let app = app();
    match name {
        "snap-radius" => view! {
            <div class="mrow" on:click=|ev| ev.stop_propagation()>
                "Radius"
                <input
                    class="text-in small"
                    type="number"
                    min="2"
                    max="60"
                    prop:value=move || app.read(|e| format!("{:.0}", e.snap.radius))
                    on:change=move |ev| {
                        let r: f32 = event_target_value(&ev).parse().unwrap_or(12.0);
                        app.act(|e| e.snap.radius = r.clamp(2.0, 60.0));
                    }
                />
                "px"
            </div>
        }
        .into_any(),
        "quantize" => {
            let field = move |label: &'static str,
                              unit: &'static str,
                              get: fn(&forma3d_core::gizmo::Quantize) -> f32,
                              set: fn(&mut forma3d_core::gizmo::Quantize, f32),
                              lo: f32,
                              hi: f32| {
                view! {
                    <div class="mrow" on:click=|ev| ev.stop_propagation()>
                        <span style="width:54px">{label}</span>
                        <input
                            class="text-in small"
                            type="number"
                            prop:value=move || app.read(|e| format!("{}", get(&e.quantize)))
                            on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                    app.act(|e| set(&mut e.quantize, v.clamp(lo, hi)));
                                }
                            }
                        />
                        {unit}
                    </div>
                }
            };
            view! {
                {field("Move", "cm", |q| q.move_step, |q, v| q.move_step = v, 0.01, 10000.0)}
                {field("Rotate", "°", |q| q.rotate_step, |q, v| q.rotate_step = v, 0.1, 180.0)}
                {field("Scale", "%", |q| q.scale_step, |q, v| q.scale_step = v, 0.1, 500.0)}
            }
            .into_any()
        }
        "projects" => view! {
            {move || {
                let slot = app.slot.get();
                app.project_names()
                    .into_iter()
                    .enumerate()
                    .map(|(i, n)| {
                        view! {
                            <div class="mi" on:click=move |_| {
                                app.menu.set(None);
                                app.switch_project(i);
                            }>
                                <span class="chk">{if i == slot { "●" } else { "" }}</span>
                                <span class="ico">{icon_s("doc")}</span>
                                {n}
                            </div>
                        }
                    })
                    .collect_view()
            }}
        }
        .into_any(),
        _ => view! { <div></div> }.into_any(),
    }
}

/// The name a menu shows for a command (Undo says what it undoes).
fn menu_name(c: Cmd, ed: &forma3d_core::editor::Editor) -> String {
    match c {
        Cmd::Undo => ed
            .history
            .undo_label()
            .map_or_else(|| "Undo".to_owned(), |l| format!("Undo {l}")),
        Cmd::Redo => ed
            .history
            .redo_label()
            .map_or_else(|| "Redo".to_owned(), |l| format!("Redo {l}")),
        _ => commands::info(c).name,
    }
}

#[component]
pub fn MenuEntries(entries: Vec<Entry>) -> impl IntoView {
    let app = app();
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Sep => view! { <div class="msep"></div> }.into_any(),
            Entry::Widget(name) => widget(name),
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
                let name = move || app.read(|ed| menu_name(c, ed));
                let key = move || app.keymap.with(|k| k.label(c));
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
                        {name}
                        <span class="key">{key}</span>
                    </div>
                }
                .into_any()
            }
        })
        .collect_view()
}

/// A menu button with its dropdown. `name` identifies the menu; `label` is
/// what it shows (the name when left out).
#[component]
pub fn MenuButton(
    name: &'static str,
    #[prop(optional)] label: Option<&'static str>,
) -> impl IntoView {
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
                {label.unwrap_or(name)}
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
    let count = move || {
        app.rev.track();
        app.project_count()
    };
    let key = move || app.keymap.with(|k| k.label(Cmd::Commander));
    // (maximized, full screen): kept current as the window changes size.
    let win_state = RwSignal::new((false, false));
    let refresh = move || leptos::task::spawn_local(async move { win_state.set(host::window_state().await) });
    refresh();
    let _ = window_event_listener(leptos::ev::resize, move |_| refresh());
    let _ = window_event_listener(leptos::ev::fullscreenchange, move |_| refresh());
    view! {
        <div class="title" data-tauri-drag-region>
            <div class="logo">
                <svg viewBox="0 0 24 24"><path d="M12 3 20 7.5v9L12 21 4 16.5v-9z" /><path d="M4 7.5 12 12l8-4.5M12 12v9" /></svg>
            </div>
            <div class="menus">
                {["File", "Edit", "Create", "Light", "Modes", "Select", "Tools", "Mesh", "View", "Render", "Window", "Help"]
                    .into_iter()
                    .map(|n| view! { <MenuButton name=n /> })
                    .collect_view()}
            </div>
            <div class="drag" data-tauri-drag-region>
                <div class="search" on:click=move |_| app.commander.set(true)>
                    {icon_s("search")}
                    "Search commands"
                    <kbd>{key}</kbd>
                </div>
            </div>
            <crate::workspace::WorkspaceTabs />
            <div class="proj" data-tauri-drag-region>
                <b>{title}{move || if dirty() { " •" } else { "" }}</b>
                {move || if count() > 1 { format!(" · {} projects", count()) } else { String::new() }}
            </div>
            <button
                class="wbtn"
                title=move || if win_state.get().1 { "Leave Full Screen (F11)" } else { "Full Screen (F11)" }
                on:click=move |_| commands::run(app, Cmd::ToggleFullscreen)
            >
                {move || if win_state.get().1 {
                    view! { <svg viewBox="0 0 12 12"><path d="M4.5 1v3.5H1M7.5 1v3.5H11M4.5 11V7.5H1M7.5 11V7.5H11" /></svg> }.into_any()
                } else {
                    view! { <svg viewBox="0 0 12 12"><path d="M1 4.5V1h3.5M11 4.5V1H7.5M1 7.5V11h3.5M11 7.5V11H7.5" /></svg> }.into_any()
                }}
            </button>
            <Show when=host::in_tauri>
                <button class="wbtn" title="Minimize" on:click=move |_| commands::run(app, Cmd::Minimize)>
                    <svg viewBox="0 0 12 12"><path d="M1 6h10" /></svg>
                </button>
                <button
                    class="wbtn"
                    title=move || if win_state.get().0 { "Restore" } else { "Maximize" }
                    on:click=move |_| commands::run(app, Cmd::ToggleMaximize)
                >
                    {move || if win_state.get().0 {
                        view! { <svg viewBox="0 0 12 12"><rect x="1.5" y="3.5" width="7" height="7" rx="1.2" /><path d="M3.5 3.5V2.2c0-.4.3-.7.7-.7h5.6c.4 0 .7.3.7.7v5.6c0 .4-.3.7-.7.7H8.5" /></svg> }.into_any()
                    } else {
                        view! { <svg viewBox="0 0 12 12"><rect x="1.5" y="1.5" width="9" height="9" rx="1.5" /></svg> }.into_any()
                    }}
                </button>
                <button class="wbtn x" title="Close" on:click=move |_| commands::run(app, Cmd::Quit)>
                    <svg viewBox="0 0 12 12"><path d="m1.5 1.5 9 9m0-9-9 9" /></svg>
                </button>
            </Show>
        </div>
    }
}

/// A toolbar button for a command, with its name and shortcut as tooltip.
#[component]
pub fn CmdButton(cmd: Cmd) -> impl IntoView {
    let app = app();
    let info = commands::info(cmd);
    let name = info.name.clone();
    let tip = move || {
        let k = app.keymap.with(|k| k.label(cmd));
        if k.is_empty() {
            name.clone()
        } else {
            format!("{name}  ({k})")
        }
    };
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
pub fn DropButton(
    id: &'static str,
    icon_name: &'static str,
    title: &'static str,
    items: Vec<Cmd>,
) -> impl IntoView {
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
                <div class="dropdown" style="top:34px" on:mousedown=|ev| ev.stop_propagation() on:click=move |_| app.menu.set(None)>
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
        let on = move || app.read(|e| e.axes.0[a as usize]);
        let tip = move || {
            format!(
                "Lock / unlock the {} axis ({})",
                ["X", "Y", "Z"][a as usize],
                app.keymap.with(|k| k.label(Cmd::ToggleAxis(a)))
            )
        };
        view! {
            <button class="axis" class:on=on style=format!("color:{color};background:{bg}") title=tip on:click=move |_| commands::run(app, Cmd::ToggleAxis(a))>
                {["X", "Y", "Z"][a as usize]}
            </button>
        }
    };
    let world = move || app.read(|e| e.coord == CoordSystem::World);
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
            </div>
            <div class="group">
                {axis(0, "var(--x)", "rgba(229,83,75,.16)")}
                {axis(1, "var(--y)", "rgba(87,171,90,.16)")}
                {axis(2, "var(--z)", "rgba(83,155,245,.16)")}
                <button
                    class="tb"
                    title=move || if world() { "Coordinate system: World (click for Object)" } else { "Coordinate system: Object (click for World)" }
                    on:click=move |_| commands::run(app, Cmd::ToggleCoordSystem)
                >
                    {move || if world() { icon("world") } else { icon("local") }}
                </button>
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::Primitive(PrimitiveKind::Cube) />
                <DropButton id="tb-shapes" icon_name="sphere" title="Shapes" items=PrimitiveKind::ALL.map(Cmd::Primitive).to_vec() />
                <DropButton id="tb-pens" icon_name="pen" title="Spline Pen and Polygon Pen" items=vec![Cmd::Pen(PenKind::Spline), Cmd::Pen(PenKind::Polygon)] />
                <CmdButton cmd=Cmd::Generator(GeneratorKind::Subdivision) />
                <DropButton id="tb-gen" icon_name="generator" title="Generators" items=GeneratorKind::ALL.map(Cmd::Generator).to_vec() />
                <DropButton id="tb-def" icon_name="deformer" title="Deformers (added under the selected object)" items=DeformerKind::ALL.map(Cmd::Deformer).to_vec() />
                <DropButton id="tb-light" icon_name="light" title="Lights" items=LightKind::ALL.map(Cmd::Light).to_vec() />
                <CmdButton cmd=Cmd::Null />
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::Duplicate />
                <CmdButton cmd=Cmd::Group />
                <CmdButton cmd=Cmd::Delete />
            </div>
            <div class="group">
                <CmdButton cmd=Cmd::ToggleSolo />
            </div>
        </div>
    }
}

/// The vertical palette on the left: modes, axis / tweak / snap / quantize,
/// and in component modes the mesh tools.
#[component]
pub fn Palette() -> impl IntoView {
    let app = app();
    let component = move || app.read(|e| e.mode.is_component());
    view! {
        <div class="panel palette">
            <CmdButton cmd=Cmd::MakeEditable />
            <div class="vsep"></div>
            {EditMode::ALL.map(|m| view! { <CmdButton cmd=Cmd::Mode(m) /> }).collect_view()}
            <div class="vsep"></div>
            <CmdButton cmd=Cmd::ToggleAxisMode />
            <CmdButton cmd=Cmd::ToggleTweak />
            <CmdButton cmd=Cmd::ToggleSnap />
            <CmdButton cmd=Cmd::ToggleQuantize />
            <Show when=component>
                <div class="vsep"></div>
                {MESH_TOOLS.map(|t| view! { <CmdButton cmd=Cmd::MeshTool(t) /> }).collect_view()}
            </Show>
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
            if e.mode.is_component() && e.edit_target().is_some() {
                format!(
                    "{} mode · {} selected",
                    e.mode.label(),
                    e.comp.count(e.mode)
                )
            } else {
                format!("{} objects · {} selected", e.scene.len(), e.selection.len())
            }
        })
    };
    // Waiting for the second key of a sequence: what can follow.
    let prefix = move || {
        app.prefix.get().map(|(first, _)| {
            let names: Vec<String> = app
                .keymap
                .with(|k| k.completions(first))
                .into_iter()
                .map(|(k, c)| format!("{} {}", k.label(), commands::info(c).name))
                .collect();
            (format!("{}~", first.label()), names.join("   ·   "))
        })
    };
    view! {
        <div class="status">
            <span>
                <i class="dot" class:dirty=dirty></i>
                {move || if dirty() { "Unsaved changes" } else { "Saved" }}
            </span>
            {move || match prefix() {
                Some((k, rest)) => view! { <span class="msg"><b class="pfx">{k}</b>" "{rest}</span> }.into_any(),
                None => view! { <span class="msg">{msg}</span> }.into_any(),
            }}
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
    let results = move || -> Vec<commands::Info> {
        let q = query.get();
        let mut all: Vec<commands::Info> = commands::infos().to_vec();
        all.extend(app.read(commands::dynamic));
        if q.trim().is_empty() {
            // Recent commands first, then everything in menu order.
            let recent = app.recent.get();
            let mut out: Vec<commands::Info> = recent
                .iter()
                .filter_map(|c| all.iter().find(|i| i.cmd == *c).cloned())
                .collect();
            out.extend(all.into_iter().filter(|i| !recent.contains(&i.cmd)));
            out.truncate(60);
            return out;
        }
        commands::search(&all, &q)
            .into_iter()
            .take(60)
            .cloned()
            .collect()
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
                        placeholder="Type a command (e.g. cube, extrude, frame all, undo)"
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
                                    if let Some(c) = results().get(hl.get_untracked()).map(|i| i.cmd) {
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
                                .map(|(k, i)| {
                                    let c = i.cmd;
                                    let (enabled, checked) = app.read(|ed| commands::state(c, ed, &app));
                                    let key = app.keymap.with(|km| km.label(c));
                                    view! {
                                        <div class="mi" class:hl=move || hl.get() == k class:off=!enabled on:mouseenter=move |_| hl.set(k) on:click=move |_| run(c)>
                                            <span class="chk">{if checked == Some(true) { "✓" } else { "" }}</span>
                                            <span class="ico">{icon_s(i.icon)}</span>
                                            {i.name.clone()}
                                            <span class="cat">{i.category}{if key.is_empty() { String::new() } else { format!(" · {key}") }}</span>
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
                    <p class="hint">"Original code; no Blender or Cinema 4D source, assets or branding."</p>
                    <p class="hint">{move || format!("Graphics: {}", app.gpu_name.get())}</p>
                    <div class="btns">
                        <button class="btn primary" on:click=move |_| app.about.set(false)>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
        <crate::prefs::PrefsDialog />
        <crate::customize::Customize />
    }
}
