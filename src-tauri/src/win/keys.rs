//! Synthetic Ctrl+C / Ctrl+V via SendInput. Never sends Enter.

use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
};

/// Keys from the hotkey (Alt+R…) that would turn Ctrl+C into Ctrl+Alt+C.
const HELD: [VIRTUAL_KEY; 6] = [VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN];

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    }
}

fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

fn control_key(vk: VIRTUAL_KEY) -> bool {
    // Give you a moment to let go of the hotkey.
    let deadline = Instant::now() + Duration::from_millis(300);
    while HELD.iter().any(|k| held(*k)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut inputs = vec![key(VK_CONTROL, false)];
    // Still held: release them while Ctrl is down, so letting go of Alt isn't
    // a lone Alt press (which would open the app's menu).
    inputs.extend(HELD.iter().filter(|k| held(**k)).map(|k| key(*k, true)));
    inputs.extend([key(vk, false), key(vk, true), key(VK_CONTROL, true)]);
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len() }
}

pub fn copy() -> bool {
    control_key(VK_C)
}

pub fn paste() -> bool {
    control_key(VK_V)
}
