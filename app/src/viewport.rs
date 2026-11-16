//! The 3D view: the engine's wgpu renderer drawing into a canvas (WebGPU
//! where the window has it, WebGL 2 otherwise), camera navigation, picking,
//! and the transform gizmo drawn as an SVG overlay.

use crate::chrome::MenuButton;
use crate::commands::Cmd;
use crate::icons::icon;
use crate::state::app;
use forma3d_core::camera::{Camera, ViewKind};
use forma3d_core::editor::{Editor, NavMode};
use forma3d_core::gizmo::{self, Axis, CoordSystem, Drag, DragView, GizmoFrame, Handle, Tool};
use forma3d_core::picking::{self, PickItem};
use forma3d_core::renderer::{self, CameraUniform, DrawItem, FrameScene, GpuResources, PostUniform, ViewDraw};
use glam::{Vec2, Vec3};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    res: GpuResources,
    srgb: bool,
}

/// What the last frame showed, for picking and the overlay.
#[derive(Default)]
struct Shown {
    pick: Vec<PickItem>,
    size: Vec2,
    triangles: usize,
    objects: usize,
}

/// A pointer gesture in progress.
#[derive(Clone, Copy, Debug)]
enum Gesture {
    /// Button down, not moved far yet: a click or the start of a drag.
    Pending { start: Vec2, button: i16, shift: bool, ctrl: bool },
    Nav(NavMode),
    Drag,
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("forma3d_depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

// Natively (only for `cargo check`) the canvas surface can't exist.
#[cfg_attr(not(target_arch = "wasm32"), allow(unreachable_code, unused_variables))]
async fn init_gpu(canvas: web_sys::HtmlCanvasElement) -> Result<(Gpu, String), String> {
    // WebGL 2 by default: it is the path that has been checked end to end.
    // Add `?webgpu` to the page address to try WebGPU instead.
    let want_webgpu = web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|q| q.contains("webgpu"));
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = if want_webgpu { wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL } else { wgpu::Backends::GL };
    let instance = if want_webgpu { wgpu::util::new_instance_with_webgpu_detection(desc).await } else { wgpu::Instance::new(desc) };
    #[cfg(target_arch = "wasm32")]
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|e| e.to_string())?;
    #[cfg(not(target_arch = "wasm32"))]
    let surface: wgpu::Surface<'static> = {
        let _ = &canvas;
        return Err("the viewport only runs in the app's window".to_owned());
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|e| format!("no graphics adapter: {e}"))?;
    let info = adapter.get_info();
    let webgl = info.backend == wgpu::Backend::Gl;
    let limits = if webgl { wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()) } else { adapter.limits() };
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { label: Some("forma3d"), required_limits: limits, ..Default::default() })
        .await
        .map_err(|e| format!("could not open the graphics device: {e}"))?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied()).ok_or("the canvas has no formats")?;
    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto)
    };
    let (w, h) = (canvas.width().max(1), canvas.height().max(1));
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: w,
        height: h,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
        color_space: Default::default(),
    };
    surface.configure(&device, &config);
    let res = GpuResources::new(&device, format);
    let srgb = res.target_is_srgb;
    let depth = make_depth(&device, w, h);
    let short: String = info.name.split(" (").next().unwrap_or(&info.name).chars().take(48).collect();
    let name = format!("{} · {short}", if webgl { "WebGL 2" } else { "WebGPU" });
    Ok((Gpu { device, queue, surface, config, depth, res, srgb }, name))
}

fn axis_color(i: usize) -> &'static str {
    ["#e5534b", "#57ab5a", "#539bf5"][i % 3]
}

const HOT: &str = "#ffd34d";

/// Builds this frame's drawing from the editor.
fn build_frame(ed: &mut Editor, size: Vec2, srgb: bool, shown: &mut Shown) -> ViewDraw {
    let mut objects = ed.render_objects();
    objects.retain(|o| ed.passes_filter(o.id));
    let fx = ed.view_opts.fx;
    let items: Vec<DrawItem> = objects
        .iter()
        .map(|o| DrawItem { mesh: o.mesh.clone(), model: o.world, color: o.color, selection: o.selection, kind: o.kind })
        .collect();
    shown.triangles = objects.iter().filter(|o| o.kind == renderer::ItemKind::Surface).map(|o| o.mesh.indices.len() / 3).sum();
    shown.objects = objects.len();
    shown.pick = objects.iter().filter(|o| !ed.scene.is_locked(o.id)).map(|o| o.pick_item()).collect();
    shown.size = size;
    let mut frame = FrameScene::new(items, ed.display, fx);
    frame.lights = ed.scene_lights();
    let frame = Arc::new(frame);

    let kind = ed.single_view;
    let cam = ed.cameras[kind.index()];
    let aspect = (size.x / size.y.max(1.0)).max(0.01);
    let wp = ed.scene.workplane;
    let on_workplane = kind == ViewKind::Perspective && !wp.is_default();
    let mut grid = if on_workplane { renderer::build_grid_on(&cam, wp.matrix()) } else { renderer::build_grid(&cam) };
    if !ed.view_opts.grid {
        grid.vertices.clear();
    }
    let mut uniform = CameraUniform::new(&cam, aspect, srgb, grid.center, grid.radius);
    let floor = ed.view_opts.grid && !on_workplane && matches!(kind, ViewKind::Perspective | ViewKind::Top);
    uniform.set_lights(&frame.lights, frame.bounds);
    uniform.set_effects(fx, frame.bounds, floor);
    uniform.set_shading(frame.mode);
    let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0) as f32;
    let size_px = [(size.x * dpr).round().max(1.0) as u32, (size.y * dpr).round().max(1.0) as u32];
    ViewDraw {
        slot: 0,
        camera: uniform,
        post: PostUniform::new(&cam, aspect, size_px, fx),
        size_px,
        grid: grid.vertices,
        scene: frame,
    }
}

fn to_screen(cam: &Camera, size: Vec2, p: Vec3) -> Option<Vec2> {
    let ndc = cam.project(p, size.x / size.y.max(1.0))?;
    Some(Vec2::new((ndc.x * 0.5 + 0.5) * size.x, (1.0 - (ndc.y * 0.5 + 0.5)) * size.y))
}

fn ray_at(cam: &Camera, size: Vec2, m: Vec2) -> (Vec3, Vec3) {
    let ndc = Vec2::new(m.x / size.x * 2.0 - 1.0, 1.0 - m.y / size.y * 2.0);
    cam.ray(ndc, size.x / size.y.max(1.0))
}

fn gizmo_frame(ed: &Editor, size: Vec2) -> Option<GizmoFrame> {
    if ed.tool == Tool::Select || ed.mode.is_component() {
        return None;
    }
    let id = ed.active().filter(|id| !ed.scene.is_locked(*id))?;
    let cam = ed.cameras[ed.single_view.index()];
    let origin = ed.scene.world_position(id);
    Some(GizmoFrame::new(&ed.scene, id, ed.coord == CoordSystem::Object, cam.world_per_pixel(origin, size.y)))
}

/// The gizmo and the corner compass as SVG.
fn overlay(ed: &Editor, size: Vec2, hot: Option<Handle>) -> String {
    let cam = ed.cameras[ed.single_view.index()];
    let project = |p: Vec3| to_screen(&cam, size, p);
    let mut s = String::new();
    if let Some(gf) = gizmo_frame(ed, size)
        && let Some(o) = project(gf.origin)
    {
        let hot = ed.drag.as_ref().map(|d| d.handle).or(hot);
        for a in Axis::ALL {
            if !ed.axes.get(a) {
                continue;
            }
            let is_hot = hot == Some(Handle::Axis(a));
            let col = if is_hot { HOT } else { axis_color(a.index()) };
            let w = if is_hot { 3.0 } else { 2.0 };
            match ed.tool {
                Tool::Move | Tool::Scale => {
                    let Some(tip) = project(gf.origin + gf.axis(a) * gf.axis_len()) else { continue };
                    s += &format!(r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{col}" stroke-width="{w}" stroke-linecap="round"/>"#, o.x, o.y, tip.x, tip.y);
                    let dir = (tip - o).normalize_or_zero();
                    if ed.tool == Tool::Move {
                        let n = Vec2::new(-dir.y, dir.x);
                        let base = tip - dir * 14.0;
                        let (a1, b1, c1) = (tip + dir * 4.0, base + n * 5.5, base - n * 5.5);
                        s += &format!(r#"<path d="M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z" fill="{col}"/>"#, a1.x, a1.y, b1.x, b1.y, c1.x, c1.y);
                    } else {
                        s += &format!(r#"<rect x="{:.1}" y="{:.1}" width="9" height="9" rx="1.5" fill="{col}"/>"#, tip.x - 4.5, tip.y - 4.5);
                    }
                }
                Tool::Rotate => {
                    let pts: Vec<Option<Vec2>> = gf.ring_points(a).into_iter().map(project).collect();
                    let mut d = String::new();
                    let mut pen_down = false;
                    for p in pts {
                        match p {
                            Some(p) => {
                                d += &format!("{}{:.1} {:.1}", if pen_down { "L" } else { "M" }, p.x, p.y);
                                pen_down = true;
                            }
                            None => pen_down = false,
                        }
                    }
                    s += &format!(r#"<path d="{d}" fill="none" stroke="{col}" stroke-width="{w}"/>"#);
                }
                Tool::Select => {}
            }
        }
        s += &format!(r##"<circle cx="{:.1}" cy="{:.1}" r="3.5" fill="#e6e8ec"/>"##, o.x, o.y);
    }
    // The XYZ compass, bottom left.
    let c = Vec2::new(40.0, size.y - 40.0);
    let mut axes: Vec<(f32, usize, Vec2)> = (0..3)
        .map(|i| {
            let dir = [Vec3::X, Vec3::Y, Vec3::Z][i];
            (dir.dot(cam.forward()), i, Vec2::new(dir.dot(cam.right()), -dir.dot(cam.up())))
        })
        .collect();
    axes.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, i, d) in axes {
        let tip = c + d * 22.0;
        let lab = tip + d * 9.0;
        let col = axis_color(i);
        s += &format!(r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{col}" stroke-width="2"/><circle cx="{:.1}" cy="{:.1}" r="3" fill="{col}"/>"#, c.x, c.y, tip.x, tip.y, tip.x, tip.y);
        s += &format!(r##"<text x="{:.1}" y="{:.1}" fill="#d2d5db" font-size="10" text-anchor="middle" dominant-baseline="central">{}</text>"##, lab.x, lab.y, ["X", "Y", "Z"][i]);
    }
    s
}

#[component]
pub fn Viewport() -> impl IntoView {
    let app = app();
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let stage = NodeRef::<leptos::html::Div>::new();
    let gpu: StoredValue<Option<Gpu>, LocalStorage> = StoredValue::new_local(None);
    let shown: StoredValue<Shown, LocalStorage> = StoredValue::new_local(Shown::default());
    let gesture: StoredValue<Option<Gesture>, LocalStorage> = StoredValue::new_local(None);
    let last: StoredValue<Vec2, LocalStorage> = StoredValue::new_local(Vec2::ZERO);
    let queued: StoredValue<bool, LocalStorage> = StoredValue::new_local(false);
    let hot = RwSignal::new(None::<Handle>);
    let svg = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let stats = RwSignal::new(String::new());

    let size_of = move || -> Vec2 {
        stage.get_untracked().map(|s| Vec2::new(s.client_width().max(1) as f32, s.client_height().max(1) as f32)).unwrap_or(Vec2::ONE)
    };

    // Draws one frame (if the GPU is ready).
    let render = move || {
        queued.set_value(false);
        let Some(cv) = canvas.get_untracked() else { return };
        let size = size_of();
        let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0);
        let (pw, ph) = (((size.x as f64) * dpr).round().max(1.0) as u32, ((size.y as f64) * dpr).round().max(1.0) as u32);
        gpu.update_value(|g| {
            let Some(g) = g.as_mut() else { return };
            if g.config.width != pw || g.config.height != ph {
                cv.set_width(pw);
                cv.set_height(ph);
                g.config.width = pw;
                g.config.height = ph;
                g.surface.configure(&g.device, &g.config);
                g.depth = make_depth(&g.device, pw, ph);
            }
            let srgb = g.srgb;
            let draw = shown.try_update_value(|sh| app.ed.try_update_value(|ed| build_frame(ed, size, srgb, sh))).flatten();
            let Some(draw) = draw else { return };
            let frame = match g.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                _ => {
                    g.surface.configure(&g.device, &g.config);
                    return;
                }
            };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut enc = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("forma3d_frame") });
            draw.prepare(&mut g.res, &g.device, &g.queue, &mut enc);
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("forma3d_view"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &g.depth,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                draw.paint(&g.res, &mut pass);
            }
            g.queue.submit([enc.finish()]);
            g.queue.present(frame);
        });
        app.ed.update_value(|ed| ed.meshes.end_frame());
        let ov = app.ed.with_value(|ed| overlay(ed, size, hot.get_untracked()));
        svg.set(ov);
        let (tri, objs) = shown.with_value(|s| (s.triangles, s.objects));
        stats.set(format!("{objs} objects · {tri} triangles"));
    };

    let schedule = move || {
        if queued.get_value() {
            return;
        }
        queued.set_value(true);
        request_animation_frame(render);
    };

    // Redraw whenever the editor or the view changes.
    Effect::new(move |_| {
        app.rev.track();
        app.view_rev.track();
        hot.track();
        schedule();
    });

    // Start the GPU once the canvas exists, and redraw on resize.
    Effect::new(move |done: Option<bool>| {
        if done == Some(true) {
            return true;
        }
        let (Some(cv), Some(st)) = (canvas.get(), stage.get()) else { return false };
        let cv: web_sys::HtmlCanvasElement = cv.clone();
        let st: web_sys::HtmlElement = st.clone().into();
        let on_resize = Closure::<dyn FnMut()>::new(schedule);
        if let Ok(ro) = web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref()) {
            ro.observe(&st);
            std::mem::forget(ro);
        }
        on_resize.forget();
        cv.set_width((st.client_width().max(1) as f64 * web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0)) as u32);
        cv.set_height((st.client_height().max(1) as f64 * web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0)) as u32);
        leptos::task::spawn_local(async move {
            match init_gpu(cv).await {
                Ok((g, name)) => {
                    gpu.set_value(Some(g));
                    app.gpu_name.set(name.clone());
                    app.log_line(format!("Viewport: {name}"), false);
                    schedule();
                }
                Err(e) => {
                    app.log_line(format!("Viewport: {e}"), true);
                    error.set(Some(e));
                }
            }
        });
        true
    });

    // ---- pointer input ---------------------------------------------------
    let pos = move |ev: &web_sys::PointerEvent| Vec2::new(ev.offset_x() as f32, ev.offset_y() as f32);

    let begin_drag = move |start: Vec2, handle: Handle| -> bool {
        let size = size_of();
        app.ed
            .try_update_value(|ed| {
                let Some(gf) = gizmo_frame(ed, size) else { return false };
                let Some(id) = ed.active() else { return false };
                let cam = ed.cameras[ed.single_view.index()];
                let view = DragView { forward: cam.forward(), right: cam.right(), up: cam.up() };
                let ray = ray_at(&cam, size, start);
                let mut d = Drag::begin(&ed.scene, id, ed.tool, handle, gf, view, ed.axes, ray, start);
                if let Some(d) = d.as_mut() {
                    d.quantize = ed.quantize;
                }
                let ok = d.is_some();
                if ok {
                    ed.label(match ed.tool {
                        Tool::Move => "Move",
                        Tool::Rotate => "Rotate",
                        Tool::Scale => "Scale",
                        Tool::Select => "Edit",
                    });
                }
                ed.drag = d;
                ok
            })
            .unwrap_or(false)
    };

    let on_down = move |ev: web_sys::PointerEvent| {
        app.menu.set(None);
        let m = pos(&ev);
        last.set_value(m);
        if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let _ = t.set_pointer_capture(ev.pointer_id());
        }
        let button = ev.button();
        let alt = ev.alt_key();
        let ortho = app.peek(|e| e.single_view.is_ortho());
        let nav = match (button, alt) {
            (0, true) => Some(if ortho { NavMode::Pan } else { NavMode::Orbit }),
            (1, _) => Some(NavMode::Pan),
            (2, true) => Some(NavMode::Zoom),
            (2, false) => Some(if ortho { NavMode::Pan } else { NavMode::Orbit }),
            _ => None,
        };
        if let Some(n) = nav {
            gesture.set_value(Some(Gesture::Nav(n)));
            return;
        }
        if button != 0 {
            return;
        }
        // A gizmo handle starts its drag at once.
        let size = size_of();
        let handle = app.peek(|ed| {
            let cam = ed.cameras[ed.single_view.index()];
            gizmo_frame(ed, size).and_then(|gf| gizmo::hit_test(ed.tool, &gf, &|p| to_screen(&cam, size, p), m, ed.axes))
        });
        if let Some(h) = handle
            && begin_drag(m, h)
        {
            gesture.set_value(Some(Gesture::Drag));
            return;
        }
        gesture.set_value(Some(Gesture::Pending { start: m, button, shift: ev.shift_key(), ctrl: ev.ctrl_key() || ev.meta_key() }));
    };

    let on_move = move |ev: web_sys::PointerEvent| {
        let m = pos(&ev);
        let delta = m - last.get_value();
        last.set_value(m);
        let size = size_of();
        match gesture.get_value() {
            Some(Gesture::Nav(n)) => app.view(|ed| {
                let kind = ed.single_view;
                ed.nav = Some(n);
                let cam = &mut ed.cameras[kind.index()];
                match n {
                    NavMode::Orbit => cam.orbit(delta.x, delta.y),
                    NavMode::Pan => cam.pan(delta.x, delta.y, size.y),
                    NavMode::Zoom => cam.zoom((1.0 - (delta.x - delta.y) * 0.005).clamp(0.5, 2.0)),
                }
            }),
            Some(Gesture::Pending { start, .. }) => {
                // Dragging anywhere (not on a handle) moves, turns or scales freely.
                if (m - start).length() > 4.0 && app.peek(|e| e.tool != Tool::Select && e.active().is_some() && !e.mode.is_component()) {
                    if begin_drag(start, Handle::Free) {
                        gesture.set_value(Some(Gesture::Drag));
                    } else {
                        gesture.set_value(None);
                    }
                }
            }
            Some(Gesture::Drag) => app.gesture(|ed| {
                if let Some(d) = ed.drag.clone() {
                    let cam = ed.cameras[ed.single_view.index()];
                    d.update(&mut ed.scene, ray_at(&cam, size, m), m);
                    ed.apply_axis_compensation();
                }
            }),
            None => {
                let h = app.peek(|ed| {
                    let cam = ed.cameras[ed.single_view.index()];
                    gizmo_frame(ed, size).and_then(|gf| gizmo::hit_test(ed.tool, &gf, &|p| to_screen(&cam, size, p), m, ed.axes))
                });
                if h != hot.get_untracked() {
                    hot.set(h);
                }
            }
        }
    };

    let on_up = move |ev: web_sys::PointerEvent| {
        let m = pos(&ev);
        let size = size_of();
        match gesture.get_value() {
            Some(Gesture::Drag) => app.act(|ed| ed.finish_drag()),
            Some(Gesture::Nav(_)) => app.view(|ed| ed.nav = None),
            Some(Gesture::Pending { button: 0, shift, ctrl, .. }) => {
                // A click: pick what is under the cursor.
                let hit = app.peek(|ed| {
                    let cam = ed.cameras[ed.single_view.index()];
                    let (ro, rd) = ray_at(&cam, size, m);
                    shown.with_value(|s| picking::pick(&s.pick, ro, rd).map(|(id, _)| id))
                });
                app.act(|ed| match (hit, shift || ctrl) {
                    (Some(id), true) => ed.toggle_selection(id),
                    (Some(id), false) => ed.select_only(id),
                    (None, false) => ed.clear_selection(),
                    (None, true) => {}
                });
            }
            _ => {}
        }
        gesture.set_value(None);
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let dy = ev.delta_y() as f32;
        app.view(|ed| {
            let k = ed.single_view.index();
            ed.cameras[k].zoom((1.0 + dy * 0.0012).clamp(0.5, 2.0));
        });
    };

    let view_label = move || app.read(|e| e.single_view.label().to_owned());
    let shading = move || app.read(|e| e.display.label().to_owned());
    let mode = move || app.read(|e| format!("{} mode", e.mode.label()));

    view! {
        <div class="panel viewport">
            <div class="vbar">
                <div class="group">
                    {ViewKind::ALL.map(|k| view! { <crate::chrome::CmdButton cmd=Cmd::Camera(k) /> }).collect_view()}
                </div>
                <div class="group">
                    <crate::chrome::CmdButton cmd=Cmd::FrameAll />
                    <crate::chrome::CmdButton cmd=Cmd::FrameSelected />
                </div>
                <div class="group">
                    {forma3d_core::renderer::DisplayMode::ALL.map(|m| view! { <crate::chrome::CmdButton cmd=Cmd::Shading(m) /> }).collect_view()}
                </div>
                <div class="group">
                    <crate::chrome::CmdButton cmd=Cmd::ToggleGrid />
                </div>
            </div>
            <div class="vmenus">
                <MenuButton name="View" />
                <MenuButton name="Display" />
                <MenuButton name="Options" />
            </div>
            <div class="stage" node_ref=stage>
                <canvas node_ref=canvas></canvas>
                <svg class="overlay" inner_html=move || svg.get()></svg>
                <div
                    class="hit"
                    on:pointerdown=on_down
                    on:pointermove=on_move
                    on:pointerup=on_up
                    on:pointercancel=move |_| gesture.set_value(None)
                    on:wheel=on_wheel
                    on:contextmenu=|ev| ev.prevent_default()
                ></div>
                <div class="chips">
                    <span>{view_label}</span>
                    <span class="a">{shading}</span>
                    <span>{mode}</span>
                </div>
                <div class="hud">
                    <button class="tb" title="Frame all (H)" on:click=move |_| crate::commands::run(app, Cmd::FrameAll)>{icon("frame")}</button>
                    <button class="tb" title="Frame selected (S)" on:click=move |_| crate::commands::run(app, Cmd::FrameSelected)>{icon("frame-sel")}</button>
                </div>
                <div class="stats">{move || stats.get()}<br />{move || app.gpu_name.get()}</div>
                {move || error.get().map(|e| view! {
                    <div class="gpu-error">"The 3D view could not start: " {e} <br /> "Everything else still works."</div>
                })}
            </div>
            <crate::timeline::Timeline />
        </div>
    }
}

