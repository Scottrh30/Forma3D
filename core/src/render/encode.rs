//! Saving rendered images: PNG (8 or 16 bits) and JPEG.

use super::ImageFormat;
use super::post::Image;

/// An 8-bit RGBA PNG.
pub fn encode_png8(img: &Image) -> Vec<u8> {
    png_bytes(img.width, img.height, png::BitDepth::Eight, &img.pixels).unwrap_or_default()
}

fn png_bytes(width: u32, height: u32, depth: png::BitDepth, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, width.max(1), height.max(1));
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(depth);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(data).map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

/// Encodes an image in a file format. `beauty16` (16-bit RGBA) is used for
/// 16-bit PNG when given.
pub fn encode(img: &Image, format: ImageFormat, quality: u8, beauty16: Option<&[u16]>) -> Result<Vec<u8>, String> {
    match format {
        ImageFormat::Png => png_bytes(img.width, img.height, png::BitDepth::Eight, &img.pixels),
        ImageFormat::Png16 => {
            let wide: Vec<u16> = match beauty16 {
                Some(b) if b.len() == img.pixels.len() => b.to_vec(),
                _ => img.pixels.iter().map(|&v| v as u16 * 257).collect(),
            };
            let bytes: Vec<u8> = wide.iter().flat_map(|v| v.to_be_bytes()).collect();
            png_bytes(img.width, img.height, png::BitDepth::Sixteen, &bytes)
        }
        ImageFormat::Jpeg => {
            if img.width > u16::MAX as u32 || img.height > u16::MAX as u32 {
                return Err("The image is too large for JPEG.".into());
            }
            // JPEG has no alpha: the background colour (or sky) shows instead.
            let rgb: Vec<u8> = img.pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
            let mut buf = Vec::new();
            let enc = jpeg_encoder::Encoder::new(&mut buf, quality.clamp(1, 100));
            enc.encode(&rgb, img.width as u16, img.height as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
            Ok(buf)
        }
    }
}
