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
    e.as_string().or_else(|| js_sys::JSON::stringify(&e).ok().and_then(|s| s.as_string())).unwrap_or_else(|| "unknown error".to_owned())
}

/// Calls a Rust command in the Tauri backend.
async fn invoke(cmd: &str, args: &serde_json::Value) -> Result<JsValue, String> {
    let t = tauri().ok_or("not running in the desktop app")?;
    let core = get(&t, "core")?;
    let f: Function = get(&core, "invoke")?.dyn_into().map_err(|_| "invoke is not a function")?;
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let p: Promise = f.call2(&core, &cmd.into(), &args).map_err(js_err)?.dyn_into().map_err(|_| "invoke did not return a promise")?;
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
    serde_wasm_bindgen::from_value(v).map(Some).map_err(|e| e.to_string())
}

/// Writes a scene, asking where first when `path` is None. Returns the path
/// written, or `None` when cancelled.
pub async fn save_scene(path: Option<String>, name: &str, text: String) -> Result<Option<String>, String> {
    let v = invoke("save_scene", &serde_json::json!({ "path": path, "suggestedName": name, "text": text })).await?;
    Ok(v.as_string())
}

/// Reads another scene file for Merge Project.
pub async fn pick_scene_text() -> Result<Option<Opened>, String> {
    open_scene().await
}

/// Runs a method of the current Tauri window (minimize, toggleMaximize, close).
pub fn window_call(method: &str) {
    let Some(t) = tauri() else { return };
    let run = || -> Result<(), String> {
        let wins = get(&t, "window")?;
        let f: Function = get(&wins, "getCurrentWindow")?.dyn_into().map_err(|_| "no getCurrentWindow")?;
        let win = f.call0(&wins).map_err(js_err)?;
        let m: Function = get(&win, method)?.dyn_into().map_err(|_| "no such method")?;
        m.call0(&win).map_err(js_err)?;
        Ok(())
    };
    if let Err(e) = run() {
        web_sys::console::warn_1(&format!("window {method}: {e}").into());
    }
}

/// Browser fallback: downloads the text as a file.
pub fn download(name: &str, text: &str) -> Result<(), String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).map_err(js_err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_err)?;
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(js_err)?.dyn_into().map_err(|_| "no anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Browser fallback: asks for a file and reads it as text.
pub async fn upload() -> Result<Option<Opened>, String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let input: web_sys::HtmlInputElement = doc.create_element("input").map_err(js_err)?.dyn_into().map_err(|_| "no input")?;
    input.set_type("file");
    input.set_accept(".json,application/json");
    let (tx, rx) = futures_channel();
    let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));
    let input2 = input.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.call1(&JsValue::NULL, &input2.files().and_then(|f| f.get(0)).map(JsValue::from).unwrap_or(JsValue::NULL));
        }
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    input.click();
    let file = JsFuture::from(rx).await.map_err(js_err)?;
    drop(on_change);
    let Ok(file) = file.dyn_into::<web_sys::File>() else { return Ok(None) };
    let text = JsFuture::from(file.text()).await.map_err(js_err)?.as_string().unwrap_or_default();
    Ok(Some(Opened { path: file.name(), text }))
}

/// A promise and the function that resolves it.
fn futures_channel() -> (Function, Promise) {
    let mut resolve: Option<Function> = None;
    let p = Promise::new(&mut |res, _rej| resolve = Some(res));
    (resolve.expect("promise executor runs at once"), p)
}
