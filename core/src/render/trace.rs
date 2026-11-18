//! The ray tracer: one sample per pixel per pass, rows at a time.

use super::build::{RLight, RLightKind, RenderScene, from_working, to_working};
use super::{Engine, Filter, RenderSettings, SketchMode};
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
    /// The filter's weight for this sample.
    pub w: f32,
}

/// The sums of every pixel's samples.
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
        }
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
        Renderer {
            scene,
            settings: settings.sanitized(),
            width,
            height,
            region,
            camera,
            pass: 0,
            row: 0,
            buffers: Buffers::new((width * height) as usize),
        }
    }

    pub fn total_passes(&self) -> u32 {
        self.settings.samples
    }

    pub fn passes_done(&self) -> u32 {
        self.pass.min(self.total_passes())
    }

    pub fn done(&self) -> bool {
        self.pass >= self.total_passes()
    }

    fn rows(&self) -> u32 {
        self.region[3] - self.region[1]
    }

    pub fn total_work(&self) -> f32 {
        self.total_passes() as f32 * self.rows() as f32
    }

    pub fn work_done(&self) -> f32 {
        if self.done() { self.total_work() } else { self.pass as f32 * self.rows() as f32 + self.row as f32 }
    }

    /// Renders up to `rows` rows of the current pass.
    pub fn step(&mut self, rows: u32) {
        for _ in 0..rows.max(1) {
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

    fn store(&mut self, y: u32, row: &[Sample]) {
        let b = &mut self.buffers;
        for (k, s) in row.iter().enumerate() {
            let i = (y * self.width + self.region[0] + k as u32) as usize;
            let w = s.w;
            b.beauty[i] += s.beauty;
            b.diffuse[i] += s.diffuse;
            b.specular[i] += s.specular;
            b.shadow[i] += s.shadow;
            b.ao[i] += s.ao;
            b.alpha[i] += s.alpha;
            b.normal[i] += s.normal;
            b.weight[i] += w;
            if b.object[i] == -1 && s.alpha > 0.0 && b.depth[i].is_infinite() {
                b.depth[i] = s.depth;
                b.object[i] = s.object;
                b.material[i] = s.material;
            }
        }
    }

    /// Samples one row (one sample per pixel).
    pub(crate) fn render_row(&self, y: u32, pass: u32) -> Vec<Sample> {
        (self.region[0]..self.region[2])
            .map(|x| {
                let seed = ((pass as u64) << 40) ^ ((y as u64) << 20) ^ x as u64;
                let mut rng = Rng::new(seed);
                // The first pass looks through pixel centres (for clean ID
                // passes); later passes spread over the filter.
                let (jx, jy, w) = if pass == 0 {
                    (0.0, 0.0, 1.0)
                } else {
                    match self.settings.filter {
                        Filter::Box => (rng.f() - 0.5, rng.f() - 0.5, 1.0),
                        Filter::Gaussian => {
                            let (dx, dy) = (rng.f() * 2.0 - 1.0, rng.f() * 2.0 - 1.0);
                            (dx * 0.9, dy * 0.9, (-2.0 * (dx * dx + dy * dy)).exp())
                        }
                    }
                };
                let s = self.sample_pixel(x as f32 + 0.5 + jx, y as f32 + 0.5 + jy, &mut rng);
                weight(s, w)
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
            Some((focus, radius)) if radius > 0.0 => {
                let t = focus / d.dot(c.forward).max(1e-3);
                let p = c.eye + d * t;
                let (lx, ly) = disk(rng);
                let o = c.eye + right * (lx * radius) + c.up * (ly * radius);
                (o, (p - o).normalize_or(d))
            }
            _ => (c.eye + d * c.near.max(0.0), d),
        }
    }

    fn sample_pixel(&self, px: f32, py: f32, rng: &mut Rng) -> Sample {
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
        let (diffuse, specular, shadow) = self.shade(&surf, d, 0, rng, ao);
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
        let rays = 2;
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
    fn shade(&self, hit: &Surface, d: Vec3, depth: u32, rng: &mut Rng, ao: f32) -> (Vec3, Vec3, f32) {
        let s = &self.settings;
        let look = &self.scene.objects[hit.object as usize].surface;
        let toon = s.sketch == SketchMode::SketchAndToon;
        let v = -d;
        let ndv = hit.n.dot(v).clamp(1e-4, 1.0);
        let alpha = (look.roughness * look.roughness).max(4e-4);
        // Metals reflect in their own colour; dielectrics a few percent.
        let f0 = Vec3::splat(look.f0).lerp(hit.albedo, look.metalness);
        let fresnel = if toon { Vec3::ZERO } else { f0 + (Vec3::ONE - f0) * (1.0 - ndv).powi(5) };
        let through = look.transparency * (1.0 - look.metalness);
        let kd = (1.0 - look.metalness) * (1.0 - through);
        let (light, highlight, lost, total) = self.direct(hit, v, rng, toon, alpha);
        let rest = Vec3::ONE - fresnel;
        let mut diffuse = hit.albedo * light * kd * rest + look.emission;
        let mut specular = highlight * fresnel;
        let physical = s.engine == Engine::Physical;
        let deeper = depth + 1 < s.max_depth;
        // Light from all around: bounced (Physical with GI) or even ambient.
        if kd > 0.0 {
            if physical && s.global_illumination && deeper {
                let dir = cosine_dir(hit.n, rng);
                let o = hit.p + hit.ng * self.eps();
                let incoming = self.radiance(o, dir, depth + 1, rng);
                diffuse += hit.albedo * incoming * kd * rest;
            } else {
                let amb = if toon { self.scene.ambient.min(Vec3::splat(0.6)) } else { self.scene.ambient };
                diffuse += hit.albedo * amb * ao * kd * rest;
            }
        }
        // Reflections: a mirror, or glossy around it for rough surfaces.
        if s.reflections && !toon && deeper {
            let mirror = d - hit.n * 2.0 * d.dot(hit.n);
            let r = if look.roughness < 0.03 {
                mirror
            } else {
                let h = ggx_half(hit.n, alpha, rng);
                let r = d - h * 2.0 * d.dot(h);
                if r.dot(hit.ng) > 0.0 { r } else { mirror }
            };
            let o = hit.p + hit.ng * self.eps();
            let incoming = if physical { self.radiance(o, r, depth + 1, rng) } else { self.reflect(o, r, depth + 1, rng) };
            specular += incoming * fresnel;
        }
        // Light passing through (refracted), tinted by the colour on the way in.
        if through > 0.0 && !toon && deeper {
            let eta = if hit.entering { 1.0 / look.ior } else { look.ior };
            let n = if look.roughness < 0.03 { hit.n } else { ggx_half(hit.n, alpha * 0.5, rng) };
            let incoming = match refract(d, n, eta) {
                Some(t) => {
                    let o = hit.p - hit.ng * self.eps();
                    if physical { self.radiance(o, t, depth + 1, rng) } else { self.reflect(o, t, depth + 1, rng) }
                }
                // Total internal reflection.
                None => {
                    let r = d - n * 2.0 * d.dot(n);
                    let o = hit.p + hit.ng * self.eps();
                    if physical { self.radiance(o, r, depth + 1, rng) } else { self.reflect(o, r, depth + 1, rng) }
                }
            };
            let tint = if hit.entering { hit.albedo } else { Vec3::ONE };
            diffuse += incoming * tint * through * rest;
        }
        let shadow = if total > 0.0 { (lost / total).clamp(0.0, 1.0) } else { 0.0 };
        (diffuse, specular, shadow)
    }

    /// How much light gets from `o` along `d` over `dist`: nothing through
    /// opaque objects, some through transparent ones.
    fn transmittance(&self, o: Vec3, d: Vec3, dist: f32) -> Vec3 {
        let mut through = Vec3::ONE;
        let (mut o, mut left) = (o, dist);
        for _ in 0..8 {
            let Some(hit) = self.scene.bvh.closest(o, d, left) else { return through };
            let tri = &self.scene.bvh.tris[hit.tri as usize];
            let obj = &self.scene.objects[tri.object as usize];
            let t = obj.surface.transparency * (1.0 - obj.surface.metalness);
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
                break;
            }
        }
        through
    }

    /// What a mirror ray sees (Standard: lit surfaces, no bounces).
    fn reflect(&self, o: Vec3, d: Vec3, depth: u32, rng: &mut Rng) -> Vec3 {
        match self.surface(o, d) {
            None => self.background(d),
            Some(hit) => {
                let (a, b, _) = self.shade(&hit, d, depth, rng, 1.0);
                a + b
            }
        }
    }

    /// Path tracing: all light arriving at `o` from direction `d`.
    fn radiance(&self, o: Vec3, d: Vec3, depth: u32, rng: &mut Rng) -> Vec3 {
        match self.surface(o, d) {
            None => self.environment(d),
            Some(hit) => {
                // Russian roulette past a few bounces.
                let keep = if depth >= 3 { hit.albedo.max_element().clamp(0.1, 0.95) } else { 1.0 };
                if keep < 1.0 && rng.f() > keep {
                    return Vec3::ZERO;
                }
                let (a, b, _) = self.shade(&hit, d, depth, rng, 1.0);
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
    fn direct(&self, hit: &Surface, v: Vec3, rng: &mut Rng, toon: bool, alpha: f32) -> (Vec3, Vec3, f32, f32) {
        let mut light = Vec3::ZERO;
        let mut highlight = Vec3::ZERO;
        let (mut lost, mut total) = (0.0, 0.0);
        let soft_samples = self.settings.engine == Engine::Physical;
        for l in &self.scene.lights {
            if !l.reaches(hit.object) {
                continue;
            }
            let Some((to_light, dist, strength)) = self.sample_light(l, hit.p, rng, soft_samples) else { continue };
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
        (light, highlight, lost, total)
    }

    /// A direction to the light from `p`, its distance and how strongly it
    /// reaches `p` (cone, falloff, the parallel light's plane).
    fn sample_light(&self, l: &RLight, p: Vec3, rng: &mut Rng, _physical: bool) -> Option<(Vec3, f32, f32)> {
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

fn weight(mut s: Sample, w: f32) -> Sample {
    if w != 1.0 {
        s.beauty *= w;
        s.diffuse *= w;
        s.specular *= w;
        s.shadow *= w;
        s.ao *= w;
        s.alpha *= w;
        s.normal *= w;
    }
    s.w = w;
    s
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
