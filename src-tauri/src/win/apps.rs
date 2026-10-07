//! Which app is in front, and bringing a previous app back to the front.

use std::path::Path;
use std::time::{Duration, Instant};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
};

fn pid_of(hwnd: HWND) -> Option<i32> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid as i32)
}

pub fn frontmost_pid() -> Option<i32> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    pid_of(hwnd)
}

/// The executable name ("chrome.exe"): what identifies an app on Windows.
pub fn bundle_id(pid: i32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Path::new(&path).file_name().map(|n| n.to_string_lossy().to_lowercase())
    }
}

pub fn own_pid() -> i32 {
    std::process::id() as i32
}

pub(super) fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        Some(String::from_utf16_lossy(&buf[..n.max(0) as usize]))
    }
}

/// The app's main window: the foreground window if it's this app's,
/// otherwise its topmost visible titled window.
pub(super) fn main_window(pid: i32) -> Option<HWND> {
    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.is_invalid() && pid_of(foreground) == Some(pid) {
        return Some(foreground);
    }
    struct Search {
        pid: i32,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let owned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|o| !o.is_invalid());
        if pid_of(hwnd) == Some(search.pid) && unsafe { IsWindowVisible(hwnd) }.as_bool() && !owned && window_title(hwnd).is_some() {
            search.found = Some(hwnd);
            return false.into(); // windows are listed topmost first
        }
        true.into()
    }
    let mut search = Search { pid, found: None };
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

/// Makes `pid` the frontmost app again and waits (briefly) until it is.
/// Normally a no-op: the hotkey and the overlays never take focus away from it.
pub fn ensure_frontmost(pid: i32) -> bool {
    if frontmost_pid() == Some(pid) {
        return true;
    }
    let Some(hwnd) = main_window(pid) else { return false };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Windows only lets the app in front hand over the foreground, so
        // borrow its input queue for the switch.
        let front_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = front_thread != 0 && front_thread != me && AttachThreadInput(me, front_thread, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        let _ = BringWindowToTop(hwnd);
        if attached {
            let _ = AttachThreadInput(me, front_thread, false);
        }
    }
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
        if frontmost_pid() == Some(pid) {
            // Let the app finish restoring its focused field.
            std::thread::sleep(Duration::from_millis(40));
            return true;
        }
    }
    false
}
