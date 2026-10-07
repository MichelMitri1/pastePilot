//! The two floating overlays on Windows: the status pill ("hud") and the
//! rewrite bar ("bar"). Small borderless webview windows that never take
//! keyboard focus (WS_EX_NOACTIVATE) and are shown without activating, so the
//! reply box keeps focus. Created hidden at launch so they appear instantly.
//!
//! Rust sends a window its content; the page lays it out and reports its size
//! back (`overlay_fit`), and only then is the window placed and shown.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, SystemParametersInfoW, HWND_TOPMOST, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

pub const HUD: &str = "hud";
pub const BAR: &str = "bar";

/// Distance from the top of the screen, in points: the bar sits just below the pill.
fn top(label: &str) -> f64 {
    if label == BAR { 12.0 + 34.0 + 8.0 } else { 12.0 }
}

/// What each overlay should show; None = hidden.
static CONTENT: LazyLock<Mutex<HashMap<String, Value>>> = LazyLock::new(Default::default);

fn content() -> MutexGuard<'static, HashMap<String, Value>> {
    CONTENT.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn prepare(app: &AppHandle) {
    for label in [HUD, BAR] {
        let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
            .title("PastePilot")
            .inner_size(240.0, 36.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .build();
        if let (Ok(window), true) = (built, label == HUD) {
            let _ = window.set_ignore_cursor_events(true);
        }
    }
}

/// Shows `payload` in the overlay (it appears once the page has measured it).
pub(super) fn show(app: &AppHandle, label: &str, payload: Value) {
    content().insert(label.to_string(), payload.clone());
    let _ = app.emit_to(label, "overlay-content", payload);
}

pub(super) fn hide(app: &AppHandle, label: &str) {
    content().remove(label);
    let app2 = app.clone();
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        if let Some(hwnd) = hwnd(&app2, &label) {
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    });
}

fn hwnd(app: &AppHandle, label: &str) -> Option<HWND> {
    Some(HWND(app.get_webview_window(label)?.hwnd().ok()?.0 as _))
}

/// The page asks what to show when it loads.
#[tauri::command]
pub fn overlay_content(label: String) -> Option<Value> {
    content().get(&label).cloned()
}

/// The page measured its content (in CSS pixels): place the window top center and show it.
#[tauri::command]
pub fn overlay_fit(app: AppHandle, label: String, width: f64, height: f64) {
    if !content().contains_key(&label) {
        return; // hidden again in the meantime
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(hwnd) = hwnd(&app2, &label) else { return };
        unsafe {
            let scale = match GetDpiForWindow(hwnd) {
                0 => 1.0,
                dpi => dpi as f64 / 96.0,
            };
            let mut area = RECT::default();
            let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut area as *mut RECT as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
            let (w, h) = ((width * scale).ceil() as i32, (height * scale).ceil() as i32);
            let x = area.left + (area.right - area.left - w) / 2;
            let y = area.top + (top(&label) * scale).round() as i32;
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    });
}
