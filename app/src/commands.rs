//! Every command the menus, toolbar, shortcuts and the command search run.

use crate::host;
use crate::state::{App, Ask};
use forma3d_core::camera::ViewKind;
use forma3d_core::component::EditMode;
use forma3d_core::deformer::DeformerKind;
use forma3d_core::editor::Editor;
use forma3d_core::generator::GeneratorKind;
use forma3d_core::gizmo::{CoordSystem, Tool};
use forma3d_core::light::LightKind;
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::renderer::DisplayMode;
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    New,
    Open,
    Merge,
    Save,
    SaveAs,
    Revert,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    Duplicate,
    Group,
    SelectAll,
    Deselect,
    InvertSelection,
    SelectChildren,
    Primitive(PrimitiveKind),
    Null,
    Generator(GeneratorKind),
    Deformer(DeformerKind),
    Light(LightKind),
    NewMaterial,
    Mode(EditMode),
    MakeEditable,
    Tool(Tool),
    ToggleAxis(u8),
    ToggleCoord,
    ResetTransform,
    ToggleEnabled,
    FrameAll,
    FrameSelected,
    Camera(ViewKind),
    Shading(DisplayMode),
    ToggleGrid,
    Commander,
    About,
}

pub struct Info {
    pub label: String,
    pub icon: &'static str,
    pub key: &'static str,
    pub category: &'static str,
}

fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "select",
        Tool::Move => "move",
        Tool::Rotate => "rotate",
        Tool::Scale => "scale",
    }
}

pub fn info(c: Cmd) -> Info {
    let i = |label: &str, icon: &'static str, key: &'static str, category: &'static str| Info { label: label.to_owned(), icon, key, category };
    match c {
        Cmd::New => i("New Project", "new", "Ctrl+N", "File"),
        Cmd::Open => i("Open...", "open", "Ctrl+O", "File"),
        Cmd::Merge => i("Merge Project...", "open", "Ctrl+Shift+O", "File"),
        Cmd::Save => i("Save", "save", "Ctrl+S", "File"),
        Cmd::SaveAs => i("Save As...", "save", "Ctrl+Shift+S", "File"),
        Cmd::Revert => i("Revert to Saved", "undo", "", "File"),
        Cmd::Quit => i("Quit", "power", "Alt+F4", "File"),
        Cmd::Undo => i("Undo", "undo", "Ctrl+Z", "Edit"),
        Cmd::Redo => i("Redo", "redo", "Ctrl+Y", "Edit"),
        Cmd::Cut => i("Cut", "cut", "Ctrl+X", "Edit"),
        Cmd::Copy => i("Copy", "copy", "Ctrl+C", "Edit"),
        Cmd::Paste => i("Paste", "paste", "Ctrl+V", "Edit"),
        Cmd::Delete => i("Delete", "trash", "Del", "Edit"),
        Cmd::Duplicate => i("Duplicate", "duplicate", "Ctrl+D", "Edit"),
        Cmd::Group => i("Group Objects", "group", "Alt+G", "Edit"),
        Cmd::SelectAll => i("Select All", "select", "Ctrl+A", "Select"),
        Cmd::Deselect => i("Deselect All", "select", "Ctrl+Shift+A", "Select"),
        Cmd::InvertSelection => i("Invert Selection", "select", "", "Select"),
        Cmd::SelectChildren => i("Select Children", "select", "", "Select"),
        Cmd::Primitive(k) => i(k.name(), crate::icons::for_primitive(k), "", "Create"),
        Cmd::Null => i("Null", "null", "", "Create"),
        Cmd::Generator(k) => i(k.name(), "generator", "", "Create"),
        Cmd::Deformer(k) => i(k.name(), "deformer", "", "Create"),
        Cmd::Light(k) => i(&format!("{} Light", k.name()), "light", "", "Light"),
        Cmd::NewMaterial => i("New Material", "material", "", "Create"),
        Cmd::Mode(m) => i(
            &format!("{} Mode", m.label()),
            match m {
                EditMode::Model => "model",
                EditMode::Object => "object",
                EditMode::Points => "points",
                EditMode::Edges => "edges",
                EditMode::Polygons => "polys",
                EditMode::Workplane => "workplane",
            },
            match m {
                EditMode::Model => "1",
                _ => "",
            },
            "Modes",
        ),
        Cmd::MakeEditable => i("Make Editable", "editable", "C", "Modes"),
        Cmd::Tool(t) => i(
            match t {
                Tool::Select => "Select",
                Tool::Move => "Move",
                Tool::Rotate => "Rotate",
                Tool::Scale => "Scale",
            },
            tool_icon(t),
            match t {
                Tool::Select => "9",
                Tool::Move => "E",
                Tool::Rotate => "R",
                Tool::Scale => "T",
            },
            "Tools",
        ),
        Cmd::ToggleAxis(a) => i(&format!("{} Axis", ["X", "Y", "Z"][a as usize % 3]), "move", ["X", "Y", "Z"][a as usize % 3], "Tools"),
        Cmd::ToggleCoord => i("World / Object Coordinates", "world", "W", "Tools"),
        Cmd::ResetTransform => i("Reset Transform", "undo", "", "Tools"),
        Cmd::ToggleEnabled => i("Enable / Disable", "power", "", "Tools"),
        Cmd::FrameAll => i("Frame All", "frame", "H", "View"),
        Cmd::FrameSelected => i("Frame Selected", "frame-sel", "S", "View"),
        Cmd::Camera(k) => i(&format!("{} Camera", k.label()), "camera", ["F1", "F2", "F3", "F4"][k.index()], "View"),
        Cmd::Shading(m) => i(
            m.label(),
            match m {
                DisplayMode::Gouraud => "shade",
                DisplayMode::GouraudLines => "shade-lines",
                DisplayMode::Wireframe => "wire",
                DisplayMode::Quick => "shade",
                DisplayMode::QuickLines => "shade-lines",
            },
            "",
            "Shading",
        ),
        Cmd::ToggleGrid => i("Grid", "grid", "", "View"),
        Cmd::Commander => i("Search Commands", "search", "Shift+C", "Help"),
        Cmd::About => i("About Forma3D", "info", "", "Help"),
    }
}

/// Every command, for the command search.
pub fn all() -> Vec<Cmd> {
    let mut v = vec![
        Cmd::New,
        Cmd::Open,
        Cmd::Merge,
        Cmd::Save,
        Cmd::SaveAs,
        Cmd::Revert,
        Cmd::Undo,
        Cmd::Redo,
        Cmd::Cut,
        Cmd::Copy,
        Cmd::Paste,
        Cmd::Delete,
        Cmd::Duplicate,
        Cmd::Group,
        Cmd::SelectAll,
        Cmd::Deselect,
        Cmd::InvertSelection,
        Cmd::SelectChildren,
    ];
    v.extend(PrimitiveKind::ALL.map(Cmd::Primitive));
    v.push(Cmd::Null);
    v.extend(GeneratorKind::ALL.map(Cmd::Generator));
    v.extend(DeformerKind::ALL.map(Cmd::Deformer));
    v.extend(LightKind::ALL.map(Cmd::Light));
    v.push(Cmd::NewMaterial);
    v.extend([Cmd::Mode(EditMode::Model), Cmd::Mode(EditMode::Object), Cmd::MakeEditable]);
    v.extend([Tool::Select, Tool::Move, Tool::Scale, Tool::Rotate].map(Cmd::Tool));
    v.extend([0, 1, 2].map(Cmd::ToggleAxis));
    v.extend([Cmd::ToggleCoord, Cmd::ResetTransform, Cmd::ToggleEnabled, Cmd::FrameAll, Cmd::FrameSelected]);
    v.extend(ViewKind::ALL.map(Cmd::Camera));
    v.extend(DisplayMode::ALL.map(Cmd::Shading));
    v.extend([Cmd::ToggleGrid, Cmd::About]);
    v
}

/// (enabled, checked)
pub fn state(c: Cmd, ed: &Editor, app: &App) -> (bool, Option<bool>) {
    let has_sel = !ed.selection.is_empty();
    match c {
        Cmd::Undo => (ed.history.can_undo(), None),
        Cmd::Redo => (ed.history.redo_label().is_some(), None),
        Cmd::Revert => (ed.file_path.is_some() && ed.is_dirty(), None),
        Cmd::Cut | Cmd::Copy | Cmd::Delete | Cmd::Duplicate | Cmd::Group | Cmd::Deselect | Cmd::SelectChildren | Cmd::ResetTransform | Cmd::ToggleEnabled => {
            (has_sel, None)
        }
        Cmd::Paste => (app.clipboard.with_value(|c| c.is_some()), None),
        Cmd::MakeEditable => (ed.active().is_some_and(|a| matches!(ed.scene.get(a).map(|o| &o.kind), Some(forma3d_core::scene::ObjectKind::Primitive(_)) | Some(forma3d_core::scene::ObjectKind::Generator(_)))), None),
        Cmd::FrameSelected => (has_sel, None),
        Cmd::Mode(m) => (true, Some(ed.mode == m)),
        Cmd::Tool(t) => (true, Some(ed.tool == t)),
        Cmd::ToggleAxis(a) => (true, Some(ed.axes.get(forma3d_core::gizmo::Axis::ALL[a as usize % 3]))),
        Cmd::ToggleCoord => (true, Some(ed.coord == CoordSystem::World)),
        Cmd::Camera(k) => (true, Some(ed.single_view == k)),
        Cmd::Shading(m) => (true, Some(ed.display == m)),
        Cmd::ToggleGrid => (true, Some(ed.view_opts.grid)),
        _ => (true, None),
    }
}

pub fn run(app: App, c: Cmd) {
    exec(app, c, false);
}

/// Runs a command the user confirmed in the discard dialog.
pub fn run_confirmed(app: App, c: Cmd) {
    app.ask.set(None);
    exec(app, c, true);
}

fn exec(app: App, c: Cmd, confirmed: bool) {
    app.menu.set(None);
    let (enabled, _) = app.peek(|ed| state(c, ed, &app));
    if !enabled {
        return;
    }
    match c {
        Cmd::New => guard(app, c, confirmed, "New Project", || app.act(|e| e.new_scene())),
        Cmd::Open => guard(app, c, confirmed, "Open", move || open(app)),
        Cmd::Merge => merge(app),
        Cmd::Save => save(app, false),
        Cmd::SaveAs => save(app, true),
        Cmd::Revert => guard(app, c, confirmed, "Revert", move || {
            // The saved scene is kept in memory, so no file read is needed.
            app.act(|e| {
                if let Err(err) = e.revert() {
                    e.status = format!("Could not revert: {err}");
                }
            })
        }),
        Cmd::Quit => guard(app, c, confirmed, "Quit", || host::window_call("close")),
        Cmd::Undo => app.act(|e| e.undo()),
        Cmd::Redo => app.act(|e| e.redo()),
        Cmd::Cut => {
            let clip = app.act(|e| e.cut_selection());
            if clip.is_some() {
                app.clipboard.set_value(clip);
            }
        }
        Cmd::Copy => {
            let clip = app.peek(|e| (!e.selection.is_empty()).then(|| e.scene.extract(&e.selection)));
            let n = clip.as_ref().map_or(0, |c| c.roots().len());
            app.clipboard.set_value(clip);
            app.status(format!("Copied {n} object(s)"));
        }
        Cmd::Paste => {
            if let Some(clip) = app.clipboard.get_value() {
                app.act(|e| {
                    let ids = e.paste(&clip);
                    e.status = format!("Pasted {} object(s)", ids.len());
                });
            }
        }
        Cmd::Delete => app.act(|e| e.delete_selection()),
        Cmd::Duplicate => app.act(|e| e.duplicate_selection()),
        Cmd::Group => app.act(|e| e.group_selection()),
        Cmd::SelectAll => app.act(|e| e.select_all()),
        Cmd::Deselect => app.act(|e| e.clear_selection()),
        Cmd::InvertSelection => app.act(|e| e.invert_selection()),
        Cmd::SelectChildren => app.act(|e| e.select_children()),
        Cmd::Primitive(k) => app.act(|e| {
            e.create_primitive(k);
        }),
        Cmd::Null => app.act(|e| {
            e.create_null();
        }),
        Cmd::Generator(k) => app.act(|e| {
            e.create_generator(k);
        }),
        Cmd::Deformer(k) => app.act(|e| {
            e.create_deformer(k);
        }),
        Cmd::Light(k) => app.act(|e| {
            e.create_light(k);
        }),
        Cmd::NewMaterial => app.act(|e| {
            let m = e.new_material();
            e.selected_material = Some(m);
            if !e.selection.is_empty() {
                let sel = e.selection.clone();
                e.assign_material(&sel, Some(m));
            }
        }),
        Cmd::Mode(m) => app.act(|e| e.set_mode(m)),
        Cmd::MakeEditable => app.act(|e| {
            e.make_editable();
        }),
        Cmd::Tool(t) => app.act(|e| e.tool = t),
        Cmd::ToggleAxis(a) => app.act(|e| {
            let ax = forma3d_core::gizmo::Axis::ALL[a as usize % 3];
            e.axes.0[ax.index()] = !e.axes.get(ax);
        }),
        Cmd::ToggleCoord => app.act(|e| {
            e.coord = if e.coord == CoordSystem::World { CoordSystem::Object } else { CoordSystem::World };
        }),
        Cmd::ResetTransform => app.act(|e| e.reset_transform()),
        Cmd::ToggleEnabled => app.act(|e| e.toggle_enabled()),
        Cmd::FrameAll => app.view(|e| e.frame_all(e.single_view)),
        Cmd::FrameSelected => app.view(|e| e.frame_selected(e.single_view)),
        Cmd::Camera(k) => app.act(|e| {
            e.single_view = k;
            e.active_view = k;
        }),
        Cmd::Shading(m) => app.act(|e| e.display = m),
        Cmd::ToggleGrid => app.act(|e| e.view_opts.grid = !e.view_opts.grid),
        Cmd::Commander => app.commander.set(true),
        Cmd::About => app.about.set(true),
    }
}

/// Asks before throwing away unsaved changes.
fn guard(app: App, c: Cmd, confirmed: bool, title: &str, then: impl FnOnce()) {
    if !confirmed && app.peek(|e| e.is_dirty()) {
        let name = app.peek(|e| e.project_name());
        app.ask.set(Some(Ask {
            title: format!("{title}: discard changes?"),
            text: format!("\"{name}\" has unsaved changes. They will be lost."),
            ok: "Discard Changes".to_owned(),
            then: c,
        }));
        return;
    }
    then();
}

fn open(app: App) {
    leptos::task::spawn_local(async move {
        let picked = if host::in_tauri() { host::open_scene().await } else { host::upload().await };
        match picked {
            Ok(Some(f)) => app.act(|e| {
                if let Err(err) = e.open_json(&f.text, f.path.clone().into()) {
                    e.status = format!("Could not open: {err}");
                }
                e.frame_all(e.single_view);
            }),
            Ok(None) => {}
            Err(e) => app.error(format!("Could not open: {e}")),
        }
    });
}

fn merge(app: App) {
    leptos::task::spawn_local(async move {
        let picked = if host::in_tauri() { host::pick_scene_text().await } else { host::upload().await };
        match picked {
            Ok(Some(f)) => match forma3d_core::scene::Scene::from_json(&f.text) {
                Ok(other) => app.act(|e| {
                    let ids = e.scene.merge(&other);
                    e.selection = ids.clone();
                    e.status = format!("Merged {} object(s) from {}", ids.len(), f.path);
                }),
                Err(err) => app.error(format!("Could not merge {}: {err}", f.path)),
            },
            Ok(None) => {}
            Err(e) => app.error(format!("Could not merge: {e}")),
        }
    });
}

fn save(app: App, save_as: bool) {
    leptos::task::spawn_local(async move {
        let (path, text, name) = app.peek(|e| {
            let path = (!save_as).then(|| e.file_path.as_ref().map(|p| p.display().to_string())).flatten();
            let name = e.file_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled.forma3d.json".to_owned());
            (path, e.scene.to_json().unwrap_or_default(), name)
        });
        if host::in_tauri() {
            match host::save_scene(path, &name, text).await {
                Ok(Some(written)) => app.act(|e| {
                    let _ = e.save_json(written.into());
                }),
                Ok(None) => {}
                Err(e) => app.error(format!("Could not save: {e}")),
            }
        } else {
            match host::download(&name, &text) {
                Ok(()) => app.act(|e| {
                    let _ = e.save_json(name.clone().into());
                }),
                Err(e) => app.error(format!("Could not save: {e}")),
            }
        }
    });
}

/// The command a key press runs, if any.
pub fn for_key(ev: &web_sys::KeyboardEvent) -> Option<Cmd> {
    let ctrl = ev.ctrl_key() || ev.meta_key();
    let shift = ev.shift_key();
    let alt = ev.alt_key();
    let key = ev.key();
    let k = key.to_lowercase();
    Some(match (ctrl, shift, alt, k.as_str()) {
        (true, false, false, "n") => Cmd::New,
        (true, false, false, "o") => Cmd::Open,
        (true, true, false, "o") => Cmd::Merge,
        (true, false, false, "s") => Cmd::Save,
        (true, true, false, "s") => Cmd::SaveAs,
        (true, false, false, "z") => Cmd::Undo,
        (true, true, false, "z") | (true, false, false, "y") => Cmd::Redo,
        (true, false, false, "x") => Cmd::Cut,
        (true, false, false, "c") => Cmd::Copy,
        (true, false, false, "v") => Cmd::Paste,
        (true, false, false, "d") => Cmd::Duplicate,
        (true, false, false, "a") => Cmd::SelectAll,
        (true, true, false, "a") => Cmd::Deselect,
        (false, false, true, "g") => Cmd::Group,
        (false, _, false, "delete") | (false, _, false, "backspace") => Cmd::Delete,
        (false, false, false, "e") => Cmd::Tool(Tool::Move),
        (false, false, false, "r") => Cmd::Tool(Tool::Rotate),
        (false, false, false, "t") => Cmd::Tool(Tool::Scale),
        (false, false, false, "9") => Cmd::Tool(Tool::Select),
        (false, false, false, "1") => Cmd::Mode(EditMode::Model),
        (false, false, false, "x") => Cmd::ToggleAxis(0),
        (false, false, false, "y") => Cmd::ToggleAxis(1),
        (false, false, false, "z") => Cmd::ToggleAxis(2),
        (false, false, false, "w") => Cmd::ToggleCoord,
        (false, false, false, "c") => Cmd::MakeEditable,
        (false, true, false, "c") => Cmd::Commander,
        (false, false, false, "h") => Cmd::FrameAll,
        (false, false, false, "s") => Cmd::FrameSelected,
        (false, false, false, "f1") => Cmd::Camera(ViewKind::Perspective),
        (false, false, false, "f2") => Cmd::Camera(ViewKind::Top),
        (false, false, false, "f3") => Cmd::Camera(ViewKind::Right),
        (false, false, false, "f4") => Cmd::Camera(ViewKind::Front),
        _ => return None,
    })
}
