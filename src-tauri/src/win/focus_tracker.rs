//! Remembers the last editable text input that had focus in each app.
//!
//! Why: selecting a student's message in a web page moves focus away from the
//! reply box. The paste step puts focus back into the input you were using.
//! Focus changes come from a system focus event hook (event-driven, no
//! polling) on a background thread.

use super::apps;
use super::ax::{self, Element};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, EVENT_OBJECT_FOCUS, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

static LAST_INPUT: LazyLock<Mutex<HashMap<i32, Element>>> = LazyLock::new(Default::default);

fn last_input() -> MutexGuard<'static, HashMap<i32, Element>> {
    LAST_INPUT.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn start() {
    let _ = std::thread::Builder::new().name("focus-tracker".into()).spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if let Some(pid) = apps::frontmost_pid() {
            seen(pid);
        }
        let hook = SetWinEventHook(
            EVENT_OBJECT_FOCUS,
            EVENT_OBJECT_FOCUS,
            None,
            Some(on_focus),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if hook.is_invalid() {
            return;
        }
        // The hook delivers its events through this thread's message loop.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    });
}

/// The last editable input the user focused in `pid`, if we saw one.
pub fn remembered_input(pid: i32) -> Option<Element> {
    last_input().get(&pid).cloned()
}

unsafe extern "system" fn on_focus(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    if let Some(pid) = apps::frontmost_pid() {
        seen(pid);
    }
}

fn seen(pid: i32) {
    if let Some(el) = ax::focused_element(pid).filter(ax::is_editable) {
        last_input().insert(pid, el);
    }
}
