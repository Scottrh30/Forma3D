//! Talks to the Tauri shell: file dialogs, reading and writing files, and the
//! window buttons. Outside Tauri (a plain browser) files go through the
//! browser's upload and download instead.

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

fn tauri() -> Option<JsValue> {
    let w = web_sys::window()?;
    let t = Reflect::get(&w, &"__TAURI__".into()).ok()?;
    (!t.is_undefined() && !t.is_null()).then_some(t)
}

pub fn in_tauri() -> bool {
    tauri().is_some()
}

fn get(obj: &JsValue, key: &str) -> Result<JsValue, String> {
    Reflect::get(obj, &key.into()).map_err(|_| format!("missing {key}"))
}

fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| js_sys::JSON::stringify(&e).ok().and_then(|s| s.as_string()))
        .unwrap_or_else(|| "unknown error".to_owned())
}

/// Calls a Rust command in the Tauri backend.
async fn invoke(cmd: &str, args: &serde_json::Value) -> Result<JsValue, String> {
    let t = tauri().ok_or("not running in the desktop app")?;
    let core = get(&t, "core")?;
    let f: Function = get(&core, "invoke")?
        .dyn_into()
        .map_err(|_| "invoke is not a function")?;
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let p: Promise = f
        .call2(&core, &cmd.into(), &args)
        .map_err(js_err)?
        .dyn_into()
        .map_err(|_| "invoke did not return a promise")?;
    JsFuture::from(p).await.map_err(js_err)
}

#[derive(serde::Deserialize)]
pub struct Opened {
    pub path: String,
    pub text: String,
}

/// Shows the Open dialog and reads the chosen scene. `None` when cancelled.
pub async fn open_scene() -> Result<Option<Opened>, String> {
    let v = invoke("open_scene", &serde_json::json!({})).await?;
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    serde_wasm_bindgen::from_value(v)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Writes a scene, asking where first when `path` is None. Returns the path
/// written, or `None` when cancelled.
pub async fn save_scene(
    path: Option<String>,
    name: &str,
    text: String,
) -> Result<Option<String>, String> {
    let v = invoke(
        "save_scene",
        &serde_json::json!({ "path": path, "suggestedName": name, "text": text }),
    )
    .await?;
    Ok(v.as_string())
}

/// Runs a method of the current Tauri window (minimize, toggleMaximize, close).
pub fn window_call(method: &str) {
    let Some(t) = tauri() else { return };
    let run = || -> Result<(), String> {
        let wins = get(&t, "window")?;
        let f: Function = get(&wins, "getCurrentWindow")?
            .dyn_into()
            .map_err(|_| "no getCurrentWindow")?;
        let win = f.call0(&wins).map_err(js_err)?;
        let m: Function = get(&win, method)?
            .dyn_into()
            .map_err(|_| "no such method")?;
        m.call0(&win).map_err(js_err)?;
        Ok(())
    };
    if let Err(e) = run() {
        web_sys::console::warn_1(&format!("window {method}: {e}").into());
    }
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Reads a settings file (`prefs.json` and so on); `None` when there is none.
pub async fn read_setting(name: &str) -> Option<String> {
    if in_tauri() {
        invoke("read_setting", &serde_json::json!({ "name": name }))
            .await
            .ok()?
            .as_string()
    } else {
        local_storage()?
            .get_item(&format!("forma3d.{name}"))
            .ok()
            .flatten()
    }
}

/// Writes a settings file.
pub async fn write_setting(name: &str, text: &str) -> Result<(), String> {
    if in_tauri() {
        invoke(
            "write_setting",
            &serde_json::json!({ "name": name, "text": text }),
        )
        .await
        .map(|_| ())
    } else {
        local_storage()
            .ok_or("no browser storage")?
            .set_item(&format!("forma3d.{name}"), text)
            .map_err(js_err)
    }
}

/// Writes an autosave copy; returns where it went.
pub async fn write_autosave(name: &str, text: &str) -> Result<String, String> {
    if in_tauri() {
        let v = invoke(
            "write_autosave",
            &serde_json::json!({ "name": name, "text": text }),
        )
        .await?;
        Ok(v.as_string().unwrap_or_default())
    } else {
        local_storage()
            .ok_or("no browser storage")?
            .set_item(&format!("forma3d.autosave.{name}"), text)
            .map_err(js_err)?;
        Ok("this browser's storage".to_owned())
    }
}

/// Closes the app for good (after any unsaved-changes question).
pub fn quit() {
    if in_tauri() {
        window_call("destroy");
    } else if let Some(w) = web_sys::window() {
        let _ = w.close();
    }
}

/// Puts text on the system clipboard (copied objects, as scene JSON).
pub fn copy_text(text: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
    }
}

/// Calls `ask` when the window's close button, Alt+F4 or the taskbar asks
/// the window to close; when it returns true, closing is cancelled.
pub fn on_close_requested(ask: impl Fn() -> bool + 'static) {
    let Some(t) = tauri() else { return };
    let run = || -> Result<(), String> {
        let wins = get(&t, "window")?;
        let f: Function = get(&wins, "getCurrentWindow")?
            .dyn_into()
            .map_err(|_| "no getCurrentWindow")?;
        let win = f.call0(&wins).map_err(js_err)?;
        let on: Function = get(&win, "onCloseRequested")?
            .dyn_into()
            .map_err(|_| "no onCloseRequested")?;
        let handler = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
            if ask()
                && let Ok(prevent) = get(&ev, "preventDefault").and_then(|p| {
                    p.dyn_into::<Function>()
                        .map_err(|_| "no preventDefault".to_owned())
                })
            {
                let _ = prevent.call0(&ev);
            }
        });
        on.call1(&win, handler.as_ref()).map_err(js_err)?;
        handler.forget();
        Ok(())
    };
    if let Err(e) = run() {
        web_sys::console::warn_1(&format!("close handler: {e}").into());
    }
}

/// The interface scale (Preferences), in percent.
pub fn set_zoom(percent: f32) {
    // In the app the whole webview zooms (like a browser's zoom), so every
    // size and pointer position stays consistent.
    if let Some(t) = tauri() {
        let run = || -> Result<(), String> {
            let wv = get(&t, "webview")?;
            let f: Function = get(&wv, "getCurrentWebview")?
                .dyn_into()
                .map_err(|_| "no getCurrentWebview")?;
            let view = f.call0(&wv).map_err(js_err)?;
            let z: Function = get(&view, "setZoom")?
                .dyn_into()
                .map_err(|_| "no setZoom")?;
            z.call1(&view, &JsValue::from_f64(percent as f64 / 100.0))
                .map_err(js_err)?;
            Ok(())
        };
        if let Err(e) = run() {
            web_sys::console::warn_1(&format!("zoom: {e}").into());
        }
        return;
    }
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = el
            .unchecked_ref::<web_sys::HtmlElement>()
            .style()
            .set_property("zoom", &format!("{}%", percent.round()));
    }
}

/// Browser fallback: downloads the text as a file.
pub fn download(name: &str, text: &str) -> Result<(), String> {
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).map_err(js_err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_err)?;
    let a: web_sys::HtmlAnchorElement = doc
        .create_element("a")
        .map_err(js_err)?
        .dyn_into()
        .map_err(|_| "no anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Browser fallback: asks for a file and reads it as text.
pub async fn upload() -> Result<Option<Opened>, String> {
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let input: web_sys::HtmlInputElement = doc
        .create_element("input")
        .map_err(js_err)?
        .dyn_into()
        .map_err(|_| "no input")?;
    input.set_type("file");
    input.set_accept(".json,application/json");
    let (tx, rx) = futures_channel();
    let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));
    let input2 = input.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.call1(
                &JsValue::NULL,
                &input2
                    .files()
                    .and_then(|f| f.get(0))
                    .map(JsValue::from)
                    .unwrap_or(JsValue::NULL),
            );
        }
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    input.click();
    let file = JsFuture::from(rx).await.map_err(js_err)?;
    drop(on_change);
    let Ok(file) = file.dyn_into::<web_sys::File>() else {
        return Ok(None);
    };
    let text = JsFuture::from(file.text())
        .await
        .map_err(js_err)?
        .as_string()
        .unwrap_or_default();
    Ok(Some(Opened {
        path: file.name(),
        text,
    }))
}

/// A promise and the function that resolves it.
fn futures_channel() -> (Function, Promise) {
    let mut resolve: Option<Function> = None;
    let p = Promise::new(&mut |res, _rej| resolve = Some(res));
    (resolve.expect("promise executor runs at once"), p)
}
