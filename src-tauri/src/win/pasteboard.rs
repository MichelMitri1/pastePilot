//! Clipboard access through the Win32 clipboard.

use std::time::Duration;
use windows::core::HSTRING;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
/// GDI handles (bitmaps, metafiles, palettes) rather than memory blocks.
/// Windows recreates them from the DIB and text formats.
const NOT_MEMORY: [u32; 8] = [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];

/// The clipboard is open while this exists. Other apps may hold it briefly, so opening retries.
struct Open;

impl Open {
    fn new() -> Option<Self> {
        for _ in 0..20 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Open);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
}

fn available(format: u32) -> bool {
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// Copies a clipboard format's memory block. The clipboard must be open.
fn read(format: u32) -> Option<Vec<u8>> {
    unsafe {
        let memory = HGLOBAL(GetClipboardData(format).ok()?.0);
        let ptr = GlobalLock(memory) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, GlobalSize(memory)).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
    }
}

/// Puts a memory block on the clipboard. The clipboard must be open and emptied.
fn write(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
        let ptr = GlobalLock(memory) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(memory);
        // On success the clipboard owns the memory.
        if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        true
    }
}

/// Increments every time anything writes to the clipboard.
pub fn change_count() -> isize {
    unsafe { GetClipboardSequenceNumber() as isize }
}

pub fn read_string() -> Option<String> {
    let _open = Open::new()?;
    let bytes = read(CF_UNICODETEXT)?;
    let wide: Vec<u16> =
        bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    Some(String::from_utf16_lossy(&wide))
}

pub fn write_string(text: &str) -> bool {
    let ok = (|| {
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.ok()?;
        let bytes: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        write(CF_UNICODETEXT, &bytes).then_some(())
    })()
    .is_some();
    crate::cliphistory::mark_own_change();
    ok
}

/// True when the copy is marked as a password or private, the way password
/// managers tell Windows clipboard history to skip it.
pub fn is_sensitive() -> bool {
    if available(format("ExcludeClipboardContentFromMonitorProcessing")) || available(format("Clipboard Viewer Ignore")) {
        return true;
    }
    let history = format("CanIncludeInClipboardHistory");
    available(history)
        && Open::new().and_then(|_open| read(history)).is_some_and(|b| b.len() >= 4 && b[..4] == [0, 0, 0, 0])
}

/// Full copy of the clipboard (every format) so it can be put back exactly as
/// it was after we borrow it for a Ctrl+C capture.
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

pub fn snapshot() -> Snapshot {
    let Some(_open) = Open::new() else { return Snapshot(Vec::new()) };
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !NOT_MEMORY.contains(&format) {
            if let Some(bytes) = read(format) {
                formats.push((format, bytes));
            }
        }
    }
    Snapshot(formats)
}

pub fn restore(snapshot: Snapshot) {
    crate::cliphistory::suppress_for(Duration::from_millis(1500));
    let Some(_open) = Open::new() else { return };
    if unsafe { EmptyClipboard() }.is_err() {
        return;
    }
    for (format, bytes) in &snapshot.0 {
        write(*format, bytes);
    }
}

/// True when the clipboard holds an image (a screenshot or "Copy image").
pub fn has_image() -> bool {
    available(format("PNG")) || available(CF_DIB) || available(CF_DIBV5)
}

/// The clipboard image as a JPEG data: URL (compact enough to send with an analysis).
pub fn image_data_url() -> Option<String> {
    let bytes = {
        let _open = Open::new()?;
        match read(format("PNG")) {
            Some(png) => png,
            None => bmp_from_dib(&read(CF_DIB)?)?,
        }
    };
    super::image::jpeg_data_url(&bytes, 0.82, 4_000_000)
}

/// A clipboard DIB is a .bmp file without its 14-byte file header.
fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |i: usize| dib.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header_size = u32_at(0)?;
    let bit_count = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let compression = u32_at(16)?;
    let colors_used = u32_at(32)?;
    let masks = if header_size == 40 && compression == 3 { 12 } else { 0 }; // BI_BITFIELDS
    let palette = if colors_used > 0 { colors_used } else if bit_count <= 8 { 1 << bit_count } else { 0 };
    let offset = 14 + header_size + masks + palette * 4;
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&offset.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}
