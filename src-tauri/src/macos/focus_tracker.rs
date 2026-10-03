//! Remembers the last editable text input that had focus in each app.
//!
//! Why: selecting a student's message in a web page moves focus away from the
//! reply box. To "return focus to the text input I was previously using", we
//! watch focus changes (event-driven, no polling) in whichever app is
//! frontmost, and remember the last focused input per app. The paste step
//! re-focuses it through Accessibility before pressing Cmd+V.

use super::apps;
use super::ax::{self, AXObserverRef, Element};
use block2::RcBlock;
use core_foundation::base::{CFRelease, TCFType};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopRemoveSource, CFRunLoopSourceRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_app_kit::{NSWorkspace, NSWorkspaceDidActivateApplicationNotification};
use objc2_foundation::NSNotification;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{LazyLock, Mutex, MutexGuard};

struct Watch {
    pid: i32,
    observer: AXObserverRef,
    source: CFRunLoopSourceRef,
}

#[derive(Default)]
struct State {
    watch: Option<Watch>,
    last_input: HashMap<i32, Element>,
    web_enabled: HashSet<i32>,
}

// Raw CF pointers are only touched on the main thread; the Mutex is just for the map.
unsafe impl Send for State {}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Must be called on the main thread (observers attach to the main run loop).
pub fn start() {
    let block = RcBlock::new(|_note: NonNull<NSNotification>| {
        if let Some(pid) = apps::frontmost_pid() {
            watch(pid);
        }
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(token); // observe for the lifetime of the app

    if let Some(pid) = apps::frontmost_pid() {
        watch(pid);
    }
}

/// The last editable input the user focused in `pid`, if we saw one.
pub fn remembered_input(pid: i32) -> Option<Element> {
    state().last_input.get(&pid).cloned()
}

fn remember(pid: i32, el: Element) {
    state().last_input.insert(pid, el);
}

extern "C" fn on_focus_changed(_observer: AXObserverRef, element: CFTypeRefAlias, _n: CFStringRef, refcon: *mut c_void) {
    let pid = refcon as usize as i32;
    if let Some(el) = unsafe { Element::retain_from(element) } {
        if ax::is_editable(&el) {
            remember(pid, el);
        }
    }
}

type CFTypeRefAlias = core_foundation::base::CFTypeRef;

fn watch(pid: i32) {
    if pid == apps::own_pid() {
        return; // our own Settings window: keep watching the previous app
    }
    if !ax::is_trusted() {
        return; // retried on the next app switch once permission is granted
    }
    let (old, first_time) = {
        let mut st = state();
        if st.watch.as_ref().map(|w| w.pid) == Some(pid) {
            return;
        }
        (st.watch.take(), st.web_enabled.insert(pid))
    };
    if let Some(old) = old {
        unsafe {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), old.source, kCFRunLoopCommonModes);
            CFRelease(old.observer);
        }
    }
    if first_time {
        ax::enable_web_accessibility(pid);
    }

    unsafe {
        let mut observer: AXObserverRef = std::ptr::null();
        if ax::AXObserverCreate(pid, on_focus_changed, &mut observer) != 0 || observer.is_null() {
            return;
        }
        let app = Element::application(pid);
        let name = CFString::from_static_string("AXFocusedUIElementChanged");
        ax::AXObserverAddNotification(observer, app.raw(), name.as_concrete_TypeRef(), pid as usize as *mut c_void);
        let source = ax::AXObserverGetRunLoopSource(observer) as CFRunLoopSourceRef;
        CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
        state().watch = Some(Watch { pid, observer, source });
    }

    // Seed with whatever is focused right now.
    if let Some(el) = ax::focused_element(pid) {
        if ax::is_editable(&el) {
            remember(pid, el);
        }
    }
}
