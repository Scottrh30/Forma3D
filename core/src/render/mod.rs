//! The final renderer: Render Settings, and rendering a scene to images.
//!
//! Two engines trace rays through a BVH of the scene's triangles:
//! - **Standard**: direct light from every light with hard or soft shadows,
//!   mirror-like reflections and ambient light (optionally with ambient
//!   occlusion). Fast and clean.
//! - **Physical**: path tracing: everything Standard does plus light
//!   bouncing between surfaces (global illumination), area-light soft
//!   shadows and depth of field.
//!
//! The third engine, **Viewport**, is the hardware (GPU) viewport renderer;
//! hosts render it with `renderer::ViewDraw` into an image.
//!
//! Rendering is progressive: each pass adds one sample to every pixel, so a
//! picture appears quickly and refines. `FrameJob` drives the passes, and
//! `FrameJob::finish` applies the effects (Sketch and Toon, Contour) and
//! colour management and returns the images of every pass.

mod bvh;
mod build;
mod encode;
mod post;
mod trace;

pub use build::{RenderScene, build_scene};
pub use encode::{encode, encode_png8};
pub use post::{Image, view_transform};
pub use trace::Renderer;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Engine {
    #[default]
    Standard,
    Physical,
    /// The viewport's own (GPU) renderer.
    Viewport,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Standard, Engine::Physical, Engine::Viewport];
    pub fn label(self) -> &'static str {
        match self {
            Engine::Standard => "Standard",
            Engine::Physical => "Physical",
            Engine::Viewport => "Viewport (Hardware)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameRange {
    #[default]
    CurrentFrame,
    AllFrames,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageFormat {
    #[default]
    Png,
    /// 16 bits per channel.
    Png16,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png | ImageFormat::Png16 => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

/// The images a render makes (Multi-Pass). Beauty is always made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pass {
    Beauty,
    Diffuse,
    Specular,
    Shadow,
    AmbientOcclusion,
    Depth,
    Normal,
    ObjectId,
    MaterialId,
    Alpha,
}

impl Pass {
    pub const ALL: [Pass; 10] =
        [Pass::Beauty, Pass::Diffuse, Pass::Specular, Pass::Shadow, Pass::AmbientOcclusion, Pass::Depth, Pass::Normal, Pass::ObjectId, Pass::MaterialId, Pass::Alpha];

    pub fn label(self) -> &'static str {
        match self {
            Pass::Beauty => "Beauty",
            Pass::Diffuse => "Diffuse",
            Pass::Specular => "Specular",
            Pass::Shadow => "Shadow",
            Pass::AmbientOcclusion => "Ambient Occlusion",
            Pass::Depth => "Depth",
            Pass::Normal => "Normal",
            Pass::ObjectId => "Object ID",
            Pass::MaterialId => "Material ID",
            Pass::Alpha => "Alpha",
        }
    }

    /// For file names ($pass).
    pub fn id(self) -> &'static str {
        match self {
            Pass::Beauty => "beauty",
            Pass::Diffuse => "diffuse",
            Pass::Specular => "specular",
            Pass::Shadow => "shadow",
            Pass::AmbientOcclusion => "ao",
            Pass::Depth => "depth",
            Pass::Normal => "normal",
            Pass::ObjectId => "objectid",
            Pass::MaterialId => "materialid",
            Pass::Alpha => "alpha",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Filter {
    Box,
    #[default]
    Gaussian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SketchMode {
    #[default]
    Off,
    /// Cartoon shading in a few bands, with outlines.
    SketchAndToon,
    /// Outlines over the normal render.
    Contour,
}

/// How the rendered light becomes display colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewTransform {
    /// Plain sRGB: bright parts clip.
    #[default]
    Standard,
    /// The ACES film look (RRT + sRGB output, fitted curve).
    AcesSdr,
    /// A soft filmic curve.
    Filmic,
    /// The linear values, unchanged (for compositing).
    Raw,
}

impl ViewTransform {
    pub const ALL: [ViewTransform; 4] = [ViewTransform::Standard, ViewTransform::AcesSdr, ViewTransform::Filmic, ViewTransform::Raw];
}

/// The colour space lighting is computed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkingSpace {
    /// Linear sRGB (Rec. 709 primaries).
    #[default]
    LinearSrgb,
    /// ACEScg (AP1 primaries): wider gamut, more natural colour bleeding.
    AcesCg,
}

/// Render Settings (Render > Edit Render Settings), saved with the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    pub engine: Engine,
    // Output
    pub width: u32,
    pub height: u32,
    pub frame_range: FrameRange,
    pub frame_from: i32,
    pub frame_to: i32,
    pub frame_step: i32,
    // Save
    pub save: bool,
    /// File path with render tokens ($prj, $camera, $take, $frame, $pass,
    /// $res, $width, $height, $renderer, $date, $time). Relative paths go
    /// next to the project file (or to the Pictures folder).
    pub path: String,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
    pub alpha: bool,
    // Multi-Pass
    pub passes: Vec<Pass>,
    pub save_passes: bool,
    // Anti-aliasing
    /// Samples per pixel.
    pub samples: u32,
    pub filter: Filter,
    // Options
    pub shadows: bool,
    pub reflections: bool,
    /// Ray depth for reflections and bounces.
    pub max_depth: u32,
    /// Physical: light bounces between surfaces.
    pub global_illumination: bool,
    /// Behind everything when there's no sky (linear colour).
    pub background: [f32; 3],
    // Effects
    pub ambient_occlusion: bool,
    pub ao_distance: f32,
    pub ao_strength: f32,
    pub sketch: SketchMode,
    pub line_width: f32,
    pub line_color: [f32; 3],
    pub toon_steps: u32,
    // Material Override
    pub material_override: bool,
    pub override_color: [f32; 3],
    // Colour management
    pub view_transform: ViewTransform,
    pub working_space: WorkingSpace,
    /// Exposure in stops.
    pub exposure: f32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings {
            engine: Engine::Standard,
            width: 1280,
            height: 720,
            frame_range: FrameRange::CurrentFrame,
            frame_from: 0,
            frame_to: 90,
            frame_step: 1,
            save: false,
            path: "$prj_$camera_$frame".to_owned(),
            format: ImageFormat::Png,
            jpeg_quality: 90,
            alpha: true,
            passes: Vec::new(),
            save_passes: false,
            samples: 16,
            filter: Filter::Gaussian,
            shadows: true,
            reflections: true,
            max_depth: 4,
            global_illumination: true,
            background: [0.0; 3],
            ambient_occlusion: false,
            ao_distance: 100.0,
            ao_strength: 100.0,
            sketch: SketchMode::Off,
            line_width: 1.5,
            line_color: [0.0; 3],
            toon_steps: 3,
            material_override: false,
            override_color: [0.8, 0.8, 0.8],
            view_transform: ViewTransform::Standard,
            working_space: WorkingSpace::LinearSrgb,
            exposure: 0.0,
        }
    }
}

impl RenderSettings {
    pub fn sanitized(&self) -> RenderSettings {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let mut passes: Vec<Pass> = Vec::new();
        for p in &self.passes {
            if *p != Pass::Beauty && !passes.contains(p) {
                passes.push(*p);
            }
        }
        RenderSettings {
            width: self.width.clamp(8, 8192),
            height: self.height.clamp(8, 8192),
            frame_step: self.frame_step.clamp(1, 1000),
            jpeg_quality: self.jpeg_quality.clamp(1, 100),
            passes,
            samples: self.samples.clamp(1, 4096),
            max_depth: self.max_depth.clamp(1, 16),
            background: self.background.map(|c| f(c, 0.0, 100.0, 0.0)),
            ao_distance: f(self.ao_distance, 0.1, 1e6, 100.0),
            ao_strength: f(self.ao_strength, 0.0, 100.0, 100.0),
            line_width: f(self.line_width, 0.5, 20.0, 1.5),
            line_color: self.line_color.map(|c| f(c, 0.0, 1.0, 0.0)),
            toon_steps: self.toon_steps.clamp(1, 16),
            override_color: self.override_color.map(|c| f(c, 0.0, 1.0, 0.8)),
            exposure: f(self.exposure, -16.0, 16.0, 0.0),
            path: if self.path.trim().is_empty() { "$prj_$frame".to_owned() } else { self.path.clone() },
            ..self.clone()
        }
    }

    /// The frames to render, given the timeline's current frame and range.
    pub fn frames(&self, current: i32, timeline: (i32, i32)) -> Vec<i32> {
        let s = self.sanitized();
        let (a, b) = match s.frame_range {
            FrameRange::CurrentFrame => return vec![current],
            FrameRange::AllFrames => timeline,
            FrameRange::Manual => (s.frame_from, s.frame_to),
        };
        let (a, b) = (a.min(b), a.max(b));
        (a..=b).step_by(s.frame_step as usize).take(100_000).collect()
    }

    /// Every pass the render makes, Beauty first.
    pub fn all_passes(&self) -> Vec<Pass> {
        let mut v = vec![Pass::Beauty];
        v.extend(self.sanitized().passes);
        v
    }
}

/// What render tokens stand for in a file name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenContext {
    pub project: String,
    pub camera: String,
    pub take: String,
    pub frame: i32,
    pub pass: String,
    pub width: u32,
    pub height: u32,
    pub renderer: String,
    /// YYYYMMDD and HHMMSS, from the host's clock.
    pub date: String,
    pub time: String,
}

/// Replaces render tokens ($prj, $camera, $take, $frame, $pass, $res,
/// $width, $height, $renderer, $date, $time) and makes the result safe as a
/// file name (folders are kept).
pub fn resolve_tokens(path: &str, ctx: &TokenContext) -> String {
    let clean = |s: &str| -> String {
        let s: String = s.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' }).collect();
        let s = s.trim().trim_end_matches(".json").trim_end_matches(".forma3d").to_owned();
        if s.is_empty() { "untitled".to_owned() } else { s }
    };
    let pairs = [
        ("$renderer", clean(&ctx.renderer)),
        ("$camera", clean(&ctx.camera)),
        ("$height", ctx.height.to_string()),
        ("$width", ctx.width.to_string()),
        ("$frame", format!("{:04}", ctx.frame)),
        ("$pass", clean(&ctx.pass)),
        ("$take", clean(&ctx.take)),
        ("$date", ctx.date.clone()),
        ("$time", ctx.time.clone()),
        ("$prj", clean(&ctx.project)),
        ("$res", format!("{}x{}", ctx.width, ctx.height)),
    ];
    let mut out = path.to_owned();
    for (k, v) in pairs {
        out = out.replace(k, &v);
    }
    out.chars().map(|c| if matches!(c, '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect()
}

/// One rendered frame: every pass as an 8-bit RGBA image (display ready),
/// plus 16-bit Beauty when asked for.
#[derive(Clone, Debug)]
pub struct FrameImages {
    pub width: u32,
    pub height: u32,
    pub frame: i32,
    pub passes: Vec<(Pass, Image)>,
    pub beauty16: Option<Vec<u16>>,
}

/// Renders one frame, progressively: one renderer per eye (two for stereo).
pub struct FrameJob {
    pub eyes: Vec<Renderer>,
    pub settings: RenderSettings,
    pub frame: i32,
    stereo: Option<crate::scene_camera::StereoMode>,
}

impl FrameJob {
    /// Prepares a frame. `fallback` is the camera when the scene has no
    /// render camera (the active view's); `region` limits rendering to a
    /// rectangle (x0, y0, x1, y1) of the image.
    pub fn new(
        ed: &mut crate::editor::Editor,
        settings: &RenderSettings,
        frame: i32,
        size: (u32, u32),
        fallback: &crate::scene_camera::CameraPose,
        region: Option<[u32; 4]>,
    ) -> FrameJob {
        let settings = settings.sanitized();
        let scene = std::sync::Arc::new(build_scene(ed, &settings, frame, size, fallback));
        let stereo = scene.camera.stereo.map(|s| s.1);
        let sides: Vec<f32> = match stereo {
            None => vec![0.0],
            Some(crate::scene_camera::StereoMode::LeftEye) => vec![-1.0],
            Some(crate::scene_camera::StereoMode::RightEye) => vec![1.0],
            Some(_) => vec![-1.0, 1.0],
        };
        let eyes = sides.into_iter().map(|side| Renderer::new(scene.clone(), &settings, size.0, size.1, region, side)).collect();
        FrameJob { eyes, settings, frame, stereo }
    }

    pub fn total_passes(&self) -> u32 {
        self.eyes.iter().map(|e| e.total_passes()).sum()
    }

    pub fn passes_done(&self) -> u32 {
        self.eyes.iter().map(|e| e.passes_done()).sum()
    }

    pub fn done(&self) -> bool {
        self.eyes.iter().all(|e| e.done())
    }

    /// 0..1
    pub fn progress(&self) -> f32 {
        let total: f32 = self.eyes.iter().map(|e| e.total_work()).sum();
        let done: f32 = self.eyes.iter().map(|e| e.work_done()).sum();
        if total <= 0.0 { 1.0 } else { (done / total).clamp(0.0, 1.0) }
    }

    /// Renders up to `rows` image rows; returns true when the frame is done.
    pub fn step(&mut self, rows: u32) -> bool {
        if let Some(e) = self.eyes.iter_mut().find(|e| !e.done()) {
            e.step(rows);
        }
        self.done()
    }

    /// Renders one whole pass (on every core with the `parallel` feature).
    pub fn pass(&mut self) -> bool {
        if let Some(e) = self.eyes.iter_mut().find(|e| !e.done()) {
            e.render_pass();
        }
        self.done()
    }

    /// The images so far (finished or not), with effects and colour.
    pub fn images(&self) -> FrameImages {
        let s = &self.settings;
        let passes = s.all_passes();
        let per_eye: Vec<Vec<(Pass, Image)>> = self.eyes.iter().map(|e| passes.iter().map(|&p| (p, post::pass_image(e, p, s))).collect()).collect();
        let (width, height, combined) = match (self.stereo, per_eye.len()) {
            (Some(crate::scene_camera::StereoMode::Anaglyph), 2) => {
                let out = per_eye[0].iter().zip(&per_eye[1]).map(|((p, l), (_, r))| (*p, post::anaglyph(l, r))).collect();
                (per_eye[0][0].1.width, per_eye[0][0].1.height, out)
            }
            (Some(crate::scene_camera::StereoMode::SideBySide), 2) => {
                let out: Vec<(Pass, Image)> = per_eye[0].iter().zip(&per_eye[1]).map(|((p, l), (_, r))| (*p, post::side_by_side(l, r))).collect();
                (out[0].1.width, out[0].1.height, out)
            }
            _ => {
                let one = per_eye.into_iter().next().unwrap_or_default();
                (one.first().map_or(0, |x| x.1.width), one.first().map_or(0, |x| x.1.height), one)
            }
        };
        let beauty16 = (s.format == ImageFormat::Png16 && self.eyes.len() == 1).then(|| post::beauty16(&self.eyes[0], s));
        FrameImages { width, height, frame: self.frame, passes: combined, beauty16 }
    }
}

/// Everything a render needs, self-contained (it can be sent to another
/// thread or process): the scene, which frames, the settings and names.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderRequest {
    pub scene: crate::scene::Scene,
    pub fps: f32,
    pub frames: Vec<i32>,
    pub settings: RenderSettings,
    /// The camera when the scene has no render camera (the view's).
    pub fallback: crate::scene_camera::CameraPose,
    /// Only this rectangle of the image (x0, y0, x1, y1).
    pub region: Option<[u32; 4]>,
    /// For file names.
    pub project: String,
    pub take: String,
}

impl RenderRequest {
    /// Prepares one frame (on a private copy of the scene).
    pub fn job(&self, frame: i32) -> FrameJob {
        let mut ed = crate::editor::Editor::new(self.scene.clone());
        ed.timeline.fps = self.fps.max(1.0);
        let s = self.settings.sanitized();
        FrameJob::new(&mut ed, &s, frame, (s.width, s.height), &self.fallback, self.region)
    }

    /// The name of the render camera ("Editor Camera" without one).
    pub fn camera_name(&self) -> String {
        self.scene.render_camera.and_then(|id| self.scene.get(id)).map_or_else(|| "Editor Camera".to_owned(), |o| o.name.clone())
    }

    /// The file a pass of a frame is saved to (relative to the project's
    /// folder unless absolute), with its extension. `date` and `time` are
    /// YYYYMMDD and HHMMSS.
    pub fn file_name(&self, frame: i32, pass: Pass, date: &str, time: &str) -> String {
        let s = self.settings.sanitized();
        let ctx = TokenContext {
            project: self.project.clone(),
            camera: self.camera_name(),
            take: self.take.clone(),
            frame,
            pass: pass.id().to_owned(),
            width: s.width,
            height: s.height,
            renderer: s.engine.label().split(' ').next().unwrap_or("").to_owned(),
            date: date.to_owned(),
            time: time.to_owned(),
        };
        let mut name = resolve_tokens(&s.path, &ctx);
        // Several passes need different names even when $pass isn't used.
        if pass != Pass::Beauty && !s.path.contains("$pass") {
            name.push('_');
            name.push_str(pass.id());
        }
        format!("{name}.{}", s.format.extension())
    }

    /// The files of one rendered frame: (file name, bytes).
    pub fn files(&self, images: &FrameImages, date: &str, time: &str) -> Vec<(String, Result<Vec<u8>, String>)> {
        let s = self.settings.sanitized();
        images
            .passes
            .iter()
            .filter(|(p, _)| *p == Pass::Beauty || s.save_passes)
            .map(|(p, img)| {
                let b16 = if *p == Pass::Beauty { images.beauty16.as_deref() } else { None };
                (self.file_name(images.frame, *p, date, time), encode(img, s.format, s.jpeg_quality, b16))
            })
            .collect()
    }
}

/// UTC date and time (YYYYMMDD, HHMMSS) from seconds since 1970.
pub fn date_time(unix_secs: u64) -> (String, String) {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (format!("{y:04}{m:02}{d:02}"), format!("{:02}{:02}{:02}", secs / 3600, secs / 60 % 60, secs % 60))
}

#[cfg(test)]
mod tests;
