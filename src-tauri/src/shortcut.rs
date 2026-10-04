//! Global hotkeys.
//!
//! - Generate reply: default ⌥R ("Alt+R").
//! - New conversation + generate: default ⌥⇧R ("Alt+Shift+R"), optional.
//! - Rewrite keys ⌥1–⌥5: registered only while the rewrite bar is visible, so
//!   they don't steal those keys from other apps the rest of the time.

use crate::flow::{self, Followup, Trigger};
use crate::rewrite::RewriteAction;
use crate::settings::Settings;
use crate::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static REWRITE_KEYS_ON: AtomicBool = AtomicBool::new(false);

fn parse(accelerator: &str) -> Result<Shortcut, String> {
    accelerator.parse().map_err(|_| format!("\"{accelerator}\" isn't a valid shortcut."))
}

fn register(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut = parse(accelerator)?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|_| format!("Couldn't register {accelerator}. Another app may be using it."))
}

/// Registers the always-on hotkeys from settings.
pub fn register_all(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    register(app, &settings.shortcut)?;
    let mut used = vec![settings.shortcut.clone()];
    for extra in optional_shortcuts(settings) {
        if !extra.is_empty() && !used.iter().any(|u| u == extra) {
            register(app, extra)?;
            used.push(extra.to_string());
        }
    }
    Ok(())
}

/// Hotkeys besides "generate". Voice is only registered while voice commands are on,
/// so ⌥V keeps working normally otherwise.
fn optional_shortcuts(s: &Settings) -> Vec<&str> {
    let mut v = vec![s.new_conversation_shortcut.trim(), s.debug_shortcut.trim(), s.case_shortcut.trim(), s.review_shortcut.trim()];
    if s.voice_enabled {
        v.push(s.voice_shortcut.trim());
    }
    v
}

/// Validates and swaps hotkeys, restoring the old ones if the new ones fail.
pub fn apply(app: &AppHandle, old: &Settings, new: &Settings) -> Result<(), String> {
    parse(&new.shortcut)?;
    let all = [
        new.shortcut.trim(),
        new.new_conversation_shortcut.trim(),
        new.debug_shortcut.trim(),
        new.case_shortcut.trim(),
        new.voice_shortcut.trim(),
        new.review_shortcut.trim(),
    ];
    for (i, s) in all.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        parse(s)?;
        if all[..i].contains(s) {
            return Err("Each shortcut must be different.".into());
        }
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    REWRITE_KEYS_ON.store(false, Ordering::SeqCst);
    if let Err(e) = register_all(app, new) {
        let _ = gs.unregister_all();
        let _ = register_all(app, old);
        return Err(e);
    }
    Ok(())
}

/// Turns ⌥1–⌥5 on or off (called when the rewrite bar shows/hides).
pub fn set_rewrite_keys(app: &AppHandle, on: bool) {
    if REWRITE_KEYS_ON.swap(on, Ordering::SeqCst) == on {
        return;
    }
    let gs = app.global_shortcut();
    for action in RewriteAction::ALL {
        if let Ok(shortcut) = parse(action.shortcut()) {
            let _ = if on { gs.register(shortcut) } else { gs.unregister(shortcut) };
        }
    }
}

/// Called for every hotkey press and release.
pub fn dispatch(app: &AppHandle, pressed: &Shortcut, key_state: ShortcutState) {
    let settings = app.state::<AppState>().settings();
    let matches = |accel: &str| !accel.trim().is_empty() && parse(accel.trim()).is_ok_and(|s| s.id() == pressed.id());

    // Voice is hold-to-talk: it's the only hotkey that cares about release.
    if settings.voice_enabled && matches(&settings.voice_shortcut) {
        match key_state {
            ShortcutState::Pressed => crate::voice::on_press(app),
            ShortcutState::Released => crate::voice::on_release(app),
        }
        return;
    }
    if key_state != ShortcutState::Pressed {
        return;
    }
    if matches(&settings.review_shortcut) {
        crate::review::open(app.clone(), false);
        return;
    }
    if matches(&settings.case_shortcut) {
        crate::case::add_selection(app.clone(), false);
        return;
    }

    if matches(&settings.shortcut) {
        flow::trigger(app.clone(), Trigger::Hotkey);
    } else if !settings.new_conversation_shortcut.trim().is_empty() && matches(settings.new_conversation_shortcut.trim()) {
        flow::trigger(app.clone(), Trigger::NewConversation);
    } else if !settings.debug_shortcut.trim().is_empty() && matches(settings.debug_shortcut.trim()) {
        crate::debug::open(app.clone(), false);
    } else if let Some(action) = RewriteAction::ALL.into_iter().find(|a| matches(a.shortcut())) {
        if REWRITE_KEYS_ON.load(Ordering::SeqCst) {
            flow::trigger_followup(app.clone(), Followup::Rewrite(action));
        }
    }
}
