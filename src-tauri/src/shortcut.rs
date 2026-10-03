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
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

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
    let extra = settings.new_conversation_shortcut.trim();
    if !extra.is_empty() && extra != settings.shortcut {
        register(app, extra)?;
    }
    Ok(())
}

/// Validates and swaps hotkeys, restoring the old ones if the new ones fail.
pub fn apply(app: &AppHandle, old: &Settings, new: &Settings) -> Result<(), String> {
    parse(&new.shortcut)?;
    if !new.new_conversation_shortcut.trim().is_empty() {
        parse(new.new_conversation_shortcut.trim())?;
        if new.new_conversation_shortcut.trim() == new.shortcut {
            return Err("The two shortcuts must be different.".into());
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

/// Called for every hotkey press.
pub fn dispatch(app: &AppHandle, pressed: &Shortcut) {
    let settings = app.state::<AppState>().settings();
    let matches = |accel: &str| parse(accel).is_ok_and(|s| s.id() == pressed.id());

    if matches(&settings.shortcut) {
        flow::trigger(app.clone(), Trigger::Hotkey);
    } else if !settings.new_conversation_shortcut.trim().is_empty() && matches(settings.new_conversation_shortcut.trim()) {
        flow::trigger(app.clone(), Trigger::NewConversation);
    } else if let Some(action) = RewriteAction::ALL.into_iter().find(|a| matches(a.shortcut())) {
        if REWRITE_KEYS_ON.load(Ordering::SeqCst) {
            flow::trigger_followup(app.clone(), Followup::Rewrite(action));
        }
    }
}
