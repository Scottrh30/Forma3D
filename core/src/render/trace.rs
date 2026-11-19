//! The ray tracer: one sample per pixel per pass, rows at a time.

use super::build::{RLight, RLightKind, RenderScene, from_working, to_working};
use super::{Engine, RenderSettings, SamplingPlan, SketchMode};
use crate::scene_camera::CameraPose;
use glam::Vec3;
use std::f32::consts::TAU;
use std::sync::Arc;

/// A small fast random generator (PCG).
#[derive(Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03);
        r.next();
        r
    }

    pub fn next(&mut self) -> u32 {
        let old = self.0;
        self.0 = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let xs = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xs.rotate_right(rot)
    }

    /// Uniform in 0..1.
    pub fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn basis(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
    let t = n.cross(a).normalize();
    (t, n.cross(t))
}

fn cosine_dir(n: Vec3, rng: &mut Rng) -> Vec3 {
    let (t, b) = basis(n);
    let r = rng.f().sqrt();
    let a = TAU * rng.f();
    (t * (r * a.cos()) + b * (r * a.sin()) + n * (1.0 - r * r).max(0.0).sqrt()).normalize_or(n)
}

fn disk(rng: &mut Rng) -> (f32, f32) {
    let r = rng.f().sqrt();
    let a = TAU * rng.f();
    (r * a.cos(), r * a.sin())
}

/// What one camera sample sees, for every pass.
#[derive(Clone, Copy, Default)]
pub(crate) struct Sample {
    pub beauty: Vec3,
    pub diffuse: Vec3,
    pub specular: Vec3,
    /// How much light shadows took away (0..1).
    pub shadow: f32,
    /// 1 = open, 0 = fully occluded.
    pub ao: f32,
    pub alpha: f32,
    pub depth: f32,
    pub normal: Vec3,
    pub object: i32,
    pub material: i32,
    /// Where in its pixel the sample was taken, from the centre (-0.5..0.5).
    pub offset: (f32, f32),
    /// Adaptive sampling left this pixel out of the pass.
    pub skip: bool,
}

/// The sums of every pixel's samples, weighted by the filter.
#[derive(Clone, Default)]
pub struct Buffers {
    pub beauty: Vec<Vec3>,
    pub diffuse: Vec<Vec3>,
    pub specular: Vec<Vec3>,
    pub shadow: Vec<f32>,
    pub ao: Vec<f32>,
    pub alpha: Vec<f32>,
    pub normal: Vec<Vec3>,
    pub weight: Vec<f32>,
    /// From each pixel's first (centre) sample.
    pub depth: Vec<f32>,
    pub object: Vec<i32>,
    pub material: Vec<i32>,
    /// Each pixel's own samples: count, and the sum and sum of squares of
    /// their brightness (for adaptive sampling).
    pub count: Vec<u32>,
    pub lum: Vec<f32>,
    pub lum2: Vec<f32>,
}

impl Buffers {
    fn new(n: usize) -> Buffers {
        Buffers {
            beauty: vec![Vec3::ZERO; n],
            diffuse: vec![Vec3::ZERO; n],
            specular: vec![Vec3::ZERO; n],
            shadow: vec![0.0; n],
            ao: vec![0.0; n],
            alpha: vec![0.0; n],
            normal: vec![Vec3::ZERO; n],
            weight: vec![0.0; n],
            depth: vec![f32::INFINITY; n],
            object: vec![-1; n],
            material: vec![-1; n],
            count: vec![0; n],
            lum: vec![0.0; n],
            lum2: vec![0.0; n],
        }
    }
}

/// Where a ray is in its path: how deep, through how many reflections and
/// diffuse bounces, and how much it can still add to the picture.
#[derive(Clone, Copy)]
struct Path {
    depth: u32,
    reflections: u32,
    bounces: u32,
    weight: f32,
}

impl Path {
    const CAMERA: Path = Path { depth: 0, reflections: 0, bounces: 0, weight: 1.0 };

    fn next(self, weight: f32) -> Path {
        Path { depth: self.depth + 1, weight: self.weight * weight, ..self }
    }
}

pub struct Renderer {
    pub scene: Arc<RenderScene>,
    pub settings: RenderSettings,
    pub width: u32,
    pub height: u32,
    /// x0, y0, x1, y1 (exclusive).
    pub region: [u32; 4],
    camera: CameraPose,
    pass: u32,
    row: u32,
    pub buffers: Buffers,
    plan: SamplingPlan,
    /// Adaptive: the pixels this pass samples (None: all of them).
    active: Option<Vec<bool>>,
    /// Adaptive sampling found nothing left to refine.
    converged: bool,
    /// Time spent so far (set by the host), for a Progressive time limit.
    elapsed_ms: f64,
}

impl Renderer {
    pub fn new(scene: Arc<RenderScene>, settings: &RenderSettings, width: u32, height: u32, region: Option<[u32; 4]>, side: f32) -> Renderer {
        let (width, height) = (width.max(1), height.max(1));
        let region = match region {
            Some([x0, y0, x1, y1]) => {
                let (x0, x1) = (x0.min(x1).min(width - 1), x0.max(x1).min(width).max(1));
                let (y0, y1) = (y0.min(y1).min(height - 1), y0.max(y1).min(height).max(1));
                [x0, y0, x1.max(x0 + 1), y1.max(y0 + 1)]
            }
            None => [0, 0, width, height],
        };
        let camera = if side == 0.0 { scene.camera } else { scene.camera.eye_offset(side) };
        let settings = settings.sanitized().with_quality();
        Renderer {
            scene,
            plan: settings.plan(),
            settings,
            width,
            height,
            region,
            camera,
            pass: 0,
            row: 0,
            buffers: Buffers::new((width * height) as usize),
            active: None,
            converged: false,
            elapsed_ms: 0.0,
        }
    }

    /// The most passes this render may take.
    pub fn total_passes(&self) -> u32 {
        self.plan.max
    }

    pub fn passes_done(&self) -> u32 {
        self.pass.min(self.total_passes())
    }

    pub fn plan(&self) -> SamplingPlan {
        self.plan
    }

    /// Tells the renderer how long it has been running (for time limits).
    pub fn set_elapsed_ms(&mut self, ms: f64) {
        self.elapsed_ms = ms;
    }

    pub fn done(&self) -> bool {
        if self.pass >= self.plan.max || self.converged {
            return true;
        }
        matches!(self.plan.time_limit_ms, Some(limit) if self.pass >= 1 && self.elapsed_ms >= limit)
    }

    fn rows(&self) -> u32 {
        self.region[3] - self.region[1]
    }

    pub fn total_work(&self) -> f32 {
        self.total_passes() as f32 * self.rows() as f32
    }

    /// How far along the render is, 0..1.
    pub fn fraction_done(&self) -> f32 {
        if self.done() {
            return 1.0;
        }
        let rows = self.rows().max(1) as f32;
        let passes = self.pass as f32 + self.row as f32 / rows;
        match (self.plan.time_limit_ms, self.plan.open_ended) {
            (Some(limit), _) => (self.elapsed_ms / limit.max(1.0)) as f32,
            // Never finishes by itself: creep towards the end.
            (None, true) => passes / (passes + 16.0),
            // Adaptive renders usually stop well before the maximum.
            _ => passes / self.plan.max.max(1) as f32,
        }
        .clamp(0.0, 0.999)
    }

    pub fn work_done(&self) -> f32 {
        self.fraction_done() * self.total_work()
    }

    /// Starts a pass: adaptive sampling picks the pixels that still need
    /// samples (and stops when none do).
    fn begin_pass(&mut self) {
        self.active = None;
        let Some(threshold) = self.plan.threshold else {
            return;
        };
        if self.pass < self.plan.min || self.pass >= self.plan.max {
            return;
        }
        let b = &self.buffers;
        let mut any = false;
        let mut active = vec![false; (self.width * self.height) as usize];
        for y in self.region[1]..self.region[3] {
            for x in self.region[0]..self.region[2] {
                let i = (y * self.width + x) as usize;
                let n = b.count[i] as f32;
                if n < 2.0 {
                    active[i] = true;
                    any = true;
                    continue;
                }
                let mean = b.lum[i] / n;
                let var = (b.lum2[i] / n - mean * mean).max(0.0);
                // The error of the mean, relative to how bright the pixel looks.
                let err = (var / n).sqrt() / (mean + 0.1);
                if err > threshold {
                    active[i] = true;
                    any = true;
                }
            }
        }
        if any {
            // Refine around noisy pixels too, so edges come out evenly.
            let mut grown = active.clone();
            for y in self.region[1]..self.region[3] {
                for x in self.region[0]..self.region[2] {
                    let i = (y * self.width + x) as usize;
                    if !active[i] {
                        continue;
                    }
                    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= self.region[0] as i32 && ny >= self.region[1] as i32 && nx < self.region[2] as i32 && ny < self.region[3] as i32 {
                            grown[(ny as u32 * self.width + nx as u32) as usize] = true;
                        }
                    }
                }
            }
            self.active = Some(grown);
        } else {
            self.converged = true;
        }
    }

    /// Renders up to `rows` rows of the current pass.
    pub fn step(&mut self, rows: u32) {
        for _ in 0..rows.max(1) {
            if self.row == 0 {
                self.begin_pass();
            }
            if self.done() {
                return;
            }
            let y = self.region[1] + self.row;
            let out = self.render_row(y, self.pass);
            self.store(y, &out);
            self.advance(1);
        }
    }

    fn advance(&mut self, rows: u32) {
        self.row += rows;
        if self.row >= self.rows() {
            self.row = 0;
            self.pass += 1;
        }
    }

    /// Renders the rest of the current pass (on every core with the
    /// `parallel` feature).
    pub fn render_pass(&mut self) {
        if self.row == 0 {
            self.begin_pass();
        }
        if self.done() {
            return;
        }
        let (from, to) = (self.region[1] + self.row, self.region[3]);
        let pass = self.pass;
        #[cfg(feature = "parallel")]
        let rows: Vec<(u32, Vec<Sample>)> = {
            use rayon::prelude::*;
            (from..to).into_par_iter().map(|y| (y, self.render_row(y, pass))).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let rows: Vec<(u32, Vec<Sample>)> = (from..to).map(|y| (y, self.render_row(y, pass))).collect();
        for (y, out) in rows {
            self.store(y, &out);
        }
        self.row = 0;
        self.pass += 1;
    }

    /// Adds a row of samples: each spreads over the pixels its filter reaches.
    fn store(&mut self, y: u32, row: &[Sample]) {
        let (fw, fh) = self.settings.filter_size();
        let filter = self.settings.filter;
        let (rx, ry) = ((fw * 0.5).ceil() as i32, (fh * 0.5).ceil() as i32);
        let [x0, y0, x1, y1] = self.region.map(|v| v as i32);
        let width = self.width;
        let b = &mut self.buffers;
        for (k, s) in row.iter().enumerate() {
            if s.skip {
                continue;
            }
            let px = self.region[0] as i32 + k as i32;
            let py = y as i32;
            let own = (py as u32 * width + px as u32) as usize;
            // The pixel's own statistics (for adaptive sampling).
            let l = s.beauty.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(0.0);
            let l = l / (1.0 + l);
            b.count[own] += 1;
            b.lum[own] += l;
            b.lum2[own] += l * l;
            if b.object[own] == -1 && s.alpha > 0.0 && b.depth[own].is_infinite() {
                b.depth[own] = s.depth;
                b.object[own] = s.object;
                b.material[own] = s.material;
            }
            let (sx, sy) = (px as f32 + 0.5 + s.offset.0, py as f32 + 0.5 + s.offset.1);
            for qy in (py - ry).max(y0)..=(py + ry).min(y1 - 1) {
                let wy = filter.weight(qy as f32 + 0.5 - sy, fh);
                if wy == 0.0 {
                    continue;
                }
                for qx in (px - rx).max(x0)..=(px + rx).min(x1 - 1) {
                    let w = wy * filter.weight(qx as f32 + 0.5 - sx, fw);
                    if w == 0.0 {
                        continue;
                    }
                    let i = (qy as u32 * width + qx as u32) as usize;
                    b.beauty[i] += s.beauty * w;
                    b.diffuse[i] += s.diffuse * w;
                    b.specular[i] += s.specular * w;
                    b.shadow[i] += s.shadow * w;
                    b.ao[i] += s.ao * w;
                    b.alpha[i] += s.alpha * w;
                    b.normal[i] += s.normal * w;
                    b.weight[i] += w;
                }
            }
        }
    }

    /// Samples one row (one sample per pixel, or none where adaptive
    /// sampling is done with it).
    pub(crate) fn render_row(&self, y: u32, pass: u32) -> Vec<Sample> {
        // Adaptive passes spend the larger shading budget on noisy pixels.
        let gi_rays = if self.active.is_some() { self.settings.shading_subdivisions_max } else { self.settings.shading_subdivisions_min };
        let clip = self.settings.clip_colors.then(|| 2f32.powf(-self.settings.exposure));
        (self.region[0]..self.region[2])
            .map(|x| {
                if let Some(active) = &self.active
                    && !active[(y * self.width + x) as usize]
                {
                    return Sample { skip: true, ..Default::default() };
                }
                let seed = ((pass as u64) << 40) ^ ((y as u64) << 20) ^ x as u64;
                let mut rng = Rng::new(seed);
                // The first pass looks through pixel centres (for clean ID
                // passes); later passes spread over the pixel.
                let (jx, jy) = if pass == 0 { (0.0, 0.0) } else { (rng.f() - 0.5, rng.f() - 0.5) };
                let mut s = self.sample_pixel(x as f32 + 0.5 + jx, y as f32 + 0.5 + jy, &mut rng, gi_rays);
                s.offset = (jx, jy);
                if let Some(cap) = clip {
                    s.beauty = s.beauty.min(Vec3::splat(cap));
                    s.diffuse = s.diffuse.min(Vec3::splat(cap));
                    s.specular = s.specular.min(Vec3::splat(cap));
                }
                s
            })
            .collect()
    }

    fn camera_ray(&self, px: f32, py: f32, rng: &mut Rng) -> (Vec3, Vec3) {
        let c = &self.camera;
        let aspect = self.width as f32 / self.height as f32;
        let nx = px / self.width as f32 * 2.0 - 1.0;
        let ny = 1.0 - py / self.height as f32 * 2.0;
        let right = c.right();
        if let Some(pw) = c.parallel_width {
            let o = c.eye + right * (nx * pw * 0.5) + c.up * (ny * pw * 0.5 / aspect);
            return (o, c.forward);
        }
        let tx = (c.fov_x * 0.5).tan();
        let ty = tx / aspect;
        let d = (c.forward + right * (nx * tx) + c.up * (ny * ty)).normalize();
        match c.dof {
            Some((focus, radius)) if radius > 0.0 && self.settings.depth_of_field => {
                let t = focus / d.dot(c.forward).max(1e-3);
                let p = c.eye + d * t;
                let (lx, ly) = disk(rng);
                let o = c.eye + right * (lx * radius) + c.up * (ly * radius);
                (o, (p - o).normalize_or(d))
            }
            _ => (c.eye + d * c.near.max(0.0), d),
        }
    }

    fn physical(&self) -> bool {
        self.settings.engine == Engine::Physical
    }

    fn sample_pixel(&self, px: f32, py: f32, rng: &mut Rng, gi_rays: u32) -> Sample {
        let (o, d) = self.camera_ray(px, py, rng);
        let scene = &*self.scene;
        let Some(hit) = scene.bvh.closest(o, d, f32::MAX) else {
            let bg = self.background(d);
            // A sky counts as part of the picture (opaque in the alpha).
            let alpha = if scene.sky.is_some() { 1.0 } else { 0.0 };
            return Sample { beauty: bg, ao: 1.0, alpha, depth: f32::INFINITY, object: -1, material: -1, ..Default::default() };
        };
        let surf = self.make_surface(o, d, &hit);
        let (p, n, ng) = (surf.p, surf.n, surf.ng);
        let tri = &scene.bvh.tris[hit.tri as usize];
        let obj = &scene.objects[tri.object as usize];
        let mut s = Sample {
            alpha: 1.0,
            depth: (p - self.camera.eye).dot(self.camera.forward),
            normal: n,
            object: tri.object as i32,
            material: obj.material.map_or(-1, |m| m as i32),
            ..Default::default()
        };
        let want_ao = self.settings.ambient_occlusion || self.settings.passes.contains(&super::Pass::AmbientOcclusion);
        let ao = if want_ao { self.ambient_occlusion(p, ng, n, rng) } else { 1.0 };
        s.ao = ao;
        let (diffuse, specular, shadow) = self.shade(&surf, d, Path::CAMERA, rng, ao, gi_rays);
        s.diffuse = diffuse;
        s.specular = specular;
        s.shadow = shadow;
        s.beauty = diffuse + specular;
        s
    }

    /// Behind everything: the sky, or the background colour.
    fn background(&self, d: Vec3) -> Vec3 {
        match &self.scene.sky {
            Some(sky) => to_working(sky.radiance(d) * (sky.intensity / 100.0), self.scene.working),
            None => to_working(Vec3::from(self.settings.background), self.scene.working),
        }
    }

    /// Light arriving from the surroundings along `d` (rays that escape).
    fn environment(&self, d: Vec3) -> Vec3 {
        match &self.scene.sky {
            Some(sky) => to_working(sky.radiance(d) * (sky.intensity / 100.0), self.scene.working) + (self.scene.ambient - self.scene.sky_ambient),
            None => self.scene.ambient,
        }
    }

    fn ambient_occlusion(&self, p: Vec3, ng: Vec3, n: Vec3, rng: &mut Rng) -> f32 {
        let dist = self.settings.ao_distance;
        let o = p + ng * self.eps();
        let rays = self.settings.ao_subdivisions.max(1);
        let mut open = 0.0;
        for _ in 0..rays {
            let d = cosine_dir(n, rng);
            if !self.scene.bvh.occluded(o, d, dist) {
                open += 1.0;
            }
        }
        let strength = self.settings.ao_strength / 100.0;
        1.0 - strength * (1.0 - open / rays as f32)
    }

    fn eps(&self) -> f32 {
        (self.scene.extent * 1e-5).max(1e-3)
    }

    /// Returns (diffuse, specular, shadow fraction) leaving a surface towards
    /// the viewer (who looks along `d`). Diffuse includes light passing
    /// through transparent surfaces and what the surface gives off.
    /// `gi_rays` is how many bounce rays a camera hit sends (Physical).
    fn shade(&self, hit: &Surface, d: Vec3, path: Path, rng: &mut Rng, ao: f32, gi_rays: u32) -> (Vec3, Vec3, f32) {
        let s = &self.settings;
        let look = &self.scene.objects[hit.object as usize].surface;
        let toon = s.sketch == SketchMode::SketchAndToon && s.post_effects;
        let v = -d;
        let ndv = hit.n.dot(v).clamp(1e-4, 1.0);
        let roughness = if s.blurriness { look.roughness } else { 0.0 };
        let alpha = (look.roughness * look.roughness).max(4e-4);
        // Metals reflect in their own colour; dielectrics a few percent.
        let f0 = Vec3::splat(look.f0).lerp(hit.albedo, look.metalness);
        let fresnel = if toon { Vec3::ZERO } else { f0 + (Vec3::ONE - f0) * (1.0 - ndv).powi(5) };
        let through = if s.transparency { look.transparency * (1.0 - look.metalness) } else { 0.0 };
        let kd = (1.0 - look.metalness) * (1.0 - through);
        let primary = path.depth == 0;
        let physical = self.physical();
        let light_rays = if primary { s.shadow_subdivisions } else { 1 };
        let (light, highlight, lost, total) = self.direct(hit, v, rng, toon, alpha, light_rays);
        let rest = Vec3::ONE - fresnel;
        let mut diffuse = hit.albedo * light * kd * rest + look.emission;
        let mut specular = highlight * fresnel;
        let deeper = path.depth + 1 < s.max_depth;
        // Rays worth less than the Ray Threshold aren't traced.
        let worth = |w: f32| path.weight * w >= s.ray_threshold / 100.0;
        let o_out = hit.p + hit.ng * self.eps();
        // Light from all around: bounced (Global Illumination) or even ambient.
        if kd > 0.0 {
            let gi = s.global_illumination && deeper && path.bounces < s.gi_depth && !toon;
            let w = (hit.albedo * kd).max_element();
            if gi && worth(w) {
                let n = if primary && physical { gi_rays.max(1) } else { 1 };
                let mut sum = Vec3::ZERO;
                let next = Path { bounces: path.bounces + 1, ..path.next(w) };
                for _ in 0..n {
                    let dir = cosine_dir(hit.n, rng);
                    sum += self.radiance(o_out, dir, next, rng);
                }
                diffuse += hit.albedo * (sum / n as f32) * kd * rest;
            } else {
                let amb = if toon { self.scene.ambient.min(Vec3::splat(0.6)) } else { self.scene.ambient };
                diffuse += hit.albedo * amb * ao * kd * rest;
            }
        }
        // Reflections: a mirror, or glossy around it for rough surfaces.
        let w = fresnel.max_element();
        if s.reflections && !toon && deeper && path.reflections < s.reflection_depth && worth(w) {
            let mirror = d - hit.n * 2.0 * d.dot(hit.n);
            let glossy = roughness >= 0.03;
            let n = if glossy && primary && physical { s.blurriness_subdivisions } else { 1 };
            let next = Path { reflections: path.reflections + 1, ..path.next(w) };
            let mut sum = Vec3::ZERO;
            for _ in 0..n {
                let r = if glossy {
                    let h = ggx_half(hit.n, alpha, rng);
                    let r = d - h * 2.0 * d.dot(h);
                    if r.dot(hit.ng) > 0.0 { r } else { mirror }
                } else {
                    mirror
                };
                sum += if s.reflections_sky_only { self.environment_for(r) } else { self.trace(o_out, r, next, rng) };
            }
            specular += sum / n as f32 * fresnel;
        }
        // Light passing through (refracted), tinted by the colour on the way in.
        if through > 0.0 && !toon && deeper && worth(through) {
            let next = path.next(through);
            let incoming = if !s.refraction {
                // Straight through, without bending.
                self.trace(hit.p - hit.ng * self.eps(), d, next, rng)
            } else {
                let eta = if hit.entering { 1.0 / look.ior } else { look.ior };
                let n = if roughness < 0.03 { hit.n } else { ggx_half(hit.n, alpha * 0.5, rng) };
                match refract(d, n, eta) {
                    Some(t) => self.trace(hit.p - hit.ng * self.eps(), t, next, rng),
                    // Total internal reflection.
                    None => self.trace(o_out, d - n * 2.0 * d.dot(n), next, rng),
                }
            };
            let tint = if hit.entering { hit.albedo } else { Vec3::ONE };
            diffuse += incoming * tint * through * rest;
        }
        let shadow = if total > 0.0 { (lost / total).clamp(0.0, 1.0) } else { 0.0 };
        (diffuse, specular, shadow)
    }

    /// What reflections see with Limit Reflections to Sky: the sky (or the
    /// background colour).
    fn environment_for(&self, d: Vec3) -> Vec3 {
        if self.physical() { self.environment(d) } else { self.background(d) }
    }

    /// Follows a secondary ray: path traced (Physical) or lit directly
    /// (Standard).
    fn trace(&self, o: Vec3, d: Vec3, path: Path, rng: &mut Rng) -> Vec3 {
        if self.physical() { self.radiance(o, d, path, rng) } else { self.reflect(o, d, path, rng) }
    }

    /// How much light gets from `o` along `d` over `dist`: nothing through
    /// opaque objects, some through transparent ones (up to Shadow Depth
    /// surfaces).
    fn transmittance(&self, o: Vec3, d: Vec3, dist: f32) -> Vec3 {
        let mut through = Vec3::ONE;
        let (mut o, mut left) = (o, dist);
        for _ in 0..self.settings.shadow_depth {
            let Some(hit) = self.scene.bvh.closest(o, d, left) else {
                return through;
            };
            let tri = &self.scene.bvh.tris[hit.tri as usize];
            let obj = &self.scene.objects[tri.object as usize];
            let t = if self.settings.transparency { obj.surface.transparency * (1.0 - obj.surface.metalness) } else { 0.0 };
            if t <= 0.0 {
                return Vec3::ZERO;
            }
            // Coloured glass tints its shadow (half-way, as light passes two faces).
            through *= Vec3::ONE.lerp(obj.color, 0.5) * t;
            if through.max_element() < 0.01 {
                return Vec3::ZERO;
            }
            let step = hit.t + self.eps();
            o += d * step;
            left -= step;
            if left <= 0.0 {
                return through;
            }
        }
        // Deeper than Shadow Depth: counts as blocked.
        Vec3::ZERO
    }

    /// What a mirror ray sees (Standard: lit surfaces, no bounces).
    fn reflect(&self, o: Vec3, d: Vec3, path: Path, rng: &mut Rng) -> Vec3 {
        match self.surface(o, d) {
            None => self.background(d),
            Some(hit) => {
                let (a, b, _) = self.shade(&hit, d, path, rng, 1.0, 1);
                a + b
            }
        }
    }

    /// Path tracing: all light arriving at `o` from direction `d`.
    fn radiance(&self, o: Vec3, d: Vec3, path: Path, rng: &mut Rng) -> Vec3 {
        match self.surface(o, d) {
            None => self.environment(d),
            Some(hit) => {
                // Russian roulette past a few bounces.
                let keep = if path.depth >= 3 { hit.albedo.max_element().clamp(0.1, 0.95) } else { 1.0 };
                if keep < 1.0 && rng.f() > keep {
                    return Vec3::ZERO;
                }
                let (a, b, _) = self.shade(&hit, d, path, rng, 1.0, 1);
                (a + b) / keep
            }
        }
    }

    fn surface(&self, o: Vec3, d: Vec3) -> Option<Surface> {
        let hit = self.scene.bvh.closest(o, d, f32::MAX)?;
        Some(self.make_surface(o, d, &hit))
    }

    /// The surface a ray hit: its normals facing the ray, which side it came
    /// from, and its colour (with the material's picture, if any).
    fn make_surface(&self, o: Vec3, d: Vec3, hit: &super::bvh::Hit) -> Surface {
        let scene = &*self.scene;
        let tri = &scene.bvh.tris[hit.tri as usize];
        let p = o + d * hit.t;
        let mut n = tri.normal_at(hit.u, hit.v);
        let mut ng = tri.face_normal();
        let entering = ng.dot(d) < 0.0;
        if !entering {
            ng = -ng;
        }
        if n.dot(ng) < 0.0 {
            n = -n;
        }
        let obj = &scene.objects[tri.object as usize];
        let mut albedo = obj.color;
        if let Some((img, tex)) = &obj.surface.texture {
            let lp = obj.to_local.transform_point3(p);
            let ln = (obj.normal_to_local * n).normalize_or(Vec3::Y);
            let c = to_working(img.sample(tex.uv(lp, ln, obj.lo, obj.hi)), scene.working);
            albedo = albedo.lerp(c, tex.mix);
        }
        Surface { p, n, ng, object: tri.object, albedo, entering }
    }

    /// Direct light: (diffuse light, highlights, light lost to shadows, light
    /// before shadows).
    /// `rays` samples soft and area lights several times (Shadow
    /// Subdivisions).
    fn direct(&self, hit: &Surface, v: Vec3, rng: &mut Rng, toon: bool, alpha: f32, rays: u32) -> (Vec3, Vec3, f32, f32) {
        let mut light = Vec3::ZERO;
        let mut highlight = Vec3::ZERO;
        let (mut lost, mut total) = (0.0, 0.0);
        for l in &self.scene.lights {
            if !l.reaches(hit.object) {
                continue;
            }
            let soft = l.softness > 0.0 || matches!(l.kind, RLightKind::Area);
            let n = if soft { rays.max(1) } else { 1 };
            let share = 1.0 / n as f32;
            for _ in 0..n {
                let Some((to_light, dist, strength)) = self.sample_light(l, hit.p, rng) else {
                    continue;
                };
                let strength = strength * share;
                let ndl = hit.n.dot(to_light);
                if ndl <= 0.0 || strength <= 0.0 {
                    continue;
                }
                let ndl = if toon {
                    let steps = self.settings.toon_steps as f32;
                    ((ndl * steps).ceil() / steps).min(1.0)
                } else {
                    ndl
                };
                let mut c = l.color * strength;
                let before = (c * ndl).element_sum();
                if l.shadows && self.settings.shadows {
                    let o = hit.p + hit.ng * self.eps();
                    let t = self.transmittance(o, to_light, dist - self.eps() * 2.0);
                    if t != Vec3::ONE {
                        let blocked = Vec3::ONE - t;
                        let shade = Vec3::splat(1.0 - l.density) + l.shadow_color * l.density;
                        c *= shade * blocked + t;
                    }
                }
                let after = (c * ndl).element_sum();
                lost += before - after;
                total += before;
                light += c * ndl;
                if !toon {
                    highlight += c * ggx_highlight(hit.n, v, to_light, alpha) * ndl;
                }
            }
        }
        (light, highlight, lost, total)
    }

    /// A direction to the light from `p`, its distance and how strongly it
    /// reaches `p` (cone, falloff, the parallel light's plane).
    fn sample_light(&self, l: &RLight, p: Vec3, rng: &mut Rng) -> Option<(Vec3, f32, f32)> {
        let soft = l.softness > 0.0;
        match l.kind {
            RLightKind::Directional { parallel } => {
                if parallel && (p - l.pos).dot(l.dir) < 0.0 {
                    return None;
                }
                let mut to = -l.dir;
                if soft {
                    let (t, b) = basis(to);
                    let (x, y) = disk(rng);
                    to = (to + (t * x + b * y) * l.softness).normalize();
                }
                Some((to, f32::MAX * 0.5, 1.0))
            }
            _ => {
                let mut at = l.pos;
                match l.kind {
                    RLightKind::Area => {
                        at += l.right * (rng.f() * 2.0 - 1.0) + l.up * (rng.f() * 2.0 - 1.0);
                    }
                    _ if soft => {
                        let (t, b) = basis((p - l.pos).normalize_or(Vec3::Y));
                        let (x, y) = disk(rng);
                        at += (t * x + b * y) * l.softness * 0.5;
                    }
                    _ => {}
                }
                let delta = at - p;
                let dist = delta.length();
                if dist < 1e-4 {
                    return None;
                }
                let to = delta / dist;
                let mut strength = 1.0;
                match l.kind {
                    RLightKind::Spot => {
                        let c = (-to).dot(l.dir);
                        strength *= smoothstep(l.outer_cos, l.inner_cos.max(l.outer_cos + 1e-4), c);
                    }
                    RLightKind::Area => {
                        // Area lights shine from their front.
                        let c = (-to).dot(l.dir);
                        if c <= 0.0 {
                            return None;
                        }
                        strength *= c.sqrt();
                    }
                    _ => {}
                }
                if let Some(r) = l.radius {
                    strength *= (1.0 - dist / r).clamp(0.0, 1.0).powi(2);
                }
                Some((to, dist, strength))
            }
        }
    }

    /// The working space's colour back as linear sRGB (for post).
    pub(crate) fn display_linear(&self, c: Vec3) -> Vec3 {
        from_working(c, self.scene.working)
    }

    /// How many samples a pixel has (for averaging).
    pub(crate) fn weight_at(&self, i: usize) -> f32 {
        self.buffers.weight[i]
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Surface {
    p: Vec3,
    n: Vec3,
    ng: Vec3,
    object: u32,
    albedo: Vec3,
    /// The ray came from the front of the face (into the object).
    entering: bool,
}

/// The GGX microfacet highlight (distribution times visibility, scaled to
/// this renderer's light units, without the Fresnel term).
fn ggx_highlight(n: Vec3, v: Vec3, l: Vec3, alpha: f32) -> f32 {
    let h = (v + l).normalize_or(n);
    let (ndh, ndl, ndv) = (n.dot(h).max(0.0), n.dot(l).max(1e-4), n.dot(v).max(1e-4));
    let a2 = alpha * alpha;
    let den = ndh * ndh * (a2 - 1.0) + 1.0;
    let dist = a2 / (std::f32::consts::PI * den * den);
    let k = alpha * 0.5;
    let vis = 1.0 / (4.0 * (ndl * (1.0 - k) + k) * (ndv * (1.0 - k) + k));
    (dist * vis * std::f32::consts::PI).min(50.0)
}

/// A microfacet normal around `n`, spread by roughness (GGX).
fn ggx_half(n: Vec3, alpha: f32, rng: &mut Rng) -> Vec3 {
    let (u1, u2) = (rng.f(), rng.f());
    let tan2 = alpha * alpha * u1 / (1.0 - u1).max(1e-6);
    let cos = 1.0 / (1.0 + tan2).sqrt();
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let phi = TAU * u2;
    let (t, b) = basis(n);
    (t * (sin * phi.cos()) + b * (sin * phi.sin()) + n * cos).normalize_or(n)
}

/// Snell refraction of `d` through a surface with normal `n` (facing `-d`),
/// `eta` = n1 / n2; None for total internal reflection.
fn refract(d: Vec3, n: Vec3, eta: f32) -> Option<Vec3> {
    let cos_i = -n.dot(d).clamp(-1.0, 1.0);
    let k = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    (k >= 0.0).then(|| (d * eta + n * (eta * cos_i - k.sqrt())).normalize())
}
