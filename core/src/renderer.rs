//! The 3D viewport renderer. `ViewDraw::prepare` records the offscreen passes
//! (shadows, SSAO) and `ViewDraw::paint` draws into the caller's render pass,
//! so any host can use it: egui's paint callback or a web canvas. Shadows add a shadow-map pass per view; SSAO
//! and Post Effects render the view offscreen first and composite it.
//!
//! One callback is issued per visible view. Each gets a *slot* holding its
//! camera uniform and grid; scene geometry (meshes and per-object instances) is
//! uploaded once per frame by slot 0 and shared by every view.

use crate::camera::{Camera, ViewKind};
use crate::mesh::{Mesh, Vertex};
use bytemuck::{Pod, Zeroable};

use glam::{Mat3, Mat4, Vec3};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::Arc;

/// The depth format of the pass `ViewDraw::paint` draws into.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const MAX_GRID_VERTICES: usize = 16_384;
const INITIAL_INSTANCES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisplayMode {
    /// Smooth shaded faces; only selected objects show their edges.
    Gouraud,
    /// Smooth shaded faces with every object's edges overlaid.
    GouraudLines,
    /// Edges only (Cinema 4D's "Lines").
    Wireframe,
    /// Fast shading from a light at the camera: no key light, specular,
    /// shadows or reflections. Only selected objects show their edges.
    Quick,
    /// Quick shading with every object's edges overlaid.
    QuickLines,
}

impl DisplayMode {
    /// In Shading menu order.
    pub const ALL: [DisplayMode; 5] =
        [DisplayMode::Gouraud, DisplayMode::GouraudLines, DisplayMode::Wireframe, DisplayMode::Quick, DisplayMode::QuickLines];

    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Gouraud => "Gouraud Shading",
            DisplayMode::GouraudLines => "Gouraud Shading (Lines)",
            DisplayMode::Wireframe => "Lines",
            DisplayMode::Quick => "Quick Shading",
            DisplayMode::QuickLines => "Quick Shading (Lines)",
        }
    }

    /// Stable command id fragment (kept from earlier versions for saved
    /// shortcuts and icon bars).
    pub fn id(self) -> &'static str {
        match self {
            DisplayMode::Gouraud => "gouraud_shading",
            DisplayMode::GouraudLines => "gouraud_shading_(lines)",
            DisplayMode::Wireframe => "wireframe",
            DisplayMode::Quick => "quick_shading",
            DisplayMode::QuickLines => "quick_shading_(lines)",
        }
    }

    pub fn is_quick(self) -> bool {
        matches!(self, DisplayMode::Quick | DisplayMode::QuickLines)
    }

    /// Every object shows its edges (not only the selected ones).
    pub fn all_lines(self) -> bool {
        matches!(self, DisplayMode::GouraudLines | DisplayMode::QuickLines | DisplayMode::Wireframe)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionState {
    None = 0,
    Selected = 1,
    /// The selected object the gizmo is attached to.
    Active = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// Triangles plus edges.
    Surface,
    /// Edges only, always drawn (Null objects).
    LinesOnly,
}

pub struct DrawItem {
    pub mesh: Arc<Mesh>,
    pub model: Mat4,
    pub color: [f32; 3],
    pub selection: SelectionState,
    pub kind: ItemKind,
}

// ---------------------------------------------------------------------------
// GPU-facing plain data
// ---------------------------------------------------------------------------

/// The viewport's render options (each view's Options menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewFx {
    PostEffects,
    Tessellation,
    Shadows,
    Reflections,
    Ssao,
    BackfaceCulling,
}

impl ViewFx {
    pub const ALL: [ViewFx; 6] = [
        ViewFx::PostEffects,
        ViewFx::Tessellation,
        ViewFx::Shadows,
        ViewFx::Reflections,
        ViewFx::Ssao,
        ViewFx::BackfaceCulling,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewFx::PostEffects => "Post Effects",
            ViewFx::Tessellation => "Tessellation",
            ViewFx::Shadows => "Shadows",
            ViewFx::Reflections => "Reflections",
            ViewFx::Ssao => "SSAO",
            ViewFx::BackfaceCulling => "Backface Culling",
        }
    }

    /// Stable id fragment for the command registry.
    pub fn id(self) -> &'static str {
        match self {
            ViewFx::PostEffects => "post_effects",
            ViewFx::Tessellation => "tessellation",
            ViewFx::Shadows => "shadows",
            ViewFx::Reflections => "reflections",
            ViewFx::Ssao => "ssao",
            ViewFx::BackfaceCulling => "backface_culling",
        }
    }

    /// Extra words the Commander matches.
    pub fn keywords(self) -> &'static str {
        match self {
            ViewFx::PostEffects => "antialiasing fxaa vignette grading",
            ViewFx::Tessellation => "smooth subdivide phong",
            ViewFx::Shadows => "shadow map light",
            ViewFx::Reflections => "environment fresnel gloss",
            ViewFx::Ssao => "ambient occlusion ao",
            ViewFx::BackfaceCulling => "back faces normals cull",
        }
    }
}

/// Which render options are on. They apply to every view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewEffects {
    pub post_effects: bool,
    pub tessellation: bool,
    pub shadows: bool,
    pub reflections: bool,
    pub ssao: bool,
    pub backface_culling: bool,
}

impl ViewEffects {
    pub fn get(&self, fx: ViewFx) -> bool {
        match fx {
            ViewFx::PostEffects => self.post_effects,
            ViewFx::Tessellation => self.tessellation,
            ViewFx::Shadows => self.shadows,
            ViewFx::Reflections => self.reflections,
            ViewFx::Ssao => self.ssao,
            ViewFx::BackfaceCulling => self.backface_culling,
        }
    }

    pub fn toggle(&mut self, fx: ViewFx) {
        let flag = match fx {
            ViewFx::PostEffects => &mut self.post_effects,
            ViewFx::Tessellation => &mut self.tessellation,
            ViewFx::Shadows => &mut self.shadows,
            ViewFx::Reflections => &mut self.reflections,
            ViewFx::Ssao => &mut self.ssao,
            ViewFx::BackfaceCulling => &mut self.backface_culling,
        };
        *flag = !*flag;
    }

    /// SSAO and post effects need the view rendered offscreen first.
    pub fn offscreen(&self) -> bool {
        self.ssao || self.post_effects
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    forward: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4],
    fade: [f32; 4],
    light_view_proj: [[f32; 4]; 4],
    /// x: shadows, y: reflections, z: shadow texel size in world units,
    /// w: the floor plane (y = 0) is shown.
    fx: [f32; 4],
    /// rgb: ambient light from Dome lights (and the Sun's sky); w: 1 when
    /// the scene's lights replace the default camera light.
    ambient: [f32; 4],
    /// x: number of direct lights in `lights`.
    light_count: [f32; 4],
    lights: [crate::light::LightRaw; crate::light::MAX_LIGHTS],
}

impl CameraUniform {
    pub fn new(cam: &Camera, aspect: f32, target_is_srgb: bool, fade_center: Vec3, fade_radius: f32) -> Self {
        let fwd = cam.forward();
        let light = Self::light_for(cam);
        let eye = cam.eye();
        CameraUniform {
            view_proj: cam.view_proj(aspect).to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            forward: [fwd.x, fwd.y, fwd.z, if cam.kind.is_ortho() { 1.0 } else { 0.0 }],
            light_dir: [light.x, light.y, light.z, 0.0],
            params: [if target_is_srgb { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            fade: [fade_center.x, fade_center.y, fade_center.z, fade_radius],
            light_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            fx: [0.0; 4],
            ambient: [0.0; 4],
            light_count: [0.0; 4],
            lights: [crate::light::LightRaw::default(); crate::light::MAX_LIGHTS],
        }
    }

    /// The key light follows the camera: above, to the left and behind it.
    pub fn light_for(cam: &Camera) -> Vec3 {
        (-cam.forward() * 0.72 + cam.up() * 0.55 - cam.right() * 0.42).normalize_or_zero()
    }

    /// Turns on the per-view effects. `floor` asks for shadows on the ground
    /// plane (y = 0), which is included in the shadow volume.
    pub fn set_effects(&mut self, fx: ViewEffects, bounds: Option<(Vec3, Vec3)>, floor: bool) {
        self.fx[1] = if fx.reflections { 1.0 } else { 0.0 };
        self.fx[3] = if floor { 1.0 } else { 0.0 };
        let no_caster = self.light_count[1] > 0.5;
        let Some((mut lo, mut hi)) = bounds.filter(|_| fx.shadows && !no_caster) else {
            return;
        };
        if floor {
            lo.y = lo.y.min(0.0);
            hi.y = hi.y.max(0.0);
        }
        let light = Vec3::from_slice(&self.light_dir[..3]);
        let (matrix, radius) = shadow_matrix(light, lo, hi);
        self.light_view_proj = matrix.to_cols_array_2d();
        self.fx[0] = 1.0;
        self.fx[2] = 2.0 * radius / SHADOW_SIZE as f32;
    }

    /// Uses the scene's lights instead of the camera light. Call before
    /// `set_effects`: the main light's direction decides where shadows fall.
    pub fn set_lights(&mut self, lights: &crate::light::SceneLights, bounds: Option<(Vec3, Vec3)>) {
        if !lights.any {
            return;
        }
        self.ambient = [lights.ambient[0], lights.ambient[1], lights.ambient[2], 1.0];
        self.light_count[0] = lights.lights.len() as f32;
        for (slot, l) in self.lights.iter_mut().zip(&lights.lights) {
            *slot = *l;
        }
        let centre = bounds.map_or(Vec3::ZERO, |(lo, hi)| (lo + hi) * 0.5);
        match lights.shadow_direction(centre) {
            Some(d) => self.light_dir = [d.x, d.y, d.z, 0.0],
            // No light casts shadows: the Shadows option has nothing to draw.
            None => self.light_count[1] = 1.0,
        }
    }

    /// Quick shading: a headlight only, and no shadows or reflections.
    pub fn set_shading(&mut self, mode: DisplayMode) {
        self.params[1] = if mode.is_quick() { 1.0 } else { 0.0 };
        if mode.is_quick() {
            self.fx[0] = 0.0;
            self.fx[1] = 0.0;
        }
    }

    pub fn shadows_on(&self) -> bool {
        self.fx[0] > 0.5
    }

    /// The floor plane is in view (it catches shadows and, with SSAO,
    /// contact occlusion).
    pub fn floor_on(&self) -> bool {
        self.fx[3] > 0.5
    }

    pub fn catcher_on(&self) -> bool {
        self.floor_on() && self.shadows_on()
    }
}

/// An orthographic light projection that covers the box `lo..hi`, looking
/// along `-light`. Also returns the covered radius.
pub fn shadow_matrix(light: Vec3, lo: Vec3, hi: Vec3) -> (Mat4, f32) {
    let center = (lo + hi) * 0.5;
    let radius = ((hi - lo).length() * 0.5).max(1.0);
    let light = if light.length_squared() > 1e-8 { light.normalize() } else { Vec3::Y };
    let up = if light.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let eye = center + light * radius * 2.0;
    let view = glam::camera::lh::view::look_at_mat4(eye, center, up);
    let proj = glam::camera::lh::proj::directx::orthographic(-radius, radius, -radius, radius, radius * 0.5, radius * 3.5);
    (proj * view, radius)
}

/// Inputs to the SSAO and composite passes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PostUniform {
    proj: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],
    /// x: AO radius (scene units), y: AO strength, z: SSAO on, w: post effects on.
    params: [f32; 4],
    /// Width, height, 1 / width, 1 / height (physical pixels).
    size: [f32; 4],
    /// x: 1 for orthographic views, y: sign of view-space depth (+1 when the
    /// camera looks down +z).
    misc: [f32; 4],
}

impl PostUniform {
    pub fn new(cam: &Camera, aspect: f32, size_px: [u32; 2], fx: ViewEffects) -> Self {
        let proj = cam.proj(aspect);
        let ahead = cam.view().transform_point3(cam.eye() + cam.forward()).z;
        let scale = if cam.kind.is_ortho() { cam.ortho_half_height * 1.6 } else { cam.distance };
        let (w, h) = (size_px[0].max(1) as f32, size_px[1].max(1) as f32);
        PostUniform {
            proj: proj.to_cols_array_2d(),
            inv_proj: proj.inverse().to_cols_array_2d(),
            params: [
                (scale * 0.045).max(0.5),
                1.15,
                if fx.ssao { 1.0 } else { 0.0 },
                if fx.post_effects { 1.0 } else { 0.0 },
            ],
            size: [w, h, 1.0 / w, 1.0 / h],
            misc: [if cam.kind.is_ortho() { 1.0 } else { 0.0 }, if ahead >= 0.0 { 1.0 } else { -1.0 }, 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InstanceRaw {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    color: [f32; 4],
    flags: [f32; 4],
}

impl InstanceRaw {
    pub fn new(item: &DrawItem, wireframe: bool) -> Self {
        let m3 = Mat3::from_mat4(item.model);
        let normal = if m3.determinant().abs() > 1e-12 {
            m3.inverse().transpose()
        } else {
            Mat3::IDENTITY
        };
        let n = normal.to_cols_array_2d();
        let lines_only = item.kind == ItemKind::LinesOnly;
        InstanceRaw {
            model: item.model.to_cols_array_2d(),
            normal: [
                [n[0][0], n[0][1], n[0][2], 0.0],
                [n[1][0], n[1][1], n[1][2], 0.0],
                [n[2][0], n[2][1], n[2][2], 0.0],
            ],
            color: [item.color[0], item.color[1], item.color[2], 1.0],
            flags: [
                item.selection as u8 as f32,
                if wireframe || lines_only { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GridVertex {
    pos: [f32; 3],
    color: [f32; 4],
}

// ---------------------------------------------------------------------------
// Grid generation
// ---------------------------------------------------------------------------

/// A view's ground grid.
pub struct Grid {
    pub vertices: Vec<GridVertex>,
    /// Centre and radius of the distance fade.
    pub center: Vec3,
    pub radius: f32,
    /// Spacing between adjacent lines, in scene units (cm).
    pub unit: f32,
}

/// The Perspective view's grid on a workplane (`wp` maps the plane's local
/// XZ onto the world).
pub fn build_grid_on(cam: &Camera, wp: Mat4) -> Grid {
    let inv = wp.inverse();
    let mut local = *cam;
    local.target = inv.transform_point3(cam.target);
    let mut grid = build_grid(&local);
    for v in &mut grid.vertices {
        v.pos = wp.transform_point3(Vec3::from(v.pos)).to_array();
    }
    grid.center = wp.transform_point3(grid.center);
    grid
}

/// Builds the ground grid for a view.
pub fn build_grid(cam: &Camera) -> Grid {
    const HALF_LINES: i32 = 60;
    let scale = match cam.kind {
        ViewKind::Perspective => cam.distance,
        _ => cam.ortho_half_height * 2.0,
    };
    let unit = 10f32.powf((scale / 4.0).max(1.0).log10().floor()).max(1.0);

    // (axis along which lines run for "u" lines, axis for "v" lines, index of the fixed axis)
    let (a_idx, b_idx) = match cam.kind {
        ViewKind::Perspective | ViewKind::Top => (0usize, 2usize), // XZ plane
        ViewKind::Front => (0, 1),                                  // XY plane
        ViewKind::Right => (2, 1),                                  // ZY plane
    };
    let center_a = (cam.target[a_idx] / unit).round() * unit;
    let center_b = (cam.target[b_idx] / unit).round() * unit;
    let extent = HALF_LINES as f32 * unit;

    let axis_color = |idx: usize| match idx {
        0 => [0.86, 0.30, 0.26, 0.85],
        1 => [0.45, 0.78, 0.32, 0.85],
        _ => [0.30, 0.50, 0.92, 0.85],
    };
    let line_color = |k: i64| -> [f32; 4] {
        if k % 10 == 0 {
            [0.80, 0.80, 0.82, 0.38]
        } else {
            [0.80, 0.80, 0.82, 0.16]
        }
    };
    let point = |a: f32, b: f32| -> [f32; 3] {
        let mut p = [0.0f32; 3];
        p[a_idx] = a;
        p[b_idx] = b;
        p
    };

    // Each line is drawn as short pieces: some GL drivers mishandle a long
    // line that runs behind the camera and drop or break it.
    const PIECES: usize = 24;
    let mut out = Vec::new();
    let mut line = |p0: [f32; 3], p1: [f32; 3], color: [f32; 4]| {
        let (p0, p1) = (Vec3::from(p0), Vec3::from(p1));
        for i in 0..PIECES {
            let t0 = i as f32 / PIECES as f32;
            let t1 = (i + 1) as f32 / PIECES as f32;
            out.push(GridVertex { pos: p0.lerp(p1, t0).to_array(), color });
            out.push(GridVertex { pos: p0.lerp(p1, t1).to_array(), color });
        }
    };
    let k_a = (center_a / unit).round() as i64;
    let k_b = (center_b / unit).round() as i64;
    // Lines of constant `a` (they run along the b axis).
    for i in -HALF_LINES..=HALF_LINES {
        let k = k_a + i as i64;
        let a = k as f32 * unit;
        let color = if k == 0 { axis_color(b_idx) } else { line_color(k) };
        line(point(a, center_b - extent), point(a, center_b + extent), color);
    }
    // Lines of constant `b` (they run along the a axis).
    for i in -HALF_LINES..=HALF_LINES {
        let k = k_b + i as i64;
        let b = k as f32 * unit;
        let color = if k == 0 { axis_color(a_idx) } else { line_color(k) };
        line(point(center_a - extent, b), point(center_a + extent, b), color);
    }

    let mut center = Vec3::ZERO;
    center[a_idx] = cam.target[a_idx];
    center[b_idx] = cam.target[b_idx];
    let radius = match cam.kind {
        ViewKind::Perspective => extent * 0.9,
        _ => extent * 2.0, // orthographic views never fade
    };
    Grid {
        vertices: out,
        center,
        radius,
        unit,
    }
}

// ---------------------------------------------------------------------------
// GPU resources
// ---------------------------------------------------------------------------

/// Shadow map resolution (one square depth texture per view).
pub const SHADOW_SIZE: u32 = 2048;
/// Format of the SSAO term.
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// The post passes read the view's depth as a plain float texture: GLSL
/// backends cannot `textureLoad` from a depth texture binding.
const DEPTH_AS_FLOAT: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: false };

struct GpuMesh {
    vertices: wgpu::Buffer,
    triangles: wgpu::Buffer,
    edges: wgpu::Buffer,
    triangle_count: u32,
    edge_count: u32,
}

/// The offscreen images a view renders into when SSAO or Post Effects is on.
struct Offscreen {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    ao: wgpu::TextureView,
    ssao_group: wgpu::BindGroup,
    composite_group: wgpu::BindGroup,
}

struct Slot {
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Camera uniform only, for the shadow pass (which writes the shadow map
    /// that `bind_group` samples).
    shadow_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    grid_buffer: wgpu::Buffer,
    grid_vertices: u32,
    post_buffer: wgpu::Buffer,
    offscreen: Option<Offscreen>,
    /// Set by `prepare` when this frame composites from `offscreen`.
    composite: bool,
}

pub struct GpuResources {
    solid: wgpu::RenderPipeline,
    solid_culled: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
    catcher: wgpu::RenderPipeline,
    floor_depth: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    ssao: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
    shadow_layout: wgpu::BindGroupLayout,
    ssao_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,
    slots: Vec<Slot>,
    meshes: HashMap<u64, GpuMesh>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: usize,
    /// Identity of the last uploaded `FrameScene`, so the first view to
    /// prepare in a frame uploads it, whichever view that is.
    uploaded: u64,
    target_format: wgpu::TextureFormat,
    pub target_is_srgb: bool,
}

fn buffer(device: &wgpu::Device, label: &str, size: usize, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4) as u64,
        usage,
        mapped_at_creation: false,
    })
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages, size: usize) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(size as u64),
        },
        count: None,
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, kind: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(kind),
        count: None,
    }
}

fn texture_2d(device: &wgpu::Device, label: &str, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl GpuResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forma3d_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forma3d_post_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()),
        });

        let vf = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let camera_size = std::mem::size_of::<CameraUniform>();
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("forma3d_camera_layout"),
            entries: &[
                uniform_entry(0, vf, camera_size),
                texture_entry(1, wgpu::TextureSampleType::Depth),
                sampler_entry(2, wgpu::SamplerBindingType::Comparison),
            ],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("forma3d_shadow_layout"),
            entries: &[uniform_entry(0, vf, camera_size)],
        });
        let post_size = std::mem::size_of::<PostUniform>();
        let ssao_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("forma3d_ssao_layout"),
            entries: &[uniform_entry(0, vf, post_size), texture_entry(2, DEPTH_AS_FLOAT)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("forma3d_composite_layout"),
            entries: &[
                uniform_entry(0, vf, post_size),
                texture_entry(1, wgpu::TextureSampleType::Float { filterable: true }),
                texture_entry(2, DEPTH_AS_FLOAT),
                texture_entry(3, wgpu::TextureSampleType::Float { filterable: true }),
                sampler_entry(4, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let pipeline_layout = |label: &str, group: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            })
        };
        let layout = pipeline_layout("forma3d_pipeline_layout", &camera_layout);
        let shadow_pl = pipeline_layout("forma3d_shadow_pipeline_layout", &shadow_layout);
        let ssao_pl = pipeline_layout("forma3d_ssao_pipeline_layout", &ssao_layout);
        let composite_pl = pipeline_layout("forma3d_composite_pipeline_layout", &composite_layout);

        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let mesh_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        };
        let instance_attrs = wgpu::vertex_attr_array![
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x4, 7 => Float32x4, 8 => Float32x4,
            9 => Float32x4, 10 => Float32x4
        ];
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attrs,
        };
        let grid_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let grid_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GridVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &grid_attrs,
        };
        let mesh_buffers = [Some(mesh_layout.clone()), Some(instance_layout.clone())];

        let depth = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias,
        };
        struct Desc<'a> {
            label: &'a str,
            layout: &'a wgpu::PipelineLayout,
            module: &'a wgpu::ShaderModule,
            vs: &'a str,
            fs: Option<&'a str>,
            buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
            topology: wgpu::PrimitiveTopology,
            cull: Option<wgpu::Face>,
            depth: Option<wgpu::DepthStencilState>,
            format: wgpu::TextureFormat,
            blend: Option<wgpu::BlendState>,
        }
        let make = |d: Desc| {
            let targets = [Some(wgpu::ColorTargetState {
                format: d.format,
                blend: d.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(d.label),
                layout: Some(d.layout),
                vertex: wgpu::VertexState {
                    module: d.module,
                    entry_point: Some(d.vs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: d.buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: d.topology,
                    // Outward-facing triangles come out clockwise on screen
                    // with this left-handed camera (checked in the app).
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: d.cull,
                    ..Default::default()
                },
                depth_stencil: d.depth,
                multisample: wgpu::MultisampleState::default(),
                fragment: d.fs.map(|fs| wgpu::FragmentState {
                    module: d.module,
                    entry_point: Some(fs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        // Push faces back a touch so coincident edges win the depth test.
        let solid_bias = wgpu::DepthBiasState {
            constant: 1,
            slope_scale: 1.0,
            clamp: 0.0,
        };
        let no_bias = wgpu::DepthBiasState::default();
        let solid_desc = |label, cull| Desc {
            label,
            layout: &layout,
            module: &shader,
            vs: "vs_solid",
            fs: Some("fs_solid"),
            buffers: &mesh_buffers,
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull,
            depth: Some(depth(true, wgpu::CompareFunction::Less, solid_bias)),
            format: target_format,
            blend: Some(wgpu::BlendState::REPLACE),
        };
        let solid = make(solid_desc("forma3d_solid", None));
        let solid_culled = make(solid_desc("forma3d_solid_culled", Some(wgpu::Face::Back)));
        let edge = make(Desc {
            label: "forma3d_edge",
            layout: &layout,
            module: &shader,
            vs: "vs_edge",
            fs: Some("fs_edge"),
            buffers: &mesh_buffers,
            topology: wgpu::PrimitiveTopology::LineList,
            cull: None,
            depth: Some(depth(false, wgpu::CompareFunction::LessEqual, no_bias)),
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        });
        let grid = make(Desc {
            label: "forma3d_grid",
            layout: &layout,
            module: &shader,
            vs: "vs_grid",
            fs: Some("fs_grid"),
            buffers: &[Some(grid_layout)],
            topology: wgpu::PrimitiveTopology::LineList,
            cull: None,
            depth: Some(depth(false, wgpu::CompareFunction::LessEqual, no_bias)),
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        });
        let catcher = make(Desc {
            label: "forma3d_shadow_catcher",
            layout: &layout,
            module: &shader,
            vs: "vs_catcher",
            fs: Some("fs_catcher"),
            buffers: &[],
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull: None,
            depth: Some(depth(false, wgpu::CompareFunction::LessEqual, no_bias)),
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        });
        // Writes the floor into the depth buffer only, so SSAO sees it.
        let floor_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("forma3d_floor_depth"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_catcher"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth(true, wgpu::CompareFunction::Less, no_bias)),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_floor_depth"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow = make(Desc {
            label: "forma3d_shadow",
            layout: &shadow_pl,
            module: &shader,
            vs: "vs_shadow",
            fs: None,
            buffers: &mesh_buffers,
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull: None,
            depth: Some(depth(
                true,
                wgpu::CompareFunction::Less,
                wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            )),
            format: target_format,
            blend: None,
        });
        let ssao = make(Desc {
            label: "forma3d_ssao",
            layout: &ssao_pl,
            module: &post_shader,
            vs: "vs_fullscreen",
            fs: Some("fs_ssao"),
            buffers: &[],
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull: None,
            depth: None,
            format: AO_FORMAT,
            blend: None,
        });
        // The offscreen image holds premultiplied colour over a transparent
        // background, so it lays over egui's gradient like any other layer.
        let composite = make(Desc {
            label: "forma3d_composite",
            layout: &composite_pl,
            module: &post_shader,
            vs: "vs_fullscreen",
            fs: Some("fs_composite"),
            buffers: &[],
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull: None,
            // egui's pass has a depth attachment; leave it untouched.
            depth: Some(depth(false, wgpu::CompareFunction::Always, no_bias)),
            format: target_format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("forma3d_shadow_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("forma3d_linear_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        GpuResources {
            solid,
            solid_culled,
            edge,
            grid,
            catcher,
            floor_depth,
            shadow,
            ssao,
            composite,
            camera_layout,
            shadow_layout,
            ssao_layout,
            composite_layout,
            shadow_sampler,
            linear_sampler,
            slots: Vec::new(),
            meshes: HashMap::new(),
            instances: buffer(
                device,
                "forma3d_instances",
                INITIAL_INSTANCES * std::mem::size_of::<InstanceRaw>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ),
            instance_capacity: INITIAL_INSTANCES,
            instance_count: 0,
            uploaded: 0,
            target_format,
            target_is_srgb: target_format.is_srgb(),
        }
    }

    fn ensure_slot(&mut self, device: &wgpu::Device, index: usize) {
        while self.slots.len() <= index {
            let camera_buffer = buffer(
                device,
                "forma3d_camera",
                std::mem::size_of::<CameraUniform>(),
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            );
            let shadow_view = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("forma3d_shadow_map"),
                    size: wgpu::Extent3d {
                        width: SHADOW_SIZE,
                        height: SHADOW_SIZE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("forma3d_camera_bind_group"),
                layout: &self.camera_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: camera_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    },
                ],
            });
            let shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("forma3d_shadow_bind_group"),
                layout: &self.shadow_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
            });
            let grid_buffer = buffer(
                device,
                "forma3d_grid",
                MAX_GRID_VERTICES * std::mem::size_of::<GridVertex>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            let post_buffer = buffer(
                device,
                "forma3d_post",
                std::mem::size_of::<PostUniform>(),
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            );
            self.slots.push(Slot {
                camera_buffer,
                bind_group,
                shadow_group,
                shadow_view,
                grid_buffer,
                grid_vertices: 0,
                post_buffer,
                offscreen: None,
                composite: false,
            });
        }
    }

    /// (Re)creates a view's offscreen images when its size changes.
    fn ensure_offscreen(&mut self, device: &wgpu::Device, index: usize, size: [u32; 2]) {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        if self.slots[index].offscreen.as_ref().is_some_and(|o| o.size == size) {
            return;
        }
        let color = texture_2d(device, "forma3d_view_color", size, self.target_format);
        let depth = texture_2d(device, "forma3d_view_depth", size, DEPTH_FORMAT);
        let ao = texture_2d(device, "forma3d_view_ao", size, AO_FORMAT);
        let post = &self.slots[index].post_buffer;
        let ssao_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("forma3d_ssao_bind_group"),
            layout: &self.ssao_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: post.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
            ],
        });
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("forma3d_composite_bind_group"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: post.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&ao),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        });
        self.slots[index].offscreen = Some(Offscreen {
            size,
            color,
            depth,
            ao,
            ssao_group,
            composite_group,
        });
    }

    fn upload_mesh(device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh) -> GpuMesh {
        let make = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
            let buf = buffer(device, label, bytes.len(), usage | wgpu::BufferUsages::COPY_DST);
            if !bytes.is_empty() {
                queue.write_buffer(&buf, 0, bytes);
            }
            buf
        };
        GpuMesh {
            vertices: make("mesh_vertices", bytemuck::cast_slice(&mesh.vertices), wgpu::BufferUsages::VERTEX),
            triangles: make("mesh_triangles", bytemuck::cast_slice(&mesh.indices), wgpu::BufferUsages::INDEX),
            edges: make("mesh_edges", bytemuck::cast_slice(&mesh.edges), wgpu::BufferUsages::INDEX),
            triangle_count: mesh.indices.len() as u32,
            edge_count: mesh.edges.len() as u32,
        }
    }

    fn prepare_scene(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[DrawItem], instances: &[InstanceRaw]) {
        let mut used = HashSet::new();
        for item in items {
            used.insert(item.mesh.id);
            self.meshes.entry(item.mesh.id).or_insert_with(|| Self::upload_mesh(device, queue, &item.mesh));
        }
        self.meshes.retain(|id, _| used.contains(id));

        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instances = buffer(
                device,
                "forma3d_instances",
                self.instance_capacity * std::mem::size_of::<InstanceRaw>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        }
        self.instance_count = instances.len();
    }

    /// Indices of the items whose triangles are drawn, with their meshes.
    fn surfaces<'a>(&'a self, scene: &'a FrameScene) -> impl Iterator<Item = (u32, &'a GpuMesh)> + 'a {
        let drawable = self.instance_count.min(scene.items.len());
        scene.items.iter().take(drawable).enumerate().filter_map(|(i, item)| {
            if item.kind != ItemKind::Surface {
                return None;
            }
            let mesh = self.meshes.get(&item.mesh.id)?;
            (mesh.triangle_count > 0).then_some((i as u32, mesh))
        })
    }

    fn draw_triangles<'a>(&'a self, pass: &mut wgpu::RenderPass<'_>, scene: &'a FrameScene) {
        pass.set_vertex_buffer(1, self.instances.slice(..));
        for (i, mesh) in self.surfaces(scene) {
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.triangles.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.triangle_count, 0, i..i + 1);
        }
    }

    /// Draws one view's scene: faces, the floor shadow, the grid, then edges.
    /// `floor_depth` puts the floor plane into the depth buffer (for SSAO).
    fn draw_scene(&self, pass: &mut wgpu::RenderPass<'_>, slot: &Slot, scene: &FrameScene, catcher: bool, floor_depth: bool) {
        // 1. Shaded faces.
        if scene.mode != DisplayMode::Wireframe {
            pass.set_pipeline(if scene.fx.backface_culling { &self.solid_culled } else { &self.solid });
            pass.set_bind_group(0, &slot.bind_group, &[]);
            self.draw_triangles(pass, scene);
        }
        if floor_depth && scene.mode != DisplayMode::Wireframe {
            pass.set_pipeline(&self.floor_depth);
            pass.set_bind_group(0, &slot.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        // 2. Shadows on the floor plane.
        if catcher && scene.mode != DisplayMode::Wireframe {
            pass.set_pipeline(&self.catcher);
            pass.set_bind_group(0, &slot.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        // 3. Grid, depth-tested so solids hide it.
        if slot.grid_vertices > 0 {
            pass.set_pipeline(&self.grid);
            pass.set_bind_group(0, &slot.bind_group, &[]);
            pass.set_vertex_buffer(0, slot.grid_buffer.slice(..));
            pass.draw(0..slot.grid_vertices, 0..1);
        }

        // 4. Edges.
        pass.set_pipeline(&self.edge);
        pass.set_bind_group(0, &slot.bind_group, &[]);
        pass.set_vertex_buffer(1, self.instances.slice(..));
        let drawable = self.instance_count.min(scene.items.len());
        for (i, item) in scene.items.iter().take(drawable).enumerate() {
            let show = match scene.mode {
                m if m.all_lines() => true,
                _ => item.selection != SelectionState::None || item.kind == ItemKind::LinesOnly,
            };
            if !show {
                continue;
            }
            let Some(mesh) = self.meshes.get(&item.mesh.id) else { continue };
            if mesh.edge_count == 0 {
                continue;
            }
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.edges.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.edge_count, 0, i as u32..i as u32 + 1);
        }
    }
}

// ---------------------------------------------------------------------------
// The paint callback
// ---------------------------------------------------------------------------

/// Everything the renderer needs for one frame, shared by all views.
pub struct FrameScene {
    pub items: Vec<DrawItem>,
    pub instances: Vec<InstanceRaw>,
    pub mode: DisplayMode,
    pub fx: ViewEffects,
    /// The scene's lights (empty: the default camera light).
    pub lights: crate::light::SceneLights,
    /// World-space bounds of every shaded surface, if there are any.
    pub bounds: Option<(Vec3, Vec3)>,
    /// Unique per `FrameScene`, so the GPU upload happens once per frame.
    serial: u64,
}

static NEXT_FRAME_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl FrameScene {
    pub fn new(items: Vec<DrawItem>, mode: DisplayMode, fx: ViewEffects) -> Self {
        let wire = mode == DisplayMode::Wireframe;
        let instances = items.iter().map(|i| InstanceRaw::new(i, wire)).collect();
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for item in items.iter().filter(|i| i.kind == ItemKind::Surface && !i.mesh.indices.is_empty()) {
            let (lo, hi) = (item.mesh.bounds_min, item.mesh.bounds_max);
            for k in 0..8 {
                let corner = Vec3::new(
                    if k & 1 == 0 { lo.x } else { hi.x },
                    if k & 2 == 0 { lo.y } else { hi.y },
                    if k & 4 == 0 { lo.z } else { hi.z },
                );
                let p = item.model.transform_point3(corner);
                if !p.is_finite() {
                    continue;
                }
                bounds = Some(match bounds {
                    Some((a, b)) => (a.min(p), b.max(p)),
                    None => (p, p),
                });
            }
        }
        FrameScene {
            items,
            instances,
            mode,
            fx,
            bounds,
            lights: crate::light::SceneLights::default(),
            serial: NEXT_FRAME_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// One view's drawing for one frame.
pub struct ViewDraw {
    pub slot: usize,
    pub camera: CameraUniform,
    pub post: PostUniform,
    /// The view's size in physical pixels (for the offscreen images).
    pub size_px: [u32; 2],
    pub grid: Vec<GridVertex>,
    pub scene: Arc<FrameScene>,
}

impl ViewDraw {
    /// Uploads this view's data and records its offscreen passes.
    pub fn prepare(&self, gpu: &mut GpuResources, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) {
        gpu.ensure_slot(device, self.slot);
        let grid_len = self.grid.len().min(MAX_GRID_VERTICES);
        {
            let slot = &mut gpu.slots[self.slot];
            queue.write_buffer(&slot.camera_buffer, 0, bytemuck::bytes_of(&self.camera));
            queue.write_buffer(&slot.post_buffer, 0, bytemuck::bytes_of(&self.post));
            if grid_len > 0 {
                queue.write_buffer(&slot.grid_buffer, 0, bytemuck::cast_slice(&self.grid[..grid_len]));
            }
            slot.grid_vertices = grid_len as u32;
        }
        if gpu.uploaded != self.scene.serial {
            gpu.prepare_scene(device, queue, &self.scene.items, &self.scene.instances);
            gpu.uploaded = self.scene.serial;
        }
        let scene = &*self.scene;
        let fx = scene.fx;
        let shadows = self.camera.shadows_on();

        // Shadow map from the key light.
        if shadows {
            let slot = &gpu.slots[self.slot];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("forma3d_shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &slot.shadow_view,
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
            if scene.mode != DisplayMode::Wireframe {
                pass.set_pipeline(&gpu.shadow);
                pass.set_bind_group(0, &slot.shadow_group, &[]);
                gpu.draw_triangles(&mut pass, scene);
            }
        }

        // SSAO and post effects work on an offscreen copy of the view.
        let offscreen = fx.offscreen();
        gpu.slots[self.slot].composite = offscreen;
        if offscreen {
            gpu.ensure_offscreen(device, self.slot, self.size_px);
            let slot = &gpu.slots[self.slot];
            let Some(off) = slot.offscreen.as_ref() else {
                return;
            };
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("forma3d_view_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &off.color,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &off.depth,
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
                gpu.draw_scene(&mut pass, slot, scene, self.camera.catcher_on(), fx.ssao && self.camera.floor_on());
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("forma3d_ssao_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &off.ao,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if fx.ssao {
                    pass.set_pipeline(&gpu.ssao);
                    pass.set_bind_group(0, &off.ssao_group, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
    }

    /// Draws the view into `pass` (colour + `DEPTH_FORMAT` depth).
    pub fn paint(&self, gpu: &GpuResources, pass: &mut wgpu::RenderPass<'_>) {
        let Some(slot) = gpu.slots.get(self.slot) else { return };
        match slot.offscreen.as_ref() {
            Some(off) if slot.composite => {
                pass.set_pipeline(&gpu.composite);
                pass.set_bind_group(0, &off.composite_group, &[]);
                pass.draw(0..3, 0..1);
            }
            _ => gpu.draw_scene(pass, slot, &self.scene, self.camera.catcher_on(), false),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn shading_modes_keep_their_ids_and_line_rules() {
        let ids: Vec<&str> = DisplayMode::ALL.iter().map(|m| m.id()).collect();
        assert_eq!(ids, ["gouraud_shading", "gouraud_shading_(lines)", "wireframe", "quick_shading", "quick_shading_(lines)"]);
        assert_eq!(DisplayMode::Wireframe.label(), "Lines");
        let lines: Vec<bool> = DisplayMode::ALL.iter().map(|m| m.all_lines()).collect();
        assert_eq!(lines, [false, true, true, false, true]);
        // Quick shading turns off the key light's effects.
        let cam = Camera::new(crate::camera::ViewKind::Perspective);
        let mut u = CameraUniform::new(&cam, 1.5, false, Vec3::ZERO, 100.0);
        let fx = ViewEffects { shadows: true, reflections: true, ..Default::default() };
        u.set_effects(fx, Some((Vec3::splat(-50.0), Vec3::splat(50.0))), true);
        assert!(u.shadows_on());
        u.set_shading(DisplayMode::Quick);
        assert!(!u.shadows_on() && !u.catcher_on() && u.params[1] == 1.0);
    }

    use super::*;

    #[test]
    fn pod_layouts_match_the_shader() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 64 + 5 * 16 + 64 + 16 + 2 * 16 + 8 * 64);
        assert_eq!(std::mem::size_of::<PostUniform>(), 2 * 64 + 3 * 16);
        assert_eq!(std::mem::size_of::<InstanceRaw>(), 64 + 48 + 16 + 16);
        assert_eq!(std::mem::size_of::<GridVertex>(), 28);
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
        // Uniform buffers need 16-byte multiples.
        assert_eq!(std::mem::size_of::<CameraUniform>() % 16, 0);
    }

    #[test]
    fn grid_fits_its_buffer_in_every_view() {
        for kind in ViewKind::ALL {
            for scale in [1.0f32, 30.0, 600.0, 90_000.0] {
                let mut cam = Camera::new(kind);
                cam.distance = scale;
                cam.ortho_half_height = scale;
                let grid = build_grid(&cam);
                let verts = &grid.vertices;
                assert!(verts.len() <= MAX_GRID_VERTICES, "{kind:?} {scale}: {}", verts.len());
                assert_eq!(verts.len() % 2, 0);
                assert!(grid.radius.is_finite() && grid.radius > 0.0);
                assert!(grid.unit >= 1.0);
                assert!(verts.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
            }
        }
    }

    #[test]
    fn grid_lies_in_the_expected_plane() {
        let check = |kind: ViewKind, fixed: usize| {
            let grid = build_grid(&Camera::new(kind));
            assert!(grid.vertices.iter().all(|v| v.pos[fixed] == 0.0), "{kind:?}");
        };
        check(ViewKind::Perspective, 1); // y = 0
        check(ViewKind::Top, 1);
        check(ViewKind::Front, 2); // z = 0
        check(ViewKind::Right, 0); // x = 0
    }

    #[test]
    fn grid_spacing_follows_zoom() {
        let mut near = Camera::new(ViewKind::Perspective);
        near.distance = 80.0;
        let mut far = Camera::new(ViewKind::Perspective);
        far.distance = 8000.0;
        let spacing = |cam: &Camera| {
            let g = build_grid(cam);
            let v = &g.vertices;
            // Distance between the first two parallel lines (each line is
            // 24 pieces of two vertices).
            let next = 24 * 2;
            let measured = (v[next].pos[0] - v[0].pos[0]).abs().max((v[next].pos[2] - v[0].pos[2]).abs());
            assert!((measured - g.unit).abs() < 1e-3, "reported unit must match the geometry");
            measured
        };
        assert!(spacing(&far) > spacing(&near) * 10.0);
    }

    #[test]
    fn instance_normal_matrix_handles_singular_models() {
        let item = DrawItem {
            mesh: Arc::new(crate::mesh::cube(Vec3::ONE, [1, 1, 1])),
            model: Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0)),
            color: [1.0; 3],
            selection: SelectionState::Active,
            kind: ItemKind::Surface,
        };
        let inst = InstanceRaw::new(&item, false);
        assert!(inst.normal.iter().flatten().all(|c| c.is_finite()));
        assert_eq!(inst.flags[0], 2.0);
    }
}
