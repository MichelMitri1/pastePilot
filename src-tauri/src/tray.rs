//! Menu bar icon and menu.

use crate::flow::{self, Followup, Trigger};
use crate::modes::Mode;
use crate::rewrite::RewriteAction;
use crate::settings::Settings;
use crate::state::{AppState, Menus};
use crate::windows;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let sep = || PredefinedMenuItem::separator(app);

    let generate = MenuItem::with_id(app, "generate", "Generate Reply", true, None::<&str>)?;
    let github_debug = MenuItem::with_id(app, "github_debug", "GitHub Debug…", true, None::<&str>)?;
    let assignment_review = MenuItem::with_id(app, "assignment_review", "Assignment Review…", true, None::<&str>)?;
    let add_case = MenuItem::with_id(app, "case_add", "Add Selection to Case", true, None::<&str>)?;
    let clear_case = MenuItem::with_id(app, "case_clear", "Clear Case", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "clipboard_history", "Clipboard History…", true, None::<&str>)?;
    let dashboard = MenuItem::with_id(app, "analytics", "Analytics…", true, None::<&str>)?;

    let rewrite_items: Vec<MenuItem<Wry>> = RewriteAction::ALL
        .iter()
        .map(|a| MenuItem::with_id(app, format!("rewrite:{}", a.id()), a.label(), true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let save_example = MenuItem::with_id(app, "save_example", "Save as Reply Example", true, None::<&str>)?;
    let rewrite_sep = sep()?;
    let mut rewrite_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        rewrite_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    rewrite_refs.push(&rewrite_sep);
    rewrite_refs.push(&save_example);
    let rewrite = Submenu::with_items(app, "Rewrite Last Reply", true, &rewrite_refs)?;

    let mut mode_items =
        vec![CheckMenuItem::with_id(app, "mode:auto", "Auto-detect", true, settings.mode_override == "auto", None::<&str>)?];
    for mode in Mode::ALL {
        mode_items.push(CheckMenuItem::with_id(
            app,
            format!("mode:{}", mode.id()),
            mode.label(),
            true,
            settings.mode_override == mode.id(),
            None::<&str>,
        )?);
    }
    let mode_sep = sep()?;
    let mut mode_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![&mode_items[0], &mode_sep];
    mode_refs.extend(mode_items[1..].iter().map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>));
    let modes = Submenu::with_items(app, "Mode", true, &mode_refs)?;

    let new_conversation = MenuItem::with_id(app, "new_conversation", "New Conversation", true, None::<&str>)?;
    let clear_conversation = MenuItem::with_id(app, "clear_conversation", "Clear Current Conversation", true, None::<&str>)?;
    let memory = CheckMenuItem::with_id(app, "memory", "Conversation Memory", true, settings.memory_enabled, None::<&str>)?;
    let auto_paste = CheckMenuItem::with_id(app, "auto_paste", "Auto Paste", true, settings.auto_paste, None::<&str>)?;
    let open_settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let check_updates = MenuItem::with_id(app, "check_updates", "Check for Updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit PastePilot", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &generate,
            &github_debug,
            &assignment_review,
            &rewrite,
            &modes,
            &sep()?,
            &new_conversation,
            &clear_conversation,
            &add_case,
            &clear_case,
            &memory,
            &auto_paste,
            &sep()?,
            &history,
            &dashboard,
            &open_settings,
            &check_updates,
            &quit,
        ],
    )?;
    *state.menus.lock().unwrap_or_else(|e| e.into_inner()) =
        Menus { auto_paste: Some(auto_paste), memory: Some(memory), modes: mode_items };

    // macOS: a monochrome menu bar glyph. Windows: the app icon, which reads on light and dark taskbars.
    #[cfg(target_os = "macos")]
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    #[cfg(windows)]
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("PastePilot")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "generate" => flow::trigger(app.clone(), Trigger::Menu),
        "github_debug" => crate::debug::open(app.clone(), true),
        "assignment_review" => crate::review::open(app.clone(), true),
        "case_add" => crate::case::add_selection(app.clone(), true),
        "case_clear" => crate::case::clear(app),
        "clipboard_history" => open_section(app, "clipboard"),
        "analytics" => open_section(app, "analytics"),
        "settings" => windows::open_settings(app),
        "check_updates" => {
            // The General page checks for updates as soon as it opens.
            windows::open_settings(app);
            let _ = app.emit_to(windows::SETTINGS_LABEL, "check-updates", ());
        }
        "save_example" => flow::save_last_as_example(app),
        "new_conversation" => flow::new_conversation(app),
        "clear_conversation" => flow::clear_current_conversation(app),
        "auto_paste" => toggle(app, |s| s.auto_paste = !s.auto_paste),
        "memory" => toggle(app, |s| s.memory_enabled = !s.memory_enabled),
        "quit" => app.exit(0),
        other => {
            if let Some(action) = other.strip_prefix("rewrite:").and_then(RewriteAction::from_id) {
                flow::trigger_followup(app.clone(), Followup::Rewrite(action));
            } else if let Some(mode) = other.strip_prefix("mode:") {
                flow::set_mode_override(app, mode);
            }
        }
    }
}

fn toggle(app: &AppHandle, change: impl FnOnce(&mut Settings)) {
    let state = app.state::<AppState>();
    let mut settings = state.settings();
    change(&mut settings);
    if state.replace_settings(settings.clone()).is_ok() {
        sync(app, &settings);
        emit_settings(app, &settings);
    }
}

/// Opens Settings on a specific page.
fn open_section(app: &AppHandle, section: &'static str) {
    windows::open_settings_at(app, section);
}

/// Keeps the menu's check marks in line with settings.
pub fn sync(app: &AppHandle, settings: &Settings) {
    let state = app.state::<AppState>();
    let menus = state.menus.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(item) = &menus.auto_paste {
        let _ = item.set_checked(settings.auto_paste);
    }
    if let Some(item) = &menus.memory {
        let _ = item.set_checked(settings.memory_enabled);
    }
    for (i, item) in menus.modes.iter().enumerate() {
        let id = if i == 0 { "auto" } else { Mode::ALL[i - 1].id() };
        let _ = item.set_checked(settings.mode_override == id);
    }
}

/// Tells an open Settings window that settings changed from the menu bar.
pub fn emit_settings(app: &AppHandle, settings: &Settings) {
    let _ = app.emit("settings-changed", settings);
}
