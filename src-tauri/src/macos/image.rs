//! Image helpers: any image macOS can read (PNG, TIFF, HEIC…) → compact JPEG data: URL.

use base64::Engine;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImageCompressionFactor};
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSString};

pub fn jpeg_data_url(bytes: &[u8], quality: f64, max_bytes: usize) -> Option<String> {
    let data = NSData::with_bytes(bytes);
    let rep = NSBitmapImageRep::imageRepWithData(&data)?;
    let q = NSNumber::numberWithDouble(quality);
    let key: &NSString = unsafe { NSImageCompressionFactor };
    let props = NSDictionary::from_slices(&[key], &[q.as_ref()]);
    let jpeg = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &props) }?;
    let out = jpeg.to_vec();
    if out.is_empty() || out.len() > max_bytes {
        return None;
    }
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}
