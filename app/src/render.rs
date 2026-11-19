//! Rendering in the app: Render View, Render to Picture Viewer, the
//! Interactive Render Region, the Render Queue, the Picture Viewer, the
//! Light Lister and the Render Settings dialog.
//!
//! In the desktop app the backend renders on every CPU core and sends
//! progress and finished frames as events; in a browser the renderer runs
//! here in small time slices so the interface stays responsive.

use crate::fields::{Choice, NumField, Toggle, parse_hex, rgb_hex};
use crate::host;
use crate::icons::icon_s;
use crate::state::{App, app};
use base64::Engine as _;
use forma3d_core::editor::Editor;
use forma3d_core::render::{self as r, Engine, FrameRange, Image, Pass, RenderRequest, RenderSettings};
use forma3d_core::scene::ObjectKind;
use forma3d_core::scene_camera::CameraPose;
use leptos::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Where a render goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Over the active view.
    View,
    /// The Interactive Render Region: part of the active view, re-rendered
    /// whenever something changes.
    Region,
    /// The Picture Viewer (and files, when Save is on).
    Viewer,
}

#[derive(Clone, Debug)]
pub struct ShotFrame {
    pub frame: i32,
    /// Each pass as a PNG data URL.
    pub passes: Vec<(Pass, String)>,
}

/// One render in the Picture Viewer's history (or over the view).
#[derive(Clone, Debug)]
pub struct Shot {
    pub id: u64,
    pub title: String,
    pub engine: Engine,
    pub width: u32,
    pub height: u32,
    pub frames: Vec<ShotFrame>,
    /// The frame being rendered, so far.
    pub preview: Option<String>,
    pub progress: f32,
    pub done: bool,
    pub cancelled: bool,
    pub seconds: f32,
    pub saved: Vec<String>,
    pub error: Option<String>,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueueStatus {
    Waiting,
    Rendering(f32),
    Done,
    Stopped,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: u64,
    pub name: String,
    pub request: RenderRequest,
    pub folder: Option<String>,
    pub status: QueueStatus,
    pub on: bool,
}

#[derive(Clone, Debug)]
pub struct RenderUi {
    /// Picture Viewer history, newest first.
    pub shots: Vec<Shot>,
    pub selected: Option<u64>,
    /// A/B compare: the other picture, and where the split is (0..1).
    pub compare: Option<u64>,
    pub split: f32,
    pub pass: Pass,
    pub frame_index: usize,
    /// 0 = fit to the panel; otherwise a scale (1 = 100 %).
    pub zoom: f32,
    /// The render over the active view (Render View, Render Region).
    pub view: Option<Shot>,
    pub region: bool,
    pub queue: Vec<QueueItem>,
    pub queue_running: bool,
    pub settings_open: bool,
}

impl Default for RenderUi {
    fn default() -> Self {
        RenderUi {
            shots: Vec::new(),
            selected: None,
            compare: None,
            split: 0.5,
            pass: Pass::Beauty,
            frame_index: 0,
            zoom: 0.0,
            view: None,
            region: false,
            queue: Vec::new(),
            queue_running: false,
            settings_open: false,
        }
    }
}

struct Running {
    id: u64,
    queue: Option<u64>,
    cancel: Rc<Cell<bool>>,
    backend: bool,
}

/// The viewport's offscreen GPU renderer (the Viewport engine): renders
/// the scene at (width, height) through a pose, then calls back with the
/// picture.
pub type HardwareFn = Rc<dyn Fn(u32, u32, CameraPose, Box<dyn FnOnce(Result<Image, String>)>)>;

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    static NEXT: Cell<u64> = const { Cell::new(1) };
    /// The active view's size in device pixels (Render View renders at it).
    static VIEW_PX: Cell<(u32, u32)> = const { Cell::new((960, 540)) };
    static HARDWARE: RefCell<Option<HardwareFn>> = const { RefCell::new(None) };
    static REGION: Cell<bool> = const { Cell::new(false) };
}

fn next_id() -> u64 {
    NEXT.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    })
}

pub fn set_view_size(w: u32, h: u32) {
    VIEW_PX.with(|v| v.set((w.clamp(16, 4096), h.clamp(16, 4096))));
}

pub fn set_hardware(f: HardwareFn) {
    HARDWARE.with(|h| *h.borrow_mut() = Some(f));
}

pub fn busy() -> bool {
    RUNNING.with(|r| r.borrow().is_some())
}

pub fn region_on() -> bool {
    REGION.with(Cell::get)
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn with_shot(app: App, id: u64, f: impl FnOnce(&mut Shot)) {
    app.render.update(|ui| {
        if let Some(s) = ui.view.as_mut().filter(|s| s.id == id) {
            f(s);
        } else if let Some(s) = ui.shots.iter_mut().find(|s| s.id == id) {
            f(s);
        }
    });
}

/// Sets up the backend's render events (desktop app).
pub fn init(app: App) {
    if !host::in_tauri() {
        return;
    }
    #[derive(serde::Deserialize)]
    struct Progress {
        id: u64,
        progress: f32,
        preview: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct FrameDone {
        id: u64,
        frame: i32,
        passes: Vec<(String, String)>,
        saved: Vec<String>,
        errors: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Finished {
        id: u64,
        cancelled: bool,
        error: Option<String>,
        seconds: f32,
    }
    host::listen("render-progress", move |v| {
        let Ok(p) = serde_wasm_bindgen::from_value::<Progress>(v) else { return };
        progress(app, p.id, p.progress, p.preview.map(|b| format!("data:image/png;base64,{b}")));
    });
    host::listen("render-frame", move |v| {
        let Ok(f) = serde_wasm_bindgen::from_value::<FrameDone>(v) else { return };
        let passes = f
            .passes
            .into_iter()
            .filter_map(|(id, b)| Pass::ALL.into_iter().find(|p| p.id() == id).map(|p| (p, format!("data:image/png;base64,{b}"))))
            .collect();
        for e in &f.errors {
            app.log_line(format!("Could not save {e}"), true);
        }
        for s in &f.saved {
            app.log_line(format!("Saved {s}"), false);
        }
        frame_done(app, f.id, ShotFrame { frame: f.frame, passes }, f.saved);
    });
    host::listen("render-done", move |v| {
        let Ok(d) = serde_wasm_bindgen::from_value::<Finished>(v) else { return };
        finish(app, d.id, d.cancelled, d.error, d.seconds);
    });
}

fn progress(app: App, id: u64, p: f32, preview: Option<String>) {
    let queue = RUNNING.with(|r| r.borrow().as_ref().filter(|r| r.id == id).and_then(|r| r.queue));
    with_shot(app, id, |s| {
        s.progress = p;
        if preview.is_some() {
            s.preview = preview;
        }
    });
    if let Some(q) = queue {
        set_queue_status(app, q, QueueStatus::Rendering(p));
    }
}

fn frame_done(app: App, id: u64, frame: ShotFrame, saved: Vec<String>) {
    with_shot(app, id, |s| {
        s.preview = frame.passes.first().map(|p| p.1.clone());
        s.frames.push(frame);
        s.saved.extend(saved);
    });
}

fn finish(app: App, id: u64, cancelled: bool, error: Option<String>, seconds: f32) {
    let run = RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        if r.as_ref().is_some_and(|x| x.id == id) { r.take() } else { None }
    });
    let mut title = String::new();
    let mut target = Target::View;
    with_shot(app, id, |s| {
        s.done = true;
        s.cancelled = cancelled;
        s.seconds = seconds;
        s.error = error.clone();
        s.progress = if cancelled { s.progress } else { 1.0 };
        title = s.title.clone();
        target = s.target;
    });
    match (&error, cancelled) {
        (Some(e), _) => app.log_line(format!("Render failed: {e}"), true),
        (None, true) if target == Target::Viewer => app.log_line("Render stopped", false),
        (None, false) if target == Target::Viewer => app.log_line(format!("Rendered {title} in {seconds:.1} s"), false),
        _ => {}
    }
    if let Some(q) = run.and_then(|r| r.queue) {
        let status = match (error, cancelled) {
            (Some(e), _) => QueueStatus::Failed(e),
            (None, true) => QueueStatus::Stopped,
            (None, false) => QueueStatus::Done,
        };
        set_queue_status(app, q, status);
        if !cancelled && app.render.with_untracked(|u| u.queue_running) {
            next_in_queue(app);
        } else {
            app.render.update(|u| u.queue_running = false);
        }
    }
}

/// The current take's name.
fn take_name(ed: &Editor) -> String {
    let t = ed.scene.takes();
    t.takes.get(ed.scene.active_take()).map_or_else(|| "Main".to_owned(), |t| t.name.clone())
}

/// What a render of the editor's scene needs.
fn request(ed: &Editor, target: Target) -> (RenderRequest, Option<String>) {
    let mut s = ed.scene.render.sanitized();
    let mut scene = ed.scene.clone();
    let frames = match target {
        Target::Viewer => s.frames(ed.timeline.frame, (ed.timeline.min, ed.timeline.max)),
        Target::View | Target::Region => {
            let (w, h) = VIEW_PX.with(Cell::get);
            s.width = w;
            s.height = h;
            s.save = false;
            s.passes.clear();
            s.frame_range = FrameRange::CurrentFrame;
            // The view shows what the view sees.
            scene.render_camera = ed.look_through;
            vec![ed.timeline.frame]
        }
    };
    let aspect = s.width as f32 / s.height.max(1) as f32;
    let fallback = CameraPose::from_editor(&ed.cameras[ed.active_view.index()], aspect);
    let region = (target == Target::Region).then(|| {
        let (w, h) = (s.width as f32, s.height as f32);
        [(w * 0.2) as u32, (h * 0.2) as u32, (w * 0.8) as u32, (h * 0.8) as u32]
    });
    let folder = ed.file_path.as_ref().and_then(|p| p.parent()).map(|p| p.display().to_string());
    let project = ed.project_name().trim_end_matches(".json").trim_end_matches(".f3d").trim_end_matches(".forma3d").to_owned();
    (RenderRequest { scene, fps: ed.timeline.fps, frames, settings: s, fallback, region, project, take: take_name(ed) }, folder)
}

/// Starts a render (stopping one that is running).
pub fn start(app: App, target: Target) {
    stop_quiet(app);
    if target != Target::Region {
        REGION.with(|r| r.set(false));
        app.render.update(|u| u.region = false);
    }
    let (req, folder) = app.peek(|ed| request(ed, target));
    launch(app, req, folder, target, None);
}

fn launch(app: App, req: RenderRequest, folder: Option<String>, target: Target, queue: Option<u64>) {
    let id = next_id();
    let s = req.settings.clone();
    let shot = Shot {
        id,
        title: format!("{} · {}", req.project, req.camera_name()),
        engine: s.engine,
        width: s.width,
        height: s.height,
        frames: Vec::new(),
        preview: None,
        progress: 0.0,
        done: false,
        cancelled: false,
        seconds: 0.0,
        saved: Vec::new(),
        error: None,
        target,
    };
    app.render.update(|u| {
        if target == Target::Viewer {
            u.shots.insert(0, shot);
            u.shots.truncate(40);
            u.selected = Some(id);
            u.frame_index = 0;
        } else {
            u.view = Some(shot);
        }
    });
    if target == Target::Viewer {
        app.show_panel(crate::workspace::Panel::PictureViewer);
    }
    let cancel = Rc::new(Cell::new(false));
    let backend = host::in_tauri() && s.engine != Engine::Viewport;
    RUNNING.with(|r| *r.borrow_mut() = Some(Running { id, queue, cancel: cancel.clone(), backend }));
    if s.engine == Engine::Viewport {
        leptos::task::spawn_local(hardware_loop(app, id, req, cancel));
    } else if backend {
        leptos::task::spawn_local(async move {
            let json = match serde_json::to_string(&req) {
                Ok(j) => j,
                Err(e) => return finish(app, id, false, Some(e.to_string()), 0.0),
            };
            if let Err(e) = host::render_start(id, json, folder).await {
                finish(app, id, false, Some(e), 0.0);
            }
        });
    } else {
        leptos::task::spawn_local(browser_loop(app, id, req, cancel));
    }
}

/// Renders here, a few rows at a time.
async fn browser_loop(app: App, id: u64, req: RenderRequest, cancel: Rc<Cell<bool>>) {
    let t0 = host::now_ms();
    let (date, time) = host::local_date_time();
    let count = req.frames.len().max(1) as f32;
    for (k, &frame) in req.frames.iter().enumerate() {
        host::yield_now().await;
        let mut job = req.job(frame);
        let mut shown = host::now_ms();
        let started = host::now_ms();
        let mut stopped = false;
        loop {
            if cancel.get() {
                // A render that only ends when stopped keeps its picture.
                if job.open_ended() && job.passes_done() > 0 {
                    stopped = true;
                    break;
                }
                return finish(app, id, true, None, ((host::now_ms() - t0) / 1000.0) as f32);
            }
            job.set_elapsed_ms(host::now_ms() - started);
            let slice = host::now_ms();
            while host::now_ms() - slice < 24.0 {
                if job.step(2) {
                    break;
                }
            }
            let done = job.done();
            let preview = (done || host::now_ms() - shown > 400.0).then(|| {
                shown = host::now_ms();
                data_url(&r::encode_png8(&job.images().passes[0].1))
            });
            progress(app, id, (k as f32 + job.progress()) / count, preview);
            if done {
                break;
            }
            host::yield_now().await;
        }
        let images = job.images();
        let mut saved = Vec::new();
        if req.settings.save {
            for (name, bytes) in req.files(&images, &date, &time) {
                let file = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_owned();
                let mime = if file.ends_with(".jpg") { "image/jpeg" } else { "image/png" };
                match bytes.and_then(|b| host::download_bytes(&file, &b, mime)) {
                    Ok(()) => saved.push(file),
                    Err(e) => app.log_line(format!("Could not save {file}: {e}"), true),
                }
            }
        }
        let passes = images.passes.iter().map(|(p, img)| (*p, data_url(&r::encode_png8(img)))).collect();
        frame_done(app, id, ShotFrame { frame, passes }, saved);
        if stopped {
            return finish(app, id, true, None, ((host::now_ms() - t0) / 1000.0) as f32);
        }
    }
    finish(app, id, false, None, ((host::now_ms() - t0) / 1000.0) as f32);
}

/// The Viewport engine: the viewport's own GPU renderer, offscreen.
async fn hardware_loop(app: App, id: u64, req: RenderRequest, cancel: Rc<Cell<bool>>) {
    let t0 = host::now_ms();
    let Some(hw) = HARDWARE.with(|h| h.borrow().clone()) else {
        return finish(app, id, false, Some("the viewport's graphics are not running".into()), 0.0);
    };
    let s = req.settings.sanitized();
    let saved_frame = app.peek(|e| e.timeline.frame);
    let count = req.frames.len().max(1) as f32;
    let (date, time) = host::local_date_time();
    for (k, &frame) in req.frames.iter().enumerate() {
        if cancel.get() {
            break;
        }
        if app.peek(|e| e.timeline.frame) != frame {
            app.view(|e| e.timeline.frame = frame);
        }
        let pose = req.scene.render_camera.and_then(|c| forma3d_core::scene_camera::pose(&req.scene, c, frame as f32 / req.fps.max(1.0))).unwrap_or(req.fallback);
        let slot: Rc<RefCell<Option<Result<Image, String>>>> = Rc::new(RefCell::new(None));
        let put = slot.clone();
        hw(s.width, s.height, pose, Box::new(move |img| *put.borrow_mut() = Some(img)));
        let img = loop {
            if let Some(v) = slot.borrow_mut().take() {
                break v;
            }
            host::yield_now().await;
        };
        match img {
            Ok(img) => {
                let png = r::encode_png8(&img);
                let mut saved = Vec::new();
                if s.save {
                    let images = r::FrameImages { width: img.width, height: img.height, frame, passes: vec![(Pass::Beauty, img.clone())], beauty16: None };
                    for (name, bytes) in req.files(&images, &date, &time) {
                        let file = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_owned();
                        if let Ok(b) = bytes {
                            if host::in_tauri() {
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&b);
                                let ext = file.rsplit('.').next().unwrap_or("png").to_owned();
                                if let Ok(Some(p)) = host::save_image(&file, &ext, b64).await {
                                    saved.push(p);
                                }
                            } else if host::download_bytes(&file, &b, "image/png").is_ok() {
                                saved.push(file);
                            }
                        }
                    }
                }
                frame_done(app, id, ShotFrame { frame, passes: vec![(Pass::Beauty, data_url(&png))] }, saved);
                progress(app, id, (k + 1) as f32 / count, None);
            }
            Err(e) => {
                restore_frame(app, saved_frame);
                return finish(app, id, false, Some(e), 0.0);
            }
        }
    }
    restore_frame(app, saved_frame);
    finish(app, id, cancel.get(), None, ((host::now_ms() - t0) / 1000.0) as f32);
}

fn restore_frame(app: App, frame: i32) {
    if app.peek(|e| e.timeline.frame) != frame {
        app.view(|e| e.timeline.frame = frame);
    }
}

/// Stops the running render, without a message.
fn stop_quiet(app: App) {
    let run = RUNNING.with(|r| r.borrow().as_ref().map(|r| (r.id, r.backend, r.cancel.clone())));
    if let Some((id, backend, cancel)) = run {
        cancel.set(true);
        if backend {
            leptos::task::spawn_local(host::render_cancel(id));
            // The backend reports the end; mark it here so a new render can start.
            RUNNING.with(|r| r.borrow_mut().take());
            with_shot(app, id, |s| {
                s.done = true;
                s.cancelled = true;
            });
        }
    }
}

pub fn stop(app: App) {
    REGION.with(|r| r.set(false));
    app.render.update(|u| {
        u.region = false;
        u.queue_running = false;
    });
    if !busy() {
        app.status("Nothing is rendering");
        return;
    }
    stop_quiet(app);
    app.status("Render stopped");
}

pub fn toggle_region(app: App) {
    let on = !region_on();
    REGION.with(|r| r.set(on));
    app.render.update(|u| u.region = on);
    if on {
        start(app, Target::Region);
    } else {
        stop_quiet(app);
        app.render.update(|u| u.view = None);
    }
}

pub fn open_settings(app: App) {
    app.render.update(|u| u.settings_open = true);
}

pub fn add_to_queue(app: App) {
    let (req, folder) = app.peek(|ed| request(ed, Target::Viewer));
    let name = format!("{} · {}", req.project, req.camera_name());
    app.render.update(|u| {
        u.queue.push(QueueItem { id: next_id(), name: name.clone(), request: req, folder, status: QueueStatus::Waiting, on: true });
    });
    app.show_panel(crate::workspace::Panel::RenderQueue);
    app.status(format!("Added {name} to the Render Queue"));
}

fn set_queue_status(app: App, id: u64, status: QueueStatus) {
    app.render.update(|u| {
        if let Some(q) = u.queue.iter_mut().find(|q| q.id == id) {
            q.status = status;
        }
    });
}

fn next_in_queue(app: App) {
    let next = app.render.with_untracked(|u| u.queue.iter().find(|q| q.on && q.status == QueueStatus::Waiting).cloned());
    match next {
        Some(q) => {
            set_queue_status(app, q.id, QueueStatus::Rendering(0.0));
            launch(app, q.request, q.folder, Target::Viewer, Some(q.id));
        }
        None => {
            app.render.update(|u| u.queue_running = false);
            app.log_line("Render Queue finished", false);
        }
    }
}

// ---------------------------------------------------------------------------
// Over the viewport
// ---------------------------------------------------------------------------

/// The Render View / Render Region picture over the active view. Changing
/// the scene or the view clears a Render View; the Region renders again.
#[component]
pub fn RenderOverlay(#[prop(into)] rect: Signal<Option<(f32, f32, f32, f32)>>) -> impl IntoView {
    let app = app();
    Effect::new(move |first: Option<()>| {
        app.rev.track();
        app.view_rev.track();
        if first.is_none() {
            return;
        }
        if region_on() {
            start(app, Target::Region);
        } else if app.render.with_untracked(|u| u.view.as_ref().is_some_and(|v| v.done)) {
            app.render.update(|u| u.view = None);
        }
    });
    move || {
        let (x, y, w, h) = rect.get()?;
        let (src, prog, done, region) = app.render.with(|u| u.view.as_ref().map(|v| (v.preview.clone(), v.progress, v.done, v.target == Target::Region)))?;
        let style = format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px");
        let frame = region.then(|| {
            let style = format!("left:{}px;top:{}px;width:{}px;height:{}px", x + w * 0.2, y + h * 0.2, w * 0.6, h * 0.6);
            view! { <div class="rregion" style=style></div> }
        });
        Some(view! {
            {src.map(|s| view! { <img class="rview" style=style.clone() src=s /> })}
            {frame}
            {(!done).then(|| view! {
                <div class="rprog" style=format!("left:{}px;top:{}px;width:{}px", x + 10.0, y + h - 14.0, (w - 20.0).max(10.0))>
                    <div style=format!("width:{:.1}%", prog * 100.0)></div>
                </div>
            })}
        })
    }
}

// ---------------------------------------------------------------------------
// Picture Viewer
// ---------------------------------------------------------------------------

fn pass_label(p: Pass) -> String {
    p.label().to_owned()
}

/// The picture of a shot for a pass and frame.
fn picture(s: &Shot, pass: Pass, frame: usize) -> Option<String> {
    let f = s.frames.get(frame.min(s.frames.len().saturating_sub(1)));
    match f {
        Some(f) => f.passes.iter().find(|p| p.0 == pass).or_else(|| f.passes.first()).map(|p| p.1.clone()),
        None => s.preview.clone(),
    }
}

async fn save_picture(app: App, name: String, url: String) {
    let b64 = url.split(',').nth(1).unwrap_or("").to_owned();
    let result = if host::in_tauri() {
        host::save_image(&name, "png", b64).await.map(|p| p.map(|p| format!("Saved {p}")))
    } else {
        base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| e.to_string())
            .and_then(|b| host::download_bytes(&name, &b, "image/png"))
            .map(|()| Some(format!("Downloaded {name}")))
    };
    match result {
        Ok(Some(m)) => app.log_line(m, false),
        Ok(None) => {}
        Err(e) => app.log_line(format!("Could not save the picture: {e}"), true),
    }
}

#[component]
pub fn PictureViewer() -> impl IntoView {
    let app = app();
    let ui = app.render;
    let current = move || ui.with(|u| u.selected.and_then(|id| u.shots.iter().find(|s| s.id == id).cloned()).or_else(|| u.shots.first().cloned()));
    let image = move || {
        let (pass, fi, zoom, compare, split) = ui.with(|u| (u.pass, u.frame_index, u.zoom, u.compare, u.split));
        let Some(s) = current() else {
            return view! { <p class="hint pv-empty">"Render to Picture Viewer (Shift+R) to see pictures here."</p> }.into_any();
        };
        let a = picture(&s, pass, fi);
        let b = compare.filter(|c| *c != s.id).and_then(|c| ui.with(|u| u.shots.iter().find(|x| x.id == c).and_then(|x| picture(x, pass, fi))));
        let size = if zoom > 0.0 { format!("width:{}px;max-width:none;max-height:none", s.width as f32 * zoom) } else { String::new() };
        let b_view = b.map(|b| {
            view! {
                <img class="pv-img pv-b" style=format!("{size};clip-path:inset(0 0 0 {:.1}%)", split * 100.0) src=b />
                <div class="pv-split" style=format!("left:{:.1}%", split * 100.0)></div>
            }
        });
        view! {
            <div class="pv-pic" class:fit=zoom == 0.0>
                <div class="pv-stack">
                    {a.map(|a| view! { <img class="pv-img" style=size.clone() src=a /> })}
                    {b_view}
                </div>
            </div>
        }
        .into_any()
    };
    let pass_opts = move || {
        let passes: Vec<Pass> = current().and_then(|s| s.frames.first().map(|f| f.passes.iter().map(|p| p.0).collect())).unwrap_or_else(|| vec![Pass::Beauty]);
        passes.into_iter().map(|p| (p.id().to_owned(), pass_label(p))).collect::<Vec<_>>()
    };
    let frames = move || current().map(|s| s.frames.iter().enumerate().map(|(i, f)| (i.to_string(), format!("Frame {}", f.frame))).collect::<Vec<_>>()).unwrap_or_default();
    let info = move || {
        current().map(|s| {
            let state = if let Some(e) = &s.error {
                format!("Failed: {e}")
            } else if s.cancelled {
                "Stopped".to_owned()
            } else if s.done {
                format!("{:.1} s", s.seconds)
            } else {
                format!("Rendering {:.0} %", s.progress * 100.0)
            };
            format!("{} × {} · {} · {}", s.width, s.height, s.engine.label(), state)
        })
    };
    let save = move |_| {
        let (pass, fi) = ui.with_untracked(|u| (u.pass, u.frame_index));
        if let Some(s) = current()
            && let Some(url) = picture(&s, pass, fi)
        {
            let frame = s.frames.get(fi).map_or(0, |f| f.frame);
            let name = format!("{}_{:04}_{}.png", s.title.replace(" · ", "_").replace(['/', '\\', ':'], "_"), frame, pass.id());
            leptos::task::spawn_local(save_picture(app, name, url));
        }
    };
    let history = move || {
        let (sel, cmp) = ui.with(|u| (u.selected, u.compare));
        ui.with(|u| u.shots.clone())
            .into_iter()
            .map(|s| {
                let id = s.id;
                let thumb = s.frames.first().and_then(|f| f.passes.first().map(|p| p.1.clone())).or(s.preview.clone());
                let when = if s.done { format!("{:.1} s", s.seconds) } else { format!("{:.0} %", s.progress * 100.0) };
                view! {
                    <div class="pv-item" class:sel=sel == Some(id) on:click=move |_| ui.update(|u| { u.selected = Some(id); u.frame_index = 0; })>
                        {thumb.map(|t| view! { <img src=t /> })}
                        <div class="pv-meta">
                            <span>{s.title.clone()}</span>
                            <span class="hint">{format!("{}×{} · {when}", s.width, s.height)}</span>
                        </div>
                        <button
                            class="tb small"
                            class:on=cmp == Some(id)
                            title="Compare with the shown picture (A/B)"
                            on:click=move |ev| {
                                ev.stop_propagation();
                                ui.update(|u| u.compare = if u.compare == Some(id) { None } else { Some(id) });
                            }
                        >"B"</button>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <div class="pv">
            <div class="bar pv-bar">
                {move || view! {
                    <Choice
                        options=pass_opts()
                        value=Signal::derive(move || ui.with(|u| u.pass.id().to_owned()))
                        on_change=Callback::new(move |v: String| ui.update(|u| u.pass = Pass::ALL.into_iter().find(|p| p.id() == v).unwrap_or(Pass::Beauty)))
                    />
                }}
                {move || (frames().len() > 1).then(|| view! {
                    <Choice
                        options=frames()
                        value=Signal::derive(move || ui.with(|u| u.frame_index.to_string()))
                        on_change=Callback::new(move |v: String| ui.update(|u| u.frame_index = v.parse().unwrap_or(0)))
                    />
                })}
                <Choice
                    options=vec![("0".into(), "Fit".into()), ("0.5".into(), "50 %".into()), ("1".into(), "100 %".into()), ("2".into(), "200 %".into())]
                    value=Signal::derive(move || { let z = ui.with(|u| u.zoom); if z == 0.0 { "0".to_owned() } else { format!("{z}") } })
                    on_change=Callback::new(move |v: String| ui.update(|u| u.zoom = v.parse().unwrap_or(0.0)))
                />
                {move || ui.with(|u| u.compare.is_some()).then(|| view! {
                    <input
                        type="range"
                        min="0"
                        max="100"
                        title="A/B split"
                        prop:value=move || ui.with(|u| (u.split * 100.0).round().to_string())
                        on:input=move |ev| {
                            let v: f32 = event_target_value(&ev).parse().unwrap_or(50.0);
                            ui.update(|u| u.split = (v / 100.0).clamp(0.0, 1.0));
                        }
                    />
                })}
                <span class="hint pv-info">{info}</span>
                {move || busy().then(|| view! { <button class="btn" on:click=move |_| stop(app)>"Stop"</button> })}
                <button class="btn" title="Save the shown picture as PNG" on:click=save>"Save..."</button>
                <button class="btn" title="Forget every picture" on:click=move |_| ui.update(|u| { u.shots.retain(|s| !s.done); u.compare = None; })>"Clear"</button>
            </div>
            <div class="pv-body">
                <div class="pv-view">{image}</div>
                <div class="pv-list">{history}</div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Render Queue
// ---------------------------------------------------------------------------

#[component]
pub fn RenderQueue() -> impl IntoView {
    let app = app();
    let ui = app.render;
    let rows = move || {
        ui.with(|u| u.queue.clone())
            .into_iter()
            .map(|q| {
                let id = q.id;
                let status = match &q.status {
                    QueueStatus::Waiting => "Waiting".to_owned(),
                    QueueStatus::Rendering(p) => format!("Rendering {:.0} %", p * 100.0),
                    QueueStatus::Done => "Done".to_owned(),
                    QueueStatus::Stopped => "Stopped".to_owned(),
                    QueueStatus::Failed(e) => format!("Failed: {e}"),
                };
                let s = &q.request.settings;
                let detail = format!(
                    "{}×{} · {} frame(s) · {}{}",
                    s.width,
                    s.height,
                    q.request.frames.len(),
                    s.engine.label(),
                    if s.save { "" } else { " · not saved (Save is off)" }
                );
                view! {
                    <div class="rq-row">
                        <Toggle on=Signal::derive(move || q.on) on_change=Callback::new(move |v: bool| ui.update(|u| if let Some(x) = u.queue.iter_mut().find(|x| x.id == id) { x.on = v }))/>
                        <div class="rq-meta">
                            <span>{q.name.clone()}</span>
                            <span class="hint">{detail}</span>
                        </div>
                        <span class="rq-status">{status}</span>
                        <button class="tb small" title="Render again" on:click=move |_| set_queue_status(app, id, QueueStatus::Waiting)>{icon_s("undo")}</button>
                        <button class="tb small" title="Remove" on:click=move |_| ui.update(|u| u.queue.retain(|x| x.id != id || matches!(x.status, QueueStatus::Rendering(_))))>{icon_s("trash")}</button>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <div class="rq">
            <div class="bar">
                <button class="btn" on:click=move |_| add_to_queue(app)>"Add Current Project"</button>
                {move || if ui.with(|u| u.queue_running) {
                    view! { <button class="btn" on:click=move |_| stop(app)>"Stop Queue"</button> }.into_any()
                } else {
                    view! {
                        <button class="btn primary" on:click=move |_| {
                            ui.update(|u| u.queue_running = true);
                            stop_quiet(app);
                            next_in_queue(app);
                        }>"Start Queue"</button>
                    }
                    .into_any()
                }}
                <button class="btn" on:click=move |_| ui.update(|u| u.queue.retain(|q| matches!(q.status, QueueStatus::Waiting | QueueStatus::Rendering(_))))>"Remove Finished"</button>
            </div>
            {move || if ui.with(|u| u.queue.is_empty()) {
                view! { <p class="hint">"Add projects with Render > Add to Render Queue; each keeps the settings it had when added. Start renders them one after another."</p> }.into_any()
            } else {
                rows().into_any()
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Light Lister
// ---------------------------------------------------------------------------

#[component]
pub fn LightLister() -> impl IntoView {
    let app = app();
    let rows = move || {
        let lights: Vec<(u64, String, ObjectKind, bool, bool)> = app.read(|e| {
            let mut v = Vec::new();
            e.scene.walk(|o, _| {
                if matches!(o.kind, ObjectKind::Light(_) | ObjectKind::Sky(_)) {
                    v.push((o.id, o.name.clone(), o.kind.clone(), e.scene.render_visible(o.id), e.selection.contains(&o.id)));
                }
            });
            v
        });
        if lights.is_empty() {
            return view! { <p class="hint">"No lights yet: add them from the Light menu."</p> }.into_any();
        }
        let set = move |id: u64, f: Box<dyn Fn(&mut ObjectKind)>| {
            app.act(|e| {
                if let Some(o) = e.scene.get_mut(id) {
                    f(&mut o.kind);
                }
                e.label("Change Light");
            })
        };
        let body = lights
            .into_iter()
            .map(|(id, name, kind, on, sel)| {
                let (kind_name, color, intensity, shadows) = match &kind {
                    ObjectKind::Light(l) => (l.kind.name().to_owned(), Some(l.color), l.intensity, Some(l.shadows)),
                    ObjectKind::Sky(s) => (s.kind.name().to_owned(), (s.kind == forma3d_core::sky::SkyKind::Gradient).then_some(s.zenith_color), s.intensity, None),
                    _ => (String::new(), None, 0.0, None),
                };
                view! {
                    <tr class:sel=sel>
                        <td>
                            <Toggle
                                on=Signal::derive(move || on)
                                on_change=Callback::new(move |v: bool| app.act(|e| {
                                    if let Some(o) = e.scene.get_mut(id) {
                                        o.vis_render = if v { forma3d_core::scene::Visibility::Default } else { forma3d_core::scene::Visibility::Off };
                                        o.vis_editor = o.vis_render;
                                    }
                                    e.label("Switch Light");
                                }))
                            />
                        </td>
                        <td class="ll-name" on:click=move |_| app.act(|e| e.select_only(id))>{name}</td>
                        <td class="hint">{kind_name}</td>
                        <td>
                            {color.map(|c| view! {
                                <input
                                    type="color"
                                    class="color"
                                    value=rgb_hex(c)
                                    on:change=move |ev| {
                                        if let Some(c) = parse_hex(&event_target_value(&ev)) {
                                            set(id, Box::new(move |k| match k {
                                                ObjectKind::Light(l) => l.color = c,
                                                ObjectKind::Sky(s) => s.zenith_color = c,
                                                _ => {}
                                            }));
                                        }
                                    }
                                />
                            })}
                        </td>
                        <td>
                            <NumField
                                value=Signal::derive(move || intensity)
                                unit="%"
                                decimals=0
                                on_change=Callback::new(move |v: f32| set(id, Box::new(move |k| match k {
                                    ObjectKind::Light(l) => l.intensity = v.clamp(0.0, 10_000.0),
                                    ObjectKind::Sky(s) => s.intensity = v.clamp(0.0, 10_000.0),
                                    _ => {}
                                })))
                            />
                        </td>
                        <td>
                            {shadows.map(|sh| view! {
                                <Toggle
                                    on=Signal::derive(move || sh)
                                    on_change=Callback::new(move |v: bool| set(id, Box::new(move |k| if let ObjectKind::Light(l) = k { l.shadows = v })))
                                />
                            })}
                        </td>
                    </tr>
                }
            })
            .collect_view();
        view! {
            <table class="ll">
                <thead>
                    <tr><th>"On"</th><th>"Name"</th><th>"Type"</th><th>"Colour"</th><th>"Intensity"</th><th>"Shadows"</th></tr>
                </thead>
                <tbody>{body}</tbody>
            </table>
        }
        .into_any()
    };
    view! { <div class="lister">{rows}</div> }
}

// ---------------------------------------------------------------------------
// Render Settings
// ---------------------------------------------------------------------------

const SECTIONS: [(&str, &[&str]); 11] = [
    ("Renderer", &["engine"]),
    ("Output", &["width", "height", "frame_range", "frame_from", "frame_to", "frame_step"]),
    ("Save", &["save", "path", "format", "jpeg_quality", "alpha"]),
    ("Multi-Pass", &["passes", "save_passes"]),
    (
        "Physical",
        &[
            "depth_of_field",
            "sampling_quality",
            "sampler",
            "sampling_subdivisions",
            "shading_subdivisions_min",
            "shading_subdivisions_max",
            "shading_error_threshold",
            "blurriness_subdivisions",
            "progressive_mode",
            "progressive_passes",
            "time_limit",
        ],
    ),
    (
        "Anti-Aliasing",
        &["antialiasing", "samples", "aa_min_level", "aa_max_level", "aa_threshold", "filter", "custom_filter_size", "filter_width", "filter_height", "clip_colors"],
    ),
    (
        "Options",
        &[
            "transparency",
            "refraction",
            "reflections",
            "shadows",
            "shadow_subdivisions",
            "blurriness",
            "reflections_sky_only",
            "max_depth",
            "reflection_depth",
            "shadow_depth",
            "ray_threshold",
            "global_brightness",
            "default_light",
            "textures",
            "post_effects",
            "background",
        ],
    ),
    ("Global Illumination", &["global_illumination", "gi_depth"]),
    (
        "Effects",
        &[
            "ambient_occlusion",
            "ao_distance",
            "ao_strength",
            "ao_subdivisions",
            "glow",
            "glow_threshold",
            "glow_radius",
            "glow_strength",
            "vignette",
            "sketch",
            "line_width",
            "line_color",
            "toon_steps",
        ],
    ),
    ("Material Override", &["material_override", "override_color"]),
    ("Colour Management", &["view_transform", "working_space", "exposure"]),
];

fn variants<T: serde::Serialize>(v: &[T]) -> Vec<String> {
    v.iter().filter_map(|x| serde_json::to_value(x).ok()?.as_str().map(str::to_owned)).collect()
}

fn choices(key: &str) -> Option<Vec<String>> {
    use forma3d_core::render::{AaLevel, AntiAliasing, Filter, ImageFormat, ProgressiveMode, Sampler, SamplingQuality, SketchMode, ViewTransform, WorkingSpace};
    Some(match key {
        "engine" => variants(&Engine::ALL),
        "frame_range" => variants(&[FrameRange::CurrentFrame, FrameRange::AllFrames, FrameRange::Manual]),
        "format" => variants(&[ImageFormat::Png, ImageFormat::Png16, ImageFormat::Jpeg]),
        "filter" => variants(&Filter::ALL),
        "antialiasing" => variants(&[AntiAliasing::None, AntiAliasing::Geometry, AntiAliasing::Best]),
        "aa_min_level" | "aa_max_level" => variants(&AaLevel::ALL),
        "sampling_quality" => variants(&[SamplingQuality::Low, SamplingQuality::Medium, SamplingQuality::High, SamplingQuality::Custom]),
        "sampler" => variants(&[Sampler::Fixed, Sampler::Adaptive, Sampler::Progressive]),
        "progressive_mode" => variants(&[ProgressiveMode::PassCount, ProgressiveMode::TimeLimit, ProgressiveMode::Infinite]),
        "sketch" => variants(&[SketchMode::Off, SketchMode::SketchAndToon, SketchMode::Contour]),
        "view_transform" => variants(&ViewTransform::ALL),
        "working_space" => variants(&[WorkingSpace::LinearSrgb, WorkingSpace::AcesCg]),
        _ => return None,
    })
}

fn choice_label(v: &str) -> String {
    match v {
        "Standard" => "Standard".into(),
        "Physical" => "Physical".into(),
        "Viewport" => "Viewport (Hardware)".into(),
        "CurrentFrame" => "Current Frame".into(),
        "AllFrames" => "All Frames".into(),
        "Manual" => "Manual".into(),
        "Png" => "PNG".into(),
        "Png16" => "PNG (16 bit)".into(),
        "Jpeg" => "JPEG".into(),
        "SketchAndToon" => "Sketch and Toon".into(),
        "Contour" => "Contour Lines".into(),
        "AcesSdr" => "ACES (SDR)".into(),
        "Filmic" => "Filmic".into(),
        "Raw" => "Raw (linear)".into(),
        "LinearSrgb" => "Linear sRGB".into(),
        "AcesCg" => "ACEScg".into(),
        "Box" => "Cube (Still Image)".into(),
        "Gaussian" => "Gauss (Animation)".into(),
        "CatmullRom" => "Catmull".into(),
        "PalNtsc" => "PAL/NTSC".into(),
        "L1x1" => "1x1".into(),
        "L2x2" => "2x2".into(),
        "L4x4" => "4x4".into(),
        "L8x8" => "8x8".into(),
        "L16x16" => "16x16".into(),
        "PassCount" => "Pass Count".into(),
        "TimeLimit" => "Time Limit".into(),
        "Infinite" => "Until Stopped".into(),
        other => other.into(),
    }
}

fn field_label(key: &str) -> &'static str {
    match key {
        "engine" => "Renderer",
        "width" => "Width",
        "height" => "Height",
        "frame_range" => "Frame Range",
        "frame_from" => "From",
        "frame_to" => "To",
        "frame_step" => "Frame Step",
        "save" => "Save Files",
        "path" => "File",
        "format" => "Format",
        "jpeg_quality" => "Quality",
        "alpha" => "Alpha Channel",
        "passes" => "Passes",
        "save_passes" => "Save Passes",
        "samples" => "Samples",
        "antialiasing" => "Anti-Aliasing",
        "aa_min_level" => "Min Level",
        "aa_max_level" => "Max Level",
        "aa_threshold" => "Threshold",
        "filter" => "Filter",
        "custom_filter_size" => "Custom Size",
        "filter_width" => "Filter Width",
        "filter_height" => "Filter Height",
        "clip_colors" => "Clip Colour Values",
        "transparency" => "Transparency",
        "refraction" => "Refraction",
        "shadows" => "Shadows",
        "reflections" => "Reflections",
        "blurriness" => "Blurriness",
        "reflections_sky_only" => "Limit Reflections to Sky",
        "max_depth" => "Ray Depth",
        "reflection_depth" => "Reflection Depth",
        "shadow_depth" => "Shadow Depth",
        "ray_threshold" => "Ray Threshold",
        "global_brightness" => "Global Brightness",
        "default_light" => "Default Light",
        "textures" => "Textures",
        "post_effects" => "Post Effects",
        "global_illumination" => "Global Illumination",
        "gi_depth" => "Diffuse Depth",
        "depth_of_field" => "Depth of Field",
        "sampling_quality" => "Sampling Quality",
        "sampler" => "Sampler",
        "sampling_subdivisions" => "Sampling Subdivisions",
        "shading_subdivisions_min" => "Shading Subdivisions (Min)",
        "shading_subdivisions_max" => "Shading Subdivisions (Max)",
        "shading_error_threshold" => "Shading Error Threshold",
        "blurriness_subdivisions" => "Blurriness Subdivisions",
        "shadow_subdivisions" => "Area Shadow Samples",
        "ao_subdivisions" => "AO Samples",
        "progressive_mode" => "Progressive Mode",
        "progressive_passes" => "Pass Count",
        "time_limit" => "Time Limit",
        "glow" => "Glow",
        "glow_threshold" => "Glow Threshold",
        "glow_radius" => "Glow Radius",
        "glow_strength" => "Glow Strength",
        "vignette" => "Vignette",
        "background" => "Background",
        "ambient_occlusion" => "Ambient Occlusion",
        "ao_distance" => "AO Distance",
        "ao_strength" => "AO Strength",
        "sketch" => "Sketch",
        "line_width" => "Line Width",
        "line_color" => "Line Colour",
        "toon_steps" => "Toon Steps",
        "material_override" => "Override Materials",
        "override_color" => "Colour",
        "view_transform" => "View Transform",
        "working_space" => "Working Space",
        "exposure" => "Exposure",
        _ => "",
    }
}

fn field_unit(key: &str) -> &'static str {
    match key {
        "width" | "height" | "line_width" | "filter_width" | "filter_height" | "glow_radius" => "px",
        "ao_distance" => "cm",
        "ao_strength" | "jpeg_quality" | "aa_threshold" | "shading_error_threshold" | "ray_threshold" | "global_brightness" | "glow_strength" | "vignette" => "%",
        "time_limit" => "s",
        "exposure" => "EV",
        _ => "",
    }
}

/// Settings that mean nothing with the others as they are.
fn hidden(s: &RenderSettings) -> Vec<&'static str> {
    use forma3d_core::render::{ImageFormat, SketchMode};
    let mut h = Vec::new();
    if s.frame_range != FrameRange::Manual {
        h.extend(["frame_from", "frame_to", "frame_step"]);
    }
    if !s.save {
        h.extend(["path", "format", "jpeg_quality", "alpha", "save_passes"]);
    }
    if s.format != ImageFormat::Jpeg {
        h.push("jpeg_quality");
    }
    use forma3d_core::render::{AntiAliasing, ProgressiveMode, Sampler, SamplingQuality};
    let physical_only = [
        "depth_of_field",
        "sampling_quality",
        "sampler",
        "sampling_subdivisions",
        "shading_subdivisions_min",
        "shading_subdivisions_max",
        "shading_error_threshold",
        "blurriness_subdivisions",
        "progressive_mode",
        "progressive_passes",
        "time_limit",
    ];
    if s.engine == Engine::Physical {
        // Physical decides its samples itself (Sampler); it keeps the filter.
        h.extend(["antialiasing", "samples", "aa_min_level", "aa_max_level", "aa_threshold"]);
        let custom = s.sampling_quality == SamplingQuality::Custom;
        if !custom {
            h.extend(["sampling_subdivisions", "shading_subdivisions_min", "shading_subdivisions_max", "shading_error_threshold", "blurriness_subdivisions", "shadow_subdivisions"]);
        }
        if s.sampler != Sampler::Adaptive {
            h.extend(["shading_subdivisions_max", "shading_error_threshold"]);
        }
        if s.sampler == Sampler::Progressive {
            h.push("sampling_subdivisions");
        } else {
            h.extend(["progressive_mode", "progressive_passes", "time_limit"]);
        }
        if s.progressive_mode != ProgressiveMode::PassCount {
            h.push("progressive_passes");
        }
        if s.progressive_mode != ProgressiveMode::TimeLimit {
            h.push("time_limit");
        }
    } else {
        h.extend(physical_only);
        if s.antialiasing != AntiAliasing::Geometry {
            h.push("samples");
        }
        if s.antialiasing != AntiAliasing::Best {
            h.extend(["aa_min_level", "aa_max_level", "aa_threshold"]);
        }
        if s.antialiasing == AntiAliasing::None {
            h.extend(["filter", "custom_filter_size", "filter_width", "filter_height", "clip_colors"]);
        }
    }
    if !s.custom_filter_size {
        h.extend(["filter_width", "filter_height"]);
    }
    if !s.reflections {
        h.extend(["reflection_depth", "reflections_sky_only"]);
    }
    if !s.shadows {
        h.extend(["shadow_depth", "shadow_subdivisions"]);
    }
    if !s.transparency {
        h.push("refraction");
    }
    if !s.global_illumination {
        h.push("gi_depth");
    }
    if !s.glow {
        h.extend(["glow_threshold", "glow_radius", "glow_strength"]);
    }
    if !s.ambient_occlusion {
        h.extend(["ao_distance", "ao_strength", "ao_subdivisions"]);
    }
    if s.sketch == SketchMode::Off {
        h.extend(["line_width", "line_color", "toon_steps"]);
    }
    if s.sketch == SketchMode::Contour {
        h.push("toon_steps");
    }
    if !s.material_override {
        h.push("override_color");
    }
    if s.engine == Engine::Viewport {
        h.extend(physical_only);
        h.extend([
            "shadow_subdivisions",
            "antialiasing",
            "aa_min_level",
            "aa_max_level",
            "aa_threshold",
            "custom_filter_size",
            "filter_width",
            "filter_height",
            "clip_colors",
            "transparency",
            "refraction",
            "blurriness",
            "reflections_sky_only",
            "reflection_depth",
            "shadow_depth",
            "ray_threshold",
            "global_brightness",
            "default_light",
            "textures",
            "post_effects",
            "global_illumination",
            "gi_depth",
            "ao_subdivisions",
            "glow",
            "glow_threshold",
            "glow_radius",
            "glow_strength",
            "vignette",
            "passes",
            "save_passes",
            "samples",
            "filter",
            "shadows",
            "reflections",
            "max_depth",
            "background",
            "ambient_occlusion",
            "ao_distance",
            "ao_strength",
            "sketch",
            "line_width",
            "line_color",
            "toon_steps",
            "material_override",
            "override_color",
            "view_transform",
            "working_space",
            "exposure",
        ]);
    }
    h
}

fn section_id(title: &str) -> String {
    format!("rs-{}", title.to_lowercase().replace(' ', "-"))
}

fn set_setting(app: App, key: &'static str, value: Value) {
    app.act(|e| {
        let Ok(mut v) = serde_json::to_value(&e.scene.render) else { return };
        if let Some(m) = v.as_object_mut() {
            m.insert(key.to_owned(), value);
        }
        if let Ok(s) = serde_json::from_value::<RenderSettings>(v) {
            e.scene.render = s.sanitized();
            e.label("Change Render Settings");
        }
    });
}

const PRESETS: [(u32, u32, &str); 8] = [
    (640, 480, "640 × 480"),
    (1280, 720, "HD 720"),
    (1920, 1080, "HD 1080"),
    (2560, 1440, "QHD 1440"),
    (3840, 2160, "4K UHD"),
    (1080, 1080, "Square 1080"),
    (1080, 1920, "Portrait 1080 × 1920"),
    (800, 600, "800 × 600"),
];

fn setting_row(app: App, key: &'static str, v: Value) -> AnyView {
    let label = field_label(key);
    let unit = field_unit(key);
    match v {
        Value::Bool(b) => view! {
            <div class="lbl">
                <span>{label}</span>
                <Toggle on=Signal::derive(move || b) on_change=Callback::new(move |nb: bool| set_setting(app, key, Value::Bool(nb)))/>
            </div>
        }
        .into_any(),
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(0.0) as f32;
            let int = !n.is_f64();
            view! {
                <div class="lbl">
                    <span>{label}</span>
                    <NumField
                        value=Signal::derive(move || x)
                        unit=unit
                        decimals=if int { 0 } else { 2 }
                        step=if int { 1.0 } else { 0.1 }
                        on_change=Callback::new(move |nv: f32| {
                            let val = if int { Value::from(nv.round().max(0.0) as u64) } else { serde_json::Number::from_f64(nv as f64).map(Value::Number).unwrap_or(Value::Null) };
                            set_setting(app, key, val)
                        })
                    />
                </div>
            }
            .into_any()
        }
        Value::String(s) => match choices(key) {
            Some(opts) => {
                let opts: Vec<(String, String)> = opts.into_iter().map(|o| (o.clone(), choice_label(&o))).collect();
                view! {
                    <div class="lbl">
                        <span>{label}</span>
                        <Choice options=opts value=Signal::derive(move || s.clone()) on_change=Callback::new(move |nv: String| set_setting(app, key, Value::String(nv)))/>
                    </div>
                }
                .into_any()
            }
            None => view! {
                <div class="lbl">
                    <span>{label}</span>
                    <input
                        class="text-in"
                        prop:value=s
                        title="Tokens: $prj $camera $take $frame $pass $res $width $height $renderer $date $time"
                        on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation()
                        on:change=move |ev| set_setting(app, key, Value::String(event_target_value(&ev)))
                    />
                </div>
            }
            .into_any(),
        },
        Value::Array(a) if key == "passes" => {
            let on: Vec<String> = a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect();
            let boxes = Pass::ALL[1..]
                .iter()
                .map(|p| {
                    let name = serde_json::to_value(p).ok().and_then(|x| x.as_str().map(str::to_owned)).unwrap_or_default();
                    let checked = on.contains(&name);
                    let all = on.clone();
                    view! {
                        <label class="check">
                            <input
                                type="checkbox"
                                prop:checked=checked
                                on:change=move |ev| {
                                    let mut next = all.clone();
                                    if event_target_checked(&ev) {
                                        next.push(name.clone());
                                    } else {
                                        next.retain(|x| *x != name);
                                    }
                                    set_setting(app, "passes", serde_json::json!(next));
                                }
                            />
                            {p.label()}
                        </label>
                    }
                })
                .collect_view();
            view! { <div class="passes">{boxes}</div> }.into_any()
        }
        Value::Array(a) if a.len() == 3 => {
            let c = [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32);
            view! {
                <div class="lbl">
                    <span>{label}</span>
                    <input
                        type="color"
                        class="color"
                        value=rgb_hex(c)
                        on:change=move |ev| {
                            if let Some(c) = parse_hex(&event_target_value(&ev)) {
                                set_setting(app, key, serde_json::json!(c));
                            }
                        }
                    />
                </div>
            }
            .into_any()
        }
        _ => ().into_any(),
    }
}

#[component]
pub fn RenderSettingsDialog() -> impl IntoView {
    let app = app();
    let close = move || app.render.update(|u| u.settings_open = false);
    let body = move || {
        let s = app.read(|e| e.scene.render.clone());
        let hide = hidden(&s);
        let Ok(Value::Object(map)) = serde_json::to_value(&s) else { return ().into_any() };
        SECTIONS
            .iter()
            .filter_map(|(title, keys)| {
                let rows: Vec<AnyView> = keys.iter().filter(|k| !hide.contains(k)).filter_map(|k| map.get(*k).map(|v| setting_row(app, k, v.clone()))).collect();
                if rows.is_empty() {
                    return None;
                }
                let preset = (*title == "Output").then(|| {
                    let opts: Vec<(String, String)> = std::iter::once(("".to_owned(), "Custom".to_owned()))
                        .chain(PRESETS.iter().map(|(w, h, n)| (format!("{w}x{h}"), (*n).to_owned())))
                        .collect();
                    let cur = format!("{}x{}", s.width, s.height);
                    let cur = if PRESETS.iter().any(|(w, h, _)| format!("{w}x{h}") == cur) { cur } else { String::new() };
                    view! {
                        <div class="lbl">
                            <span>"Preset"</span>
                            <Choice
                                options=opts
                                value=Signal::derive(move || cur.clone())
                                on_change=Callback::new(move |v: String| {
                                    if let Some((w, h)) = v.split_once('x') {
                                        let (w, h) = (w.parse::<u32>().unwrap_or(1280), h.parse::<u32>().unwrap_or(720));
                                        app.act(|e| {
                                            e.scene.render.width = w;
                                            e.scene.render.height = h;
                                            e.label("Change Render Settings");
                                        });
                                    }
                                })
                            />
                        </div>
                    }
                });
                let note = match *title {
                    "Renderer" => Some(match s.engine {
                        Engine::Standard => "Ray tracing with direct light, shadows, reflections and refraction; anti-aliasing decides the samples. Fast and clean.",
                        Engine::Physical => "Path tracing: light bounces between surfaces, soft area shadows, blurry reflections and depth of field. Sampling Quality and the Sampler decide how long it works on noise.",
                        Engine::Viewport => "The viewport's own graphics renderer, as the Perspective view shades it: instant, no anti-aliasing passes.",
                    }),
                    "Save" if s.save => Some(if host::in_tauri() { "Relative names go next to the project (or into Pictures/Forma3D)." } else { "In a browser the files are downloaded." }),
                    "Anti-Aliasing" if s.engine == Engine::Standard && s.antialiasing == forma3d_core::render::AntiAliasing::Best => {
                        Some("Every pixel gets Min Level samples; noisy pixels and edges get more, up to Max Level.")
                    }
                    "Physical" if s.sampler == forma3d_core::render::Sampler::Progressive && s.progressive_mode == forma3d_core::render::ProgressiveMode::Infinite => {
                        Some("Keeps refining until you press Stop; the picture so far is kept.")
                    }
                    "Physical" if s.sampling_quality != forma3d_core::render::SamplingQuality::Custom => {
                        Some("Low, Medium and High set the subdivisions; choose Custom to set them yourself.")
                    }
                    _ => None,
                };
                Some(view! {
                    <h3 id=section_id(title)>{*title}</h3>
                    {preset}
                    {rows}
                    {note.map(|n| view! { <p class="hint">{n}</p> })}
                })
            })
            .collect_view()
            .into_any()
    };
    // The sections this renderer uses, to jump to.
    let nav = move || {
        let s = app.read(|e| e.scene.render.clone());
        let hide = hidden(&s);
        SECTIONS
            .iter()
            .filter(|(_, keys)| keys.iter().any(|k| !hide.contains(k)))
            .map(|(title, _)| {
                let id = section_id(title);
                view! {
                    <button
                        class="rs-tab"
                        on:click=move |_| {
                            if let Some(el) = document().get_element_by_id(&id) {
                                el.scroll_into_view();
                            }
                        }
                    >
                        {*title}
                    </button>
                }
            })
            .collect_view()
    };
    view! {
        <Show when=move || app.render.with(|u| u.settings_open)>
            <div class="modal-bg" on:mousedown=move |_| close()>
                <div class="dialog prefs rsettings" on:mousedown=|ev| ev.stop_propagation()>
                    <h2>"Render Settings"</h2>
                    <div class="rs-wrap">
                        <nav class="rs-nav">{nav}</nav>
                        <div class="rs-body">{body}</div>
                    </div>
                    <div class="btns">
                        <button class="btn" on:click=move |_| app.act(|e| { e.scene.render = RenderSettings::default(); e.label("Reset Render Settings"); })>"Reset to Defaults"</button>
                        <button class="btn" on:click=move |_| { close(); start(app, Target::Viewer); }>"Render"</button>
                        <button class="btn primary" on:click=move |_| close()>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
