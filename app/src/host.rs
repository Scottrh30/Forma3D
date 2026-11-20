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

#[derive(serde::Deserialize)]
pub struct BuiltMesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub edges: Vec<u32>,
    pub ms: f64,
}

/// Builds a mesh on a backend worker thread (desktop app).
pub async fn build_mesh(scene: String, object: u64) -> Result<Option<BuiltMesh>, String> {
    let v = invoke("build_mesh", &serde_json::json!({ "scene": scene, "object": object })).await?;
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    serde_wasm_bindgen::from_value(v).map(Some).map_err(|e| e.to_string())
}

/// Reads a scene file by path (desktop app).
pub async fn read_scene(path: &str) -> Result<Opened, String> {
    let v = invoke("read_scene", &serde_json::json!({ "path": path })).await?;
    serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string())
}

/// The scene files the app was started with (double-clicked or named on
/// the command line).
pub async fn startup_files() -> Vec<String> {
    match invoke("startup_files", &serde_json::json!({})).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
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

/// The current Tauri window.
fn current_window(t: &JsValue) -> Result<JsValue, String> {
    let wins = get(t, "window")?;
    let f: Function = get(&wins, "getCurrentWindow")?.dyn_into().map_err(|_| "no getCurrentWindow")?;
    f.call0(&wins).map_err(js_err)
}

/// Calls an async window method and waits for its answer.
async fn window_ask(method: &str, arg: Option<JsValue>) -> Result<JsValue, String> {
    let t = tauri().ok_or("not in the desktop app")?;
    let win = current_window(&t)?;
    let m: Function = get(&win, method)?.dyn_into().map_err(|_| "no such method")?;
    let p = match arg {
        Some(a) => m.call1(&win, &a),
        None => m.call0(&win),
    }
    .map_err(js_err)?;
    match p.dyn_into::<js_sys::Promise>() {
        Ok(p) => JsFuture::from(p).await.map_err(js_err),
        Err(v) => Ok(v),
    }
}

/// Whether the window is (maximized, full screen).
pub async fn window_state() -> (bool, bool) {
    if tauri().is_some() {
        let max = window_ask("isMaximized", None).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
        let full = window_ask("isFullscreen", None).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
        (max, full)
    } else {
        (false, browser_fullscreen())
    }
}

fn browser_fullscreen() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| Reflect::get(&d, &"fullscreenElement".into()).ok())
        .is_some_and(|e| !e.is_null() && !e.is_undefined())
}

/// Full screen on or off (the desktop window, or the page in a browser).
pub async fn toggle_fullscreen() {
    if tauri().is_some() {
        let full = window_ask("isFullscreen", None).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
        if let Err(e) = window_ask("setFullscreen", Some(JsValue::from_bool(!full))).await {
            web_sys::console::warn_1(&format!("full screen: {e}").into());
        }
        return;
    }
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let (target, method): (JsValue, &str) = if browser_fullscreen() {
        (doc.clone().into(), "exitFullscreen")
    } else {
        match doc.document_element() {
            Some(e) => (e.into(), "requestFullscreen"),
            None => return,
        }
    };
    if let Ok(f) = Reflect::get(&target, &method.into()).and_then(|f| f.dyn_into::<Function>().map_err(|v| v)) {
        if let Ok(p) = f.call0(&target).and_then(|p| p.dyn_into::<js_sys::Promise>().map_err(|v| v)) {
            let _ = JsFuture::from(p).await;
        }
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
    input.set_accept(".f3d,.json,application/json");
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

/// Lets the user pick a file; its name and bytes, or None when cancelled.
pub async fn pick_bytes(accept: &str) -> Result<Option<(String, Vec<u8>)>, String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let input: web_sys::HtmlInputElement = doc.create_element("input").map_err(js_err)?.dyn_into().map_err(|_| "no input")?;
    input.set_type("file");
    input.set_accept(accept);
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
    let buf = JsFuture::from(file.array_buffer()).await.map_err(js_err)?;
    Ok(Some((file.name(), js_sys::Uint8Array::new(&buf).to_vec())))
}

/// Lets the user pick one or more files; their names and bytes (empty when
/// cancelled).
pub async fn pick_files(accept: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let input: web_sys::HtmlInputElement = doc.create_element("input").map_err(js_err)?.dyn_into().map_err(|_| "no input")?;
    input.set_type("file");
    input.set_accept(accept);
    input.set_multiple(true);
    let (tx, rx) = futures_channel();
    let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));
    let tx2 = tx.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.call1(&JsValue::NULL, &JsValue::TRUE);
        }
    });
    // A cancelled picker fires "cancel" where supported.
    let on_cancel = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx2.borrow_mut().take() {
            let _ = tx.call1(&JsValue::NULL, &JsValue::FALSE);
        }
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    let _ = input.add_event_listener_with_callback("cancel", on_cancel.as_ref().unchecked_ref());
    input.click();
    JsFuture::from(rx).await.map_err(js_err)?;
    drop(on_change);
    drop(on_cancel);
    let mut out = Vec::new();
    if let Some(list) = input.files() {
        for i in 0..list.length() {
            if let Some(f) = list.get(i) {
                let buf = JsFuture::from(f.array_buffer()).await.map_err(js_err)?;
                out.push((f.name(), js_sys::Uint8Array::new(&buf).to_vec()));
            }
        }
    }
    Ok(out)
}

/// Desktop: asks where to save a file; the chosen path, or None.
pub async fn choose_save_path(title: &str, name: &str, filter: &str, extension: &str) -> Result<Option<String>, String> {
    let v = invoke("choose_save_path", &serde_json::json!({ "title": title, "suggestedName": name, "filterName": filter, "extension": extension })).await?;
    Ok(v.as_string())
}

/// Desktop: writes a file into the folder last chosen with
/// [`choose_save_path`] (only a file name is accepted).
pub async fn write_chosen(name: &str, bytes: &[u8]) -> Result<(), String> {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    invoke("write_chosen", &serde_json::json!({ "name": name, "bytes": b64 })).await.map(|_| ())
}

/// A promise and the function that resolves it.
fn futures_channel() -> (Function, Promise) {
    let mut resolve: Option<Function> = None;
    let p = Promise::new(&mut |res, _rej| resolve = Some(res));
    (resolve.expect("promise executor runs at once"), p)
}

/// Calls `f` with the payload of every `event` the backend emits.
pub fn listen(event: &str, f: impl Fn(JsValue) + 'static) {
    let Some(t) = tauri() else { return };
    let run = || -> Result<(), String> {
        let ev = get(&t, "event")?;
        let l: Function = get(&ev, "listen")?.dyn_into().map_err(|_| "no listen")?;
        let cb = Closure::<dyn Fn(JsValue)>::new(move |e: JsValue| {
            if let Ok(p) = Reflect::get(&e, &"payload".into()) {
                f(p);
            }
        });
        l.call2(&ev, &event.into(), cb.as_ref().unchecked_ref()).map_err(js_err)?;
        cb.forget();
        Ok(())
    };
    if let Err(e) = run() {
        web_sys::console::warn_1(&format!("listen {event}: {e}").into());
    }
}

/// Starts render `id` in the backend (its events carry the id).
pub async fn render_start(id: u64, request: String, folder: Option<String>) -> Result<(), String> {
    invoke("render_start", &serde_json::json!({ "id": id, "request": request, "folder": folder })).await.map(|_| ())
}

pub async fn render_cancel(id: u64) {
    let _ = invoke("render_cancel", &serde_json::json!({ "id": id })).await;
}

/// Saves picture bytes (base64) through a Save dialog; the path, or None
/// when cancelled.
pub async fn save_image(name: &str, extension: &str, b64: String) -> Result<Option<String>, String> {
    let v = invoke("save_image", &serde_json::json!({ "suggestedName": name, "extension": extension, "bytes": b64 })).await?;
    Ok(v.as_string())
}

/// Browser fallback: downloads bytes as a file.
pub fn download_bytes(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&arr);
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js_err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_err)?;
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(js_err)?.dyn_into().map_err(|_| "no anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Lets the browser breathe (draw, handle input) before going on.
pub async fn yield_now() {
    let (tx, p) = futures_channel();
    if let Some(w) = web_sys::window() {
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&tx, 0);
    } else {
        let _ = tx.call0(&JsValue::NULL);
    }
    let _ = JsFuture::from(p).await;
}

/// Milliseconds on a steady clock.
pub fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

/// Local date and time as YYYYMMDD and HHMMSS.
pub fn local_date_time() -> (String, String) {
    let d = js_sys::Date::new_0();
    (
        format!("{:04}{:02}{:02}", d.get_full_year(), d.get_month() + 1, d.get_date()),
        format!("{:02}{:02}{:02}", d.get_hours(), d.get_minutes(), d.get_seconds()),
    )
}
