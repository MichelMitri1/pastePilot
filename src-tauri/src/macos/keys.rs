//! Synthetic Cmd+C / Cmd+V via CoreGraphics events.
//! Requires the Accessibility permission. Never sends Return.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KEY_C: CGKeyCode = 8; // kVK_ANSI_C
const KEY_V: CGKeyCode = 9; // kVK_ANSI_V

fn command_key(code: CGKeyCode) -> bool {
    // A private event source keeps physically held keys (e.g. the Option of
    // the Option+R hotkey) from leaking into the synthetic event.
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::Private) else {
        return false;
    };
    for down in [true, false] {
        let Ok(event) = CGEvent::new_keyboard_event(source.clone(), code, down) else {
            return false;
        };
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    true
}

pub fn copy() -> bool {
    command_key(KEY_C)
}

pub fn paste() -> bool {
    command_key(KEY_V)
}
