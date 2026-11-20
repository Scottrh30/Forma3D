//! File > Import... and File > Export...: models in OBJ, STL, glTF, FBX,
//! USD (text) and Alembic, with a choice of file units.

use crate::fields::{Choice, Toggle};
use crate::host;
use crate::state::{App, app};
use forma3d_core::io::{self, Format};
use leptos::prelude::*;

/// File units: id, label, file units per centimetre.
const UNITS: [(&str, &str, f32); 5] =
    [("mm", "Millimetres", 10.0), ("cm", "Centimetres", 1.0), ("m", "Metres", 0.01), ("in", "Inches", 1.0 / 2.54), ("ft", "Feet", 1.0 / 30.48)];

fn units_for(format: Format) -> &'static str {
    let s = format.default_scale();
    UNITS.iter().min_by(|a, b| (a.2 - s).abs().total_cmp(&(b.2 - s).abs())).map_or("cm", |u| u.0)
}

fn scale_of(units: &str) -> f32 {
    UNITS.iter().find(|u| u.0 == units).map_or(1.0, |u| u.2)
}

#[derive(Clone, Debug, PartialEq)]
pub struct IoState {
    pub export: bool,
    pub format: Format,
    pub units: &'static str,
    pub selected_only: bool,
    /// Import: the model's file name, and the companions picked with it.
    pub file: String,
    pub extra: Vec<String>,
}

thread_local! {
    /// The last Export's choices, offered again next time.
    static LAST_EXPORT: std::cell::RefCell<Option<IoState>> = const { std::cell::RefCell::new(None) };
}

/// What every format can open.
fn accept() -> String {
    let mut v: Vec<String> = Format::ALL.iter().flat_map(|f| f.extensions().iter().map(|e| format!(".{e}"))).collect();
    v.push(".mtl".to_owned());
    v.join(",")
}

pub fn start_import(app: App) {
    leptos::task::spawn_local(async move {
        let files = match host::pick_files(&accept()).await {
            Ok(f) => f,
            Err(e) => return app.error(format!("Could not read the file: {e}")),
        };
        if files.is_empty() {
            return;
        }
        let Some(main) = files.iter().position(|(n, _)| Format::from_file_name(n).is_some()) else {
            let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
            return app.error(format!("{} isn't a model Forma3D can import (OBJ, STL, glTF, FBX, USD or Alembic)", names.join(", ")));
        };
        let format = Format::from_file_name(&files[main].0).unwrap_or(Format::Obj);
        let file = files[main].0.clone();
        let extra = files.iter().enumerate().filter(|(i, _)| *i != main).map(|(_, f)| f.0.clone()).collect();
        let mut files = files;
        let first = files.remove(main);
        files.insert(0, first);
        app.io_files.set_value(files);
        app.io.set(Some(IoState { export: false, format, units: units_for(format), selected_only: false, file, extra }));
    });
}

pub fn start_export(app: App) {
    let last = LAST_EXPORT.with(|l| l.borrow().clone());
    let format = last.as_ref().filter(|s| s.export).map_or(Format::Gltf, |s| s.format);
    let any_selected = app.peek(|e| !e.selection.is_empty());
    app.io.set(Some(IoState {
        export: true,
        format,
        units: last.as_ref().filter(|s| s.export).map_or(units_for(format), |s| s.units),
        selected_only: any_selected,
        file: String::new(),
        extra: Vec::new(),
    }));
}

fn finish_import(app: App, st: &IoState) {
    let files = app.io_files.get_value();
    let Some((name, bytes)) = files.first() else { return };
    let scale = scale_of(st.units);
    // The format chosen in the dialog wins over the file's extension.
    let result =
        if Format::from_file_name(name) == Some(st.format) { io::import_with(name, bytes, &files[1..], scale) } else { io::import_as(st.format, bytes, scale) };
    app.io_files.set_value(Vec::new());
    match result {
        Ok(imported) => {
            let stem = name.rsplit_once('.').map_or(name.as_str(), |s| s.0).to_owned();
            let polys = imported.polygon_count();
            let meshes = imported.mesh_count();
            app.act(|e| {
                let id = io::add_to_scene(&mut e.scene, &imported, &stem);
                e.selection = vec![id];
                e.expanded.insert(id);
                e.label("Import");
                e.status = format!("Imported {name}: {meshes} mesh(es), {polys} polygon(s)");
            });
            for n in &imported.notes {
                app.log_line(format!("{name}: {n}"), false);
            }
            if !imported.notes.is_empty() {
                app.status(format!("Imported {name} ({meshes} mesh(es), {polys} polygons); {}", imported.notes.join("; ")));
            }
        }
        Err(e) => app.error(format!("Could not import {name}: {e}")),
    }
}

fn finish_export(app: App, st: IoState) {
    leptos::task::spawn_local(async move {
        let items = app.view(|ed| io::gather(ed, st.selected_only));
        if items.is_empty() {
            return app.error(if st.selected_only { "Nothing selected to export" } else { "There is nothing to export" });
        }
        let project = app.peek(|e| e.project_name());
        let stem = project.rsplit_once('.').map_or(project.as_str(), |s| s.0).to_owned();
        let ext = st.format.extension();
        let scale = scale_of(st.units);
        let count = items.len();
        if host::in_tauri() {
            let path = match host::choose_save_path("Export", &format!("{stem}.{ext}"), st.format.label(), ext).await {
                Ok(Some(p)) => p,
                Ok(None) => return,
                Err(e) => return app.error(format!("Could not export: {e}")),
            };
            let file = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
            let base = file.rsplit_once('.').map_or(file.as_str(), |s| s.0).to_owned();
            let files = match io::export_files(st.format, &items, scale, &base) {
                Ok(f) => f,
                Err(e) => return app.error(format!("Could not export: {e}")),
            };
            for (name, bytes) in &files {
                if let Err(e) = host::write_chosen(name, bytes).await {
                    return app.error(format!("Could not write {name}: {e}"));
                }
            }
            let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
            app.status(format!("Exported {count} object(s) to {} ({})", path, names.join(" + ")));
        } else {
            let files = match io::export_files(st.format, &items, scale, &stem) {
                Ok(f) => f,
                Err(e) => return app.error(format!("Could not export: {e}")),
            };
            for (name, bytes) in &files {
                let mime = if name.ends_with(".mtl") { "text/plain" } else { st.format.mime() };
                if let Err(e) = host::download_bytes(name, bytes, mime) {
                    return app.error(format!("Could not download {name}: {e}"));
                }
            }
            let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
            app.status(format!("Exported {count} object(s) as {}", names.join(" + ")));
        }
    });
}

fn about(format: Format, export: bool) -> &'static str {
    match (format, export) {
        (Format::Obj, true) => "Polygons and material colours (an MTL file is written beside it).",
        (Format::Obj, false) => "Pick the MTL file too (Ctrl+click) to keep its colours.",
        (Format::Stl, _) => "Triangles only, one solid; no colours. For 3D printing.",
        (Format::Gltf, true) => "A single .glb: meshes, transforms and PBR material colours.",
        (Format::Gltf, false) => ".glb or .gltf with embedded data: meshes, transforms and material colours.",
        (Format::Fbx, _) => "Binary FBX: polygons, transforms, hierarchy and material colours.",
        (Format::Usd, true) => "Text USD (.usda): meshes, transforms and preview materials.",
        (Format::Usd, false) => "Text USD (.usda) only; binary .usdc and .usdz aren't read.",
        (Format::Alembic, _) => "Ogawa Alembic, first frame: meshes and transforms (Alembic has no materials).",
    }
}

#[component]
pub fn IoDialog() -> impl IntoView {
    let app = app();
    let close = move || {
        app.io.set(None);
        app.io_files.set_value(Vec::new());
    };
    let formats: Vec<(String, String)> = Format::ALL.iter().map(|f| (f.id().to_owned(), format!("{} (.{})", f.label(), f.extension()))).collect();
    let units: Vec<(String, String)> = UNITS.iter().map(|u| (u.0.to_owned(), u.1.to_owned())).collect();
    let edit = move |f: &dyn Fn(&mut IoState)| {
        app.io.update(|s| {
            if let Some(s) = s {
                f(s)
            }
        })
    };
    let go = move || {
        let Some(st) = app.io.get_untracked() else { return };
        // Keep the choice for next time (Export), close the dialog.
        if st.export {
            app.io.set(None);
            LAST_EXPORT.with(|l| *l.borrow_mut() = Some(st.clone()));
            finish_export(app, st);
        } else {
            app.io.set(None);
            finish_import(app, &st);
        }
    };
    view! {
        <Show when=move || app.io.with(|s| s.is_some())>
            {
                let formats = formats.clone();
                let units = units.clone();
                let export = app.io.with_untracked(|s| s.as_ref().is_some_and(|s| s.export));
                view! {
                    <div class="modal-bg" on:mousedown=move |_| close()>
                        <div class="dialog prefs viewcfg iodlg" on:mousedown=|ev| ev.stop_propagation() on:keydown=move |ev| if ev.key() == "Enter" { go() }>
                            <h2>{if export { "Export" } else { "Import" }}</h2>
                            <div class="rs-body">
                                {(!export).then(|| view! {
                                    <div class="lbl">
                                        <span>"File"</span>
                                        <span class="bg-name">{move || app.io.with(|s| s.as_ref().map(|s| {
                                            if s.extra.is_empty() { s.file.clone() } else { format!("{} + {}", s.file, s.extra.join(", ")) }
                                        }).unwrap_or_default())}</span>
                                    </div>
                                })}
                                <div class="lbl">
                                    <span>"Format"</span>
                                    <Choice
                                        options=formats
                                        value=Signal::derive(move || app.io.with(|s| s.as_ref().map(|s| s.format.id().to_owned()).unwrap_or_default()))
                                        on_change=Callback::new(move |id: String| {
                                            if let Some(f) = Format::ALL.into_iter().find(|f| f.id() == id) {
                                                edit(&|s| {
                                                    s.format = f;
                                                    s.units = units_for(f);
                                                });
                                            }
                                        })
                                    />
                                </div>
                                <div class="lbl">
                                    <span>"File units"</span>
                                    <Choice
                                        options=units
                                        value=Signal::derive(move || app.io.with(|s| s.as_ref().map(|s| s.units.to_owned()).unwrap_or_default()))
                                        on_change=Callback::new(move |id: String| {
                                            if let Some(u) = UNITS.iter().find(|u| u.0 == id) {
                                                edit(&|s| s.units = u.0);
                                            }
                                        })
                                    />
                                </div>
                                {export.then(|| view! {
                                    <div class="lbl">
                                        <span>"Selected only"</span>
                                        <Toggle
                                            on=Signal::derive(move || app.io.with(|s| s.as_ref().is_some_and(|s| s.selected_only)))
                                            on_change=Callback::new(move |v: bool| edit(&|s| s.selected_only = v))
                                        />
                                    </div>
                                })}
                                <p class="hint">{move || app.io.with(|s| s.as_ref().map(|s| about(s.format, s.export)).unwrap_or(""))}</p>
                                <p class="hint">"The length of one unit in the file. glTF is always metres; Blender's OBJ, STL and Alembic files are usually metres too. FBX and USD files carry their own units."</p>
                            </div>
                            <div class="btns">
                                <button class="btn" on:click=move |_| close()>"Cancel"</button>
                                <button class="btn primary" on:click=move |_| go()>{if export { "Export..." } else { "Import" }}</button>
                            </div>
                        </div>
                    </div>
                }
            }
        </Show>
    }
}
