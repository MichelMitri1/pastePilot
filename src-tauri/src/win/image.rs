//! Image helpers: a PNG, BMP or JPEG → compact JPEG data: URL.

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;

pub fn jpeg_data_url(bytes: &[u8], quality: f64, max_bytes: usize) -> Option<String> {
    let image = image::load_from_memory(bytes).ok()?.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, (quality * 100.0).round().clamp(1.0, 100.0) as u8).encode_image(&image).ok()?;
    if out.is_empty() || out.len() > max_bytes {
        return None;
    }
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}
