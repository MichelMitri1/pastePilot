//! Rewrite bar: a small floating row of buttons shown after a reply.
//!
//! [Mode ▾] Shorter Alt+1 · Friendlier Alt+2 · Professional Alt+3 · Explain more Alt+4 · Regenerate Alt+5 · ★ Save · ✕
//!
//! Clicking it doesn't take keyboard focus from the reply box, so the new
//! version can be pasted straight back. It hides itself after a short while.

use super::overlay::{self, BAR};
use crate::flow::{self, Followup};
use crate::modes::Mode;
use crate::rewrite::RewriteAction;
use crate::shortcut;
use crate::state::AppState;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const AUTO_HIDE: Duration = Duration::from_secs(30);

const TAG_SAVE: isize = 6;
const TAG_CLOSE: isize = 7;
const TAG_DETAILS: isize = 8;

/// Shows the "Diagnosis" button (replies that came from GitHub Debug).
static DETAILS: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Index of the mode currently shown.
static MODE: AtomicUsize = AtomicUsize::new(0);

pub fn set_details_available(on: bool) {
    DETAILS.store(on, Ordering::SeqCst);
}

/// Shows the bar for the reply that was just produced, with `mode` selected.
pub fn show(app: &AppHandle, mode: Mode) {
    let shortcuts_on = app.state::<AppState>().settings().rewrite_shortcuts;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    MODE.store(mode.index(), Ordering::SeqCst);

    let mut buttons: Vec<_> = RewriteAction::ALL
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let hint = format!("Alt+{}", i + 1);
            json!({
                "tag": i + 1,
                "title": if shortcuts_on { format!("{} {hint}", short_label(*action)) } else { short_label(*action).to_string() },
                "tip": format!("{} ({hint})", action.label()),
            })
        })
        .collect();
    if DETAILS.load(Ordering::SeqCst) {
        buttons.push(json!({ "tag": TAG_DETAILS, "title": "Diagnosis", "tip": "See what GitHub Debug found: files, lines, diff" }));
    }
    buttons.push(json!({ "tag": TAG_SAVE, "title": "★ Save", "tip": "Save this reply as a style example" }));
    buttons.push(json!({ "tag": TAG_CLOSE, "title": "✕", "tip": "Hide" }));

    overlay::show(
        app,
        BAR,
        json!({
            "seq": generation,
            "mode": mode.index(),
            "modes": Mode::ALL.iter().map(|m| m.label()).collect::<Vec<_>>(),
            "buttons": buttons,
        }),
    );
    shortcut::set_rewrite_keys(app, shortcuts_on);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTO_HIDE).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    overlay::hide(app, BAR);
    shortcut::set_rewrite_keys(app, false);
}

fn short_label(action: RewriteAction) -> &'static str {
    match action {
        RewriteAction::Professional => "Professional",
        other => other.label(),
    }
}

#[tauri::command]
pub fn bar_click(app: AppHandle, tag: isize) {
    match tag {
        1..=5 => flow::trigger_followup(app.clone(), Followup::Rewrite(RewriteAction::ALL[(tag - 1) as usize])),
        TAG_SAVE => flow::save_last_as_example(&app),
        TAG_DETAILS => crate::debug::open_review(&app),
        TAG_CLOSE => hide(&app),
        _ => {}
    }
}

#[tauri::command]
pub fn bar_mode(app: AppHandle, index: usize) {
    let Some(mode) = Mode::ALL.get(index).copied() else { return };
    if MODE.swap(index, Ordering::SeqCst) != index {
        flow::trigger_followup(app, Followup::Mode(mode));
    }
}
