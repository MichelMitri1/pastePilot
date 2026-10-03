//! Reads the user's current text selection in the frontmost app.
//!
//! 1. Accessibility (`AXSelectedText`): instant, doesn't touch the clipboard.
//! 2. Fallback for apps that don't expose page selections via AX (most
//!    browsers): send Cmd+C, read the clipboard, then restore the clipboard
//!    exactly as it was.

use crate::macos::{ax, keys, pasteboard};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// How long to wait for the app to respond to Cmd+C before deciding nothing is selected.
const COPY_TIMEOUT: Duration = Duration::from_millis(350);

pub fn capture(pid: i32) -> Option<String> {
    if let Some(text) = ax::selected_text(pid) {
        return Some(text);
    }
    capture_via_copy()
}

fn capture_via_copy() -> Option<String> {
    let before = pasteboard::change_count();
    let saved = pasteboard::snapshot();
    if !keys::copy() {
        return None;
    }

    let deadline = Instant::now() + COPY_TIMEOUT;
    let mut text = None;
    while Instant::now() < deadline {
        sleep(Duration::from_millis(8));
        if pasteboard::change_count() != before {
            // The app may clear the clipboard a moment before writing to it.
            text = pasteboard::read_string();
            if text.is_some() {
                break;
            }
        }
    }

    if pasteboard::change_count() != before {
        pasteboard::restore(saved);
    }
    text.filter(|t| !t.trim().is_empty())
}
