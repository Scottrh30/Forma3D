//! Forma3D's desktop shell. The whole app (interface and modelling engine)
//! runs as WebAssembly in the window; this side only does what a web page
//! can't: native file dialogs and reading and writing files.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
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
    // Write a temporary file first, so a failed write can't destroy the old save.
    let tmp = path.with_extension("tmp-save");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })?;
    Ok(Some(path.display().to_string()))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![open_scene, save_scene])
        .run(tauri::generate_context!())
        .expect("Forma3D could not start");
}
