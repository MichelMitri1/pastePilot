//! PastePilot: select a student's message, press Option+R, get a reply pasted.
//!
//! Module map:
//! - flow.rs       the end-to-end reply workflow + rewrites
//! - selection.rs  reading the selected text (AX, then Cmd+C fallback)
//! - memory.rs     conversation memory (scoping, history budget)
//! - modes.rs      smart mode classifier + per-mode instructions
//! - retrieval.rs  knowledge base / reply example search, style profile
//! - rewrite.rs    quick rewrites, replacing the pasted reply in place
//! - sent_watch.rs saves the reply you actually sent (with edits) to memory
//! - debug.rs      GitHub Debug Mode pipeline (read-only repo analysis → reply)
//! - github.rs     public GitHub API client + cache
//! - repo_search.rs  file filtering, filename search, import following
//! - db.rs         SQLite schema, migrations, queries
//! - import.rs     bulk import parsers
//! - openai.rs     streaming Chat Completions client
//! - prompt.rs     prompt pipeline, reply cleanup
//! - settings.rs   settings JSON on disk
//! - keychain.rs   API key in the macOS Keychain
//! - state.rs      shared in-memory state
//! - shortcut.rs   global hotkey
//! - tray.rs       menu bar icon and menu
//! - updater.rs    in-app updates from GitHub Releases
//! - checks.rs     deterministic static checks for GitHub Debug
//! - project.rs    project type detection (React, Next.js, Vue, vanilla…)
//! - casebook.rs   issue history + saved fix library
//! - case.rs       multi-message cases (⌥A)
//! - cliphistory.rs  local clipboard history with privacy controls
//! - analytics.rs  local usage metrics for the dashboard
//! - feedback.rs   learns style rules from your edits
//! - fluff.rs      removes generic AI phrasing
//! - voice.rs      hold-to-talk voice commands
//! - review.rs     Assignment Review Mode (example site vs student site vs repo)
//! - browser.rs    headless website inspection (rendered DOM, console, screenshots)
//! - codebase.rs   shared read-only repository reader (GitHub Debug + Assignment Review)
//! - windows.rs    Settings and GitHub Debug windows
//! - commands.rs   commands called by the React Settings UI
//! - platform.rs    the native OS layer: macos/ (Accessibility, AppKit) or win/ (UI Automation, Win32)
//! - macos/        Accessibility, key events, clipboard, app focus, HUD, rewrite bar
//! - win/          the same on Windows

mod analytics;
mod browser;
mod case;
mod casebook;
mod checks;
mod codebase;
mod cliphistory;
mod commands;
mod db;
mod debug;
mod feedback;
mod flow;
mod fluff;
mod github;
mod import;
mod keychain;
#[cfg(target_os = "macos")]
mod macos;
mod memory;
mod modes;
mod openai;
mod platform;
mod project;
mod prompt;
mod repo_search;
mod retrieval;
mod review;
mod rewrite;
mod selection;
mod sent_watch;
mod settings;
mod shortcut;
mod state;
mod tray;
mod updater;
mod voice;
#[cfg(windows)]
mod win;
mod windows;

use platform::{ax, focus_tracker, hud};
use state::AppState;
use std::time::Duration;
use tauri::{Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| shortcut::dispatch(app, shortcut, event.state()))
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_status,
            commands::set_api_key,
            commands::clear_api_key,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::list_examples,
            commands::save_example,
            commands::delete_example,
            commands::clear_examples,
            commands::import_examples,
            commands::list_kb,
            commands::save_kb,
            commands::set_kb_enabled,
            commands::delete_kb,
            commands::clear_kb,
            commands::import_kb,
            commands::get_memory,
            commands::new_conversation,
            commands::clear_current_conversation,
            commands::clear_all_conversations,
            debug::debug_get_context,
            debug::debug_analyze,
            debug::debug_generate_reply,
            debug::debug_paste,
            debug::debug_copy,
            debug::open_github_url,
            commands::list_fixes,
            commands::save_fix,
            commands::delete_fix,
            commands::clear_fixes,
            commands::import_fixes,
            commands::list_issues,
            commands::delete_issue,
            commands::clear_issues,
            commands::list_clipboard,
            commands::delete_clipboard_item,
            commands::clear_clipboard,
            commands::copy_clipboard_item,
            commands::case_add_items,
            commands::case_status,
            commands::case_clear,
            commands::get_dashboard,
            commands::reset_analytics,
            commands::get_learning,
            commands::reset_learning,
            windows::take_pending_section,
            review::review_get_context,
            review::review_run,
            review::review_feedback,
            review::review_paste,
            review::review_copy,
            review::review_save_preset,
            review::review_delete_preset,
            updater::check_for_update,
            updater::install_update,
            #[cfg(windows)]
            win::overlay::overlay_content,
            #[cfg(windows)]
            win::overlay::overlay_fit,
            #[cfg(windows)]
            win::action_bar::bar_click,
            #[cfg(windows)]
            win::action_bar::bar_mode,
        ])
        .setup(|app| {
            // Menu bar only: no Dock icon, never steals focus on launch.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            app.manage(AppState::load(&handle));
            let state = app.state::<AppState>();
            let settings = state.settings();

            ax::set_global_timeout(1.0);
            focus_tracker::start();
            #[cfg(windows)]
            win::overlay::prepare(&handle);
            tray::create(&handle)?;

            if let Err(message) = shortcut::register_all(&handle, &settings) {
                hud::show(&handle, &message, Some(Duration::from_secs(4)));
            }
            if let Some(message) = &state.db_error {
                hud::show(&handle, message, Some(Duration::from_secs(4)));
            }

            let needs_setup = !ax::is_trusted() || state.api_key().is_none();
            if !ax::is_trusted() {
                ax::prompt_for_trust();
            }
            if needs_setup {
                windows::open_settings(&handle);
            }

            updater::start_background_checks(&handle);
            cliphistory::start_watcher(&handle);

            let warm = handle.clone();
            tauri::async_runtime::spawn(async move {
                warm.state::<AppState>().openai.warm_up().await;
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start PastePilot")
        .run(|_app, event| {
            // Closing the Settings window must not quit the menu bar app.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
