//! Image textures on materials: the picture (kept inside the scene file),
//! how it is projected onto an object, and sampling it in renders.
//!
//! Objects have no UV coordinates yet, so textures are projected from the
//! object's own space, as with a projection tag: Cubic (from the side the
//! surface faces), Flat (along Z), Spherical and Cylindrical (around Y).

use base64::Engine as _;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Projection {
    #[default]
    Cubic,
    Flat,
    Spherical,
    Cylindrical,
}

impl Projection {
    pub const ALL: [Projection; 4] = [Projection::Cubic, Projection::Flat, Projection::Spherical, Projection::Cylindrical];

    pub fn name(self) -> &'static str {
        match self {
            Projection::Cubic => "Cubic",
            Projection::Flat => "Flat",
            Projection::Spherical => "Spherical",
            Projection::Cylindrical => "Cylindrical",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Texture {
    /// The file it came from (for display).
    pub name: String,
    /// The PNG or JPEG file, base64.
    pub data: String,
    #[serde(default)]
    pub projection: Projection,
    /// Repeats across the object (U, V).
    #[serde(default = "one2")]
    pub tiles: [f32; 2],
    #[serde(default)]
    pub offset: [f32; 2],
    /// How much the image replaces the material colour (0..1).
    #[serde(default = "one")]
    pub mix: f32,
}

fn one() -> f32 {
    1.0
}

fn one2() -> [f32; 2] {
    [1.0, 1.0]
}

impl Texture {
    pub fn from_file(name: &str, bytes: &[u8]) -> Result<Texture, String> {
        // Checks it can be read before keeping it.
        Image::decode(bytes)?;
        Ok(Texture {
            name: name.to_owned(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            projection: Projection::Cubic,
            tiles: [1.0, 1.0],
            offset: [0.0, 0.0],
            mix: 1.0,
        })
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        base64::engine::general_purpose::STANDARD.decode(self.data.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn image(&self) -> Result<Image, String> {
        Image::decode(&self.bytes()?)
    }

    pub fn sanitized(&self) -> Texture {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        Texture {
            name: self.name.clone(),
            data: self.data.clone(),
            projection: self.projection,
            tiles: self.tiles.map(|t| f(t, 0.01, 1000.0, 1.0)),
            offset: self.offset.map(|o| f(o, -1000.0, 1000.0, 0.0)),
            mix: f(self.mix, 0.0, 1.0, 1.0),
        }
    }

    /// Texture coordinates for a point and normal in the object's own space,
    /// given the object's bounding box there.
    pub fn uv(&self, p: Vec3, n: Vec3, lo: Vec3, hi: Vec3) -> Vec2 {
        let size = (hi - lo).max(Vec3::splat(1e-6));
        let q = (p - lo) / size; // 0..1 across the box
        let c = (p - (lo + hi) * 0.5) / size;
        let tau = std::f32::consts::TAU;
        let uv = match self.projection {
            Projection::Flat => Vec2::new(q.x, q.y),
            Projection::Cubic => {
                let a = n.abs();
                if a.x >= a.y && a.x >= a.z {
                    Vec2::new(if n.x > 0.0 { 1.0 - q.z } else { q.z }, q.y)
                } else if a.y >= a.z {
                    Vec2::new(q.x, if n.y > 0.0 { 1.0 - q.z } else { q.z })
                } else {
                    Vec2::new(if n.z > 0.0 { q.x } else { 1.0 - q.x }, q.y)
                }
            }
            Projection::Spherical => {
                let d = c.normalize_or(Vec3::Z);
                Vec2::new(d.x.atan2(d.z) / tau + 0.5, d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI * -1.0 + 1.0)
            }
            Projection::Cylindrical => Vec2::new(c.x.atan2(c.z) / tau + 0.5, q.y),
        };
        uv * Vec2::from(self.tiles) + Vec2::from(self.offset)
    }
}

/// A decoded picture in linear colour, ready to sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Linear RGB, rows from the top.
    pub pixels: Vec<Vec3>,
}

fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// The largest side kept; bigger pictures are shrunk (renders don't need more).
const MAX_SIDE: u32 = 2048;

impl Image {
    /// Reads a PNG or JPEG file.
    pub fn decode(bytes: &[u8]) -> Result<Image, String> {
        let (w, h, rgb) = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            decode_png(bytes)?
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            decode_jpeg(bytes)?
        } else {
            return Err("only PNG and JPEG pictures can be used".into());
        };
        if w == 0 || h == 0 {
            return Err("the picture is empty".into());
        }
        let lut: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
        let step = w.max(h).div_ceil(MAX_SIDE).max(1);
        let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
        let mut pixels = Vec::with_capacity((ow * oh) as usize);
        for y in 0..oh {
            for x in 0..ow {
                let (sx, sy) = ((x * step).min(w - 1), (y * step).min(h - 1));
                let i = ((sy * w + sx) * 3) as usize;
                pixels.push(Vec3::new(lut[rgb[i] as usize], lut[rgb[i + 1] as usize], lut[rgb[i + 2] as usize]));
            }
        }
        Ok(Image { width: ow, height: oh, pixels })
    }

    /// Bilinear sample at (u, v), repeating; v = 0 is the bottom.
    pub fn sample(&self, uv: Vec2) -> Vec3 {
        let (w, h) = (self.width as f32, self.height as f32);
        let x = uv.x.rem_euclid(1.0) * w - 0.5;
        let y = (1.0 - uv.y.rem_euclid(1.0)) * h - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let px = |xi: f32, yi: f32| {
            let xi = (xi as i64).rem_euclid(self.width as i64) as u32;
            let yi = (yi as i64).rem_euclid(self.height as i64) as u32;
            self.pixels[(yi * self.width + xi) as usize]
        };
        let a = px(x0, y0).lerp(px(x0 + 1.0, y0), fx);
        let b = px(x0, y0 + 1.0).lerp(px(x0 + 1.0, y0 + 1.0), fx);
        a.lerp(b, fy)
    }

    /// The average colour (for the viewport and material previews).
    pub fn average(&self) -> Vec3 {
        if self.pixels.is_empty() {
            return Vec3::splat(0.5);
        }
        self.pixels.iter().copied().sum::<Vec3>() / self.pixels.len() as f32
    }
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let buf = &buf[..info.buffer_size()];
    let (w, h) = (info.width, info.height);
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf.to_vec(),
        png::ColorType::Rgba => buf.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        png::ColorType::Indexed => return Err("PNG: palette pictures aren't supported".into()),
    };
    Ok((w, h, rgb))
}

fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = dec.decode().map_err(|e| format!("JPEG: {e}"))?;
    let info = dec.info().ok_or("JPEG: no picture information")?;
    let (w, h) = (info.width as u32, info.height as u32);
    let rgb = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels,
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&g| [g, g, g]).collect(),
        jpeg_decoder::PixelFormat::L16 => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        jpeg_decoder::PixelFormat::CMYK32 => pixels
            .chunks_exact(4)
            .flat_map(|p| {
                let k = 255 - p[3] as u32;
                [(255 - p[0] as u32) * k / 255, (255 - p[1] as u32) * k / 255, (255 - p[2] as u32) * k / 255].map(|v| v as u8)
            })
            .collect(),
    };
    Ok((w, h, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker_png() -> Vec<u8> {
        let (w, h) = (4u32, 4u32);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let on = (x / 2 + y / 2) % 2 == 0;
                data.extend(if on { [255u8, 0, 0, 255] } else { [0, 0, 255, 255] });
            }
        }
        let img = crate::render::Image { width: w, height: h, pixels: data };
        crate::render::encode_png8(&img)
    }

    #[test]
    fn decodes_png_and_jpeg_and_samples() {
        let tex = Texture::from_file("checker.png", &checker_png()).unwrap();
        let img = tex.image().unwrap();
        assert_eq!((img.width, img.height), (4, 4));
        // Top-left is red (v = 1 is the top).
        let c = img.sample(Vec2::new(0.125, 0.875));
        assert!(c.x > 0.9 && c.z < 0.1, "{c:?}");
        let c = img.sample(Vec2::new(0.875, 0.875));
        assert!(c.z > 0.9, "{c:?}");
        // JPEG round trip through the encoder.
        let jpg = crate::render::encode(&crate::render::Image { width: 8, height: 8, pixels: vec![200; 256] }, crate::render::ImageFormat::Jpeg, 90, None).unwrap();
        let j = Image::decode(&jpg).unwrap();
        assert_eq!(j.width, 8);
        assert!((j.average().x - srgb_to_linear(200.0 / 255.0)).abs() < 0.05);
        assert!(Image::decode(b"nonsense").is_err());
        let back: Texture = serde_json::from_str(&serde_json::to_string(&tex).unwrap()).unwrap();
        assert_eq!(back, tex);
    }

    #[test]
    fn projections_cover_the_box() {
        let mut t = Texture::from_file("c.png", &checker_png()).unwrap();
        let (lo, hi) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        t.projection = Projection::Flat;
        assert!((t.uv(Vec3::new(1.0, 1.0, 0.0), Vec3::Z, lo, hi) - Vec2::ONE).length() < 1e-5);
        t.projection = Projection::Cubic;
        let uv = t.uv(Vec3::new(0.0, 1.0, 0.0), Vec3::Y, lo, hi);
        assert!((uv.x - 0.5).abs() < 1e-5);
        t.tiles = [2.0, 2.0];
        let uv = t.uv(Vec3::new(1.0, 1.0, 0.0), Vec3::Z, lo, hi);
        assert!((uv - Vec2::new(2.0, 2.0)).length() < 1e-5);
        t.projection = Projection::Spherical;
        let uv = t.uv(Vec3::new(0.0, 1.0, 0.0), Vec3::Y, lo, hi);
        assert!(uv.y > 1.9, "top of the sphere is the top of the picture: {uv:?}");
    }
}
