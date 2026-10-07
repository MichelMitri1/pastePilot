//! Tiny floating status pill ("Generating...", "Reply pasted", errors).
//! Never takes keyboard focus and ignores the mouse.

use super::overlay::{self, HUD};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::AppHandle;

/// Bumped on every show/hide so a stale auto-hide timer doesn't hide a newer message.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Shows `text`. With `hide_after`, hides itself after that long.
pub fn show(app: &AppHandle, text: &str, hide_after: Option<Duration>) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // `seq` makes the same text shown twice still count as new.
    overlay::show(app, HUD, serde_json::json!({ "text": text, "seq": generation }));
    if let Some(delay) = hide_after {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            if GENERATION.load(Ordering::SeqCst) == generation {
                overlay::hide(&app, HUD);
            }
        });
    }
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    overlay::hide(app, HUD);
}
