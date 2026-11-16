//! After the tracer: averaging, exposure, view transforms, outlines (Sketch
//! and Toon, Contour), pass images and stereo combining.

use super::trace::Renderer;
use super::{Pass, RenderSettings, SketchMode, ViewTransform};
use glam::Vec3;

/// An 8-bit RGBA image (sRGB, straight alpha), rows from the top.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        Image { width, height, pixels: vec![0; (width * height * 4) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    fn set(&mut self, i: usize, c: [u8; 4]) {
        self.pixels[i * 4..i * 4 + 4].copy_from_slice(&c);
    }
}

fn srgb_encode(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

fn hable(x: Vec3) -> Vec3 {
    let (a, b, c, d, e, f) = (0.15, 0.50, 0.10, 0.20, 0.02, 0.30);
    ((x * (x * a + c * b) + d * e) / (x * (x * a + b) + d * f)) - e / f
}

/// Linear light (linear sRGB) → display colour 0..1 for a view transform.
pub fn view_transform(c: Vec3, vt: ViewTransform) -> Vec3 {
    let c = c.max(Vec3::ZERO);
    match vt {
        ViewTransform::Standard => c.map(srgb_encode),
        ViewTransform::AcesSdr => {
            let x = c * 0.6;
            let t = (x * (x * 2.51 + 0.03)) / (x * (x * 2.43 + 0.59) + 0.14);
            t.map(srgb_encode)
        }
        ViewTransform::Filmic => {
            let white = 11.2;
            let t = hable(c * 2.0) / hable(Vec3::splat(white));
            t.map(srgb_encode)
        }
        ViewTransform::Raw => c.min(Vec3::ONE),
    }
}

fn byte(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// A stable colour for an id (Object ID / Material ID passes).
fn id_color(id: i32) -> [u8; 4] {
    if id < 0 {
        return [0, 0, 0, 0];
    }
    let mut h = (id as u32).wrapping_add(1).wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    let hue = (h % 360) as f32;
    let c = hsv(hue, 0.65, 0.95);
    [byte(c.x), byte(c.y), byte(c.z), 255]
}

fn hsv(h: f32, s: f32, v: f32) -> Vec3 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Vec3::new(r, g, b) + Vec3::splat(v - c)
}

/// The colour of a pass's pixel, in linear light (before the view transform).
fn colour(r: &Renderer, pass: Pass, i: usize, s: &RenderSettings) -> Vec3 {
    let b = &r.buffers;
    let w = b.weight[i].max(1e-6);
    let c = match pass {
        Pass::Diffuse => b.diffuse[i],
        Pass::Specular => b.specular[i],
        _ => b.beauty[i],
    } / w;
    r.display_linear(c) * 2f32.powf(s.exposure)
}

/// One pass as a display image.
pub fn pass_image(r: &Renderer, pass: Pass, s: &RenderSettings) -> Image {
    let (w, h) = (r.width, r.height);
    let mut img = Image::new(w, h);
    let b = &r.buffers;
    let n = (w * h) as usize;
    // Depth is shown from near (white) to far (black).
    let (mut dmin, mut dmax) = (f32::MAX, f32::MIN);
    if pass == Pass::Depth {
        for &d in &b.depth {
            if d.is_finite() {
                dmin = dmin.min(d);
                dmax = dmax.max(d);
            }
        }
    }
    for i in 0..n {
        let weight = b.weight[i];
        if weight <= 0.0 {
            continue;
        }
        let alpha = b.alpha[i] / weight;
        let px = match pass {
            Pass::Beauty | Pass::Diffuse | Pass::Specular => {
                let c = view_transform(colour(r, pass, i, s), s.view_transform);
                let a = if pass == Pass::Beauty && s.alpha { alpha } else { 1.0 };
                [byte(c.x), byte(c.y), byte(c.z), byte(a)]
            }
            Pass::Shadow => {
                let v = byte(b.shadow[i] / weight);
                [v, v, v, 255]
            }
            Pass::AmbientOcclusion => {
                let v = byte(if alpha > 0.0 { b.ao[i] / weight } else { 1.0 });
                [v, v, v, 255]
            }
            Pass::Depth => {
                let d = b.depth[i];
                let v = if d.is_finite() && dmax > dmin { byte(1.0 - (d - dmin) / (dmax - dmin)) } else if d.is_finite() { 255 } else { 0 };
                [v, v, v, 255]
            }
            Pass::Normal => {
                let nn = (b.normal[i] / weight).normalize_or_zero();
                if nn == Vec3::ZERO {
                    [0, 0, 0, 0]
                } else {
                    let c = nn * 0.5 + Vec3::splat(0.5);
                    [byte(c.x), byte(c.y), byte(c.z), 255]
                }
            }
            Pass::ObjectId => id_color(b.object[i]),
            Pass::MaterialId => id_color(b.material[i]),
            Pass::Alpha => {
                let v = byte(alpha);
                [v, v, v, 255]
            }
        };
        img.set(i, px);
    }
    if pass == Pass::Beauty && s.sketch != SketchMode::Off {
        outlines(r, s, &mut img);
    }
    img
}

/// Draws lines where objects meet, where depth jumps and at creases.
fn outlines(r: &Renderer, s: &RenderSettings, img: &mut Image) {
    let (w, h) = (r.width as i32, r.height as i32);
    let b = &r.buffers;
    let normal = |i: usize| (b.normal[i] / b.weight[i].max(1e-6)).normalize_or_zero();
    let edge = |a: usize, c: usize| -> bool {
        if b.weight[a] <= 0.0 || b.weight[c] <= 0.0 {
            return false;
        }
        if b.object[a] != b.object[c] {
            return true;
        }
        if b.object[a] < 0 {
            return false;
        }
        let (da, dc) = (b.depth[a], b.depth[c]);
        if da.is_finite() && dc.is_finite() && (da - dc).abs() > 0.04 * da.min(dc).max(1.0) {
            return true;
        }
        normal(a).dot(normal(c)) < 0.75
    };
    let radius = (s.line_width * 0.5).max(0.5);
    let reach = radius.ceil() as i32;
    let line = Vec3::from(s.line_color).map(srgb_encode);
    let mut cover = vec![0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            // An edge between this pixel and its right or lower neighbour.
            let right = x + 1 < w && edge(i, i + 1);
            let down = y + 1 < h && edge(i, i + w as usize);
            if !(right || down) {
                continue;
            }
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    let c = (radius + 0.5 - d).clamp(0.0, 1.0);
                    let k = (ny * w + nx) as usize;
                    cover[k] = cover[k].max(c);
                }
            }
        }
    }
    for (i, &c) in cover.iter().enumerate() {
        if c <= 0.0 {
            continue;
        }
        let p = &mut img.pixels[i * 4..i * 4 + 4];
        let base = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 255.0;
        let out = base.lerp(line, c);
        p[0] = byte(out.x);
        p[1] = byte(out.y);
        p[2] = byte(out.z);
        p[3] = p[3].max(byte(c));
    }
}

/// Red from the left eye, green and blue from the right (red/cyan glasses).
pub fn anaglyph(l: &Image, r: &Image) -> Image {
    let mut out = l.clone();
    for (k, px) in out.pixels.chunks_exact_mut(4).enumerate() {
        let q = &r.pixels[k * 4..k * 4 + 4];
        px[1] = q[1];
        px[2] = q[2];
        px[3] = px[3].max(q[3]);
    }
    out
}

/// The two eyes next to each other, left first.
pub fn side_by_side(l: &Image, r: &Image) -> Image {
    let mut out = Image::new(l.width + r.width, l.height.max(r.height));
    for y in 0..out.height {
        for (img, x0) in [(l, 0), (r, l.width)] {
            if y >= img.height {
                continue;
            }
            let src = &img.pixels[(y * img.width * 4) as usize..((y + 1) * img.width * 4) as usize];
            let at = ((y * out.width + x0) * 4) as usize;
            out.pixels[at..at + src.len()].copy_from_slice(src);
        }
    }
    out
}

/// Beauty as 16-bit RGBA (for 16-bit PNG).
pub fn beauty16(r: &Renderer, s: &RenderSettings) -> Vec<u16> {
    let n = (r.width * r.height) as usize;
    let mut out = vec![0u16; n * 4];
    let q = |x: f32| (x.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
    for i in 0..n {
        let weight = r.buffers.weight[i];
        if weight <= 0.0 {
            continue;
        }
        let c = view_transform(colour(r, Pass::Beauty, i, s), s.view_transform);
        let a = if s.alpha { r.buffers.alpha[i] / weight } else { 1.0 };
        out[i * 4..i * 4 + 4].copy_from_slice(&[q(c.x), q(c.y), q(c.z), q(a)]);
    }
    out
}
