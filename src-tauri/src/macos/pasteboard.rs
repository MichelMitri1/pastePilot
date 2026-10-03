//! General pasteboard (clipboard) access.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardType, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

fn general() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

/// Increments every time anything writes to the clipboard.
pub fn change_count() -> isize {
    general().changeCount()
}

pub fn read_string() -> Option<String> {
    general().stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string())
}

pub fn write_string(text: &str) -> bool {
    let pb = general();
    pb.clearContents();
    pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
}

/// Full copy of the clipboard (every item, every type) so it can be put back
/// exactly as it was after we borrow it for a Cmd+C capture.
pub struct Snapshot(Vec<Vec<(Retained<NSPasteboardType>, Retained<NSData>)>>);

pub fn snapshot() -> Snapshot {
    let mut items = Vec::new();
    if let Some(list) = general().pasteboardItems() {
        for item in list.iter() {
            let mut entries = Vec::new();
            for ty in item.types().iter() {
                if let Some(data) = item.dataForType(&ty) {
                    entries.push((ty, data));
                }
            }
            items.push(entries);
        }
    }
    Snapshot(items)
}

pub fn restore(snapshot: Snapshot) {
    let pb = general();
    pb.clearContents();
    if snapshot.0.is_empty() {
        return;
    }
    let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = snapshot
        .0
        .iter()
        .map(|entries| {
            let item = NSPasteboardItem::new();
            for (ty, data) in entries {
                item.setData_forType(data, ty);
            }
            ProtocolObject::from_retained(item)
        })
        .collect();
    pb.writeObjects(&NSArray::from_retained_slice(&items));
}
