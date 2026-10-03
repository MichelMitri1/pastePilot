//! Which app is in front, and bringing a previous app back to the front.

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use std::time::{Duration, Instant};

pub fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier())
}

pub fn bundle_id(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?.bundleIdentifier().map(|b| b.to_string())
}

pub fn own_pid() -> i32 {
    std::process::id() as i32
}

/// Makes `pid` the frontmost app again and waits (briefly) until it is.
/// Normally a no-op: the hotkey and the HUD never take focus away from it.
pub fn ensure_frontmost(pid: i32) -> bool {
    if frontmost_pid() == Some(pid) {
        return true;
    }
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut raised_via_ax = false;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
        if frontmost_pid() == Some(pid) {
            // Let the app finish restoring its key window / focused field.
            std::thread::sleep(Duration::from_millis(40));
            return true;
        }
        if !raised_via_ax {
            raised_via_ax = super::ax::raise_app(pid);
        }
    }
    false
}
