//! The 3D view: the engine's wgpu renderer drawing into a canvas (WebGL 2 by
//! default, WebGPU with `?webgpu`), one view or four, camera navigation, and
//! the tools (gizmo, components, pens) with their SVG overlay.

use crate::chrome::{Entry, MenuButton, MenuEntries};
use crate::commands::{Cmd, MESH_TOOLS};
use crate::components::component_mode;
use crate::icons::icon;
use crate::interact::{SnapCtx, object_mode, view_chrome};
use crate::keymap::Mods;
use crate::pen::pen_mode;
use crate::state::{App, app};
use crate::vinput::{Input, Painter, View};
use forma3d_core::camera::ViewKind;
use forma3d_core::editor::{Editor, NavMode, RenderObject, ViewLayout};
use forma3d_core::picking::{self, PickItem};
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::renderer::{
    self, CameraUniform, DrawItem, FrameScene, GpuResources, PostUniform, ViewDraw,
};
use glam::Vec2;
use leptos::prelude::*;
use std::fmt::Write as _;
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

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("forma3d_depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
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
    let want_webgpu = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|q| q.contains("webgpu"));
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = if want_webgpu {
        wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
    } else {
        wgpu::Backends::GL
    };
    let instance = if want_webgpu {
        wgpu::util::new_instance_with_webgpu_detection(desc).await
    } else {
        wgpu::Instance::new(desc)
    };
    #[cfg(target_arch = "wasm32")]
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .map_err(|e| e.to_string())?;
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
    let limits = if webgl {
        wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
    } else {
        adapter.limits()
    };
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("forma3d"),
            required_limits: limits,
            ..Default::default()
        })
        .await
        .map_err(|e| format!("could not open the graphics device: {e}"))?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or_else(|| caps.formats.first().copied())
        .ok_or("the canvas has no formats")?;
    let alpha_mode = if caps
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
    {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        caps.alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto)
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
    let short: String = info
        .name
        .split(" (")
        .next()
        .unwrap_or(&info.name)
        .chars()
        .take(48)
        .collect();
    let name = format!("{} · {short}", if webgl { "WebGL 2" } else { "WebGPU" });
    Ok((
        Gpu {
            device,
            queue,
            surface,
            config,
            depth,
            res,
            srgb,
        },
        name,
    ))
}

thread_local! {
    /// Tessellated copies of the scene's meshes (Options > Tessellation).
    static TESSELLATION: std::cell::RefCell<forma3d_core::tessellate::Cache> = Default::default();
}

/// One view's place on the stage, in CSS pixels.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub kind: ViewKind,
    pub min: Vec2,
    pub size: Vec2,
}

impl Rect {
    fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x
            && p.y >= self.min.y
            && p.x < self.min.x + self.size.x
            && p.y < self.min.y + self.size.y
    }
}

/// Where the views go: one, or four (Top, Perspective / Right, Front).
pub fn view_rects(ed: &Editor, stage: Vec2) -> Vec<Rect> {
    match ed.layout {
        ViewLayout::Single => vec![Rect {
            kind: ed.single_view,
            min: Vec2::ZERO,
            size: stage,
        }],
        ViewLayout::Quad => {
            let gap = 2.0;
            let w = ((stage.x - gap) * 0.5).floor();
            let h = ((stage.y - gap) * 0.5).floor();
            let cell = |c: f32, r: f32| Vec2::new(c * (w + gap), r * (h + gap));
            vec![
                Rect {
                    kind: ViewKind::Top,
                    min: cell(0.0, 0.0),
                    size: Vec2::new(w, h),
                },
                Rect {
                    kind: ViewKind::Perspective,
                    min: cell(1.0, 0.0),
                    size: Vec2::new(stage.x - w - gap, h),
                },
                Rect {
                    kind: ViewKind::Right,
                    min: cell(0.0, 1.0),
                    size: Vec2::new(w, stage.y - h - gap),
                },
                Rect {
                    kind: ViewKind::Front,
                    min: cell(1.0, 1.0),
                    size: Vec2::new(stage.x - w - gap, stage.y - h - gap),
                },
            ]
        }
    }
}

/// The pointer over the stage, between events.
#[derive(Clone, Copy, Debug, Default)]
struct Pointer {
    pos: Option<Vec2>,
    down: bool,
    origin: Option<Vec2>,
    dragging: bool,
    /// The view the pointer pressed in (it keeps the pointer until release).
    capture: Option<usize>,
    mods: Mods,
    nav: Option<(NavMode, usize, i16, Vec2)>,
    last_click: (f64, Vec2),
}

/// What happened in this event.
#[derive(Clone, Copy, Debug, Default)]
struct Flags {
    clicked: bool,
    double_clicked: bool,
    drag_started: bool,
}

fn input_for(i: usize, r: &Rect, ptr: &Pointer, f: Flags) -> Input {
    let owns = ptr.capture == Some(i);
    let free = ptr.capture.is_none();
    let over = ptr.pos.is_some_and(|p| r.contains(p));
    let hovered = over && (free || owns) && ptr.nav.is_none();
    let local = |p: Vec2| p - r.min;
    Input {
        mouse: ptr.pos.filter(|_| hovered || owns).map(local),
        hovered,
        press_origin: ptr.origin.filter(|_| owns).map(local),
        primary_down: ptr.down && owns,
        clicked: f.clicked && owns,
        double_clicked: f.double_clicked && owns,
        drag_started: f.drag_started && owns,
        dragged: ptr.dragging && ptr.down && owns,
        mods: ptr.mods,
        navigating: ptr.nav.is_some(),
    }
}

/// The visible objects this frame.
fn visible_objects(ed: &mut Editor) -> Vec<RenderObject> {
    let mut objects = ed.render_objects();
    objects.retain(|o| ed.passes_filter(o.id));
    objects
}

fn grid_for(ed: &Editor, cam: &forma3d_core::camera::Camera, kind: ViewKind) -> renderer::Grid {
    let wp = ed.scene.workplane;
    if kind == ViewKind::Perspective && !wp.is_default() {
        renderer::build_grid_on(cam, wp.matrix())
    } else {
        renderer::build_grid(cam)
    }
}

/// Runs the tools of every view on this pointer event and returns the
/// overlay (SVG, one nested `<svg>` per view).
fn run_tools(
    ed: &mut Editor,
    objects: &[RenderObject],
    rects: &[Rect],
    ptr: &Pointer,
    f: Flags,
) -> String {
    let pick: Vec<PickItem> = objects
        .iter()
        .filter(|o| !ed.scene.is_locked(o.id))
        .map(|o| o.pick_item())
        .collect();
    let quad = rects.len() > 1;
    let mut svg = String::new();
    for (i, r) in rects.iter().enumerate() {
        if r.size.x < 40.0 || r.size.y < 40.0 {
            continue;
        }
        let cam = ed.cameras[r.kind.index()];
        let v = View {
            kind: r.kind,
            size: r.size,
            cam,
        };
        let inp = input_for(i, r, ptr, f);
        let snap = SnapCtx {
            objects,
            grid_unit: grid_for(ed, &cam, r.kind).unit,
        };
        let mut p = Painter::default();
        if ed.pen.is_some() {
            pen_mode(ed, &v, &inp, &snap, &mut p);
        } else if ed.mode.is_component() {
            component_mode(ed, &v, &inp, &snap, &mut p);
        } else {
            object_mode(ed, &v, &inp, &pick, &snap, &mut p);
        }
        view_chrome(&mut p, ed, &v, ed.active_view == r.kind, quad);
        let _ = write!(
            svg,
            r#"<svg x="{}" y="{}" width="{}" height="{}" overflow="hidden">{}</svg>"#,
            r.min.x, r.min.y, r.size.x, r.size.y, p.svg
        );
    }
    svg
}

/// Builds every view's drawing for this frame.
fn build_draws(
    ed: &Editor,
    objects: &[RenderObject],
    rects: &[Rect],
    srgb: bool,
    dpr: f32,
) -> Vec<ViewDraw> {
    let fx = ed.view_opts.fx;
    let items: Vec<DrawItem> = TESSELLATION.with_borrow_mut(|cache| {
        let items = objects
            .iter()
            .map(|o| DrawItem {
                mesh: if fx.tessellation && o.kind == renderer::ItemKind::Surface {
                    cache.get(&o.mesh)
                } else {
                    o.mesh.clone()
                },
                model: o.world,
                color: o.color,
                selection: o.selection,
                kind: o.kind,
            })
            .collect();
        cache.end_frame();
        items
    });
    let mut frame = FrameScene::new(items, ed.display, fx);
    frame.lights = ed.scene_lights();
    let frame = Arc::new(frame);
    rects
        .iter()
        .enumerate()
        .filter(|(_, r)| r.size.x >= 40.0 && r.size.y >= 40.0)
        .map(|(slot, r)| {
            let cam = ed.cameras[r.kind.index()];
            let aspect = (r.size.x / r.size.y.max(1.0)).max(0.01);
            let wp = ed.scene.workplane;
            let on_workplane = r.kind == ViewKind::Perspective && !wp.is_default();
            let mut grid = grid_for(ed, &cam, r.kind);
            if !ed.view_opts.grid {
                grid.vertices.clear();
            }
            let mut uniform = CameraUniform::new(&cam, aspect, srgb, grid.center, grid.radius);
            let floor = ed.view_opts.grid
                && !on_workplane
                && matches!(r.kind, ViewKind::Perspective | ViewKind::Top);
            uniform.set_lights(&frame.lights, frame.bounds);
            uniform.set_effects(fx, frame.bounds, floor);
            uniform.set_shading(frame.mode);
            let size_px = [
                (r.size.x * dpr).round().max(1.0) as u32,
                (r.size.y * dpr).round().max(1.0) as u32,
            ];
            ViewDraw {
                slot,
                camera: uniform,
                post: PostUniform::new(&cam, aspect, size_px, fx),
                size_px,
                grid: grid.vertices,
                scene: frame.clone(),
            }
        })
        .collect()
}

/// Entries of the right-click menu in the view.
fn context_entries(component: bool) -> Vec<Entry> {
    use Entry::*;
    if component {
        let mut v = vec![
            C(Cmd::MakeEditable),
            Sep,
            C(Cmd::Pen(forma3d_core::pen::PenKind::Polygon)),
        ];
        v.extend(MESH_TOOLS.map(|t| C(Cmd::MeshTool(t))));
        v.extend([
            Sep,
            C(Cmd::Bridge),
            C(Cmd::Weld),
            C(Cmd::Optimize),
            C(Cmd::SubdivideSmooth),
            C(Cmd::SubdivideLinear),
            Sep,
        ]);
        v.extend([
            C(Cmd::Delete),
            C(Cmd::SelectAll),
            C(Cmd::LoopSelect),
            C(Cmd::ReverseNormals),
        ]);
        v
    } else {
        let mut v: Vec<Entry> = PrimitiveKind::ALL.map(|k| C(Cmd::Primitive(k))).to_vec();
        v.extend([
            C(Cmd::Null),
            Sep,
            C(Cmd::Delete),
            C(Cmd::Duplicate),
            C(Cmd::Group),
            Sep,
            C(Cmd::FrameSelected),
            C(Cmd::FrameAll),
        ]);
        v
    }
}

/// Finishes a pen with Enter / removes its last point with Backspace.
pub fn pen_key(app: App, enter: bool) {
    app.act(|ed| {
        let k = ed.active_view;
        let v = View {
            kind: k,
            size: Vec2::ONE,
            cam: ed.cameras[k.index()],
        };
        if enter {
            crate::pen::enter(ed, &v)
        } else {
            crate::pen::backspace(ed)
        }
    });
}

#[component]
pub fn Viewport() -> impl IntoView {
    let app = app();
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let stage = NodeRef::<leptos::html::Div>::new();
    let gpu: StoredValue<Option<Gpu>, LocalStorage> = StoredValue::new_local(None);
    let ptr: StoredValue<Pointer, LocalStorage> = StoredValue::new_local(Pointer::default());
    let queued: StoredValue<bool, LocalStorage> = StoredValue::new_local(false);
    // A queued frame redraws the 3D view too (not only the overlay).
    let full: StoredValue<bool, LocalStorage> = StoredValue::new_local(true);
    let svg = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let stats = RwSignal::new(String::new());
    let rects_sig = RwSignal::new(Vec::<Rect>::new());
    let ctx_menu = RwSignal::new(None::<(f32, f32, bool)>);

    let size_of = move || -> Vec2 {
        stage
            .get_untracked()
            .map(|s| {
                Vec2::new(
                    s.client_width().max(1) as f32,
                    s.client_height().max(1) as f32,
                )
            })
            .unwrap_or(Vec2::ONE)
    };
    let dpr = || {
        web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0) as f32
    };

    // Draws one frame: the 3D view and the overlay, or (when only the pointer
    // moved) just the overlay.
    let render = move || {
        queued.set_value(false);
        let redraw = full.get_value();
        full.set_value(false);
        if !redraw {
            let size = size_of();
            let p = ptr.get_value();
            if let Some(overlay) = app.ed.try_update_value(|ed| {
                let objects = visible_objects(ed);
                let rects = view_rects(ed, size);
                run_tools(ed, &objects, &rects, &p, Flags::default())
            }) {
                svg.set(overlay);
            }
            return;
        }
        let Some(cv) = canvas.get_untracked() else {
            return;
        };
        let size = size_of();
        let d = dpr();
        let (pw, ph) = (
            (size.x * d).round().max(1.0) as u32,
            (size.y * d).round().max(1.0) as u32,
        );
        let srgb = gpu
            .with_value(|g| g.as_ref().map(|g| g.srgb))
            .unwrap_or(false);
        let p = ptr.get_value();
        let Some((draws, rects, overlay, tri, objs)) = app.ed.try_update_value(|ed| {
            let objects = visible_objects(ed);
            let rects = view_rects(ed, size);
            let draws = build_draws(ed, &objects, &rects, srgb, d);
            let overlay = run_tools(ed, &objects, &rects, &p, Flags::default());
            let tri: usize = objects
                .iter()
                .filter(|o| o.kind == renderer::ItemKind::Surface)
                .map(|o| o.mesh.indices.len() / 3)
                .sum();
            ed.meshes.end_frame();
            (draws, rects, overlay, tri, objects.len())
        }) else {
            return;
        };
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
            let frame = match g.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f)
                | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                _ => {
                    g.surface.configure(&g.device, &g.config);
                    return;
                }
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut enc = g
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("forma3d_frame"),
                });
            for draw in &draws {
                draw.prepare(&mut g.res, &g.device, &g.queue, &mut enc);
            }
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("forma3d_views"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &g.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                for draw in &draws {
                    let Some(r) = rects.get(draw.slot) else {
                        continue;
                    };
                    let x = (r.min.x * d).round().clamp(0.0, pw as f32 - 1.0);
                    let y = (r.min.y * d).round().clamp(0.0, ph as f32 - 1.0);
                    let w = (draw.size_px[0] as f32).min(pw as f32 - x).max(1.0);
                    let h = (draw.size_px[1] as f32).min(ph as f32 - y).max(1.0);
                    pass.set_viewport(x, y, w, h, 0.0, 1.0);
                    pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
                    draw.paint(&g.res, &mut pass);
                }
            }
            g.queue.submit([enc.finish()]);
            g.queue.present(frame);
        });
        svg.set(overlay);
        if rects_sig.with_untracked(|r| {
            r.len() != rects.len()
                || r.iter()
                    .zip(&rects)
                    .any(|(a, b)| a.kind != b.kind || a.size != b.size || a.min != b.min)
        }) {
            rects_sig.set(rects);
        }
        stats.set(format!("{objs} objects · {tri} triangles"));
    };

    let queue = move |whole: bool| {
        if whole {
            full.set_value(true);
        }
        if queued.get_value() {
            return;
        }
        queued.set_value(true);
        request_animation_frame(render);
    };
    let schedule = move || queue(true);
    let schedule_overlay = move || queue(false);

    Effect::new(move |_| {
        app.rev.track();
        app.view_rev.track();
        schedule();
    });

    // Start the GPU once the canvas exists, and redraw on resize.
    Effect::new(move |done: Option<bool>| {
        if done == Some(true) {
            return true;
        }
        let (Some(cv), Some(st)) = (canvas.get(), stage.get()) else {
            return false;
        };
        let cv: web_sys::HtmlCanvasElement = cv.clone();
        let st: web_sys::HtmlElement = st.clone().into();
        let on_resize = Closure::<dyn FnMut()>::new(schedule);
        if let Ok(ro) = web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref()) {
            ro.observe(&st);
            std::mem::forget(ro);
        }
        on_resize.forget();
        let d = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        cv.set_width((st.client_width().max(1) as f64 * d) as u32);
        cv.set_height((st.client_height().max(1) as f64 * d) as u32);
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

    // ---- pointer input -----------------------------------------------------
    let pos = |ev: &web_sys::MouseEvent| Vec2::new(ev.offset_x() as f32, ev.offset_y() as f32);
    let mods = |ev: &web_sys::MouseEvent| Mods {
        ctrl: ev.ctrl_key() || ev.meta_key(),
        shift: ev.shift_key(),
        alt: ev.alt_key(),
    };
    let view_at = move |p: Vec2| -> Option<(usize, Rect)> {
        let size = size_of();
        app.peek(|ed| view_rects(ed, size))
            .into_iter()
            .enumerate()
            .find(|(_, r)| r.contains(p))
    };

    // Runs the tools for an event that may change the scene.
    let fire = move |f: Flags| {
        let size = size_of();
        let p = ptr.get_value();
        let status = app.ed.try_update_value(|ed| {
            let before = ed.status.clone();
            let objects = visible_objects(ed);
            let rects = view_rects(ed, size);
            let _ = run_tools(ed, &objects, &rects, &p, f);
            ed.prune_selection();
            ed.commit_history(p.down);
            (ed.status != before).then(|| ed.status.clone())
        });
        if let Some(Some(s)) = status {
            app.log_line(s, false);
        }
        app.rev.update(|r| *r += 1);
    };

    let on_down = move |ev: web_sys::PointerEvent| {
        app.menu.set(None);
        ctx_menu.set(None);
        let m = pos(&ev);
        if let Some(t) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = t.set_pointer_capture(ev.pointer_id());
        }
        let Some((i, r)) = view_at(m) else { return };
        app.view(|ed| ed.active_view = r.kind);
        let button = ev.button();
        let alt = ev.alt_key();
        let ortho = r.kind.is_ortho();
        let nav = match (button, alt) {
            (0, true) => Some(if ortho { NavMode::Pan } else { NavMode::Orbit }),
            (1, _) => Some(NavMode::Pan),
            (2, true) => Some(NavMode::Zoom),
            (2, false) => Some(if ortho { NavMode::Pan } else { NavMode::Orbit }),
            _ => None,
        };
        ptr.update_value(|p| {
            p.pos = Some(m);
            p.mods = mods(&ev);
            if let Some(n) = nav {
                p.nav = Some((n, i, button, m));
            } else if button == 0 {
                p.down = true;
                p.origin = Some(m);
                p.dragging = false;
                p.capture = Some(i);
            }
        });
        app.view_rev.update(|r| *r += 1);
    };

    let on_move = move |ev: web_sys::PointerEvent| {
        let m = pos(&ev);
        let before = ptr.get_value();
        let delta = before.pos.map_or(Vec2::ZERO, |p| m - p);
        ptr.update_value(|p| {
            p.pos = Some(m);
            p.mods = mods(&ev);
        });
        if let Some((n, i, _, _)) = before.nav {
            let size = size_of();
            app.view(|ed| {
                let Some(r) = view_rects(ed, size).get(i).copied() else {
                    return;
                };
                ed.nav = Some(n);
                let cam = &mut ed.cameras[r.kind.index()];
                match n {
                    NavMode::Orbit => cam.orbit(delta.x, delta.y),
                    NavMode::Pan => cam.pan(delta.x, delta.y, r.size.y),
                    NavMode::Zoom => cam.zoom((1.0 - (delta.x - delta.y) * 0.005).clamp(0.5, 2.0)),
                }
            });
            return;
        }
        if before.down {
            if !before.dragging && before.origin.is_some_and(|o| (m - o).length() > 4.0) {
                ptr.update_value(|p| p.dragging = true);
                fire(Flags {
                    drag_started: true,
                    ..Default::default()
                });
            } else if before.dragging {
                fire(Flags::default());
            }
            return;
        }
        schedule_overlay();
    };

    let on_up = move |ev: web_sys::PointerEvent| {
        let m = pos(&ev);
        let before = ptr.get_value();
        if let Some((_, _, button, start)) = before.nav {
            ptr.update_value(|p| p.nav = None);
            app.view(|ed| ed.nav = None);
            // A right click (no drag): the context menu.
            if button == 2 && (m - start).length() < 4.0 {
                let comp = app.peek(|ed| ed.mode.is_component());
                ctx_menu.set(Some((m.x, m.y, comp)));
            }
            return;
        }
        if !before.down {
            return;
        }
        let clicked = !before.dragging;
        let now = window().performance().map(|p| p.now()).unwrap_or(0.0);
        let double = clicked
            && now - before.last_click.0 < 350.0
            && (m - before.last_click.1).length() < 6.0;
        ptr.update_value(|p| {
            p.pos = Some(m);
            p.down = false;
            p.mods = mods(&ev);
            if clicked {
                p.last_click = if double { (0.0, m) } else { (now, m) };
            }
        });
        fire(Flags {
            clicked,
            double_clicked: double,
            drag_started: false,
        });
        ptr.update_value(|p| {
            p.dragging = false;
            p.capture = None;
            p.origin = None;
        });
        app.view_rev.update(|r| *r += 1);
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let dy = ev.delta_y() as f32;
        let m = Vec2::new(ev.offset_x() as f32, ev.offset_y() as f32);
        let Some((_, r)) = view_at(m) else { return };
        app.view(|ed| ed.cameras[r.kind.index()].zoom((1.0 + dy * 0.0012).clamp(0.5, 2.0)));
    };

    // A material dropped from the Material Manager goes onto the object under it.
    let on_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        let Some(mat) = ev
            .data_transfer()
            .and_then(|dt| dt.get_data("application/x-forma3d-material").ok())
            .and_then(|s| s.parse::<u64>().ok())
        else {
            return;
        };
        let m = Vec2::new(ev.offset_x() as f32, ev.offset_y() as f32);
        let Some((_, r)) = view_at(m) else { return };
        app.act(|ed| {
            let objects = visible_objects(ed);
            let pick: Vec<PickItem> = objects
                .iter()
                .filter(|o| !ed.scene.is_locked(o.id))
                .map(|o| o.pick_item())
                .collect();
            let v = View {
                kind: r.kind,
                size: r.size,
                cam: ed.cameras[r.kind.index()],
            };
            let (ro, rd) = v.ray(m - r.min);
            if let Some((id, _)) = picking::pick(&pick, ro, rd) {
                ed.assign_material(&[id], Some(mat));
                ed.status = "Material assigned".to_owned();
            }
        });
    };

    let quad = move || app.read(|e| e.layout == ViewLayout::Quad);
    let shown_timeline = move || app.timeline_shown();

    view! {
        <div class="panel viewport">
            <div class="vbar">
                <div class="group">
                    {ViewKind::ALL.map(|k| view! { <crate::chrome::CmdButton cmd=Cmd::Camera(k) /> }).collect_view()}
                    <crate::chrome::CmdButton cmd=Cmd::FourViews />
                </div>
                <div class="group">
                    <crate::chrome::CmdButton cmd=Cmd::FrameAll />
                    <crate::chrome::CmdButton cmd=Cmd::FrameSelected />
                </div>
                <div class="group">
                    {forma3d_core::renderer::DisplayMode::ALL.map(|m| view! { <crate::chrome::CmdButton cmd=Cmd::Display(m) /> }).collect_view()}
                </div>
                <div class="group">
                    <crate::chrome::CmdButton cmd=Cmd::ToggleGrid />
                    <crate::chrome::CmdButton cmd=Cmd::ToggleSolo />
                </div>
                <div class="group">
                    {forma3d_core::renderer::ViewFx::ALL.map(|fx| view! { <crate::chrome::CmdButton cmd=Cmd::ViewFx(fx) /> }).collect_view()}
                </div>
            </div>
            <div class="vmenus">
                <MenuButton name="v-View" label="View" />
                <MenuButton name="v-Cameras" label="Cameras" />
                <MenuButton name="v-Display" label="Display" />
                <MenuButton name="v-Shading" label="Shading" />
                <MenuButton name="v-Options" label="Options" />
                <MenuButton name="v-Filter" label="Filter" />
                <MenuButton name="v-Panel" label="Panel" />
            </div>
            <div class="stage" node_ref=stage>
                <canvas node_ref=canvas></canvas>
                <svg class="overlay" inner_html=move || svg.get()></svg>
                <div
                    class="hit"
                    on:pointerdown=on_down
                    on:pointermove=on_move
                    on:pointerup=on_up
                    on:pointercancel=move |_| {
                        ptr.update_value(|p| {
                            p.nav = None;
                            p.down = false;
                            p.dragging = false;
                            p.capture = None;
                        });
                        app.act(|ed| ed.finish_drag());
                    }
                    on:pointerleave=move |_| {
                        if !ptr.with_value(|p| p.down || p.nav.is_some()) {
                            ptr.update_value(|p| p.pos = None);
                            schedule_overlay();
                        }
                    }
                    on:wheel=on_wheel
                    on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                    on:drop=on_drop
                    on:contextmenu=|ev| ev.prevent_default()
                ></div>
                // In four views, each view's corner button makes it the only one.
                {move || {
                    if !quad() {
                        return Vec::new();
                    }
                    rects_sig
                        .get()
                        .into_iter()
                        .map(|r| {
                            let k = r.kind;
                            view! {
                                <button
                                    class="tb vmax"
                                    style=format!("left:{}px;top:{}px", r.min.x + r.size.x - 30.0, r.min.y + 6.0)
                                    title=format!("Show only the {} view", k.label())
                                    on:click=move |_| crate::commands::run(app, Cmd::Camera(k))
                                >
                                    {icon("single")}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()
                }}
                <div class="hud">
                    <button class="tb" title="Frame all (H)" on:click=move |_| crate::commands::run(app, Cmd::FrameAll)>{icon("frame")}</button>
                    <button class="tb" title="Frame selected (S)" on:click=move |_| crate::commands::run(app, Cmd::FrameSelected)>{icon("frame-sel")}</button>
                    <button class="tb" title="1 view / 4 views (F5)" on:click=move |_| {
                        let q = app.peek(|e| e.layout == ViewLayout::Quad);
                        let k = app.peek(|e| e.active_view);
                        crate::commands::run(app, if q { Cmd::Camera(k) } else { Cmd::FourViews });
                    }>{move || if quad() { icon("single") } else { icon("quad") }}</button>
                </div>
                <div class="stats">{move || stats.get()}<br />{move || app.gpu_name.get()}</div>
                {move || error.get().map(|e| view! {
                    <div class="gpu-error">"The 3D view could not start: " {e} <br /> "Everything else still works."</div>
                })}
                {move || ctx_menu.get().map(|(x, y, comp)| {
                    view! {
                        <div class="scrim" on:mousedown=move |_| ctx_menu.set(None)></div>
                        <div class="dropdown ctx" style=format!("left:{x}px;top:{y}px") on:mousedown=|ev| ev.stop_propagation() on:click=move |_| ctx_menu.set(None)>
                            <MenuEntries entries=context_entries(comp) />
                        </div>
                    }
                })}
            </div>
            <Show when=shown_timeline>
                <crate::timeline::Timeline />
            </Show>
        </div>
    }
}
