//! The Settings window. Created on demand and destroyed when closed, so no
//! webview is kept in memory while the app idles in the menu bar.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";
pub const DEBUG_LABEL: &str = "debug";

/// GitHub Debug Mode window. Same frontend bundle; it renders the debug UI by window label.
pub fn open_debug(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(DEBUG_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, DEBUG_LABEL, WebviewUrl::App("index.html".into()))
        .title("GitHub Debug")
        .inner_size(760.0, 820.0)
        .min_inner_size(560.0, 520.0)
        .center()
        .focused(true)
        .build();
    if let Ok(window) = built {
        let _ = window.set_focus();
    }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("index.html".into()))
        .title("PastePilot Settings")
        .inner_size(780.0, 640.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .focused(true)
        .build();
    if let Ok(window) = built {
        let _ = window.set_focus();
    }
}
