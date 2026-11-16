//! Forma3D's desktop shell. The whole app (interface and modelling engine)
//! runs as WebAssembly in the window; this side only does what a web page
//! can't: native file dialogs, reading and writing files, and the settings
//! and autosave folders, and final renders (on every CPU core, in a thread).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine as _;
use forma3d_core::render::{Pass, RenderRequest, date_time, encode_png8};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Serialize)]
struct Opened {
    path: String,
    text: String,
}

/// Shows the Open dialog and reads the chosen scene file.
#[tauri::command]
async fn open_scene(app: tauri::AppHandle) -> Result<Option<Opened>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Open a Forma3D scene")
        .add_filter("Forma3D scene", &["json"])
        .add_filter("All files", &["*"])
        .blocking_pick_file();
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(Opened { path: path.display().to_string(), text }))
}

/// Writes a scene; asks where first when there is no path yet (Save As).
#[tauri::command]
async fn save_scene(app: tauri::AppHandle, path: Option<String>, suggested_name: String, text: String) -> Result<Option<String>, String> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let picked = app
                .dialog()
                .file()
                .set_title("Save the Forma3D scene")
                .add_filter("Forma3D scene", &["json"])
                .set_file_name(&suggested_name)
                .blocking_save_file();
            match picked {
                Some(p) => p.into_path().map_err(|e| e.to_string())?,
                None => return Ok(None),
            }
        }
    };
    write_atomic(&path, &text)?;
    Ok(Some(path.display().to_string()))
}

/// Writes a file through a temporary copy, so a failed write can't destroy the old one.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp-save");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// Settings files are plain names (`prefs.json`), kept in the app's config folder.
fn setting_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) && !name.starts_with('.');
    if !ok {
        return Err(format!("not a settings file name: {name}"));
    }
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join(name))
}

/// Reads a settings file; `None` when it doesn't exist yet.
#[tauri::command]
fn read_setting(app: tauri::AppHandle, name: String) -> Result<Option<String>, String> {
    let path = setting_path(&app, &name)?;
    match std::fs::read_to_string(&path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[tauri::command]
fn write_setting(app: tauri::AppHandle, name: String, text: String) -> Result<(), String> {
    write_atomic(&setting_path(&app, &name)?, &text)
}

/// Writes an autosave copy into the autosave folder; returns the folder.
#[tauri::command]
fn write_autosave(app: tauri::AppHandle, name: String, text: String) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("autosave");
    let file = Path::new(&name).file_name().ok_or("bad autosave name")?.to_owned();
    write_atomic(&dir.join(file), &text)?;
    Ok(dir.display().to_string())
}

/// Renders that are running, by id, with their cancel flags.
#[derive(Default)]
struct Renders {
    running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    id: u64,
    /// 0..1 over all frames.
    progress: f32,
    frame: i32,
    /// The frame's Beauty so far (PNG, base64), now and then.
    preview: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct FrameDone {
    id: u64,
    frame: i32,
    width: u32,
    height: u32,
    /// (pass id, PNG base64)
    passes: Vec<(String, String)>,
    saved: Vec<String>,
    errors: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
struct Finished {
    id: u64,
    cancelled: bool,
    error: Option<String>,
    seconds: f32,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Starts render `id` in the background; progress, frames and the end
/// arrive as `render-progress`, `render-frame` and `render-done` events.
#[tauri::command]
fn render_start(app: tauri::AppHandle, renders: tauri::State<'_, Renders>, id: u64, request: String, folder: Option<String>) -> Result<(), String> {
    let req: RenderRequest = serde_json::from_str(&request).map_err(|e| format!("bad render request: {e}"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    renders.running.lock().map_err(|e| e.to_string())?.insert(id, cancel.clone());
    // Relative file names go next to the project, or into Pictures/Forma3D.
    let base = folder.map(PathBuf::from).or_else(|| app.path().picture_dir().ok().map(|p| p.join("Forma3D"))).unwrap_or_else(|| PathBuf::from("."));
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let result = run_render(&app, id, &req, &base, &cancel);
        let _ = app.emit(
            "render-done",
            Finished { id, cancelled: cancel.load(Ordering::Relaxed), error: result.err(), seconds: t0.elapsed().as_secs_f32() },
        );
        if let Some(r) = app.try_state::<Renders>()
            && let Ok(mut m) = r.running.lock()
        {
            m.remove(&id);
        }
    });
    Ok(())
}

fn run_render(app: &tauri::AppHandle, id: u64, req: &RenderRequest, base: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (date, time) = date_time(secs);
    let count = req.frames.len().max(1) as f32;
    for (k, &frame) in req.frames.iter().enumerate() {
        let mut job = req.job(frame);
        let mut shown = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let done = job.pass();
            let preview = (done || shown.elapsed() > Duration::from_millis(350)).then(|| {
                shown = Instant::now();
                let images = job.images();
                b64(&encode_png8(&images.passes[0].1))
            });
            let _ = app.emit("render-progress", Progress { id, progress: (k as f32 + job.progress()) / count, frame, preview });
            if done {
                break;
            }
        }
        let images = job.images();
        let (mut saved, mut errors) = (Vec::new(), Vec::new());
        if req.settings.save {
            for (name, bytes) in req.files(&images, &date, &time) {
                let path = base.join(&name);
                let wrote = bytes.and_then(|b| {
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    }
                    std::fs::write(&path, b).map_err(|e| e.to_string())
                });
                match wrote {
                    Ok(()) => saved.push(path.display().to_string()),
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                }
            }
        }
        let passes = images.passes.iter().map(|(p, img): &(Pass, _)| (p.id().to_owned(), b64(&encode_png8(img)))).collect();
        let _ = app.emit("render-frame", FrameDone { id, frame, width: images.width, height: images.height, passes, saved, errors });
    }
    Ok(())
}

#[tauri::command]
fn render_cancel(renders: tauri::State<'_, Renders>, id: u64) {
    if let Ok(m) = renders.running.lock()
        && let Some(c) = m.get(&id)
    {
        c.store(true, Ordering::Relaxed);
    }
}

/// Saves a rendered picture: asks where, then writes the bytes (base64).
#[tauri::command]
async fn save_image(app: tauri::AppHandle, suggested_name: String, extension: String, bytes: String) -> Result<Option<String>, String> {
    let data = base64::engine::general_purpose::STANDARD.decode(bytes.as_bytes()).map_err(|e| e.to_string())?;
    let picked = app
        .dialog()
        .file()
        .set_title("Save the picture")
        .add_filter(extension.to_uppercase(), &[extension.as_str()])
        .set_file_name(&suggested_name)
        .blocking_save_file();
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Renders::default())
        .invoke_handler(tauri::generate_handler![
            open_scene,
            save_scene,
            read_setting,
            write_setting,
            write_autosave,
            render_start,
            render_cancel,
            save_image
        ])
        .run(tauri::generate_context!())
        .expect("Forma3D could not start");
}
