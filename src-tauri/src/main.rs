//! Forma3D's desktop shell. The whole app (interface and modelling engine)
//! runs as WebAssembly in the window; this side only does what a web page
//! can't: native file dialogs, reading and writing files, and the settings
//! and autosave folders.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use tauri::Manager;
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![open_scene, save_scene, read_setting, write_setting, write_autosave])
        .run(tauri::generate_context!())
        .expect("Forma3D could not start");
}
